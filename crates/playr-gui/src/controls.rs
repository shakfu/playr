//! The window's fixed controls and the action each performs.
//!
//! Menus, row menus and buttons are drawn from these tables, and the parity
//! test reads them, so a control cannot perform one thing and be counted as
//! another.

use playr_app::action::{Action, Slicing, Zoom};
use playr_app::{Display, View};

/// A button or menu item.
pub struct Control {
    pub label: &'static str,
    pub action: Action,
}

const fn control(label: &'static str, action: Action) -> Control {
    Control { label, action }
}

pub const TRANSPORT: &[Control] = &[
    control("Prev", Action::Prev),
    control("Play", Action::TogglePause),
    control("Stop", Action::Stop),
    control("Next", Action::Next),
];

pub const MARKS: &[Control] = &[
    control("Mark", Action::Mark),
    control("Undo mark", Action::UndoMark),
    control("Previous mark", Action::PrevMark),
    control("Next mark", Action::NextMark),
    control("Clear marks", Action::ClearMarks),
];

pub const FILE_MENU: &[Control] = &[control("Quit", Action::Quit)];

pub const VIEW_MENU: &[Control] = &[
    control("Library", Action::ShowView(View::Library)),
    control("Selection", Action::ShowView(View::Selection)),
    control("Playlists", Action::ShowView(View::Playlists)),
    control("Sampler", Action::ShowView(View::Sampler)),
    control("Next view", Action::NextView),
    control("Search", Action::StartSearch),
    control("Clear search", Action::ClearSearch),
];

pub const PLAYBACK_MENU: &[Control] = &[
    control("Play or pause", Action::TogglePause),
    control("Stop", Action::Stop),
    control("Next track", Action::Next),
    control("Previous track", Action::Prev),
    control("Next mode", Action::CycleMode(true)),
    control("Previous mode", Action::CycleMode(false)),
    control("Normal speed", Action::SetSpeed(0)),
];

/// Slicing the playing track; in the sampler view these plan instead of write.
pub const SLICE_MENU: &[Control] = &[
    control("Slice the region", Action::Slice(Slicing::Region)),
    control("Slice at every mark", Action::Slice(Slicing::Marks)),
    control("Slice at onsets", Action::Slice(Slicing::Onsets(None))),
];

pub const HELP_MENU: &[Control] = &[
    control("Keys", Action::Help),
    control("Commands", Action::CommandHelp),
    control("Command line", Action::StartCommand),
];

/// A library row's menu; the action applies to the row right-clicked.
pub const LIBRARY_ROW: &[Control] = &[
    control("Play from here", Action::Activate),
    control("Select or unselect", Action::Add),
];

pub const SELECTION_ROW: &[Control] = &[
    control("Play from here", Action::Activate),
    control("Remove", Action::Remove),
    control("Move up", Action::MoveTrack(-1)),
    control("Move down", Action::MoveTrack(1)),
];

/// Buttons above the selection.
pub const SELECTION_BAR: &[Control] = &[
    control("Save as playlist", Action::StartSave),
    control("Clear selection", Action::ClearSelection),
];

pub const PLAYLIST_ROW: &[Control] = &[
    control("Play", Action::Activate),
    control("Add to selection", Action::Add),
    control("Rename", Action::StartRename),
    control("Delete", Action::DeletePlaylist),
];

/// Buttons under the waveform.
pub const SAMPLER_BAR: &[Control] = &[
    control("Zoom in", Action::Zoom(Zoom::In)),
    control("Zoom out", Action::Zoom(Zoom::Out)),
    control("Whole track", Action::Zoom(Zoom::All)),
    control("Envelope", Action::Display(Some(Display::Envelope))),
    control("dB", Action::Display(Some(Display::Decibels))),
    control("Waveform", Action::Display(Some(Display::Braille))),
    control("Write slices", Action::WriteSlices),
    control("Discard slices", Action::DiscardSlices),
];

/// Every table above.
pub const TABLES: &[&[Control]] = &[
    TRANSPORT,
    MARKS,
    FILE_MENU,
    VIEW_MENU,
    PLAYBACK_MENU,
    SLICE_MENU,
    HELP_MENU,
    LIBRARY_ROW,
    SELECTION_ROW,
    SELECTION_BAR,
    PLAYLIST_ROW,
    SAMPLER_BAR,
];

/// Actions whose value comes from how a control is used, by name: a slider's
/// position, a click on the progress bar, a row dragged, a file chosen.
pub const WITH_VALUES: &[(&str, &str)] = &[
    ("SetVolume", "the volume slider"),
    ("SetSpeed", "the speed slider"),
    ("SetMode", "the mode menu"),
    ("SeekTo", "a click on the progress bar or the waveform"),
    (
        "MarkAt",
        "a shift-click on the progress bar or the waveform",
    ),
    ("Zoom", "the mouse wheel over the waveform"),
    ("MoveTrack", "a selection row dragged to another place"),
    ("Add", "a library row's tick box"),
    ("Activate", "a double click on a row"),
    ("Open", "File, Open files and Open folder"),
    ("Scan", "File, Add folder to library"),
];
