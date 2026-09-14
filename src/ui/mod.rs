//! Terminal interface.
//!
//! One thread: it renders, reads keys, and talks to the player over a channel.
//! Nothing here blocks on audio.

pub mod action;
pub mod command;
pub mod config;
pub mod notice;
pub mod render;
pub mod sampler;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use ratatui::widgets::ListState;
use rusqlite::Connection;

use action::{Action, Keymap, Slicing};
use command::{CommandLine, History};
use config::Config;
use notice::Message;
use playr_core::audio::{Cmd, Player, State, Status};
use playr_core::db::query::{Mark, Playlist};
use playr_core::db::Track;
use playr_core::event::{Event, EventSink};
use playr_core::notice::{Notice, Outcome, Refusal, Task};
use playr_core::samples;
use playr_core::session::Session;
use sampler::{Sampler, Wave};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Library,
    Selection,
    Playlists,
    /// The playing track's waveform, for marking and slicing it.
    Sampler,
}

impl View {
    fn next(self) -> Self {
        match self {
            View::Library => View::Selection,
            View::Selection => View::Playlists,
            View::Playlists => View::Sampler,
            View::Sampler => View::Library,
        }
    }

    fn title(self) -> &'static str {
        match self {
            View::Library => "Library",
            View::Selection => "Selection",
            View::Playlists => "Playlists",
            View::Sampler => "Sampler",
        }
    }
}

/// What typed input is currently being collected.
pub enum Input {
    None,
    Search(String),
    SavePlaylist(String),
    /// A new name for the playlist `from`, being typed.
    RenamePlaylist {
        from: Playlist,
        name: String,
    },
    /// Waiting for `y` before an action that cannot be undone.
    Confirm(Confirm),
    /// The key list is open; the next key closes it.
    Help,
    /// The command list is open; the next key closes it.
    CommandHelp,
    /// A `:` command being typed.
    Command(CommandLine),
}

/// A destructive action held until the listener confirms it.
pub enum Confirm {
    DeletePlaylist(Playlist),
    /// Overwrite the playlist of this name with the selection.
    ReplacePlaylist(String),
    /// Empty the selection, which holds this many tracks.
    ClearSelection(usize),
    /// Remove this many marks from the playing track.
    ClearMarks(usize),
}

impl Confirm {
    pub fn prompt(&self) -> String {
        match self {
            Confirm::DeletePlaylist(p) => format!("delete playlist \"{}\"? (y/n)", p.name),
            Confirm::ReplacePlaylist(name) => {
                format!("replace playlist \"{name}\" with the selection? (y/n)")
            }
            Confirm::ClearSelection(n) => format!("clear all {n} tracks from the selection? (y/n)"),
            Confirm::ClearMarks(n) => format!("clear all {n} marks from this track? (y/n)"),
        }
    }
}

pub struct App {
    /// The library and player, and everything done with them.
    session: Session,
    view: View,

    /// Search results; when set, the library view shows these instead.
    results: Option<Vec<Track>>,
    library_state: ListState,

    /// Rows for the list the player is playing from.
    playing: Vec<Track>,
    /// The player list `playing` was built from, compared by identity.
    playing_source: Arc<[PathBuf]>,

    selection_state: ListState,
    playlist_state: ListState,

    input: Input,
    /// `:` command lines entered this session.
    history: History,
    keys: Keymap,
    /// For `:slice onsets` without a sensitivity.
    onset_sensitivity: f32,
    /// Events from the session's engine and background work, drained each frame.
    events: Receiver<Event>,
    sampler: Sampler,
    /// Rows the key or command list is scrolled by.
    help_scroll: usize,
    /// The message on the bottom line, its words, and when it was shown.
    message: Option<(Message, String, Instant)>,
    quit: bool,

    /// The peak shown, as a sample magnitude, and when it was reached.
    peak_hold: Option<(f32, Instant)>,

    /// One snapshot of the player per frame.
    ///
    /// Taken once and shared by every widget: reading the player separately in
    /// each one can mix three different instants into a single frame, showing a
    /// track title from before a change next to a position from after it.
    snapshot: Snapshot,
}

