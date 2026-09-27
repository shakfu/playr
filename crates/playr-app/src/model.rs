//! The interface's state, shared by every frontend.
//!
//! [`Model`] holds what an interface shows and edits, apart from how it is
//! drawn: the session, the view and each view's cursor, search results, the
//! list playing, the prompt or question open, the message showing, the
//! sampler's state, and a snapshot of the player taken once a frame. It
//! implements [`Frontend`], so [`dispatch`] does every action on it the same
//! way in every frontend. A frontend keeps only its drawing state, such as
//! scroll offsets, and turns its own input into calls here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use playr_core::audio::{Cmd, Player, State, Status};
use playr_core::columns::{Column, Measures};
use playr_core::db::query::{Mark, Playlist};
use playr_core::db::Track;
use playr_core::event::{Event, EventSink, JobId};
use playr_core::notice::{Notice, Outcome, Refusal, Task};
use playr_core::samples::{Cut, Plan};
use playr_core::session::Session;
use rusqlite::Connection;

use crate::action::{Action, Keymap, Zoom};
use crate::command::{self, CommandLine, History};
use crate::config::Config;
use crate::dispatch::{self, Confirm, Frontend, Presentation, Prompt};
use crate::media::Media;
use crate::message::{self, Message};
use crate::sampler::{DetailRead, Sampler, Wave, DETAIL_MARGIN};
use crate::View;

/// How long a message stays showing.
pub const MESSAGE_FOR: Duration = Duration::from_secs(4);

/// How long the meter keeps showing a peak after it passes.
pub const PEAK_HOLD: Duration = Duration::from_millis(1500);

/// What typed input is being collected, or what is open over the view.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Input {
    #[default]
    None,
    Search(String),
    SavePlaylist(String),
    /// A new name for the playlist `from`, being typed.
    RenamePlaylist {
        from: Playlist,
        name: String,
    },
    /// Waiting for an answer before an action that cannot be undone.
    Confirm(Confirm),
    /// The key list is open.
    Help,
    /// The command list is open.
    CommandHelp,
    /// The list of the library's directories is open, read as it was opened.
    Roots(Vec<PathBuf>),
    /// What analysis measured about one track is open.
    Info(TrackInfo),
    /// A `:` command being typed.
    Command(CommandLine),
}

/// The row under each list's cursor, if the list has rows and one is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursors {
    pub library: Option<usize>,
    pub selection: Option<usize>,
    pub playlists: Option<usize>,
}

/// What the interface shows about playback, sampled once per frame.
///
/// What `:info` shows: a track named, and label and value rows under it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackInfo {
    pub title: String,
    pub rows: Vec<(String, String)>,
}

/// Taken once and shared by everything drawn: reading the player separately
/// for each part can mix three different instants into a single frame, showing
/// a track title from before a change next to a position from after it.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub status: Status,
    pub position: Duration,
    pub volume: f32,
    /// Momentary loudness in LUFS; `None` for silence.
    pub loudness: Option<f32>,
    /// The highest recent sample peak in dBFS, held for [`PEAK_HOLD`].
    pub peak: Option<f32>,
    /// Marks in the playing track, as times into it, earliest first.
    pub marks: Vec<Duration>,
    /// Loops saved in the playing track, by slot from 1, in source frames.
    pub loops: playr_core::session::Loops,
}

/// The interface's state. See the module documentation.
pub struct Model {
    /// The library and player, and everything done with them.
    session: Session,
    view: View,
    /// Search results; when set, the library view shows these instead.
    results: Option<Vec<Track>>,
    cursors: Cursors,

    /// Rows for the list the player is playing from.
    playing: Vec<Track>,
    /// The player list `playing` was built from, compared by identity.
    playing_source: Arc<[PathBuf]>,

