//! What the interface can do, and the keys that do it.
//!
//! Every key press becomes an [`Action`] before anything happens, and `:`
//! commands parse into the same actions. A key and its command therefore run
//! the same code and cannot drift apart.
//!
//! The key map is data. The defaults are in `settings.toml`, read by the same
//! code as a user's settings file, so user bindings change the table the
//! defaults built.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::View;
use playr_core::audio::Mode;

/// One thing the interface can do.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Quit,
    /// Show the key list.
    Help,
    /// Show the command list.
    CommandHelp,
    ShowView(View),
    NextView,
    /// Move the cursor this many rows; negative is up.
    Cursor(i64),
    CursorFirst,
    CursorLast,
    /// Open the search prompt.
    StartSearch,
    /// Show the tracks matching a query.
    Search(String),
    ClearSearch,
    /// Open the command prompt.
    StartCommand,
    /// Play the list in view from the cursor, or play the playlist under it.
    Activate,

    /// Select or unselect the track under the cursor, or add a playlist's tracks.
    Add,
    /// Remove the track under the cursor from the selection.
    Remove,
    /// Move the selected track this many places.
    MoveTrack(i64),
    ClearSelection,
    /// Open the prompt for a playlist name to save the selection as.
    StartSave,
    SaveAs(String),
    DeletePlaylist,
    /// Open the prompt to rename the playlist under the cursor.
    StartRename,
    RenameTo(String),
    PlayPlaylist(String),
    /// Scan a directory into the library, in the background.
    Scan(PathBuf),
    /// Re-scan every directory previously given to a scan.
    Rescan,
    /// Analyse a directory's tracks, or every track for `None`.
    Analyze(Option<PathBuf>),
    /// Show the directories the library covers.
    ShowRoots,
    /// Show what analysis measured about the track under the cursor.
    ShowInfo,
    /// Forget a directory, and the tracks and marks under it, once confirmed.
    ForgetRoot(PathBuf),
    /// Remove tracks and marks under a directory whose files are gone, once
    /// confirmed; with `None`, every directory previously scanned.
    Prune(Option<PathBuf>),
    /// Play files or directories, adding them to the selection.
    Open(Vec<PathBuf>),

    TogglePause,
    Next,
    Prev,
    Stop,
    /// Seek this many seconds; negative is back.
    SeekBy(i64),
    SeekTo(Duration),
    /// Change the volume by this fraction of full.
    VolumeBy(f32),
    /// Set the volume, from 0 to 1.
    SetVolume(f32),
    SpeedBy(i32),
    SetSpeed(i32),
    /// Next playback mode, or the previous one when false.
    CycleMode(bool),
    SetMode(Mode),
    /// Choose which ReplayGain applies.
    SetReplayGain(playr_core::gain::ReplayGain),

    /// Mark the playing position.
    Mark,
    MarkAt(Duration),
    UndoMark,
    ClearMarks,
    NextMark,
    PrevMark,

    /// Write slices of the playing track to the samples directory, or in the
    /// sampler view, plan them to be written.
    Slice(Slicing),
    /// Zoom the sampler view.
    Zoom(Zoom),
    /// Show the waveform this way, or switch to the other way when `None`.
    Display(Option<crate::Display>),
    /// Move the playhead along the sampler view's waveform.
    Nudge(Nudge),
    /// Snap the sampler view's moves and marks to zero crossings, or switch
    /// when `None`.
    Snap(Option<bool>),
    /// Start the sampler's range at the playhead.
    RangeIn,
    /// End the sampler's range at the playhead.
    RangeOut,
    /// Set the sampler's range, or clear it for `None`.
    SetRange(Option<(Duration, Duration)>),
    /// Play the range over and over, or stop; switch when `None`.
    Loop(Option<bool>),
    /// Move the sampler's cursor, which is apart from the playhead.
    MoveCursor(Nudge),
    /// Put the sampler's cursor at a time, or return it to the playhead.
    SetCursor(Option<Duration>),
    /// Move the cursor to the next mark, or the previous one.
    PickMark(bool),
    /// Move the mark under the cursor, and the cursor with it.
    MoveMark(Nudge),
    /// Move the mark under the cursor to this time; what a drag does.
    MoveMarkTo(Duration),
    /// Move the mark under the cursor to the nearest rise in the sound.
    SnapMark,
    /// Remove the mark under the cursor.
    DeleteMark,
    /// Play the range, the planned slice at the playhead, or the region
    /// around it, once, and pause at its end.
    Audition,
    /// Choose which end of the range edge moves shift.
    PickEdge(crate::sampler::Edge),
    /// Move the chosen end of the range, as a nudge moves the playhead.
    MoveEdge(Nudge),
    /// Write the slices planned in the sampler view.
    WriteSlices,
    DiscardSlices,
    /// Draw in these colours.
    Theme(crate::Theme),
    /// Show these columns, in this order, until playr exits.
    SetColumns(Vec<playr_core::columns::Column>),
    /// Sort every track list by these keys, until playr exits.
    SetSort(Vec<playr_core::columns::SortKey>),

    /// Bind `key` in one view, or in every view, to an action or to nothing.
    Map {
        view: Option<View>,
        key: Key,
        action: Option<Box<Action>>,
    },
    /// Remove a binding. A key unbound in one view falls back to its binding
    /// for every view.
    Unmap {
        view: Option<View>,
        key: Key,
    },
}

/// A zoom step in the sampler view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zoom {
    In,
    Out,
    /// Back to the whole track.
    All,
}

/// A move of the playhead in the sampler view; negative is back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nudge {
    /// This many columns.
    Columns(i64),
    /// This percentage of the columns shown.
    Percent(i64),
}

