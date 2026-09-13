//! Terminal interface.
//!
//! One thread: it renders, reads keys, and talks to the player over a channel.
//! Nothing here blocks on audio.

pub mod render;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;
use rusqlite::Connection;

use crate::audio::{Cmd, Player, State, Status};
use crate::db::query::{self, Playlist};
use crate::db::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Library,
    Queue,
    Playlists,
}

impl View {
    fn next(self) -> Self {
        match self {
            View::Library => View::Queue,
            View::Queue => View::Playlists,
            View::Playlists => View::Library,
        }
    }

    fn title(self) -> &'static str {
        match self {
            View::Library => "Library",
            View::Queue => "Queue",
            View::Playlists => "Playlists",
        }
    }
}

/// What typed input is currently being collected.
pub enum Input {
    None,
    Search(String),
    SavePlaylist(String),
    /// Waiting for `y` before an action that cannot be undone.
    Confirm(Confirm),
    /// The key list is open; the next key closes it.
    Help,
}

/// A destructive action held until the listener confirms it.
pub enum Confirm {
    DeletePlaylist(Playlist),
    /// Overwrite the playlist of this name with the queue.
    ReplacePlaylist(String),
}

impl Confirm {
    pub fn prompt(&self) -> String {
        match self {
            Confirm::DeletePlaylist(p) => format!("delete playlist \"{}\"? (y/n)", p.name),
            Confirm::ReplacePlaylist(name) => {
                format!("replace playlist \"{name}\" with the queue? (y/n)")
            }
        }
    }
}

pub struct App {
    conn: Connection,
    player: Player,
    view: View,

    /// Every track in the library, loaded once.
    all: Vec<Track>,
    /// Search results; when set, the library view shows these instead.
    results: Option<Vec<Track>>,
    library_state: ListState,

    /// Rows for the player's queue, which is the only queue.
    queue: Vec<Track>,
    /// The player queue `queue` was built from, compared by identity.
    queue_source: Arc<[PathBuf]>,
    queue_state: ListState,

    playlists: Vec<Playlist>,
    playlist_state: ListState,

    input: Input,
    message: Option<(String, Instant)>,
    quit: bool,

    /// Last error sequence shown, so each new one is surfaced exactly once.
    seen_error: u64,
    /// Playing index the queue cursor was last moved to.
    followed: Option<usize>,
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
    pub queue: &'a [Track],
    pub playlists: &'a [Playlist],
    pub input: &'a Input,
    pub message: Option<&'a str>,
    pub library_state: &'a mut ListState,
    pub queue_state: &'a mut ListState,
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
}

impl App {
    pub fn new(conn: Connection, player: Player) -> Self {
        Self::with_queue(conn, player, Vec::new())
    }

    /// Builds the app and starts playing `queue`, as handed over by the CLI.
    pub fn with_queue(conn: Connection, player: Player, queue: Vec<Track>) -> Self {
        let mut app = App {
            conn,
            player,
            view: View::Library,
            all: Vec::new(),
            results: None,
            library_state: ListState::default(),
            queue: Vec::new(),
            queue_source: Arc::default(),
            queue_state: ListState::default(),
            playlists: Vec::new(),
            playlist_state: ListState::default(),
            input: Input::None,
            message: None,
            quit: false,
            seen_error: 0,
            followed: None,
            peak_hold: None,
            snapshot: Snapshot::default(),
        };
        if !queue.is_empty() {
            app.play(queue, 0);
            app.view = View::Queue;
        }
        app.reload();
        app
    }

    /// Rebuilds `queue` if the player's queue is no longer the one it shows.
    ///
    /// Changes made here update `queue` directly; this catches any other.
    fn follow_player_queue(&mut self) {
        let current = self.player.queue();
        if Arc::ptr_eq(&current, &self.queue_source) {
            return;
        }
        let known: HashMap<&str, &Track> = self
            .queue
            .iter()
            .chain(&self.all)
            .map(|t| (t.path.as_str(), t))
            .collect();
        self.queue = current
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
        self.queue_source = current;
    }

