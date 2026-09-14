//! Terminal interface.
//!
//! One thread: it renders, reads keys, and talks to the player over a channel.
//! Nothing here blocks on audio.

pub mod notice;
pub mod render;
pub mod sampler;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use playr_app::dispatch::Confirm;
pub use playr_app::View;
use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use rusqlite::Connection;

use notice::Message;
use playr_app::action::{Action, Key, Keymap, Modifiers, Zoom};
use playr_app::command::{self, CommandLine, History};
use playr_app::config::Config;
use playr_app::dispatch::{
    confirmed, dispatch, rename, save_as, search, Frontend, Presentation, Prompt,
};
use playr_core::audio::{Cmd, Player, State, Status};
use playr_core::db::query::{Mark, Playlist};
use playr_core::db::Track;
use playr_core::event::{Event, EventSink};
use playr_core::notice::{Notice, Outcome, Task};
use playr_core::samples::Plan;
use playr_core::session::Session;
use sampler::{Sampler, Wave};

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

pub struct App {
    /// The library and player, and everything done with them.
    session: Session,
    view: View,

    /// Search results; when set, the library view shows these instead.
    results: Option<Vec<Track>>,
    /// The cursor and scroll position of each list.
    lists: Lists,

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

/// A list's cursor row, if one is chosen, and the first row it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Scroll {
    pub row: Option<usize>,
    pub offset: usize,
}

/// The cursor and scroll position of each list in the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Lists {
    pub library: Scroll,
    pub selection: Scroll,
    pub playlists: Scroll,
}

/// What drawing a frame settled, for the next frame to start from: where each
/// list scrolled to, with its cursor kept inside the list, and the help
/// scroll and zoom kept within what can be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drawn {
    pub lists: Lists,
    pub help_scroll: usize,
    pub zoom: u32,
}

/// No typed input, for a [`Screen`] that collects none.
static NO_INPUT: Input = Input::None;

/// Everything the drawing code reads, and nothing it writes.
///
/// Rendering takes this rather than the whole `App` so it can be exercised
/// against a `TestBackend` without an audio device. What drawing settles comes
/// back as a [`Drawn`].
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
    pub sampler: &'a Sampler,
    /// Rows the key or command list is scrolled by.
    pub help_scroll: usize,
    pub message: Option<&'a str>,
    pub lists: Lists,
}