/// How `:slice` cuts the playing track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Slicing {
    Region,
    Marks,
    Equal(usize),
    /// At onsets, with this sensitivity, or with `onset_sensitivity` from the
    /// settings when `None`.
    Onsets(Option<f32>),
}

/// A key, as bindings name it. Each frontend converts its own key events to
/// these with [`Key::new`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    /// A function key, 1 to 12.
    F(u8),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Insert,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
}

/// The modifier keys held with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// A key with its Ctrl, Alt and Shift modifiers, as a binding names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    code: KeyCode,
    mods: Modifiers,
}

/// Names for keys that are not a printable character.
const KEY_NAMES: &[(&str, KeyCode)] = &[
    ("space", KeyCode::Char(' ')),
    ("enter", KeyCode::Enter),
    ("esc", KeyCode::Esc),
    ("tab", KeyCode::Tab),
    ("backtab", KeyCode::BackTab),
    ("backspace", KeyCode::Backspace),
    ("delete", KeyCode::Delete),
    ("insert", KeyCode::Insert),
    ("up", KeyCode::Up),
    ("down", KeyCode::Down),
    ("left", KeyCode::Left),
    ("right", KeyCode::Right),
    ("home", KeyCode::Home),
    ("end", KeyCode::End),
    ("pageup", KeyCode::PageUp),
    ("pagedown", KeyCode::PageDown),
];

/// Modifier prefixes in names, in the order a name writes them.
const PREFIXES: [&str; 3] = ["ctrl-", "alt-", "shift-"];

impl Key {
    /// The key `code` with `mods` held, as a binding would name it.
    pub fn new(code: KeyCode, mut mods: Modifiers) -> Self {
        let code = match code {
            KeyCode::Char(c) => {
                // The character already says whether Shift was held: `J`, `?`.
                mods.shift = false;
                // A terminal sends Ctrl-S and Ctrl-s as the same control code.
                if mods.ctrl {
                    KeyCode::Char(c.to_ascii_lowercase())
                } else {
                    code
                }
            }
            KeyCode::BackTab => {
                mods.shift = false;
                code
            }
            _ => code,
        };
        Key { code, mods }
    }

    /// Parses `j`, `J`, `space`, `pagedown`, `f5`, `ctrl-s` or `shift-right`.
    pub fn parse(text: &str) -> Result<Key, String> {
        let mut rest = text;
        let mut mods = Modifiers::default();
        // A prefix needs a key after it, so `-` and `ctrl--` name the minus key.
        'prefixes: loop {
            for prefix in PREFIXES {
                if let Some(r) = rest.strip_prefix(prefix).filter(|r| !r.is_empty()) {
                    match prefix {
                        "ctrl-" => mods.ctrl = true,
                        "alt-" => mods.alt = true,
                        _ => mods.shift = true,
                    }
                    rest = r;
                    continue 'prefixes;
                }
            }
            break;
        }
        let mut chars = rest.chars();
        let code = match (chars.next(), chars.next()) {
            (Some(c), None) => KeyCode::Char(c),
            _ => {
                let lower = rest.to_ascii_lowercase();
                let f = lower
                    .strip_prefix('f')
                    .and_then(|n| n.parse::<u8>().ok())
                    .filter(|n| (1..=12).contains(n));
                match (KEY_NAMES.iter().find(|(n, _)| *n == lower), f) {
                    (Some((_, code)), _) => *code,
                    (None, Some(n)) => KeyCode::F(n),
                    (None, None) => return Err(format!("not a key: {text}")),
                }
            }
        };
        Ok(Key::new(code, mods))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let held = [self.mods.ctrl, self.mods.alt, self.mods.shift];
        for (prefix, held) in PREFIXES.iter().zip(held) {
            if held {
                f.write_str(prefix)?;
            }
        }
        if let Some((name, _)) = KEY_NAMES.iter().find(|(_, c)| *c == self.code) {
            return f.write_str(name);
        }
        match self.code {
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::F(n) => write!(f, "f{n}"),
            // Every other code has a name above.
            other => write!(f, "{other:?}"),
        }
    }
}

/// One key binding. An `action` of `None` makes the key do nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    /// The view it applies in, or `None` for every view.
    pub view: Option<View>,
    pub key: Key,
    pub action: Option<Action>,
}

/// Every key binding, in the order they were made.
#[derive(Debug, Clone, PartialEq)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

/// The bindings in the default settings, `settings.toml`.
impl Default for Keymap {
    fn default() -> Self {
        crate::config::Config::default().keys
    }
}

impl Keymap {
    pub fn empty() -> Self {
        Keymap {
            bindings: Vec::new(),
        }
    }

    /// The action for `key` in `view`. A binding for the view wins over one
    /// for every view, and a binding to nothing ends the search.
    pub fn lookup(&self, key: Key, view: View) -> Option<&Action> {
        let find = |v| self.bindings.iter().find(|b| b.key == key && b.view == v);
        find(Some(view)).or_else(|| find(None))?.action.as_ref()
    }

    /// Binds `key`, replacing its binding in the same view if it has one.
    pub fn bind(&mut self, view: Option<View>, key: Key, action: Option<Action>) {
        let binding = Binding { view, key, action };
        match self
            .bindings
            .iter_mut()
            .find(|b| b.key == key && b.view == view)
        {
            Some(old) => *old = binding,
            None => self.bindings.push(binding),
        }
    }

    /// Removes the binding of `key` in `view`; false if it had none.
    pub fn unbind(&mut self, view: Option<View>, key: Key) -> bool {
        let before = self.bindings.len();
        self.bindings.retain(|b| !(b.key == key && b.view == view));
        self.bindings.len() < before
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }
}
