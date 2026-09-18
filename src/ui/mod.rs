//! Terminal interface.
//!
//! One thread: it renders, reads keys, and talks to the player over a channel.
//! Nothing here blocks on audio.

pub mod palette;
pub mod render;
pub mod sampler;

use std::time::Duration;

pub use playr_app::dispatch::Confirm;
pub use playr_app::model::{Input, Snapshot};
pub use playr_app::{Theme, View};
use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use rusqlite::Connection;

use playr_app::action::{Action, Key, Keymap, Modifiers};
use playr_app::command::CommandLine;
use playr_app::config::Config;
use playr_app::dispatch::Frontend;
use playr_app::message::Message;
use playr_app::model::Model;
use playr_app::sampler::Sampler;
use playr_core::audio::{Player, State};
use playr_core::db::query::Playlist;
use playr_core::db::Track;

/// The terminal interface: the shared [`Model`], and what only drawing in a
/// terminal needs.
pub struct App {
    model: Model,
    /// The first row each list shows.
    offsets: Offsets,
    /// Rows the key or command list is scrolled by.
    help_scroll: usize,
    /// False when `NO_COLOR` asks for none; see [`Screen::colour`].
    colour: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct Offsets {
    library: usize,
    selection: usize,
    playlists: usize,
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
    /// The sampler's columns, when it drew a waveform.
    pub scale: Option<playr_app::sampler::Scale>,
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
    /// Whether to draw in colour. Without it the cursor row is reversed.
    pub colour: bool,
    pub theme: Theme,
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
            colour: true,
            theme: Theme::Dark,
        }
    }

    /// The colours [`Screen::theme`] draws in.
    pub fn palette(&self) -> &'static palette::Palette {
        palette::of(self.theme)
    }

    /// The track list the library pane is showing.
    pub fn visible(&self) -> &[Track] {
        self.results.unwrap_or(self.all)
    }
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
        let mut model = Model::new(conn, player, tracks, config);
        // In a terminal the Braille display shows a waveform's shape best.
        model.set_display(playr_app::Display::Braille);
        App {
            model,
            offsets: Offsets::default(),
            help_scroll: 0,
            colour: true,
        }
    }

    /// Registers with the system's media keys and now-playing panel. Called
    /// once, by `main`, so tests that build an `App` stay off the bus.
    pub fn attach_media(&mut self) {
        self.model.attach_media();
    }

    /// Sets the library file `:scan` writes to when playr started without one.
    pub fn set_library_path(&mut self, path: std::path::PathBuf) {
        self.model.session_mut().set_library_path(path);
    }

    /// Whether to draw in colour; false when `NO_COLOR` is set.
    pub fn set_colour(&mut self, colour: bool) {
        self.colour = colour;
    }

    /// Borrows the state the renderer needs.
    pub fn screen(&self) -> Screen<'_> {
        let m = &self.model;
        let c = m.cursors();
        let o = self.offsets;
        let scroll = |row, offset| Scroll { row, offset };
        Screen {
            all: m.session().tracks(),
            results: m.results(),
            playing: m.playing(),
            selection: m.session().selection(),
            playlists: m.session().playlists(),
            input: m.input(),
            help_scroll: self.help_scroll,
            colour: self.colour,
            theme: m.theme(),
            message: m.message_text(),
            lists: Lists {
                library: scroll(c.library, o.library),
                selection: scroll(c.selection, o.selection),
                playlists: scroll(c.playlists, o.playlists),
            },
            ..Screen::new(m.view(), m.snapshot(), m.keymap(), m.sampler())
        }
    }

    /// Takes what drawing a frame settled: scroll positions, and cursors,
    /// help scroll and zoom kept within what could be shown.
    pub fn drawn(&mut self, drawn: Drawn) {
        let Lists {
            library,
            selection,
            playlists,
        } = drawn.lists;
        self.offsets = Offsets {
            library: library.offset,
            selection: selection.offset,
            playlists: playlists.offset,
        };
        self.model.set_cursor(View::Library, library.row);
        self.model.set_cursor(View::Selection, selection.row);
        self.model.set_cursor(View::Playlists, playlists.row);
        self.model.set_zoom(drawn.zoom);
        if let Some(scale) = drawn.scale {
            self.model.set_scale(scale);
        }
        self.help_scroll = drawn.help_scroll;
    }

    pub fn run(mut self, terminal: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
        while !self.model.quitting() {
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
            self.model.expire_message();
        }
        Ok(())
    }

    /// Samples the player for the next frame, and takes in any events.
    pub fn refresh(&mut self) {
        self.model.refresh();
    }

    /// The message on the bottom line, if one is showing.
    pub fn message(&self) -> Option<&Message> {
        self.model.message()
    }

    /// Whether a key has asked the interface to exit.
    pub fn quitting(&self) -> bool {
        self.model.quitting()
    }

    /// Handles one key press.
    pub fn on_key(&mut self, key: KeyEvent) {
        self.model.follow_player();
        // Before text entry, which would otherwise type it as `c`.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.model.quit();
            return;
        }

        // Text entry swallows most keys.
        match self.model.input() {
            // Anything but `y` cancels, so a stray key cannot confirm.
            Input::Confirm(_) => self.model.answer(typed(&key) == Some('y')),
            Input::Help | Input::CommandHelp | Input::Roots(_) => {
                // The lists can be longer than the screen.
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => self.help_scroll += 1,
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.help_scroll = self.help_scroll.saturating_sub(1)
                    }
                    KeyCode::PageDown => self.help_scroll += 10,
                    KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
                    _ => self.model.set_input(Input::None),
                }
            }
            Input::Command(line) => {
                let line = line.clone();
                self.command_key(key, line);
            }
            Input::Search(buf) => {
                let buf = buf.clone();
                self.search_key(key, buf);
            }
            Input::SavePlaylist(buf) => {
                let buf = buf.clone();
                self.save_key(key, buf);
            }
            Input::RenamePlaylist { from, name } => {
                let (from, name) = (from.clone(), name.clone());
                self.rename_key(key, from, name);
            }
            Input::None => {
                let bound = key_of(&key)
                    .and_then(|k| self.model.keymap().lookup(k, self.model.view()).cloned());
                if let Some(action) = bound {
                    self.model.perform(action);
                }
            }
        }
        self.follow_help();
    }

    /// Does `action`. Keys and `:` commands both arrive here.
    pub fn perform(&mut self, action: Action) {
        self.model.perform(action);
        self.follow_help();
    }

    /// Starts a key, command or root list at its top each time it opens.
    fn follow_help(&mut self) {
        if !matches!(
            self.model.input(),
            Input::Help | Input::CommandHelp | Input::Roots(_)
        ) {
            self.help_scroll = 0;
        }
    }

    fn command_key(&mut self, key: KeyEvent, mut line: CommandLine) {
        match key.code {
            KeyCode::Esc => return self.model.set_input(Input::None),
            KeyCode::Enter => return self.model.run_command(&line.text),
            // Deleting past the colon closes the prompt, as in vim.
            KeyCode::Backspace if !line.pop() => return self.model.set_input(Input::None),
            KeyCode::Tab | KeyCode::BackTab => {
                let names: Vec<String> = self
                    .model
                    .session()
                    .playlists()
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                line.complete(key.code == KeyCode::Tab, self.model.view(), &names);
            }
            KeyCode::Up => line.recall(true, self.model.history()),
            KeyCode::Down => line.recall(false, self.model.history()),
            _ => {
                if let Some(c) = typed(&key) {
                    line.push(c);
                }
            }
        }
        self.model.set_input(Input::Command(line));
    }

    fn search_key(&mut self, key: KeyEvent, mut buf: String) {
        match key.code {
            KeyCode::Esc => self.model.end_search(false),
            KeyCode::Enter => self.model.end_search(true),
            KeyCode::Backspace => {
                buf.pop();
                self.model.search_as_typed(buf);
            }
            KeyCode::Char(c) if typed(&key).is_some() => {
                buf.push(c);
                self.model.search_as_typed(buf);
            }
            _ => self.model.set_input(Input::Search(buf)),
        }
    }

    fn save_key(&mut self, key: KeyEvent, mut buf: String) {
        match key.code {
            KeyCode::Esc => self.model.set_input(Input::None),
            KeyCode::Enter => self.model.save_as(&buf),
            KeyCode::Backspace => {
                buf.pop();
                self.model.set_input(Input::SavePlaylist(buf));
            }
            KeyCode::Char(c) if typed(&key).is_some() => {
                buf.push(c);
                self.model.set_input(Input::SavePlaylist(buf));
            }
            _ => self.model.set_input(Input::SavePlaylist(buf)),
        }
    }

    fn rename_key(&mut self, key: KeyEvent, from: Playlist, mut name: String) {
        match key.code {
            KeyCode::Esc => self.model.set_input(Input::None),
            KeyCode::Enter => self.model.rename_to(&from, &name),
            KeyCode::Backspace => {
                name.pop();
                self.model.set_input(Input::RenamePlaylist { from, name });
            }
            KeyCode::Char(c) if typed(&key).is_some() => {
                name.push(c);
                self.model.set_input(Input::RenamePlaylist { from, name });
            }
            _ => self.model.set_input(Input::RenamePlaylist { from, name }),
        }
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
    view.title()
}

/// The question asked before `confirm`, with how to answer it.
pub fn confirm_prompt(confirm: &Confirm) -> String {
    format!("{} (y/n)", confirm.question())
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

/// Short label for the playback state.
pub fn state_glyph(s: State) -> &'static str {
    match s {
        State::Playing => ">",
        State::Paused => "||",
        State::Stopped => "#",
    }
}
