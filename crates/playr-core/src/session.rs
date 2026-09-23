//! A session: one library and one player, and what a frontend can do with them.
//!
//! Operations take what they act on as arguments, a track, an index into the
//! selection or a playlist id, never a cursor or a view, and report what they
//! did as a [`Notice`]. A frontend turns its own cursor or click into those
//! arguments and words the notice. `docs/architecture.md` sets out the
//! design. Work that takes seconds runs on its own thread and reports through
//! the session's [`EventSink`], as the engine does.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;

use crate::analysis;
use crate::audio::{Cmd, Mode, Player, State};
use crate::columns::{self, Measures, SortKey};
use crate::db;
use crate::db::query::{self, Mark, Playlist};
use crate::db::{Pruned, Track};
use crate::event::{Event, EventSink, JobId};
use crate::gain::ReplayGain;
use crate::notice::{Notice, Outcome, Refusal, Task};
use crate::samples::{self, Cut, Job, Plan};
use crate::scan;
use crate::wave::Peaks;

/// Marks closer than this to one another are the same mark.
pub const MARK_NEAR: Duration = Duration::from_millis(500);
/// How far past a mark playback must be before seeking back returns to it
/// rather than the one before.
pub const MARK_BACK: Duration = Duration::from_secs(1);

pub struct Session {
    conn: Connection,
    player: Player,
    /// Every track in the library.
    tracks: Vec<Track>,
    playlists: Vec<Playlist>,
    /// Tracks collected to edit and save as a playlist. It does not change
    /// what plays unless it is played itself.
    selection: Vec<Track>,
    /// Marks in the track `marks_for`, earliest first.
    marks: Vec<Mark>,
    marks_for: Option<PathBuf>,
    /// Where exported slices are written.
    samples: PathBuf,
    events: EventSink,
    /// The number given to the next piece of background work.
    next_job: JobId,
    /// Stops the peaks read in progress, which a newer read replaces.
    reading: Option<Arc<AtomicBool>>,
    /// The library file a scan writes to.
    library: Option<PathBuf>,
    /// Set while a scan or a prune runs.
    scanning: Arc<AtomicBool>,
    /// Set while an analysis runs. Separate from `scanning`: an analysis
    /// writes only its own tables, so the two may run together.
    analysing: Arc<AtomicBool>,
    replaygain: ReplayGain,
    /// What every track list is sorted by. It orders the library and every
    /// search, so the list a frontend shows is the list it plays.
    sort: Vec<SortKey>,
    /// What `playr analyze` measured, for the columns that show it and the
    /// keys that sort by it. Read with the library.
    measures: HashMap<String, Measures>,
}

impl Session {
    /// A session over the library `conn`, playing through `player`. Events
    /// from the engine and from background work go to `events`.
    pub fn new(conn: Connection, player: Player, events: EventSink) -> Session {
        player.set_events(events.clone());
        let library = conn.path().filter(|p| !p.is_empty()).map(PathBuf::from);
        let mut session = Session {
            conn,
            player,
            tracks: Vec::new(),
            playlists: Vec::new(),
            selection: Vec::new(),
            marks: Vec::new(),
            marks_for: None,
            samples: PathBuf::new(),
            events,
            next_job: 1,
            reading: None,
            library,
            scanning: Arc::default(),
            analysing: Arc::default(),
            replaygain: ReplayGain::Off,
            sort: Vec::new(),
            measures: HashMap::new(),
        };
        session.reload();
        session
    }

    /// Sets the directory exported slices are written under.
    pub fn set_samples_dir(&mut self, dir: PathBuf) {
        self.samples = dir;
    }

    /// Sets the library file a scan writes to, for a session over an
    /// in-memory library. A session over a file scans into that file.
    pub fn set_library_path(&mut self, path: PathBuf) {
        self.library = Some(path);
    }

    /// Reads the library's tracks and playlists again, and its gains while
    /// ReplayGain is on.
    pub fn reload(&mut self) {
        self.tracks = query::all(&self.conn).unwrap_or_default();
        self.playlists = query::playlists(&self.conn).unwrap_or_default();
        self.measures = db::analysis::measures(&self.conn, &self.tracks).unwrap_or_default();
        let mut tracks = std::mem::take(&mut self.tracks);
        self.order(&mut tracks);
        self.tracks = tracks;
        self.send_gains();
    }

    /// Sorts `tracks` by the current keys. Every list a frontend shows goes
    /// through here, so ordering never depends on where the list came from.
    fn order(&self, tracks: &mut [Track]) {
        if self.sort.is_empty() {
            return;
        }
        let measures = |t: &Track| self.measures.get(&t.path).copied().unwrap_or_default();
        tracks.sort_by(|a, b| columns::compare((a, measures(a)), (b, measures(b)), &self.sort));
    }