    input: Input,
    /// `:` command lines entered this session.
    history: History,
    keys: Keymap,
    /// For `:slice onsets` without a sensitivity.
    onset_sensitivity: f32,
    /// After a scan, remove missing tracks without asking.
    auto_prune: bool,
    analyze_on_scan: bool,
    /// The columns a track list shows, from the settings or `:columns`.
    columns: Vec<Column>,
    /// Events from the session's engine and background work, drained each frame.
    events: Receiver<Event>,
    /// Called from a sending thread when something arrives, so a frontend that
    /// sleeps between frames draws it. Kept so media controls can share it.
    wake: Arc<dyn Fn() + Send + Sync>,
    /// The system's media keys and now-playing panel; nothing until a frontend
    /// calls [`Model::attach_media`], so tests stay off the bus.
    media: Media,
    sampler: Sampler,
    theme: crate::Theme,
    transport_text_buttons: bool,
    /// The message showing, its words, and when it was shown.
    message: Option<(Message, String, Instant)>,
    quit: bool,

    /// The peak shown, as a sample magnitude, and when it was reached.
    peak_hold: Option<(f32, Instant)>,
    snapshot: Snapshot,
    /// The playing track's tempo, and which track it was read for. Read on
    /// each track change rather than every frame.
    bpm: Option<f32>,
    bpm_for: Option<PathBuf>,
}

impl Model {
    /// A model over the library `conn` and `player`, with the keys, volume,
    /// mode and speed of `config`. They apply before `tracks` start, so a
    /// shuffle covers them. `tracks`, as a command line hands them over, are
    /// selected and played, and the selection view shows them.
    pub fn new(conn: Connection, player: Player, tracks: Vec<Track>, config: Config) -> Model {
        Model::waking(conn, player, tracks, config, || {})
    }

