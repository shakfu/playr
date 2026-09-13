//! Terminal interface.
//!
//! One thread: it renders, reads keys, and talks to the player over a channel.
//! Nothing here blocks on audio.

pub mod render;

use std::collections::{HashMap, HashSet};
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
    Selection,
    Playlists,
}

impl View {
    fn next(self) -> Self {
        match self {
            View::Library => View::Selection,
            View::Selection => View::Playlists,
            View::Playlists => View::Library,
        }
    }

    fn title(self) -> &'static str {
        match self {
            View::Library => "Library",
            View::Selection => "Selection",
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
    /// Overwrite the playlist of this name with the selection.
    ReplacePlaylist(String),
    /// Empty the selection, which holds this many tracks.
    ClearSelection(usize),
}

impl Confirm {
    pub fn prompt(&self) -> String {
        match self {
            Confirm::DeletePlaylist(p) => format!("delete playlist \"{}\"? (y/n)", p.name),
            Confirm::ReplacePlaylist(name) => {
                format!("replace playlist \"{name}\" with the selection? (y/n)")
            }
            Confirm::ClearSelection(n) => format!("clear all {n} tracks from the selection? (y/n)"),
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

    /// Rows for the list the player is playing from.
    playing: Vec<Track>,
    /// The player list `playing` was built from, compared by identity.
    playing_source: Arc<[PathBuf]>,

    /// Tracks collected with `a`, to edit and save as a playlist. It does not
    /// change what plays unless it is played itself.
    selection: Vec<Track>,
    selection_state: ListState,

    playlists: Vec<Playlist>,
    playlist_state: ListState,

    input: Input,
    message: Option<(String, Instant)>,
    quit: bool,

    /// Last error sequence shown, so each new one is surfaced exactly once.
    seen_error: u64,
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
}

impl App {
    pub fn new(conn: Connection, player: Player) -> Self {
        Self::with_selection(conn, player, Vec::new())
    }

    /// Builds the app with `tracks` selected and playing, as the CLI hands
    /// them over, so the files played are also listed.
    pub fn with_selection(conn: Connection, player: Player, tracks: Vec<Track>) -> Self {
        let mut app = App {
            conn,
            player,
            view: View::Library,
            all: Vec::new(),
            results: None,
            library_state: ListState::default(),
            playing: Vec::new(),
            playing_source: Arc::default(),
            selection: Vec::new(),
            selection_state: ListState::default(),
            playlists: Vec::new(),
            playlist_state: ListState::default(),
            input: Input::None,
            message: None,
            quit: false,
            seen_error: 0,
            peak_hold: None,
            snapshot: Snapshot::default(),
        };
        if !tracks.is_empty() {
            app.selection = tracks.clone();
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
        let current = self.player.queue();
        if Arc::ptr_eq(&current, &self.playing_source) {
            return;
        }
        let known: HashMap<&str, &Track> = self
            .playing
            .iter()
            .chain(&self.selection)
            .chain(&self.all)
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
            playing: &self.playing,
            selection: &self.selection,
            playlists: &self.playlists,
            input: &self.input,
            message: self.message.as_ref().map(|(m, _)| m.as_str()),
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
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.on_key(key);
                    }
                }
            }
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
        self.follow_player();
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

    fn notify(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), Instant::now()));
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
            KeyCode::Char('2') => self.view = View::Selection,
            KeyCode::Char('3') => self.view = View::Playlists,