    /// `tracks` in the order every list is shown in.
    pub fn sorted(&self, mut tracks: Vec<Track>) -> Vec<Track> {
        self.order(&mut tracks);
        tracks
    }

    /// What `playr analyze` measured, by path, for the columns that show it.
    pub fn measures(&self) -> &HashMap<String, Measures> {
        &self.measures
    }

    /// What `playr analyze` measured about `track`, for its columns.
    pub fn measures_of(&self, path: &str) -> Measures {
        self.measures.get(path).copied().unwrap_or_default()
    }

    /// Sorts every track list by `sort`, from now on.
    pub fn set_sort(&mut self, sort: Vec<SortKey>) {
        self.sort = sort;
        let mut tracks = std::mem::take(&mut self.tracks);
        self.order(&mut tracks);
        self.tracks = tracks;
    }

    pub fn sort(&self) -> &[SortKey] {
        &self.sort
    }

    /// Hands the player the library's gains; read only while ReplayGain is
    /// on, so a library that never uses it never loads them.
    fn send_gains(&self) {
        if self.replaygain != ReplayGain::Off {
            let gains = db::analysis::gains(&self.conn, &self.tracks).unwrap_or_default();
            self.player.send(Cmd::SetGains(Arc::new(gains)));
        }
    }

    /// The player, for reading its state. Changes go through the session.
    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn playlists(&self) -> &[Playlist] {
        &self.playlists
    }

    pub fn selection(&self) -> &[Track] {
        &self.selection
    }

    /// Tracks matching `input`, in the order the library is sorted in; none
    /// if the query fails.
    pub fn search(&self, input: &str) -> Vec<Track> {
        let mut hits = query::search(&self.conn, input).unwrap_or_default();
        self.order(&mut hits);
        hits
    }

    /// The tracks of playlist `id`, in order; none if it cannot be read.
    pub fn playlist_tracks(&self, id: i64) -> Vec<Track> {
        query::playlist_tracks(&self.conn, id).unwrap_or_default()
    }

    /// Whether the library is a file, so playlists and marks outlast the session.
    pub fn has_library_file(&self) -> bool {
        self.conn.path().is_some_and(|p| !p.is_empty())
    }

    // --- playback ---

    /// Plays `tracks` from `index`. The selection is not touched.
    pub fn play(&mut self, tracks: &[Track], index: usize) {
        let paths = tracks.iter().map(|t| PathBuf::from(&t.path)).collect();
        self.player.send(Cmd::Play(paths, index));
    }

    /// Plays `path` alone, from `at`, without touching the selection. For
    /// taking up where playr left off.
    pub fn resume(&mut self, path: PathBuf, at: Duration) {
        self.player.send(Cmd::Play(vec![path], 0));
        self.player.send(Cmd::Seek(at));
    }

    /// What playr was playing when it last closed, if the file is still there.
    ///
    /// A file that has gone is not offered: the question would be about a
    /// track that cannot play, and answering yes would do nothing.
    pub fn resumable(&self) -> Option<(PathBuf, Duration)> {
        let (path, at) = db::resume(&self.conn).ok().flatten()?;
        path.is_file().then_some((path, at))
    }

    /// Remembers `path` and `at` as where to take up next time.
    pub fn remember(&self, path: &Path, at: Duration) {
        let _ = db::set_resume(&self.conn, path, at);
    }

    /// Forgets where to take up, after a refused offer or a stop.
    pub fn forget_resume(&self) {
        let _ = db::clear_resume(&self.conn);
    }

    /// Sends a playback command: pause, next, seek, speed and so on.
    pub fn send(&self, cmd: Cmd) {
        self.player.send(cmd);
    }

    /// Changes the volume by `delta`, from 0 to 1.
    pub fn volume_by(&self, delta: f32) {
        // The player's own value, not a frame-old copy, so quick presses all count.
        let v = (self.player.volume() + delta).clamp(0.0, 1.0);
        self.player.send(Cmd::SetVolume(v));
    }

    /// Which ReplayGain applies, from the audible position on.
    pub fn set_replaygain(&mut self, replaygain: ReplayGain) -> Notice {
        let loading = self.replaygain == ReplayGain::Off;
        self.replaygain = replaygain;
        if loading {
            self.send_gains();
        }
        self.player.send(Cmd::SetReplayGain(replaygain));
        Outcome::ReplayGain(replaygain).into()
    }

    pub fn replaygain(&self) -> ReplayGain {
        self.replaygain
    }