impl<'a> Screen<'a> {
    /// A screen of `view` with empty lists, no input and no message. Set the
    /// rest with struct update syntax: `Screen { all: &tracks, ..Screen::new(..) }`.
    pub fn new(
        view: View,
        snapshot: &'a Snapshot,
        keys: &'a Keymap,
        sampler: &'a Sampler,
    ) -> Screen<'a> {
        Screen {
            view,
            snapshot,
            all: &[],
            results: None,
            playing: &[],
            selection: &[],
            playlists: &[],
            input: &NO_INPUT,
            keys,
            sampler,
            help_scroll: 0,
            message: None,
            lists: Lists::default(),
        }
    }

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
        session.set_samples_dir(config.settings.samples);
        let mut app = App {
            session,
            view: View::Library,
            results: None,
            lists: Lists::default(),
            playing: Vec::new(),
            playing_source: Arc::default(),
            input: Input::None,
            history: History::default(),
            keys: config.keys,
            onset_sensitivity: config.settings.onset_sensitivity,
            events,
            sampler: Sampler::default(),
            help_scroll: 0,
            message: None,
            quit: false,
            peak_hold: None,
            snapshot: Snapshot::default(),
        };
        app.session.send(Cmd::SetVolume(config.settings.volume));
        app.session.send(Cmd::SetMode(config.settings.mode));
        app.session.send(Cmd::SetSpeed(config.settings.speed));
        if !tracks.is_empty() {
            app.session.set_selection(tracks.clone());
            app.lists.selection.row = Some(0);
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
        if !self.session.tracks().is_empty() && self.lists.library.row.is_none() {
            self.lists.library.row = Some(0);
        }
        if !self.session.playlists().is_empty() && self.lists.playlists.row.is_none() {
            self.lists.playlists.row = Some(0);
        }
    }

    /// The track list the library view is currently showing.
    fn visible(&self) -> &[Track] {
        self.results.as_deref().unwrap_or(self.session.tracks())
    }

    /// Borrows the state the renderer needs.
    pub fn screen(&self) -> Screen<'_> {
        Screen {
            all: self.session.tracks(),
            results: self.results.as_deref(),
            playing: &self.playing,
            selection: self.session.selection(),
            playlists: self.session.playlists(),
            input: &self.input,
            help_scroll: self.help_scroll,
            message: self.message.as_ref().map(|(_, text, _)| text.as_str()),
            lists: self.lists,
            ..Screen::new(self.view, &self.snapshot, &self.keys, &self.sampler)
        }
    }

    /// Takes what drawing a frame settled: scroll positions, and cursors,
    /// help scroll and zoom kept within what could be shown.
    pub fn drawn(&mut self, drawn: Drawn) {
        self.lists = drawn.lists;
        self.help_scroll = drawn.help_scroll;
        self.sampler.zoom = drawn.zoom;
    }

    pub fn run(mut self, terminal: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
        while !self.quit {
            self.refresh();
            let mut drawn = None;
            terminal.draw(|f| drawn = Some(render::draw(&self.screen(), f)))?;
            if let Some(drawn) = drawn {
                self.drawn(drawn);
            }

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
                    confirmed(action, self);
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

        if let Some(action) = key_of(&key).and_then(|k| self.keys.lookup(k, self.view).cloned()) {
            self.perform(action);
        }
    }

    /// Does `action`. Keys and `:` commands both arrive here.
    pub fn perform(&mut self, action: Action) {
        self.follow_player();
        dispatch(action, self);
        // An action that plays changes the player's list; show its rows at once.
        self.follow_player();
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
                search(self, &buf);
                self.input = Input::Search(buf);
            }
            KeyCode::Char(c) if typed(&key).is_some() => {
                buf.push(c);
                search(self, &buf);
                self.input = Input::Search(buf);
            }
            _ => self.input = Input::Search(buf),
        }
    }

    fn save_key(&mut self, key: KeyEvent, mut buf: String) {
        match key.code {
            KeyCode::Esc => self.input = Input::None,
            KeyCode::Enter => {
                self.input = Input::None;
                save_as(self, &buf);
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

    fn rename_key(&mut self, key: KeyEvent, from: Playlist, mut name: String) {
        match key.code {
            KeyCode::Esc => self.input = Input::None,
            KeyCode::Enter => {
                self.input = Input::None;
                rename(self, &from, &name);
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

impl Frontend for App {
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
            View::Library => self.lists.library.row,
            View::Selection => self.lists.selection.row,
            View::Playlists => self.lists.playlists.row,
            View::Sampler => None,
        }
    }

    fn set_cursor(&mut self, view: View, row: Option<usize>) {
        match view {
            View::Library => self.lists.library.row = row,
            View::Selection => self.lists.selection.row = row,
            View::Playlists => self.lists.playlists.row = row,
            View::Sampler => {}
        }
    }

    fn listed(&self) -> &[Track] {
        self.visible()
    }

    fn set_results(&mut self, results: Option<Vec<Track>>) -> Option<Vec<Track>> {
        std::mem::replace(&mut self.results, results)
    }

    fn onset_sensitivity(&self) -> f32 {
        self.onset_sensitivity
    }

    fn notify(&mut self, message: Message) {
        App::notify(self, message);
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
            Presentation::KeyList => {
                self.help_scroll = 0;
                self.input = Input::Help;
            }
            Presentation::CommandList => {
                self.help_scroll = 0;
                self.input = Input::CommandHelp;
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
                App::notify(self, Message::Display(self.sampler.display));
            }
        }
    }

    fn planning(&mut self) {
        self.sampler.planning = true;
    }

    fn take_plan(&mut self) -> Option<Plan> {
        self.sampler.pending.take()
    }
}

/// The key a terminal key event names, or `None` for a key no binding can name.
pub fn key_of(event: &KeyEvent) -> Option<Key> {
    use playr_app::action::KeyCode as K;
    let code = match event.code {
        KeyCode::Char(c) => K::Char(c),
        KeyCode::F(n) => K::F(n),
        KeyCode::Enter => K::Enter,
        KeyCode::Esc => K::Esc,
        KeyCode::Tab => K::Tab,
        KeyCode::BackTab => K::BackTab,
        KeyCode::Backspace => K::Backspace,
        KeyCode::Delete => K::Delete,
        KeyCode::Insert => K::Insert,
        KeyCode::Up => K::Up,
        KeyCode::Down => K::Down,
        KeyCode::Left => K::Left,
        KeyCode::Right => K::Right,
        KeyCode::Home => K::Home,
        KeyCode::End => K::End,
        KeyCode::PageUp => K::PageUp,
        KeyCode::PageDown => K::PageDown,
        _ => return None,
    };
    let mods = Modifiers {
        ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        alt: event.modifiers.contains(KeyModifiers::ALT),
        shift: event.modifiers.contains(KeyModifiers::SHIFT),
    };
    Some(Key::new(code, mods))
}

/// The name of `view` on its tab.
pub fn view_title(view: View) -> &'static str {
    match view {
        View::Library => "Library",
        View::Selection => "Selection",
        View::Playlists => "Playlists",
        View::Sampler => "Sampler",
    }
}

/// The question asked before `confirm`.
pub fn confirm_prompt(confirm: &Confirm) -> String {
    match confirm {
        Confirm::DeletePlaylist(p) => format!("delete playlist \"{}\"? (y/n)", p.name),
        Confirm::ReplacePlaylist(name) => {
            format!("replace playlist \"{name}\" with the selection? (y/n)")
        }
        Confirm::ClearSelection(n) => format!("clear all {n} tracks from the selection? (y/n)"),
        Confirm::ClearMarks(n) => format!("clear all {n} marks from this track? (y/n)"),
    }
}

/// `path`, with the home directory shown as `~` to keep messages short.
pub fn home_as_tilde(path: &std::path::Path) -> String {
    let home = std::env::home_dir();
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