    /// As [`Model::new`], calling `wake` from the sending thread each time an
    /// event arrives, so a frontend that sleeps between frames can draw one.
    pub fn waking(
        conn: Connection,
        player: Player,
        tracks: Vec<Track>,
        config: Config,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Model {
        let (send, events) = mpsc::channel();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let sending = wake.clone();
        let sink: EventSink = Arc::new(move |event| {
            let _ = send.send(event);
            sending();
        });
        let mut session = Session::new(conn, player, sink);
        session.set_samples_dir(config.settings.samples);
        session.set_slice_edges(config.settings.slice_edges);
        session.set_fades(config.settings.slice_fades);
        let mut model = Model {
            session,
            view: View::Library,
            results: None,
            cursors: Cursors::default(),
            playing: Vec::new(),
            playing_source: Arc::default(),
            input: Input::None,
            history: History::default(),
            keys: config.keys,
            onset_sensitivity: config.settings.onset_sensitivity,
            auto_prune: config.settings.auto_prune,
            analyze_on_scan: config.settings.analyze_on_scan,
            columns: config.settings.columns.clone(),
            events,
            wake,
            media: Media::none(),
            sampler: Sampler::default(),
            theme: config.theme,
            transport_text_buttons: config.transport_text_buttons,
            message: None,
            quit: false,
            peak_hold: None,
            bpm: None,
            bpm_for: None,
            snapshot: Snapshot::default(),
        };
        model.session.send(Cmd::SetVolume(config.settings.volume));
        model.session.send(Cmd::SetMode(config.settings.mode));
        model.session.send(Cmd::SetSpeed(config.settings.speed));
        // `set_sort` re-orders the library the session already read.
        model.session.set_sort(config.settings.sort.clone());
        // Silent, as the other settings are: nothing was asked for.
        let _ = model.session.set_replaygain(config.settings.replaygain);
        if !tracks.is_empty() {
            model.open_tracks(tracks);
        } else if let Some((path, at)) = model.session.resumable() {
            // Only when nothing was handed over: a command line that named
            // tracks has already said what to play.
            model.input = Input::Confirm(Confirm::Resume { path, at });
        }
        model.reload();
        model
    }

    /// Samples the player for the next frame, and takes in what the engine and
    /// background work have sent since the last one.
    pub fn refresh(&mut self) {
        self.follow_player();
        let player = self.session.player();
        self.peak_hold = hold_peak(self.peak_hold, player.take_peak(), Instant::now());
        self.snapshot = Snapshot {
            status: player.status(),
            position: player.position(),
            volume: player.volume(),
            loudness: player.loudness(),
            peak: self.peak_hold.map(|(p, _)| 20.0 * p.log10()),
            marks: Vec::new(),
            loops: Default::default(),
        };
        let current = self.snapshot.status.current().cloned();
        self.snapshot.loops = self.session.loops_for(current.as_ref());
        self.snapshot.marks = self
            .session
            .marks_for(current.as_ref())
            .iter()
            .map(Mark::time)
            .collect();
        if current != self.bpm_for {
            self.bpm = current.as_deref().and_then(|p| self.session.bpm(p));
            self.bpm_for = current.clone();
        }
        self.drain_events(current.as_ref());
        // After the events, so what the panel asks for acts on this frame.
        for action in self
            .media
            .drain(self.snapshot.status.state == State::Playing)
        {
            self.perform(action);
        }
        self.publish_media();
        self.follow_wave(current.as_ref());
    }

    /// Does `action`. Keys, `:` commands and controls all arrive here.
    pub fn perform(&mut self, action: Action) {
        self.follow_player();
        dispatch::dispatch(action, self);
        // An action that plays changes the player's list; show its rows at once.
        self.follow_player();
    }

    /// Answers the open question: the action it asked about is done on yes,
    /// and cancelled otherwise. Does nothing when no question is open.
    pub fn answer(&mut self, yes: bool) {
        let Input::Confirm(question) = std::mem::take(&mut self.input) else {
            return;
        };
        if yes {
            dispatch::confirmed(question, self);
        } else {
            self.notify(Message::Cancelled);
        }
    }

    /// Runs a `:` command line and closes the prompt. A line is recorded in the
    /// history even when it fails, so a typo can be recalled and fixed.
    pub fn run_command(&mut self, line: &str) {
        self.input = Input::None;
        if line.trim().is_empty() {
            return;
        }
        self.history.push(line);
        match command::parse(line, self.view) {
            Ok(action) => self.perform(action),
            Err(e) => self.notify(Message::Command(e)),
        }
    }

    /// Shows the tracks matching `query`, as typed so far, with the search
    /// prompt still open.
    pub fn search_as_typed(&mut self, query: String) {
        dispatch::search(self, &query);
        self.input = Input::Search(query);
    }

    /// Closes the search prompt, keeping its results, or with `keep` false,
    /// showing the whole library again.
    pub fn end_search(&mut self, keep: bool) {
        self.input = Input::None;
        if !keep {
            self.results = None;
        } else if self.listed().is_empty() {
            self.notify(Message::NoMatches);
        }
    }

    /// Closes the save prompt and saves the selection as `name`.
    pub fn save_as(&mut self, name: &str) {
        self.input = Input::None;
        dispatch::save_as(self, name);
    }

    /// Closes the rename prompt and renames `from` to `name`.
    pub fn rename_to(&mut self, from: &Playlist, name: &str) {
        self.input = Input::None;
        dispatch::rename(self, from, name);
    }

    /// Clears the message once it has shown for [`MESSAGE_FOR`].
    pub fn expire_message(&mut self) {
        if self
            .message
            .as_ref()
            .is_some_and(|(_, _, at)| at.elapsed() > MESSAGE_FOR)
        {
            self.message = None;
        }
    }

    /// Asks the interface to exit.
    pub fn quit(&mut self) {
        self.remember_position();
        self.quit = true;
    }

    /// Stores the playing track and position, to offer at the next start.
    ///
    /// Written when playr closes and again at each track change, rather than
    /// every frame: the position is worth a database write twice a session,
    /// not sixty times a second, and a track change leaves something useful
    /// behind if playr is killed rather than closed.
    fn remember_position(&mut self) {
        match self.snapshot.status.current() {
            Some(path) => self.session.remember(path, self.snapshot.position),
            None => self.session.forget_resume(),
        }
    }

    /// Registers playr with the system's media keys and now-playing panel.
    /// A frontend calls this once; without it playr has neither.
    pub fn attach_media(&mut self) {
        let wake = self.wake.clone();
        self.media = Media::new(move || wake());
    }

    /// Hands the panel the playing track and position, once a frame.
    fn publish_media(&mut self) {
        let status = &self.snapshot.status;
        let row = self.playing.get(status.index);
        // A file played from outside the list has no row, so its name stands in.
        let named = || {
            status
                .current()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
        };
        let track = match row {
            Some(t) => Some((
                t.display_title(),
                t.display_artist().to_string(),
                t.display_album().to_string(),
            )),
            None => named().map(|n| (n, String::new(), String::new())),
        };
        let playing = match status.state {
            State::Playing => Some(true),
            State::Paused => Some(false),
            State::Stopped => None,
        };
        let track = track
            .as_ref()
            .map(|(t, a, b)| (t.as_str(), a.as_str(), b.as_str(), status.duration));
        let position = self.snapshot.position;
        self.media.publish(track, playing, position);
    }

    /// Whether an action has asked the interface to exit.
    pub fn quitting(&self) -> bool {
        self.quit
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Rows for the list the player is playing from.
    pub fn playing(&self) -> &[Track] {
        &self.playing
    }

    /// Search results, when the library view shows them.
    pub fn results(&self) -> Option<&[Track]> {
        self.results.as_deref()
    }

    pub fn cursors(&self) -> Cursors {
        self.cursors
    }

    pub fn input(&self) -> &Input {
        &self.input
    }

    /// Replaces the input being collected, as a frontend does while text is
    /// typed into a prompt.
    pub fn set_input(&mut self, input: Input) {
        self.input = input;
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn keymap(&self) -> &Keymap {
        &self.keys
    }

    pub fn sampler(&self) -> &Sampler {
        &self.sampler
    }

    /// The track `:info` describes: the row under the cursor in the library
    /// and selection views, and the playing track in the others, since
    /// neither lists tracks to point at.
    fn info_for_cursor(&self) -> Option<TrackInfo> {
        let track = match self.view {
            View::Library => self.cursor_row(self.listed(), self.cursors.library),
            View::Selection => self.cursor_row(self.session.selection(), self.cursors.selection),
            _ => None,
        };
        let track = track.or_else(|| {
            let path = self.snapshot.status.current()?;
            self.session
                .tracks()
                .iter()
                .find(|t| Path::new(&t.path) == path)
                .cloned()
                .or_else(|| {
                    Some(Track {
                        path: path.to_string_lossy().into_owned(),
                        ..Default::default()
                    })
                })
        })?;
        let path = PathBuf::from(&track.path);
        Some(TrackInfo {
            title: format!("{} - {}", track.display_title(), track.display_artist()),
            rows: message::info_rows(
                &track,
                self.session.analysis_of(&path).as_ref(),
                self.session.gains_of(&path),
            ),
        })
    }

    fn cursor_row(&self, rows: &[Track], at: Option<usize>) -> Option<Track> {
        rows.get(at?).cloned()
    }

    /// The playing track's tempo as it sounds, so varispeed moves it.
    /// `None` until `playr analyze` has measured the track.
    pub fn bpm(&self) -> Option<f32> {
        let speed = playr_core::audio::speed_for(self.snapshot.status.semitones) as f32;
        self.bpm.map(|bpm| bpm * speed)
    }

    /// The colours to draw in, from the settings or `:theme`.
    pub fn theme(&self) -> crate::Theme {
        self.theme
    }

    /// Whether the window's transport buttons show words rather than symbols.
    pub fn transport_text_buttons(&self) -> bool {
        self.transport_text_buttons
    }

    /// The columns a track list shows, in order.
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// What `playr analyze` measured about `track`, for its columns.
    pub fn measures(&self, track: &Track) -> Measures {
        self.session.measures_of(&track.path)
    }

    /// The same, by path, for a frontend drawing many rows at once.
    pub fn measures_map(&self) -> &std::collections::HashMap<String, Measures> {
        self.session.measures()
    }

    /// Which ReplayGain applies, from the settings or `:replaygain`.
    pub fn replaygain(&self) -> playr_core::gain::ReplayGain {
        self.session.replaygain()
    }

    /// Sets how the sampler draws the waveform, as a frontend does to choose
    /// the display it starts with. `:display` sets it too, with a message.
    pub fn set_display(&mut self, display: crate::Display) {
        self.sampler.display = display;
    }

    /// Sets the sampler's zoom, as a frontend does once it has clamped it to
    /// what the track and view allow.
    pub fn set_zoom(&mut self, zoom: u32) {
        self.sampler.zoom = zoom;
    }

    /// Records the columns the sampler view drew, which nudges count in.
    pub fn set_scale(&mut self, scale: crate::sampler::Scale) {
        self.sampler.scale = Some(scale);
    }

    /// The message showing, if any.
    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref().map(|(m, _, _)| m)
    }

    /// The words of the message showing, if any.
    pub fn message_text(&self) -> Option<&str> {
        self.message.as_ref().map(|(_, text, _)| text.as_str())
    }

    /// Rebuilds the playing list if the player's list is no longer the one it
    /// shows. Playing from here updates it directly; this catches any other
    /// change, and costs a pointer comparison when there is none.
    pub fn follow_player(&mut self) {
        let current = self.session.player().queue();
        if Arc::ptr_eq(&current, &self.playing_source) {
            return;
        }
        let known: HashMap<&str, &Track> = self
            .playing
            .iter()
            .chain(self.session.selection())
            .chain(self.session.tracks())
            .map(|t| (t.path.as_str(), t))
            .collect();
        self.playing = current
            .iter()
            .map(|p| {
                let path = p.to_string_lossy();
                known
                    .get(path.as_ref())
                    .map(|t| (*t).clone())
                    .unwrap_or(Track {
                        path: path.into_owned(),
                        ..Default::default()
                    })
            })
            .collect();
        self.playing_source = current;
    }

    fn reload(&mut self) {
        self.session.reload();
        self.choose_first_rows();
    }

    /// Puts the cursor on the first row of a list that has rows and no cursor.
    fn choose_first_rows(&mut self) {
        if !self.session.tracks().is_empty() && self.cursors.library.is_none() {
            self.cursors.library = Some(0);
        }
        if !self.session.playlists().is_empty() && self.cursors.playlists.is_none() {
            self.cursors.playlists = Some(0);
        }
    }

    /// Takes in what the engine and background work have sent since the last
    /// frame. Of several playback errors, the last is shown with a count of
    /// the others, so a run of bad files is not hidden behind one name.
    fn drain_events(&mut self, current: Option<&PathBuf>) {
        let mut error: Option<(String, u64)> = None;
        while let Ok(event) = self.events.try_recv() {
            match event {
                // The frame's snapshot already shows the track and state;
                // the new track is stored so a kill still leaves it behind.
                Event::TrackChanged { .. } => self.remember_position(),
                Event::StateChanged(_) => {}
                Event::PlaybackError(e) => {
                    let missed = error.map_or(0, |(_, n)| n + 1);
                    error = Some((e, missed));
                }
                Event::Peaks { job, track, result } => {
                    if !matches!(self.sampler.wave, Wave::Reading { job: reading, .. } if reading == job)
                    {
                        continue;
                    }
                    self.sampler.wave = match result {
                        Ok(peaks) => Wave::Ready { path: track, peaks },
                        Err(error) => Wave::Failed { path: track, error },
                    };
                }
                Event::Detail { job, track, result } => {
                    let DetailRead::Reading {
                        job: reading,
                        start,
                        end,
                        ..
                    } = self.sampler.detail
                    else {
                        continue;
                    };
                    if reading != job {
                        continue;
                    }
                    self.sampler.detail = match result {
                        Ok(detail) => DetailRead::Ready {
                            path: track,
                            detail,
                        },
                        Err(_) => DetailRead::Failed {
                            path: track,
                            start,
                            end,
                        },
                    };
                }
                Event::Planned { job, track, result } => {
                    if self.sampler.planning != Some(job) {
                        continue;
                    }
                    self.sampler.planning = None;
                    if Some(&track) != current {
                        continue;
                    }
                    match result {
                        // Edges or a sensitivity chosen while it was planning:
                        // plan it their way.
                        Ok(plan)
                            if !self.session.plans_current(&plan.job)
                                || self
                                    .sampler
                                    .onsets_wanted
                                    .is_some_and(|s| plan.job.cut != Cut::Onsets(s)) =>
                        {
                            let mut job = plan.job;
                            if let Some(s) = self.sampler.onsets_wanted.take() {
                                job.cut = Cut::Onsets(s);
                            }
                            let id = self.session.replan(job);
                            self.sampler.planning = Some(id);
                        }
                        Ok(plan) => {
                            self.sampler.onsets_wanted = None;
                            let slices = plan.spans.len();
                            self.sampler.pending = Some(plan);
                            self.notify(Outcome::Planned { slices });
                        }
                        Err(error) => self.notify(Notice::Failed {
                            task: Task::Slice,
                            error,
                        }),
                    }
                }
                Event::Snapped {
                    track,
                    from,
                    result,
                    ..
                } => {
                    // The track may have changed while the window decoded; a
                    // mark moved in one that is no longer playing would be a
                    // surprise, so it is dropped.
                    if current != Some(&track) {
                        continue;
                    }
                    match result {
                        Ok(Some(to)) => {
                            let notice = self.session.move_mark(from, to);
                            if matches!(notice, Notice::Done(Outcome::MarkMoved { .. })) {
                                self.sampler.cursor = Some(to);
                            }
                            self.notify(notice);
                        }
                        Ok(None) => self.notify(Notice::Refused(Refusal::NoOnsetNear)),
                        Err(error) => self.notify(Notice::Failed {
                            task: Task::MoveMark,
                            error,
                        }),
                    }
                }
                Event::ScanProgress { seen, added, .. } => {
                    self.notify(Outcome::Scanning { seen, added })
                }
                Event::AnalyzeProgress { done, total, .. } => {
                    self.notify(Outcome::Analysing { done, total })
                }
                Event::Analysed { result, .. } => {
                    self.session.analysed();
                    match result {
                        Ok(stats) => self.notify(Outcome::Analysed {
                            analysed: stats.analysed,
                            failed: stats.failed,
                        }),
                        Err(error) => self.notify(Notice::Failed {
                            task: Task::Analyze,
                            error,
                        }),
                    }
                }
                Event::Scanned { dir, result, .. } => {
                    self.session.scanned();
                    self.choose_first_rows();
                    match result {
                        Ok(report) => {
                            let (missing, unavailable) = (report.missing, report.unavailable);
                            let added = report.stats.added > 0;
                            self.notify(Outcome::Scanned {
                                dir: dir.clone(),
                                report,
                            });
                            // The scan has written the new rows, so an
                            // analysis now finds exactly them out of date.
                            if self.analyze_on_scan && added {
                                match self.session.analyze(dir.clone()) {
                                    Ok(_) => {
                                        self.notify(Outcome::AnalysisStarted { dir: dir.clone() })
                                    }
                                    Err(refusal) => self.notify(refusal),
                                }
                            }
                            if missing > 0 {
                                after_missing(self, dir, unavailable == 0);
                            }
                        }
                        Err(error) => self.notify(Notice::Failed {
                            task: Task::Scan,
                            error,
                        }),
                    }
                }
                Event::Pruned { dir, result, .. } => {
                    self.session.pruned();
                    match result {
                        Ok(removed) => self.notify(Outcome::Pruned { dir, removed }),
                        Err(error) => self.notify(Notice::Failed {
                            task: Task::Prune,
                            error,
                        }),
                    }
                }
                Event::Opened { playable, .. } => {
                    let skipped = playable.problems.len();
                    let tracks = playable.tracks.len();
                    if tracks == 0 {
                        self.notify(Notice::Failed {
                            task: Task::Open,
                            error: "nothing playable in those paths".into(),
                        });
                        continue;
                    }
                    self.open_tracks(playable.tracks);
                    self.notify(Outcome::Opened { tracks, skipped });
                }
                Event::Exported { result, .. } => match result {
                    Ok(out) => self.notify(Outcome::Exported {
                        slices: out.slices.len(),
                        dir: out.dir,
                    }),
                    Err(error) => self.notify(Notice::Failed {
                        task: Task::Export,
                        error,
                    }),
                },
            }
        }
        if let Some((error, missed)) = error {
            self.notify(Notice::PlaybackError { error, missed });
        }
    }

    fn notify(&mut self, message: impl Into<Message>) {
        let message = message.into();
        let text = message::text(&message);
        self.message = Some((message, text, Instant::now()));
    }

    /// Adds `tracks`, as opened or handed over at startup, to the end of the
    /// selection and plays them from the first, with the selection showing
    /// and its cursor on that track.
    fn open_tracks(&mut self, tracks: Vec<Track>) {
        let mut selection = self.session.selection().to_vec();
        self.cursors.selection = Some(selection.len());
        selection.extend(tracks.iter().cloned());
        self.session.set_selection(selection);
        self.play(tracks, 0);
        self.view = View::Selection;
    }

    /// Plays `tracks` from `index`. The selection is not touched.
    fn play(&mut self, tracks: Vec<Track>, index: usize) {
        self.session.play(&tracks, index);
        self.playing = tracks;
        self.playing_source = self.session.player().queue();
    }

    /// Keeps the sampler's waveform and planned slices on the playing track.
    /// The waveform is only read while the view is open, since reading
    /// decodes the whole track.
    fn follow_wave(&mut self, current: Option<&PathBuf>) {
        if self
            .sampler
            .pending
            .as_ref()
            .is_some_and(|p| Some(&p.job.path) != current)
        {
            self.sampler.pending = None;
        }
        if self
            .sampler
            .range
            .as_ref()
            .is_some_and(|r| Some(&r.path) != current)
        {
            self.sampler.range = None;
        }
        if self
            .sampler
            .detail
            .path()
            .is_some_and(|p| Some(p) != current)
        {
            self.sampler.detail = DetailRead::None;
        }
        if self.view == View::Sampler && self.sampler.wave.path() == current {
            self.follow_detail(current);
        }
        if self.view != View::Sampler || self.sampler.wave.path() == current {
            return;
        }
        self.sampler.wave = match current.cloned() {
            Some(path) => Wave::Reading {
                job: self.session.read_peaks(path.clone()),
                path,
            },
            None => {
                self.session.cancel_peaks();
                Wave::None
            }
        };
    }
}

impl Model {
    /// Reads the frames the drawn view shows, and [`DETAIL_MARGIN`] either
    /// side, when its columns are finer than the peaks and nothing read or
    /// reading covers them.
    fn follow_detail(&mut self, current: Option<&PathBuf>) {
        let (Some(path), Some(scale)) = (current, self.sampler.scale) else {
            return;
        };
        let Wave::Ready { peaks, .. } = &self.sampler.wave else {
            return;
        };
        if !scale.needs_detail() {
            return;
        }
        let (rate, frames) = (peaks.rate, peaks.frames);
        let (a, b) = scale.shown();
        let b = b.min(frames);
        if a >= b || self.sampler.detail.answers(path, a, b) {
            return;
        }
        let margin = crate::sampler::frame_of(DETAIL_MARGIN, rate);
        let (start, end) = (a.saturating_sub(margin), (b + margin).min(frames));
        let job = self.session.read_detail(path.clone(), rate, start, end);
        self.sampler.detail = DetailRead::Reading {
            path: path.clone(),
            job,
            start,
            end,
        };
    }
}

impl Frontend for Model {
    fn session(&self) -> &Session {
        &self.session
    }

    fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    fn keys(&mut self) -> &mut Keymap {
        &mut self.keys
    }

    fn view(&self) -> View {
        self.view
    }

    fn set_view(&mut self, view: View) {
        self.view = view;
    }

    fn cursor(&self, view: View) -> Option<usize> {
        match view {
            View::Library => self.cursors.library,
            View::Selection => self.cursors.selection,
            View::Playlists => self.cursors.playlists,
            View::Sampler => None,
        }
    }

    fn set_cursor(&mut self, view: View, row: Option<usize>) {
        match view {
            View::Library => self.cursors.library = row,
            View::Selection => self.cursors.selection = row,
            View::Playlists => self.cursors.playlists = row,
            View::Sampler => {}
        }
    }

    fn listed(&self) -> &[Track] {
        self.results.as_deref().unwrap_or(self.session.tracks())
    }

    fn set_results(&mut self, results: Option<Vec<Track>>) -> Option<Vec<Track>> {
        std::mem::replace(&mut self.results, results)
    }

    fn onset_sensitivity(&self) -> f32 {
        self.onset_sensitivity
    }

    fn notify(&mut self, message: Message) {
        Model::notify(self, message);
    }

    fn confirm(&mut self, question: Confirm) {
        self.input = Input::Confirm(question);
    }

    fn prompt(&mut self, prompt: Prompt) {
        self.input = match prompt {
            Prompt::Search => Input::Search(String::new()),
            Prompt::Command => Input::Command(CommandLine::default()),
            Prompt::Save => Input::SavePlaylist(String::new()),
            // Starts from the current name, which is usually a small edit away.
            Prompt::Rename(from) => {
                let name = from.name.clone();
                Input::RenamePlaylist { from, name }
            }
        };
    }

    fn present(&mut self, presentation: Presentation) {
        match presentation {
            Presentation::Quit => self.quit = true,
            Presentation::KeyList => self.input = Input::Help,
            Presentation::CommandList => self.input = Input::CommandHelp,
            Presentation::RootList => self.input = Input::Roots(self.session.roots()),
            Presentation::TrackInfo => {
                self.input = match self.info_for_cursor() {
                    Some(info) => Input::Info(info),
                    None => {
                        self.notify(Notice::Refused(Refusal::NothingPlaying));
                        Input::None
                    }
                }
            }
            Presentation::Zoom(zoom) => {
                self.sampler.zoom = match zoom {
                    Zoom::In => self.sampler.zoom + 1,
                    Zoom::Out => self.sampler.zoom.saturating_sub(1),
                    Zoom::All => 0,
                }
            }
            Presentation::Display(display) => {
                self.sampler.display = display.unwrap_or(self.sampler.display.next());
                Model::notify(self, Message::Display(self.sampler.display));
            }
            Presentation::Columns(columns) => {
                self.columns = columns.clone();
                Model::notify(self, Message::Columns(columns));
            }
            Presentation::Sorted(keys) => Model::notify(self, Message::Sorted(keys)),
            Presentation::Theme(theme) => {
                self.theme = theme;
                Model::notify(self, Message::Theme(theme));
            }
        }
    }

    fn sampler(&self) -> &Sampler {
        &self.sampler
    }

    fn sampler_mut(&mut self) -> &mut Sampler {
        &mut self.sampler
    }

    fn planning(&mut self, job: JobId) {
        self.sampler.planning = Some(job);
    }

    fn take_plan(&mut self) -> Option<Plan> {
        self.sampler.pending.take()
    }
}

/// After a scan found missing files: prune them, or ask first.
///
/// A scan finishes on a background thread, so the question is asked only when
/// nothing else is open. Asking over a `:` line or a search would take the
/// next keystroke as the answer. The count is on the bottom line either way,
/// and `:prune` asks again whenever the user is ready.
///
/// `read` is whether every directory the scan covered could be read. When one
/// could not, `auto_prune` steps aside and the question is asked instead: an
/// unmounted drive counts every track under it as missing, and nobody is
/// watching a machine that prunes on its own.
fn after_missing(model: &mut Model, dir: Option<PathBuf>, read: bool) {
    if model.session.check_prune(dir.as_deref()).is_err() {
        return;
    }
    if model.auto_prune && read {
        match model.session.prune(dir.clone()) {
            Ok(_) => model.notify(Outcome::PruneStarted { dir }),
            Err(refusal) => model.notify(refusal),
        }
    } else if model.input == Input::None {
        model.confirm(Confirm::Prune(dir));
    }
}

/// The peak to show, given the one `held` and a new `reading`, both as sample
/// magnitudes. A higher reading replaces the held peak at once; a lower one
/// only once the held peak is [`PEAK_HOLD`] old.
pub fn hold_peak(
    held: Option<(f32, Instant)>,
    reading: f32,
    now: Instant,
) -> Option<(f32, Instant)> {
    match held {
        Some((level, at)) if level >= reading && now.duration_since(at) < PEAK_HOLD => held,
        _ => (reading > 0.0).then_some((reading, now)),
    }
}

/// What is playing, for a now-playing line: the title and artist from the
/// playing list, or the file name when the list has no row for it.
pub fn now_playing(playing: &[Track], status: &Status) -> Option<String> {
    playing
        .get(status.index)
        .map(|t| format!("{} - {}", t.display_title(), t.display_artist()))
        .or_else(|| {
            status.current().map(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
        })
}