    pub fn set_mode(&self, mode: Mode) -> Notice {
        self.player.send(Cmd::SetMode(mode));
        Outcome::Mode(mode).into()
    }

    /// Moves to the next playback mode, or the previous one.
    pub fn cycle_mode(&self, forward: bool) -> Notice {
        let current = self.player.mode();
        self.set_mode(if forward {
            current.next()
        } else {
            current.prev()
        })
    }

    /// Plays playlist `id` from its first track.
    pub fn play_playlist(&mut self, id: i64) -> Notice {
        let Some(name) = self.playlist_name(id) else {
            return Refusal::PlaylistEmpty.into();
        };
        let tracks = self.playlist_tracks(id);
        if tracks.is_empty() {
            return Refusal::PlaylistEmpty.into();
        }
        self.play(&tracks, 0);
        Outcome::PlayingPlaylist { name }.into()
    }

    /// Plays the one playlist named `name`, matched as `playr playlist` does.
    pub fn play_playlist_named(&mut self, name: &str) -> Notice {
        let name = name.trim();
        match query::find_playlist(&self.playlists, name) {
            Some(pl) => self.play_playlist(pl.id),
            None => Refusal::NoPlaylistNamed(name.to_string()).into(),
        }
    }

    /// The playing track's path and source rate, read from the player now.
    pub fn playing_track(&self) -> Result<(PathBuf, u32), Refusal> {
        let status = self.player.status();
        match (status.state, status.current(), status.source) {
            (State::Playing | State::Paused, Some(path), Some(source)) => {
                Ok((path.clone(), source.rate))
            }
            _ => Err(Refusal::NothingPlaying),
        }
    }

    // --- selection ---

    /// Replaces the selection, as when files are given on the command line.
    pub fn set_selection(&mut self, tracks: Vec<Track>) {
        self.selection = tracks;
    }

    /// Selects `track`, or unselects every copy of it if it is selected.
    pub fn toggle_selected(&mut self, track: Track) -> Outcome {
        if self.selection.iter().any(|t| t.path == track.path) {
            self.selection.retain(|t| t.path != track.path);
            Outcome::RemovedFromSelection
        } else {
            self.selection.push(track);
            Outcome::AddedToSelection
        }
    }

    /// Adds the tracks of playlist `id` that are not selected yet, or nothing
    /// if the playlist has no tracks. Repeats within the playlist stay, so a
    /// playlist that repeats a track on purpose keeps doing so.
    pub fn add_playlist_to_selection(&mut self, id: i64) -> Option<Outcome> {
        let added = self.playlist_tracks(id);
        if added.is_empty() {
            return None;
        }
        let selected: HashSet<&str> = self.selection.iter().map(|t| t.path.as_str()).collect();
        let new: Vec<Track> = added
            .into_iter()
            .filter(|t| !selected.contains(t.path.as_str()))
            .collect();
        if new.is_empty() {
            return Some(Outcome::AlreadyInSelection);
        }
        self.selection.extend(new);
        Some(Outcome::AddedToSelection)
    }

    /// Removes the track at `index`, or nothing if there is none.
    pub fn remove_from_selection(&mut self, index: usize) -> Option<Outcome> {
        (index < self.selection.len()).then(|| Outcome::RemovedTrack {
            title: self.selection.remove(index).display_title(),
        })
    }

    /// Moves the track at `index` by `by` places, returning where it is now,
    /// or nothing if either place is outside the selection.
    pub fn move_in_selection(&mut self, index: usize, by: i64) -> Option<usize> {
        let to = usize::try_from(index as i64 + by).ok()?;
        if index >= self.selection.len() || to >= self.selection.len() {
            return None;
        }
        // Moved, not swapped: the tracks between keep their order.
        let track = self.selection.remove(index);
        self.selection.insert(to, track);
        Some(to)
    }

    pub fn clear_selection(&mut self) -> Outcome {
        self.selection.clear();
        Outcome::SelectionCleared
    }

    // --- playlists ---

    /// Whether the selection can be saved as a playlist at all.
    pub fn check_save(&self) -> Result<(), Refusal> {
        if self.selection.is_empty() {
            Err(Refusal::SelectionEmpty)
        } else if !self.has_library_file() {
            // In memory, the playlist would be lost on exit.
            Err(Refusal::NoLibraryFile)
        } else {
            Ok(())
        }
    }