/// Everything the drawing code reads.
///
/// Rendering takes this rather than the whole `App` so it can be exercised
/// against a `TestBackend` without an audio device.
pub struct Screen<'a> {
    pub view: View,
    pub snapshot: &'a Snapshot,
    pub all: &'a [Track],
    pub results: Option<&'a [Track]>,
    /// Rows for the list the player is playing from.
    pub playing: &'a [Track],
    pub selection: &'a [Track],
    pub playlists: &'a [Playlist],
    pub input: &'a Input,
    pub keys: &'a Keymap,
    /// The sampler view's state; drawing clamps its zoom.
    pub sampler: &'a mut Sampler,
    /// Rows the key or command list is scrolled by; drawing clamps it.
    pub help_scroll: &'a mut usize,
    pub message: Option<&'a str>,
    pub library_state: &'a mut ListState,
    pub selection_state: &'a mut ListState,
    pub playlist_state: &'a mut ListState,
}

impl Screen<'_> {
    /// The track list the library pane is showing.
    pub fn visible(&self) -> &[Track] {
        self.results.unwrap_or(self.all)
    }
}

/// What the widgets need to know about playback, sampled once per frame.
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
}

impl App {
    pub fn new(conn: Connection, player: Player) -> Self {
        Self::with_selection(conn, player, Vec::new())
    }

    /// Builds the app with `tracks` selected and playing, as the CLI hands
    /// them over, so the files played are also listed.
    pub fn with_selection(conn: Connection, player: Player, tracks: Vec<Track>) -> Self {
        Self::configured(conn, player, tracks, Config::default())
    }

    /// As [`App::with_selection`], with the keys, volume, mode and speed of
    /// `config`. They apply before `tracks` start, so a shuffle covers them.
    pub fn configured(
        conn: Connection,
        player: Player,
        tracks: Vec<Track>,
        config: Config,
    ) -> Self {
        let (send, events) = mpsc::channel();
        let sink: EventSink = Arc::new(move |event| {
            let _ = send.send(event);
        });
        let mut session = Session::new(conn, player, sink);
        session.set_samples_dir(config.samples);
        let mut app = App {
            session,
            view: View::Library,
            results: None,
            library_state: ListState::default(),
            playing: Vec::new(),
            playing_source: Arc::default(),
            selection_state: ListState::default(),
            playlist_state: ListState::default(),
            input: Input::None,
            history: History::default(),
            keys: config.keys,
            onset_sensitivity: config.onset_sensitivity,
            events,
            sampler: Sampler::default(),
            help_scroll: 0,
            message: None,
            quit: false,
            peak_hold: None,
            snapshot: Snapshot::default(),
        };
        app.session.send(Cmd::SetVolume(config.volume));
        app.session.send(Cmd::SetMode(config.mode));
        app.session.send(Cmd::SetSpeed(config.speed));
        if !tracks.is_empty() {
            app.session.set_selection(tracks.clone());
            app.selection_state.select(Some(0));
            app.play(tracks, 0);
            app.view = View::Selection;
        }
        app.reload();
        app
    }

    /// Rebuilds `playing` if the player's list is no longer the one it shows.
    ///
    /// Playing from here updates `playing` directly; this catches any other change.
    fn follow_player(&mut self) {
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
        if !self.session.tracks().is_empty() && self.library_state.selected().is_none() {
            self.library_state.select(Some(0));
        }
        if !self.session.playlists().is_empty() && self.playlist_state.selected().is_none() {
            self.playlist_state.select(Some(0));
        }
    }

    /// The track list the library view is currently showing.
    fn visible(&self) -> &[Track] {
        self.results.as_deref().unwrap_or(self.session.tracks())
    }