    fn reload(&mut self) {
        self.all = query::all(&self.conn).unwrap_or_default();
        self.playlists = query::playlists(&self.conn).unwrap_or_default();
        if !self.all.is_empty() && self.library_state.selected().is_none() {
            self.library_state.select(Some(0));
        }
        if !self.playlists.is_empty() && self.playlist_state.selected().is_none() {
            self.playlist_state.select(Some(0));
        }
    }

    /// The track list the library view is currently showing.
    fn visible(&self) -> &[Track] {
        self.results.as_deref().unwrap_or(&self.all)
    }

    /// Borrows the state the renderer needs.
    pub fn screen(&mut self) -> Screen<'_> {
        Screen {
            view: self.view,
            snapshot: &self.snapshot,
            all: &self.all,
            results: self.results.as_deref(),
            queue: &self.queue,
            playlists: &self.playlists,
            input: &self.input,
            message: self.message.as_ref().map(|(m, _)| m.as_str()),
            library_state: &mut self.library_state,
            queue_state: &mut self.queue_state,
            playlist_state: &mut self.playlist_state,
        }
    }

    pub fn run(mut self, terminal: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
        while !self.quit {
            self.refresh();
            terminal.draw(|f| render::draw(&mut self.screen(), f))?;

            // A short poll keeps the progress bar moving without busy-waiting.
            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.on_key(key);
                    }
                }
            }
            self.sync_queue();
            if let Some((_, at)) = &self.message {
                if at.elapsed() > Duration::from_secs(4) {
                    self.message = None;
                }
            }
        }
        Ok(())
    }

    /// Samples the player for the next frame, and shows any new error once.
    pub fn refresh(&mut self) {
        self.follow_player_queue();
        self.peak_hold = hold_peak(self.peak_hold, self.player.take_peak(), Instant::now());
        self.snapshot = Snapshot {
            status: self.player.status(),
            position: self.player.position(),
            volume: self.player.volume(),
            loudness: self.player.loudness(),
            peak: self.peak_hold.map(|(p, _)| 20.0 * p.log10()),
        };
        let seq = self.snapshot.status.error_seq;
        if seq > self.seen_error {
            let missed = seq - self.seen_error - 1;
            self.seen_error = seq;
            if let Some(e) = self.snapshot.status.error.clone() {
                // Only the latest error is kept, so a run of bad files would
                // otherwise show one name and hide the rest.
                if missed > 0 {
                    self.notify(format!("{e} (and {missed} more)"));
                } else {
                    self.notify(e);
                }
            }
        }
    }

    /// Keeps the displayed queue aligned with the engine's.
    fn sync_queue(&mut self) {
        let status = &self.snapshot.status;
        if self.queue.is_empty() {
            return;
        }
        let playing = match status.state {
            State::Playing | State::Paused => Some(status.index),
            State::Stopped => None,
        };

        if let Some(target) = follow_target(self.followed, playing, self.queue_state.selected()) {
            self.followed = Some(target);
            self.queue_state.select(Some(target));
        }
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), Instant::now()));
    }

    /// Whether a key has asked the interface to exit.
    pub fn quitting(&self) -> bool {
        self.quit
    }

    /// Handles one key press.
    pub fn on_key(&mut self, key: KeyEvent) {
        self.follow_player_queue();
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
                    self.notify("cancelled");
                }
                return;
            }
            Input::Help => {
                self.input = Input::None;
                return;
            }
            Input::Search(buf) => {
                let buf = buf.clone();
                return self.search_key(key, buf);
            }
            Input::SavePlaylist(buf) => {
                let buf = buf.clone();
                return self.save_key(key, buf);
            }
            Input::None => {}
        }

        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.input = Input::Help,
            KeyCode::Tab => self.view = self.view.next(),
            KeyCode::Char('1') => self.view = View::Library,
            KeyCode::Char('2') => self.view = View::Queue,
            KeyCode::Char('3') => self.view = View::Playlists,

            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Char('g') | KeyCode::Home => self.select(0),
            KeyCode::Char('G') | KeyCode::End => self.select(self.len().saturating_sub(1)),

            KeyCode::Char(' ') => self.player.send(Cmd::TogglePause),
            KeyCode::Char('n') => self.player.send(Cmd::Next),
            KeyCode::Char('p') => self.player.send(Cmd::Prev),
            KeyCode::Char('x') => self.player.send(Cmd::Stop),
            KeyCode::Char('+') | KeyCode::Char('=') => self.nudge_volume(0.05),
            KeyCode::Char('-') | KeyCode::Char('_') => self.nudge_volume(-0.05),
            KeyCode::Right => self.player.send(Cmd::SeekBy(seek_step(&key))),
            KeyCode::Left => self.player.send(Cmd::SeekBy(-seek_step(&key))),
            // Varispeed: one semitone per press, pitch moving with tempo.
            KeyCode::Char(']') => self.player.send(Cmd::SpeedBy(1)),
            KeyCode::Char('[') => self.player.send(Cmd::SpeedBy(-1)),
            KeyCode::Char('\\') => self.player.send(Cmd::SpeedReset),

            KeyCode::Char('/') => self.input = Input::Search(String::new()),
            KeyCode::Esc => {
                if self.results.take().is_some() {
                    self.library_state.select(Some(0));
                }
            }
            KeyCode::Enter => self.activate(),
            KeyCode::Char('a') => self.append_selection(),
            KeyCode::Char('s') => {
                if self.queue.is_empty() {
                    self.notify("queue is empty");
                } else if self.conn.path().is_none_or(str::is_empty) {
                    // In memory, the playlist would be lost on exit.
                    self.notify("no library to save to; `playr scan <dir>` creates one");
                } else {
                    self.input = Input::SavePlaylist(String::new());
                }
            }
            KeyCode::Char('d') => self.delete_playlist(),
            _ => {}
        }
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
                    self.notify("no matches");
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
            self.results = Some(query::search(&self.conn, term).unwrap_or_default());
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
                let name = buf.trim().to_string();
                if name.is_empty() {
                    self.notify("playlist name cannot be empty");
                } else if self.playlists.iter().any(|p| p.name == name) {
                    self.input = Input::Confirm(Confirm::ReplacePlaylist(name));
                } else {
                    self.save_queue(&name);
                }
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

    fn save_queue(&mut self, name: &str) {
        let ids: Vec<i64> = self
            .queue
            .iter()
            .map(|t| t.id)
            .filter(|id| *id != 0)
            .collect();
        match query::save_playlist(&mut self.conn, name, &ids) {
            Ok(_) => {
                // A playlist can only hold library tracks; `playr <path>` queues others.
                let left_out = self.queue.len() - ids.len();
                let note = if left_out > 0 {
                    format!(", {left_out} not in the library left out")
                } else {
                    String::new()
                };
                self.notify(format!("saved \"{name}\" ({} tracks{note})", ids.len()));
                self.playlists = query::playlists(&self.conn).unwrap_or_default();
            }
            Err(e) => self.notify(format!("could not save: {e}")),
        }
    }

    fn confirm(&mut self, action: Confirm) {
        match action {
            Confirm::ReplacePlaylist(name) => self.save_queue(&name),
            Confirm::DeletePlaylist(pl) => {
                if query::delete_playlist(&self.conn, pl.id).is_ok() {
                    self.playlists = query::playlists(&self.conn).unwrap_or_default();
                    self.view = View::Playlists;
                    self.select(self.playlist_state.selected().unwrap_or(0));
                    self.notify(format!("deleted \"{}\"", pl.name));
                }
            }
        }
    }

    fn len(&self) -> usize {
        match self.view {
            View::Library => self.visible().len(),
            View::Queue => self.queue.len(),
            View::Playlists => self.playlists.len(),
        }
    }

    fn state_mut(&mut self) -> &mut ListState {
        match self.view {
            View::Library => &mut self.library_state,
            View::Queue => &mut self.queue_state,
            View::Playlists => &mut self.playlist_state,
        }
    }

    fn select(&mut self, i: usize) {
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

    /// Enter: play from here in the library, jump within the queue, or load a playlist.
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
            View::Queue => {
                if let Some(i) = self.queue_state.selected() {
                    self.player.send(Cmd::Jump(i));
                }
            }
            View::Playlists => {
                let Some(i) = self.playlist_state.selected() else {
                    return;
                };
                let Some(pl) = self.playlists.get(i) else {
                    return;
                };
                let tracks = query::playlist_tracks(&self.conn, pl.id).unwrap_or_default();
                if tracks.is_empty() {
                    self.notify("playlist is empty");
                    return;
                }
                let name = pl.name.clone();
                self.play(tracks, 0);
                self.notify(format!("playing \"{name}\""));
            }
        }
    }

    fn play(&mut self, tracks: Vec<Track>, index: usize) {
        let paths = tracks.iter().map(|t| PathBuf::from(&t.path)).collect();
        self.player.send(Cmd::Play(paths, index));
        self.queue = tracks;
        self.queue_source = self.player.queue();
        self.queue_state.select(Some(index));
    }

    fn append_selection(&mut self) {
        let added: Vec<Track> = match self.view {
            View::Library => self
                .library_state
                .selected()
                .and_then(|i| self.visible().get(i).cloned())
                .into_iter()
                .collect(),
            View::Playlists => self
                .playlist_state
                .selected()
                .and_then(|i| self.playlists.get(i))
                .map(|pl| query::playlist_tracks(&self.conn, pl.id).unwrap_or_default())
                .unwrap_or_default(),
            View::Queue => return,
        };
        if added.is_empty() {
            return;
        }
        let count = added.len();
        let paths = added.iter().map(|t| PathBuf::from(&t.path)).collect();
        self.player.send(Cmd::Enqueue(paths));
        self.queue.extend(added);
        self.queue_source = self.player.queue();
        self.notify(format!("queued {count} track(s)"));
    }

    fn delete_playlist(&mut self) {
        if self.view != View::Playlists {
            return;
        }
        let Some(i) = self.playlist_state.selected() else {
            return;
        };
        let Some(pl) = self.playlists.get(i).cloned() else {
            return;
        };
        self.input = Input::Confirm(Confirm::DeletePlaylist(pl));
    }

    fn nudge_volume(&mut self, delta: f32) {
        // Not the snapshot: it is a frame old, so quick presses would repeat a step.
        let v = (self.player.volume() + delta).clamp(0.0, 1.0);
        self.player.send(Cmd::SetVolume(v));
    }
}