    /// Saves the selection as the playlist `name`. Replacing a playlist of that
    /// name is refused with [`Refusal::WouldReplace`] unless `replace` is set,
    /// so a frontend can ask first.
    pub fn save_selection(&mut self, name: &str, replace: bool) -> Notice {
        let name = name.trim();
        if name.is_empty() {
            return Refusal::NameEmpty.into();
        }
        if !replace && self.playlists.iter().any(|p| p.name == name) {
            return Refusal::WouldReplace(name.to_string()).into();
        }
        // A playlist holds only library tracks; files given on the command line
        // are selected without being in the library.
        let ids: Vec<i64> = self
            .selection
            .iter()
            .map(|t| t.id)
            .filter(|id| *id != 0)
            .collect();
        match query::save_playlist(&mut self.conn, name, &ids) {
            Ok(_) => {
                self.playlists = query::playlists(&self.conn).unwrap_or_default();
                Outcome::Saved {
                    name: name.to_string(),
                    tracks: ids.len(),
                    left_out: self.selection.len() - ids.len(),
                }
                .into()
            }
            Err(e) => Notice::Failed {
                task: Task::Save,
                error: e.to_string(),
            },
        }
    }

    /// Renames playlist `id` to `name`, unless the name is empty, unchanged
    /// or taken.
    pub fn rename_playlist(&mut self, id: i64, name: &str) -> Notice {
        let name = name.trim();
        let from = self.playlist_name(id).unwrap_or_default();
        if name.is_empty() {
            return Refusal::NameEmpty.into();
        }
        if name == from {
            return Refusal::NameUnchanged.into();
        }
        if self.playlists.iter().any(|p| p.name == name) {
            // Renaming onto it would have to merge or replace two playlists.
            return Refusal::NameTaken(name.to_string()).into();
        }
        if let Err(e) = query::rename_playlist(&self.conn, id, name) {
            return Notice::Failed {
                task: Task::Rename,
                error: e.to_string(),
            };
        }
        self.playlists = query::playlists(&self.conn).unwrap_or_default();
        Outcome::Renamed {
            from,
            to: name.to_string(),
        }
        .into()
    }

    /// Deletes playlist `id`. Returns nothing if the library refuses.
    pub fn delete_playlist(&mut self, id: i64) -> Option<Notice> {
        let name = self.playlist_name(id)?;
        query::delete_playlist(&self.conn, id).ok()?;
        self.playlists = query::playlists(&self.conn).unwrap_or_default();
        Some(Outcome::Deleted { name }.into())
    }

    fn playlist_name(&self, id: i64) -> Option<String> {
        self.playlists
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
    }

    // --- marks ---

    /// The marks in the track at `path`, earliest first, read from the
    /// library when `path` is not the track they were last read for.
    pub fn marks_for(&mut self, path: Option<&PathBuf>) -> &[Mark] {
        if self.marks_for.as_ref() != path {
            self.marks = path
                .and_then(|p| query::marks(&self.conn, &p.to_string_lossy()).ok())
                .unwrap_or_default();
            self.marks_for = path.cloned();
        }
        &self.marks
    }

    /// Marks `at` in the playing track, or its position now, unless a mark is
    /// already within [`MARK_NEAR`].
    pub fn add_mark(&mut self, at: Option<Duration>) -> Notice {
        self.add_mark_within(at, MARK_NEAR)
    }

    /// As [`Session::add_mark`], refusing a mark within `near` of another, or
    /// on the same frame.
    pub fn add_mark_within(&mut self, at: Option<Duration>, near: Duration) -> Notice {
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        let at = at.unwrap_or_else(|| self.player.position());
        let mark = Mark::at_time(at, rate);
        if let Some(other) = self
            .marks
            .iter()
            .find(|m| m.frame == mark.frame || m.time().abs_diff(at) < near)
        {
            return Refusal::AlreadyMarked { at: other.time() }.into();
        }
        if let Err(e) = query::add_mark(&self.conn, &path.to_string_lossy(), mark) {
            return Notice::Failed {
                task: Task::Mark,
                error: e.to_string(),
            };
        }
        self.marks.push(mark);
        self.marks.sort_by_key(|m| m.frame);
        Outcome::Marked {
            at,
            kept: self.has_library_file(),
        }
        .into()
    }

    /// The mark within `within` frames of `frame` in the playing track, nearest
    /// first. For a frontend acting on "the mark under the cursor".
    pub fn mark_near(&mut self, frame: u64, within: u64) -> Option<Mark> {
        let path = self.playing_track().ok()?.0;
        self.marks_for(Some(&path));
        self.marks
            .iter()
            .filter(|m| m.frame.abs_diff(frame) <= within)
            .min_by_key(|m| m.frame.abs_diff(frame))
            .copied()
    }

