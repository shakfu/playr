//! A session: one library and one player, and what a frontend can do with them.
//!
//! Operations take what they act on as arguments, a track, an index into the
//! selection or a playlist id, never a cursor or a view, and report what they
//! did as a [`Notice`]. A frontend turns its own cursor or click into those
//! arguments and words the notice. `docs/dev/architecture.md` sets out the
//! design. Work that takes seconds runs on its own thread and reports through
//! the session's [`EventSink`], as the engine does.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;

use crate::audio::{Cmd, Mode, Player, State};
use crate::db::query::{self, Mark, Playlist};
use crate::db::Track;
use crate::event::{Event, EventSink, JobId};
use crate::notice::{Notice, Outcome, Refusal, Task};
use crate::samples::{self, Cut, Job, Plan};
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
}

impl Session {
    /// A session over the library `conn`, playing through `player`. Events
    /// from the engine and from background work go to `events`.
    pub fn new(conn: Connection, player: Player, events: EventSink) -> Session {
        player.set_events(events.clone());
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
        };
        session.reload();
        session
    }

    /// Sets the directory exported slices are written under.
    pub fn set_samples_dir(&mut self, dir: PathBuf) {
        self.samples = dir;
    }

    /// Reads the library's tracks and playlists again.
    pub fn reload(&mut self) {
        self.tracks = query::all(&self.conn).unwrap_or_default();
        self.playlists = query::playlists(&self.conn).unwrap_or_default();
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

    /// Tracks matching `input`, in library order; none if the query fails.
    pub fn search(&self, input: &str) -> Vec<Track> {
        query::search(&self.conn, input).unwrap_or_default()
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
        self.selection.swap(index, to);
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
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        let at = at.unwrap_or_else(|| self.player.position());
        if let Some(near) = self
            .marks
            .iter()
            .find(|m| m.time().abs_diff(at) < MARK_NEAR)
        {
            return Refusal::AlreadyMarked { at: near.time() }.into();
        }
        let mark = Mark::at_time(at, rate);
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

    /// How many marks clearing the playing track would remove, for a
    /// frontend to confirm before [`Session::clear_marks`].
    pub fn marks_to_clear(&mut self) -> Result<usize, Refusal> {
        let (path, _) = self.playing_track()?;
        match self.marks_for(Some(&path)).len() {
            0 => Err(Refusal::NoMarks),
            n => Ok(n),
        }
    }

    /// Clears the marks of the track they were last read for, or does nothing
    /// if none were read.
    pub fn clear_marks(&mut self) -> Option<Notice> {
        let path = self.marks_for.clone()?;
        Some(
            match query::clear_marks(&self.conn, &path.to_string_lossy()) {
                Ok(_) => {
                    self.marks.clear();
                    Outcome::MarksCleared.into()
                }
                Err(e) => Notice::Failed {
                    task: Task::ClearMarks,
                    error: e.to_string(),
                },
            },
        )
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

    /// Stops the peaks read in progress, if there is one.
    pub fn cancel_peaks(&mut self) {
        if let Some(cancel) = self.reading.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Plans slices of the playing track, `cut`'s way. Finishes with
    /// [`Event::Planned`]; nothing is written.
    pub fn plan_slices(&mut self, cut: Cut) -> Result<JobId, Refusal> {
        let job = self.slice_job(cut)?;
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
    pub fn export(&mut self, cut: Cut) -> Result<JobId, Refusal> {
        let work = self.slice_job(cut)?;
        Ok(self.spawn(move |job| {
            Some(Event::Exported {
                job,
                result: samples::export(&work),
            })
        }))
    }

    // --- slices ---

    /// The job for cutting the playing track `cut`'s way, at its marks and
    /// position now.
    pub fn slice_job(&mut self, cut: Cut) -> Result<Job, Refusal> {
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
            samples: self.samples.clone(),
        })
    }
}