/// Seconds an arrow key seeks, or a shift-arrow.
const SEEK_STEP: i64 = 5;
const SEEK_JUMP: i64 = 30;

fn seek_step(key: &KeyEvent) -> i64 {
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        SEEK_JUMP
    } else {
        SEEK_STEP
    }
}

/// Every key binding, as `(keys, action)`, for the help view.
pub const KEYS: &[(&str, &str)] = &[
    ("tab  1 2 3", "switch between library, queue and playlists"),
    ("j k  up down", "move"),
    ("g G  home end", "jump to first or last"),
    ("page up/down", "move by ten"),
    ("enter", "play from here; in playlists, load it"),
    ("a", "queue the selection; plays if stopped"),
    ("/", "search; esc clears"),
    ("s", "save the queue as a playlist"),
    ("d", "delete the selected playlist"),
    ("space", "play or pause"),
    ("n p", "next or previous track"),
    ("x", "stop"),
    ("left right", "seek back or forward 5 seconds"),
    ("shift left right", "seek back or forward 30 seconds"),
    ("[ ]", "varispeed down or up a semitone"),
    ("\\", "back to normal speed"),
    ("+ -", "volume"),
    ("?", "show this list"),
    ("q  ctrl-c", "quit"),
];

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

/// Where the queue cursor belongs after a status update, or `None` to leave it.
///
/// The cursor follows playback so the playing track stays on screen in a long
/// queue: without it, a track change scrolls the current song out of view and
/// it has to be hunted for.
///
/// It moves only when the track actually changes, not on every frame, so
/// scrolling with `j`/`k` is not fought for as long as the track keeps playing.
pub fn follow_target(
    last_seen: Option<usize>,
    playing: Option<usize>,
    selected: Option<usize>,
) -> Option<usize> {
    match playing {
        // Nothing is playing: leave the cursor wherever the listener put it.
        None => None,
        // First status after a queue is loaded.
        Some(now) if selected.is_none() => Some(now),
        // The track changed, so follow it.
        Some(now) if last_seen != Some(now) => Some(now),
        _ => None,
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