    /// Moves the mark at `from` to `to` in the playing track.
    pub fn move_mark(&mut self, from: u64, to: u64) -> Notice {
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        let time = |f: u64| Duration::from_secs_f64(f as f64 / rate.max(1) as f64);
        if !self.marks.iter().any(|m| m.frame == from) {
            return Refusal::NoMarkHere.into();
        }
        if let Some(other) = self.marks.iter().find(|m| m.frame == to && m.frame != from) {
            return Refusal::MarkInTheWay { at: other.time() }.into();
        }
        match query::move_mark(&self.conn, &path.to_string_lossy(), from, to) {
            Ok(true) => {
                self.marks_for(None);
                self.marks_for(Some(&path));
                Outcome::MarkMoved {
                    from: time(from),
                    to: time(to),
                }
                .into()
            }
            Ok(false) => Refusal::NoMarkHere.into(),
            Err(e) => Notice::Failed {
                task: Task::MoveMark,
                error: e.to_string(),
            },
        }
    }

    /// Removes the mark at `frame` in the playing track, wherever it sits in
    /// the order marks were made.
    pub fn remove_mark(&mut self, frame: u64) -> Notice {
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        match query::remove_mark(&self.conn, &path.to_string_lossy(), frame) {
            Ok(true) => {
                self.marks.retain(|m| m.frame != frame);
                Outcome::MarkRemoved {
                    at: Duration::from_secs_f64(frame as f64 / rate.max(1) as f64),
                }
                .into()
            }
            Ok(false) => Refusal::NoMarkHere.into(),
            Err(e) => Notice::Failed {
                task: Task::RemoveMark,
                error: e.to_string(),
            },
        }
    }

    /// Looks for the onset nearest the mark at `from`, on a job, since it
    /// decodes. Finishes with [`Event::Snapped`].
    pub fn snap_mark(&mut self, from: u64, sensitivity: f32) -> Result<JobId, Refusal> {
        let (track, rate) = self.playing_track()?;
        Ok(self.spawn(move |job| {
            let result = crate::samples::nearest_onset(&track, rate, from, sensitivity);
            Some(Event::Snapped {
                job,
                track,
                from,
                result,
            })
        }))
    }