            // In the selection, shift moves the track rather than the cursor.
            KeyCode::Char('J') if self.view == View::Selection => self.move_in_selection(1),
            KeyCode::Char('K') if self.view == View::Selection => self.move_in_selection(-1),
            KeyCode::Down if shifted(&key) && self.view == View::Selection => {
                self.move_in_selection(1)
            }
            KeyCode::Up if shifted(&key) && self.view == View::Selection => {
                self.move_in_selection(-1)
            }
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
                if self.selection.is_empty() {
                    self.notify("selection is empty");
                } else if self.conn.path().is_none_or(str::is_empty) {
                    // In memory, the playlist would be lost on exit.
                    self.notify("no library to save to; `playr scan <dir>` creates one");
                } else {
                    self.input = Input::SavePlaylist(String::new());
                }
            }
            KeyCode::Char('d') if self.view == View::Selection => self.remove_from_selection(),
            KeyCode::Char('d') => self.delete_playlist(),
            KeyCode::Char('c') if self.view == View::Selection && !self.selection.is_empty() => {
                self.input = Input::Confirm(Confirm::ClearSelection(self.selection.len()));
            }
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
                    self.save_selection(&name);
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

    fn save_selection(&mut self, name: &str) {
        let ids: Vec<i64> = self
            .selection
            .iter()
            .map(|t| t.id)
            .filter(|id| *id != 0)
            .collect();
        match query::save_playlist(&mut self.conn, name, &ids) {
            Ok(_) => {
                // A playlist can only hold library tracks; `playr <path>` selects others.
                let left_out = self.selection.len() - ids.len();
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
            Confirm::ReplacePlaylist(name) => self.save_selection(&name),
            Confirm::ClearSelection(_) => {
                self.selection.clear();
                self.selection_state.select(None);
                self.notify("selection cleared");
            }
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
            View::Selection => self.selection.len(),
            View::Playlists => self.playlists.len(),
        }
    }

    fn state_mut(&mut self) -> &mut ListState {
        match self.view {
            View::Library => &mut self.library_state,
            View::Selection => &mut self.selection_state,
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
                    self.play(self.selection.clone(), i);
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

    /// Plays `tracks` from `index`. The selection is not touched.
    fn play(&mut self, tracks: Vec<Track>, index: usize) {
        let paths = tracks.iter().map(|t| PathBuf::from(&t.path)).collect();
        self.player.send(Cmd::Play(paths, index));
        self.playing = tracks;
        self.playing_source = self.player.queue();
    }

    /// `a`: in the library, selects the track or unselects it if it was
    /// selected; on a playlist, adds its tracks. Either way the cursor moves on.
    fn append_selection(&mut self) {
        if self.view == View::Library {
            return self.toggle_selected_track();
        }
        let added: Vec<Track> = match self.view {
            View::Playlists => self
                .playlist_state
                .selected()
                .and_then(|i| self.playlists.get(i))
                .map(|pl| query::playlist_tracks(&self.conn, pl.id).unwrap_or_default())
                .unwrap_or_default(),
            View::Library | View::Selection => return,
        };
        if added.is_empty() {
            return;
        }
        // Skip what is already selected. Repeats within `added` stay, so a
        // playlist that repeats a track on purpose keeps doing so.
        let selected: HashSet<&str> = self.selection.iter().map(|t| t.path.as_str()).collect();
        let new: Vec<Track> = added
            .into_iter()
            .filter(|t| !selected.contains(t.path.as_str()))
            .collect();
        // On to the next row either way, so a run of tracks takes one key each.
        self.move_selection(1);
        if new.is_empty() {
            self.notify("already in selection");
            return;
        }
        self.selection.extend(new);
        if self.selection_state.selected().is_none() {
            self.selection_state.select(Some(0));
        }
        // The tab already shows the total.
        self.notify("added to selection");
    }

    fn toggle_selected_track(&mut self) {
        let Some(track) = self
            .library_state
            .selected()
            .and_then(|i| self.visible().get(i).cloned())
        else {
            return;
        };
        if self.selection.iter().any(|t| t.path == track.path) {
            // Every copy, so the track's marker goes with it.
            self.selection.retain(|t| t.path != track.path);
            let len = self.selection.len();
            let cursor = self
                .selection_state
                .selected()
                .map(|i| i.min(len.saturating_sub(1)));
            self.selection_state
                .select(if len == 0 { None } else { cursor });
            self.notify("removed from selection");
        } else {
            self.selection.push(track);
            if self.selection_state.selected().is_none() {
                self.selection_state.select(Some(0));
            }
            self.notify("added to selection");
        }
        // On to the next row, so a run of tracks takes one key each.
        self.move_selection(1);
    }

    fn remove_from_selection(&mut self) {
        let Some(i) = self
            .selection_state
            .selected()
            .filter(|i| *i < self.selection.len())
        else {
            return;
        };
        let removed = self.selection.remove(i);
        self.select(i);
        self.notify(format!("removed \"{}\"", removed.display_title()));
    }

    /// Moves the selected track in the selection `delta` places, keeping it selected.
    fn move_in_selection(&mut self, delta: i64) {
        let Some(i) = self
            .selection_state
            .selected()
            .filter(|i| *i < self.selection.len())
        else {
            return;
        };
        let j = i as i64 + delta;
        if j < 0 || j >= self.selection.len() as i64 {
            return;
        }
        self.selection.swap(i, j as usize);
        self.selection_state.select(Some(j as usize));
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

fn shifted(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::SHIFT)
}

fn seek_step(key: &KeyEvent) -> i64 {
    if shifted(key) {
        SEEK_JUMP
    } else {
        SEEK_STEP
    }
}

/// Every key binding, as `(keys, action)`, for the help view.
pub const KEYS: &[(&str, &str)] = &[
    (
        "tab  1 2 3",
        "switch between library, selection and playlists",
    ),
    ("j k  up down", "move"),
    ("g G  home end", "jump to first or last"),
    ("page up/down", "move by ten"),
    ("enter", "play from here; in playlists, play it"),
    (
        "a",
        "select or unselect a track, or add a playlist; move down",
    ),
    ("/", "search; esc clears"),
    ("s", "save the selection as a playlist"),
    ("d", "remove from the selection; delete a playlist"),
    ("J K  shift up down", "move a track in the selection"),
    ("c", "clear the selection"),
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