    /// Borrows the state the renderer needs.
    pub fn screen(&mut self) -> Screen<'_> {
        Screen {
            view: self.view,
            snapshot: &self.snapshot,
            all: self.session.tracks(),
            results: self.results.as_deref(),
            playing: &self.playing,
            selection: self.session.selection(),
            playlists: self.session.playlists(),
            input: &self.input,
            keys: &self.keys,
            sampler: &mut self.sampler,
            help_scroll: &mut self.help_scroll,
            message: self.message.as_ref().map(|(_, text, _)| text.as_str()),
            library_state: &mut self.library_state,
            selection_state: &mut self.selection_state,
            playlist_state: &mut self.playlist_state,
        }
    }

    pub fn run(mut self, terminal: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
        while !self.quit {
            self.refresh();
            terminal.draw(|f| render::draw(&mut self.screen(), f))?;

            // A short poll keeps the progress bar moving without busy-waiting.
            if event::poll(Duration::from_millis(200))? {
                if let TermEvent::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.on_key(key);
                    }
                }
            }
            if let Some((_, _, at)) = &self.message {
                if at.elapsed() > Duration::from_secs(4) {
                    self.message = None;
                }
            }
        }
        Ok(())
    }

    /// Samples the player for the next frame, and shows any new error once.
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
        };
        let current = self.snapshot.status.current().cloned();
        self.snapshot.marks = self
            .session
            .marks_for(current.as_ref())
            .iter()
            .map(Mark::time)
            .collect();
        self.drain_events(current.as_ref());
        self.follow_wave(current.as_ref());
    }

    /// Takes in what the engine and background work have sent since the last
    /// frame. Of several playback errors, the last is shown with a count of
    /// the others, so a run of bad files is not hidden behind one name.
    fn drain_events(&mut self, current: Option<&PathBuf>) {
        let mut error: Option<(String, u64)> = None;
        while let Ok(event) = self.events.try_recv() {
            match event {
                // The frame's snapshot already shows the track and state.
                Event::TrackChanged { .. } | Event::StateChanged(_) => {}
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
                Event::Planned { track, result, .. } => {
                    self.sampler.planning = false;
                    if Some(&track) != current {
                        continue;
                    }
                    match result {
                        Ok(plan) => {
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
        let text = notice::text(&message);
        self.message = Some((message, text, Instant::now()));
    }

    /// The message on the bottom line, if one is showing.
    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref().map(|(m, _, _)| m)
    }

    /// Whether a key has asked the interface to exit.
    pub fn quitting(&self) -> bool {
        self.quit
    }

    /// Handles one key press.
    pub fn on_key(&mut self, key: KeyEvent) {
        self.follow_player();
        // Before text entry, which would otherwise type it as `c`.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }

        // Text entry swallows most keys.
        match &self.input {
            Input::Confirm(_) => {
                let Input::Confirm(action) = std::mem::replace(&mut self.input, Input::None) else {
                    unreachable!()
                };
                // Anything but `y` cancels, so a stray key cannot confirm.
                if typed(&key) == Some('y') {
                    self.confirm(action);
                } else {
                    self.notify(Message::Cancelled);
                }
                return;
            }
            Input::Help | Input::CommandHelp => {
                // The lists can be longer than the screen.
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => self.help_scroll += 1,
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.help_scroll = self.help_scroll.saturating_sub(1)
                    }
                    KeyCode::PageDown => self.help_scroll += 10,
                    KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
                    _ => self.input = Input::None,
                }
                return;
            }
            Input::Command(line) => {
                let line = line.clone();
                return self.command_key(key, line);
            }
            Input::Search(buf) => {
                let buf = buf.clone();
                return self.search_key(key, buf);
            }
            Input::SavePlaylist(buf) => {
                let buf = buf.clone();
                return self.save_key(key, buf);
            }
            Input::RenamePlaylist { from, name } => {
                let (from, name) = (from.clone(), name.clone());
                return self.rename_key(key, from, name);
            }
            Input::None => {}
        }

        if let Some(action) = self.keys.lookup((&key).into(), self.view).cloned() {
            self.perform(action);
        }
    }

    /// Does `action`. Keys and `:` commands both arrive here.
    pub fn perform(&mut self, action: Action) {
        self.follow_player();
        match action {
            Action::Quit => self.quit = true,
            Action::Help => {
                self.help_scroll = 0;
                self.input = Input::Help;
            }
            Action::CommandHelp => {
                self.help_scroll = 0;
                self.input = Input::CommandHelp;
            }
            Action::ShowView(view) => self.view = view,
            Action::NextView => self.view = self.view.next(),
            Action::Cursor(rows) => self.move_selection(rows),
            Action::CursorFirst => self.select(0),
            Action::CursorLast => self.select(self.len().saturating_sub(1)),
            Action::StartSearch => self.input = Input::Search(String::new()),
            Action::Search(query) => {
                self.apply_search(&query);
                if self.visible().is_empty() {
                    self.notify(Message::NoMatches);
                }
            }
            Action::ClearSearch => {
                if self.results.take().is_some() {
                    self.library_state.select(Some(0));
                }
            }
            Action::StartCommand => self.input = Input::Command(CommandLine::default()),
            Action::Activate => self.activate(),

            Action::Add => self.append_selection(),
            Action::Remove => self.remove_from_selection(),
            Action::MoveTrack(delta) => self.move_in_selection(delta),
            Action::ClearSelection => match self.session.selection().len() {
                0 => self.notify(Refusal::SelectionEmpty),
                n => self.input = Input::Confirm(Confirm::ClearSelection(n)),
            },
            Action::StartSave => match self.session.check_save() {
                Ok(()) => self.input = Input::SavePlaylist(String::new()),
                Err(refusal) => self.notify(refusal),
            },
            Action::SaveAs(name) => match self.session.check_save() {
                Ok(()) => self.save_as(&name),
                Err(refusal) => self.notify(refusal),
            },
            Action::DeletePlaylist => self.delete_playlist(),
            Action::StartRename => self.start_rename(),
            Action::RenameTo(name) => match self.playlist_under_cursor() {
                Some(from) => self.rename_to(&from, &name),
                None => self.notify(Message::NoPlaylistUnderCursor),
            },
            Action::PlayPlaylist(name) => {
                let notice = self.session.play_playlist_named(&name);
                self.follow_player();
                self.notify(notice);
            }

            Action::TogglePause => self.session.send(Cmd::TogglePause),
            Action::Next => self.session.send(Cmd::Next),
            Action::Prev => self.session.send(Cmd::Prev),
            Action::Stop => self.session.send(Cmd::Stop),
            Action::SeekBy(seconds) => self.session.send(Cmd::SeekBy(seconds)),
            Action::SeekTo(at) => self.session.send(Cmd::Seek(at)),
            Action::VolumeBy(delta) => self.session.volume_by(delta),
            Action::SetVolume(v) => self.session.send(Cmd::SetVolume(v)),
            Action::SpeedBy(semitones) => self.session.send(Cmd::SpeedBy(semitones)),
            Action::SetSpeed(semitones) => self.session.send(Cmd::SetSpeed(semitones)),
            Action::CycleMode(forward) => {
                let notice = self.session.cycle_mode(forward);
                self.notify(notice);
            }
            Action::SetMode(mode) => {
                let notice = self.session.set_mode(mode);
                self.notify(notice);
            }

            Action::Mark => {
                let notice = self.session.add_mark(None);
                self.notify(notice);
            }
            Action::MarkAt(at) => {
                let notice = self.session.add_mark(Some(at));
                self.notify(notice);
            }
            Action::UndoMark => {
                let notice = self.session.undo_mark();
                self.notify(notice);
            }
            Action::ClearMarks => match self.session.marks_to_clear() {
                Ok(n) => self.input = Input::Confirm(Confirm::ClearMarks(n)),
                Err(refusal) => self.notify(refusal),
            },
            Action::NextMark => {
                let notice = self.session.seek_to_mark(true);
                self.notify(notice);
            }
            Action::PrevMark => {
                let notice = self.session.seek_to_mark(false);
                self.notify(notice);
            }

            Action::Slice(slicing) => {
                let cut = match slicing {
                    Slicing::Region => samples::Cut::Region,
                    Slicing::Marks => samples::Cut::Marks,
                    Slicing::Equal(n) => samples::Cut::Equal(n),
                    Slicing::Onsets(s) => samples::Cut::Onsets(s.unwrap_or(self.onset_sensitivity)),
                };
                // The sampler view shows slices before they are written.
                if self.view == View::Sampler {
                    self.plan(cut);
                } else {
                    self.export(cut);
                }
            }
            Action::Zoom(zoom) => {
                self.sampler.zoom = match zoom {
                    action::Zoom::In => self.sampler.zoom + 1,
                    action::Zoom::Out => self.sampler.zoom.saturating_sub(1),
                    action::Zoom::All => 0,
                }
            }
            Action::Display(display) => {
                self.sampler.display = display.unwrap_or(self.sampler.display.next());
                self.notify(Message::Display(self.sampler.display));
            }
            Action::WriteSlices => match self.sampler.pending.take() {
                Some(plan) => {
                    self.session.write_slices(plan);
                    self.notify(Outcome::ExportStarted);
                }
                None => self.notify(Message::NoSlicesPlanned),
            },
            Action::DiscardSlices => match self.sampler.pending.take() {
                Some(_) => self.notify(Message::SlicesDiscarded),
                None => self.notify(Message::NoSlicesPlanned),
            },

            Action::Map { view, key, action } => {
                let shown = Action::Map {
                    view,
                    key,
                    action: action.clone(),
                };
                self.keys.bind(view, key, action.map(|a| *a));
                self.notify(Message::Mapped(shown));
            }
            Action::Unmap { view, key } => {
                if self.keys.unbind(view, key) {
                    self.notify(Message::Unmapped(key));
                } else {
                    self.notify(Message::NotBound { key, view });
                }
            }
        }
    }

    /// Saves the selection as `name`, asking first if that replaces a playlist.
    fn save_as(&mut self, name: &str) {
        match self.session.save_selection(name, false) {
            Notice::Refused(Refusal::WouldReplace(name)) => {
                self.input = Input::Confirm(Confirm::ReplacePlaylist(name));
            }
            notice => self.notify(notice),
        }
    }

    fn playlist_under_cursor(&self) -> Option<Playlist> {
        if self.view != View::Playlists {
            return None;
        }
        self.playlist_state
            .selected()
            .and_then(|i| self.session.playlists().get(i))
            .cloned()
    }

    fn command_key(&mut self, key: KeyEvent, mut line: CommandLine) {
        match key.code {
            KeyCode::Esc => return self.input = Input::None,
            KeyCode::Enter => {
                self.input = Input::None;
                if line.text.trim().is_empty() {
                    return;
                }
                // Recorded even when it fails, so a typo can be recalled and fixed.
                self.history.push(&line.text);
                match command::parse(&line.text, self.view) {
                    Ok(action) => self.perform(action),
                    Err(e) => self.notify(Message::Command(e)),
                }
                return;
            }
            // Deleting past the colon closes the prompt, as in vim.
            KeyCode::Backspace if !line.pop() => return self.input = Input::None,
            KeyCode::Tab | KeyCode::BackTab => {
                let names: Vec<String> = self
                    .session
                    .playlists()
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                line.complete(key.code == KeyCode::Tab, self.view, &names);
            }
            KeyCode::Up => line.recall(true, &self.history),
            KeyCode::Down => line.recall(false, &self.history),
            _ => {
                if let Some(c) = typed(&key) {
                    line.push(c);
                }
            }
        }
        self.input = Input::Command(line);
    }

    fn search_key(&mut self, key: KeyEvent, mut buf: String) {
        match key.code {
            KeyCode::Esc => {
                self.input = Input::None;
                self.results = None;
            }
            KeyCode::Enter => {
                self.input = Input::None;
                if self.visible().is_empty() {
                    self.notify(Message::NoMatches);
                }
            }
            KeyCode::Backspace => {
                buf.pop();
                self.apply_search(&buf);
                self.input = Input::Search(buf);
            }
            KeyCode::Char(c) if typed(&key).is_some() => {
                buf.push(c);
                self.apply_search(&buf);
                self.input = Input::Search(buf);
            }
            _ => self.input = Input::Search(buf),
        }
    }

    fn apply_search(&mut self, term: &str) {
        self.view = View::Library;
        if term.is_empty() {
            self.results = None;
        } else {
            self.results = Some(self.session.search(term));
        }
        self.library_state.select(if self.visible().is_empty() {
            None
        } else {
            Some(0)
        });
    }

    fn save_key(&mut self, key: KeyEvent, mut buf: String) {
        match key.code {
            KeyCode::Esc => self.input = Input::None,
            KeyCode::Enter => {
                self.input = Input::None;
                self.save_as(&buf);
            }
            KeyCode::Backspace => {
                buf.pop();
                self.input = Input::SavePlaylist(buf);
            }
            KeyCode::Char(c) if typed(&key).is_some() => {
                buf.push(c);
                self.input = Input::SavePlaylist(buf);
            }
            _ => self.input = Input::SavePlaylist(buf),
        }
    }

    fn start_rename(&mut self) {
        let Some(from) = self.playlist_under_cursor() else {
            return self.notify(Message::NoPlaylistUnderCursor);
        };
        // Starts from the current name, which is usually a small edit away.
        let name = from.name.clone();
        self.input = Input::RenamePlaylist { from, name };
    }

    fn rename_key(&mut self, key: KeyEvent, from: Playlist, mut name: String) {
        match key.code {
            KeyCode::Esc => self.input = Input::None,
            KeyCode::Enter => {
                self.input = Input::None;
                self.rename_to(&from, &name);
            }
            KeyCode::Backspace => {
                name.pop();
                self.input = Input::RenamePlaylist { from, name };
            }
            KeyCode::Char(c) if typed(&key).is_some() => {
                name.push(c);
                self.input = Input::RenamePlaylist { from, name };
            }
            _ => self.input = Input::RenamePlaylist { from, name },
        }
    }

    /// Renames `from` to `name`; the cursor follows it to its new place.
    fn rename_to(&mut self, from: &Playlist, name: &str) {
        let notice = self.session.rename_playlist(from.id, name);
        if matches!(notice, Notice::Done(_)) {
            // The list is sorted by name, so the renamed playlist may have moved.
            let at = self
                .session
                .playlists()
                .iter()
                .position(|p| p.id == from.id);
            self.playlist_state.select(at);
        }
        self.notify(notice);
    }

    fn confirm(&mut self, action: Confirm) {
        match action {
            Confirm::ReplacePlaylist(name) => {
                let notice = self.session.save_selection(&name, true);
                self.notify(notice);
            }
            Confirm::ClearMarks(_) => {
                if let Some(notice) = self.session.clear_marks() {
                    self.notify(notice);
                }
            }
            Confirm::ClearSelection(_) => {
                let outcome = self.session.clear_selection();
                self.selection_state.select(None);
                self.notify(outcome);
            }
            Confirm::DeletePlaylist(pl) => {
                if let Some(notice) = self.session.delete_playlist(pl.id) {
                    self.view = View::Playlists;
                    self.select(self.playlist_state.selected().unwrap_or(0));
                    self.notify(notice);
                }
            }
        }
    }

    fn len(&self) -> usize {
        match self.view {
            View::Library => self.visible().len(),
            View::Selection => self.session.selection().len(),
            View::Playlists => self.session.playlists().len(),
            View::Sampler => 0,
        }
    }

    fn state_mut(&mut self) -> &mut ListState {
        match self.view {
            View::Library => &mut self.library_state,
            View::Selection => &mut self.selection_state,
            // Never moved: the sampler has no rows, and `select` leaves it.
            View::Playlists | View::Sampler => &mut self.playlist_state,
        }
    }

    fn select(&mut self, i: usize) {
        if self.view == View::Sampler {
            return;
        }
        let len = self.len();
        if len == 0 {
            self.state_mut().select(None);
        } else {
            self.state_mut().select(Some(i.min(len - 1)));
        }
    }

    fn move_selection(&mut self, delta: i64) {
        let len = self.len();
        if len == 0 {
            return;
        }
        let cur = self.state_mut().selected().unwrap_or(0) as i64;
        let next = (cur + delta).clamp(0, len as i64 - 1) as usize;
        self.state_mut().select(Some(next));
    }

    /// Enter: play the list in view from the selected track, or play a playlist.
    fn activate(&mut self) {
        match self.view {
            View::Library => {
                let Some(i) = self.library_state.selected() else {
                    return;
                };
                let tracks = self.visible().to_vec();
                if tracks.is_empty() {
                    return;
                }
                self.play(tracks, i);
            }
            View::Selection => {
                if let Some(i) = self.selection_state.selected() {
                    self.play(self.session.selection().to_vec(), i);
                }
            }
            View::Sampler => {}
            View::Playlists => {
                let Some(pl) = self.playlist_under_cursor() else {
                    return;
                };
                let notice = self.session.play_playlist(pl.id);
                self.follow_player();
                self.notify(notice);
            }
        }
    }

    /// Plays `tracks` from `index`. The selection is not touched.
    fn play(&mut self, tracks: Vec<Track>, index: usize) {
        self.session.play(&tracks, index);
        self.playing = tracks;
        self.playing_source = self.session.player().queue();
    }

    /// `a`: in the library, selects the track or unselects it if it was
    /// selected; on a playlist, adds its tracks. Either way the cursor moves on.
    fn append_selection(&mut self) {
        let outcome = match self.view {
            View::Library => {
                let Some(track) = self
                    .library_state
                    .selected()
                    .and_then(|i| self.visible().get(i).cloned())
                else {
                    return;
                };
                self.session.toggle_selected(track)
            }
            View::Playlists => {
                let Some(pl) = self.playlist_under_cursor() else {
                    return;
                };
                let Some(outcome) = self.session.add_playlist_to_selection(pl.id) else {
                    return;
                };
                outcome
            }
            View::Selection | View::Sampler => return,
        };
        // Keep the selection's cursor on a track: on the first once a track is
        // added, and within the list when unselecting shortens it.
        let len = self.session.selection().len();
        match outcome {
            Outcome::RemovedFromSelection => {
                let cursor = self
                    .selection_state
                    .selected()
                    .map(|i| i.min(len.saturating_sub(1)));
                self.selection_state
                    .select(if len == 0 { None } else { cursor });
            }
            Outcome::AddedToSelection if self.selection_state.selected().is_none() => {
                self.selection_state.select(Some(0));
            }
            _ => {}
        }
        // On to the next row, so a run of tracks takes one key each.
        self.move_selection(1);
        self.notify(outcome);
    }

    fn remove_from_selection(&mut self) {
        let Some(i) = self.selection_state.selected() else {
            return;
        };
        if let Some(outcome) = self.session.remove_from_selection(i) {
            self.select(i);
            self.notify(outcome);
        }
    }

    /// Moves the selected track in the selection `delta` places, keeping it selected.
    fn move_in_selection(&mut self, delta: i64) {
        let Some(i) = self.selection_state.selected() else {
            return;
        };
        if let Some(to) = self.session.move_in_selection(i, delta) {
            self.selection_state.select(Some(to));
        }
    }

    fn delete_playlist(&mut self) {
        if let Some(pl) = self.playlist_under_cursor() {
            self.input = Input::Confirm(Confirm::DeletePlaylist(pl));
        }
    }

    /// Cuts the playing track on another thread. Reading a long region takes
    /// seconds, so [`App::refresh`] reports the result when it arrives.
    fn export(&mut self, cut: samples::Cut) {
        match self.session.export(cut) {
            Ok(_) => self.notify(Outcome::ExportStarted),
            Err(refusal) => self.notify(refusal),
        }
    }

    /// Plans slices of the playing track on another thread, for the sampler
    /// view to show until they are written or discarded.
    fn plan(&mut self, cut: samples::Cut) {
        match self.session.plan_slices(cut) {
            Ok(_) => {
                self.sampler.planning = true;
                self.notify(Outcome::PlanStarted);
            }
            Err(refusal) => self.notify(refusal),
        }
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

/// `path`, with the home directory shown as `~` to keep messages short.
pub fn home_as_tilde(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.and_then(|h| path.strip_prefix(h).ok().map(|rest| rest.to_path_buf())) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// The character `key` types, or `None` for any other key and for a Ctrl or
/// Alt chord, which is a command rather than text.
fn typed(key: &KeyEvent) -> Option<char> {
    match key.code {
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            Some(c)
        }
        _ => None,
    }
}

/// How long the meter keeps showing a peak after it passes.
pub const PEAK_HOLD: Duration = Duration::from_millis(1500);

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

/// `m:ss`, or `h:mm:ss` past an hour.
pub fn fmt_time(d: Duration) -> String {
    let total = d.as_secs();
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Short label for the playback state.
pub fn state_glyph(s: State) -> &'static str {
    match s {
        State::Playing => ">",
        State::Paused => "||",
        State::Stopped => "#",
    }
}