    /// Removes the mark added most recently to the playing track: marks are a
    /// chain, undone in the order they were made.
    pub fn undo_mark(&mut self) -> Notice {
        let path = match self.playing_track() {
            Ok((path, _)) => path,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        match query::remove_last_mark(&self.conn, &path.to_string_lossy()) {
            Ok(Some(mark)) => {
                self.marks.retain(|m| m.frame != mark.frame);
                Outcome::MarkRemoved { at: mark.time() }.into()
            }
            Ok(None) => Refusal::NoMarks.into(),
            Err(e) => Notice::Failed {
                task: Task::RemoveMark,
                error: e.to_string(),
            },
        }
    }

    /// The playing track and how many marks clearing it would remove, for a
    /// frontend to confirm before [`Session::clear_marks`].
    pub fn marks_to_clear(&mut self) -> Result<(PathBuf, usize), Refusal> {
        let (path, _) = self.playing_track()?;
        match self.marks_for(Some(&path)).len() {
            0 => Err(Refusal::NoMarks),
            n => Ok((path, n)),
        }
    }

    /// Clears the marks of the track at `path`, or does nothing if it has none.
    ///
    /// The path is the one the frontend asked about, since the playing track
    /// can change before the answer.
    pub fn clear_marks(&mut self, path: &Path) -> Option<Notice> {
        match query::clear_marks(&self.conn, &path.to_string_lossy()) {
            Ok(0) => None,
            Ok(_) => {
                if self.marks_for.as_deref() == Some(path) {
                    self.marks.clear();
                }
                Some(Outcome::MarksCleared.into())
            }
            Err(e) => Some(Notice::Failed {
                task: Task::ClearMarks,
                error: e.to_string(),
            }),
        }
    }

    /// Seeks to the next mark, or back to the previous one.
    ///
    /// Back skips a mark less than [`MARK_BACK`] behind, as going to the
    /// previous track restarts one rather than leaving it, so stepping back
    /// twice passes two marks.
    pub fn seek_to_mark(&mut self, forward: bool) -> Notice {
        let path = match self.playing_track() {
            Ok((path, _)) => path,
            Err(refusal) => return refusal.into(),
        };
        let at = self.player.position();
        let mut times = self.marks_for(Some(&path)).iter().map(Mark::time);
        let target = if forward {
            times.find(|t| *t > at + MARK_NEAR / 2)
        } else {
            times.rev().find(|t| *t + MARK_BACK < at)
        };
        match target {
            Some(t) => {
                self.player.send(Cmd::Seek(t));
                Outcome::AtMark { at: t }.into()
            }
            None if forward => Refusal::NoLaterMark.into(),
            None => Refusal::NoEarlierMark.into(),
        }
    }

    // --- background work ---

    /// Runs `work` on its own thread, which sends its result as an event.
    fn spawn(&mut self, work: impl FnOnce(JobId) -> Option<Event> + Send + 'static) -> JobId {
        let job = self.next_job;
        self.next_job += 1;
        let events = self.events.clone();
        std::thread::spawn(move || {
            if let Some(event) = work(job) {
                events(event);
            }
        });
        job
    }

    /// Reads the peaks of `track`, stopping any read still running, which
    /// then sends nothing. Finishes with [`Event::Peaks`].
    pub fn read_peaks(&mut self, track: PathBuf) -> JobId {
        self.cancel_peaks();
        let cancel = Arc::new(AtomicBool::new(false));
        self.reading = Some(cancel.clone());
        self.spawn(move |job| {
            let result = match Peaks::read(&track, &cancel) {
                Ok(Some(peaks)) => Ok(Arc::new(peaks)),
                Ok(None) => return None,
                Err(e) => Err(e),
            };
            Some(Event::Peaks { job, track, result })
        })
    }

    /// Decodes frames `start..end` of `track`, which counts frames at `rate`,
    /// for a view too close for its peaks. Finishes with [`Event::Detail`].
    pub fn read_detail(&mut self, track: PathBuf, rate: u32, start: u64, end: u64) -> JobId {
        self.spawn(move |job| {
            let result = crate::wave::Detail::read(&track, rate, start, end).map(Arc::new);
            Some(Event::Detail { job, track, result })
        })
    }

    /// Stops the peaks read in progress, if there is one.
    pub fn cancel_peaks(&mut self) {
        if let Some(cancel) = self.reading.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Plans slices of the playing track, `cut`'s way. Finishes with
    /// [`Event::Planned`]; nothing is written.
    pub fn plan_slices(&mut self, cut: Cut, range: Option<(u64, u64)>) -> Result<JobId, Refusal> {
        let job = self.slice_job(cut, range)?;
        Ok(self.spawn(move |id| {
            let result = samples::plan(&job).map(|spans| Plan {
                job: job.clone(),
                spans,
            });
            Some(Event::Planned {
                job: id,
                track: job.path,
                result,
            })
        }))
    }

    /// Writes `plan`'s slices. Finishes with [`Event::Exported`].
    pub fn write_slices(&mut self, plan: Plan) -> JobId {
        self.spawn(move |job| {
            Some(Event::Exported {
                job,
                result: samples::write(&plan.job, &plan.spans),
            })
        })
    }

    /// Plans and writes slices of the playing track at once, `cut`'s way.
    /// Finishes with [`Event::Exported`].
    pub fn export(&mut self, cut: Cut, range: Option<(u64, u64)>) -> Result<JobId, Refusal> {
        let work = self.slice_job(cut, range)?;
        Ok(self.spawn(move |job| {
            Some(Event::Exported {
                job,
                result: samples::export(&work),
            })
        }))
    }

    /// Scans `dir` into the library file, creating it if there is none.
    /// Sends [`Event::ScanProgress`] as it goes and finishes with
    /// [`Event::Scanned`], after which the frontend calls [`Session::scanned`].
    /// The directory is recorded so a later [`Session::rescan`] covers it.
    pub fn scan(&mut self, dir: PathBuf) -> Result<JobId, Refusal> {
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryPath);
        };
        if !dir.is_dir() {
            return Err(Refusal::NotADirectory(dir));
        }
        if self.scanning.swap(true, Ordering::Relaxed) {
            return Err(Refusal::ScanRunning);
        }
        let (scanning, events) = (self.scanning.clone(), self.events.clone());
        Ok(self.spawn(move |job| {
            let result = caught("scan", || {
                scan::scan_into(&library, &dir, |stats| {
                    if stats.seen % crate::event::SCAN_PROGRESS_EVERY == 0 {
                        events(Event::ScanProgress {
                            job,
                            seen: stats.seen,
                            added: stats.added,
                        });
                    }
                })
            });
            scanning.store(false, Ordering::Relaxed);
            Some(Event::Scanned {
                job,
                dir: Some(dir),
                result,
            })
        }))
    }

    /// Re-scans every directory previously given to [`Session::scan`] or
    /// `playr scan`. Sends the same progress events as a single scan.
    pub fn rescan(&mut self) -> Result<JobId, Refusal> {
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryPath);
        };
        if !self.has_library_file() {
            return Err(Refusal::NoLibraryFile);
        }
        let roots = db::roots(&self.conn).unwrap_or_default();
        if roots.is_empty() {
            return Err(Refusal::NoRoots);
        }
        if self.scanning.swap(true, Ordering::Relaxed) {
            return Err(Refusal::ScanRunning);
        }
        let (scanning, events) = (self.scanning.clone(), self.events.clone());
        Ok(self.spawn(move |job| {
            let result = caught("scan", || {
                scan::scan_roots(&library, &roots, |stats| {
                    if stats.seen > 0 && stats.seen % crate::event::SCAN_PROGRESS_EVERY == 0 {
                        events(Event::ScanProgress {
                            job,
                            seen: stats.seen,
                            added: stats.added,
                        });
                    }
                })
            });
            scanning.store(false, Ordering::Relaxed);
            Some(Event::Scanned {
                job,
                dir: None,
                result,
            })
        }))
    }

    /// Analyses the library tracks under `dir`, or every track for `None`,
    /// whose stored analysis is missing or out of date. Sends
    /// [`Event::AnalyzeProgress`] per file and finishes with
    /// [`Event::Analysed`], after which the frontend calls
    /// [`Session::analysed`].
    ///
    /// It runs alongside playback and alongside a scan: it writes only the
    /// `analysis` and `album_loudness` tables, which nothing here caches.
    pub fn analyze(&mut self, dir: Option<PathBuf>) -> Result<JobId, Refusal> {
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryPath);
        };
        if !self.has_library_file() {
            return Err(Refusal::NoLibraryFile);
        }
        if self.analysing.swap(true, Ordering::Relaxed) {
            return Err(Refusal::AnalysisRunning);
        }
        let paths: Vec<PathBuf> = dir.into_iter().collect();
        let (analysing, events) = (self.analysing.clone(), self.events.clone());
        Ok(self.spawn(move |job| {
            let result = caught("analysis", || {
                analysis::run_into(
                    &library,
                    &paths,
                    false,
                    analysis::default_workers(),
                    |done, total| events(Event::AnalyzeProgress { job, done, total }),
                )
            });
            analysing.store(false, Ordering::Relaxed);
            Some(Event::Analysed { job, result })
        }))
    }

    /// Takes in a finished analysis: reads the library's gains again, so
    /// tracks analysed just now play at their measured level.
    pub fn analysed(&mut self) {
        self.send_gains();
    }

    /// What `playr analyze` measured about `path`, when the row still
    /// describes the file as the library knows it.
    pub fn analysis_of(&self, path: &Path) -> Option<analysis::Analysis> {
        let key = path.to_str()?;
        let (stat, a) = db::analysis::row_of(&self.conn, key).ok().flatten()?;
        let track = self.tracks.iter().find(|t| t.path == key)?;
        analysis::is_current(track, Some(&stat)).then_some(a)
    }

    /// The gains `path` would play at, whether or not ReplayGain is on.
    ///
    /// It reads the whole library's gains, as turning ReplayGain on does, so
    /// it belongs to a dialog rather than to a frame.
    pub fn gains_of(&self, path: &Path) -> crate::gain::Gains {
        db::analysis::gains(&self.conn, &self.tracks)
            .ok()
            .and_then(|mut gains| gains.remove(path))
            .unwrap_or_default()
    }

    /// The tempo to show for `path`, from `playr analyze`.
    pub fn bpm(&self, path: &Path) -> Option<f32> {
        db::analysis::bpm_of(&self.conn, path.to_str()?)
            .ok()
            .flatten()
    }

    /// Takes in a finished scan: reads the library again, from its file if
    /// the session was running without one. Marks added to a library that was
    /// only in memory are not in the file, and are gone.
    pub fn scanned(&mut self) {
        if !self.has_library_file() {
            if let Some(conn) = self.library.as_deref().and_then(|p| db::open(p).ok()) {
                self.conn = conn;
                self.marks_for = None;
                self.marks.clear();
            }
        }
        self.reload();
    }

    /// Whether `dir` can be pruned, or every recorded root when `None`, for a
    /// frontend to refuse before it asks.
    pub fn check_prune(&self, dir: Option<&Path>) -> Result<(), Refusal> {
        if !self.has_library_file() {
            Err(Refusal::NoLibraryFile)
        } else if self.scanning.load(Ordering::Relaxed) {
            Err(Refusal::ScanRunning)
        } else {
            match dir {
                Some(dir) if !dir.is_dir() => Err(Refusal::NotADirectory(dir.to_path_buf())),
                None if db::roots(&self.conn).ok().is_none_or(|r| r.is_empty()) => {
                    Err(Refusal::NoRoots)
                }
                _ => Ok(()),
            }
        }
    }

    /// Removes the tracks under `dir` whose files are gone, and the marks of
    /// every file under it that is gone; with `None`, every recorded root.
    /// Finishes with [`Event::Pruned`], after which the frontend calls
    /// [`Session::pruned`]. Shares a scan's turn: one of either runs at a time.
    pub fn prune(&mut self, dir: Option<PathBuf>) -> Result<JobId, Refusal> {
        self.check_prune(dir.as_deref())?;
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryFile);
        };
        let dirs = match &dir {
            Some(d) => vec![d.clone()],
            None => db::roots(&self.conn).unwrap_or_default(),
        };
        if self.scanning.swap(true, Ordering::Relaxed) {
            return Err(Refusal::ScanRunning);
        }
        let scanning = self.scanning.clone();
        Ok(self.spawn(move |job| {
            let result = caught("prune", || {
                let fail = |e: rusqlite::Error| format!("{}: {e}", library.display());
                let conn = db::open(&library).map_err(fail)?;
                let mut removed = crate::db::Pruned::default();
                for d in &dirs {
                    let batch = db::prune_missing(&conn, d).map_err(fail)?;
                    removed.tracks += batch.tracks;
                    removed.marks += batch.marks;
                }
                Ok(removed)
            });
            scanning.store(false, Ordering::Relaxed);
            Some(Event::Pruned { job, dir, result })
        }))
    }

    /// Directories previously scanned into this library.
    pub fn roots(&self) -> Vec<PathBuf> {
        db::roots(&self.conn).unwrap_or_default()
    }

    /// Whether `dir` can be forgotten, for a frontend to refuse before it asks.
    pub fn check_forget(&self, dir: &Path) -> Result<(), Refusal> {
        if !self.has_library_file() {
            Err(Refusal::NoLibraryFile)
        } else if self.scanning.load(Ordering::Relaxed) {
            Err(Refusal::ScanRunning)
        } else if db::stored_root(&self.conn, dir).ok().flatten().is_none() {
            Err(Refusal::NotARoot(dir.to_path_buf()))
        } else {
            Ok(())
        }
    }

    /// Forgets `dir` as a root and removes the tracks and marks under it,
    /// then reads the library again.
    ///
    /// Done here rather than on a job: it asks the filesystem nothing, so it
    /// is one transaction rather than a walk of every file. It still waits for
    /// a scan, which writes the same rows.
    pub fn forget_root(&mut self, dir: &Path) -> Result<Pruned, Refusal> {
        self.check_forget(dir)?;
        let removed = db::forget_root(&self.conn, dir)
            .ok()
            .flatten()
            .ok_or_else(|| Refusal::NotARoot(dir.to_path_buf()))?;
        self.pruned();
        Ok(removed)
    }

    /// Takes in a finished prune: reads the library and its marks again.
    pub fn pruned(&mut self) {
        self.marks_for = None;
        self.marks.clear();
        self.reload();
    }

    /// Gathers `paths`, files and directories, into tracks to play, with tags
    /// from the library where it has them. Finishes with [`Event::Opened`].
    pub fn open(&mut self, paths: Vec<PathBuf>) -> JobId {
        let library = self.library.clone().filter(|_| self.has_library_file());
        self.spawn(move |job| {
            let conn = library.and_then(|p| db::open(&p).ok());
            let known = |key: &str| {
                conn.as_ref()
                    .and_then(|c| query::by_path(c, key).ok().flatten())
            };
            Some(Event::Opened {
                job,
                playable: scan::playable(&paths, known),
            })
        })
    }

    // --- slices ---

    /// The job for cutting the playing track `cut`'s way, at its marks and
    /// position now, or within `range`.
    pub fn slice_job(&mut self, cut: Cut, range: Option<(u64, u64)>) -> Result<Job, Refusal> {
        let (path, rate) = self.playing_track()?;
        let marks = self
            .marks_for(Some(&path))
            .iter()
            .map(|m| m.frame)
            .collect();
        Ok(Job {
            path,
            rate,
            marks,
            at: Mark::at_time(self.player.position(), rate).frame,
            cut,
            range,
            samples: self.samples.clone(),
        })
    }
}

/// `work`'s result, or its panic as an error. A panic in a tag reader or a
/// decoder would otherwise end a job's thread with no event sent.
fn caught<T>(what: &str, work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|panic| {
        let text = panic
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("no message");
        Err(format!("the {what} stopped: {text}"))
    })
}
