//! The window's fixed controls and the action each performs.
//!
//! Menus, row menus and buttons are drawn from these tables, and the parity
//! test reads them, so a control cannot perform one thing and be counted as
//! another.

use playr_app::action::{Action, Nudge, Slicing, Zoom};
use playr_app::sampler::Edge;
use playr_app::{Display, Theme, View};

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
    control("From start", Action::Restart),
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

/// View, Theme.
pub const THEME_MENU: &[Control] = &[
    control("System", Action::Theme(Theme::System)),
    control("Light", Action::Theme(Theme::Light)),
    control("Dark", Action::Theme(Theme::Dark)),
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
    control("Track info", Action::ShowInfo),
];

pub const SELECTION_ROW: &[Control] = &[
    control("Play from here", Action::Activate),
    control("Remove", Action::Remove),
    control("Track info", Action::ShowInfo),
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
    control("Spectrogram", Action::Display(Some(Display::Spectrogram))),
    control("Waveform", Action::Display(Some(Display::Braille))),
    // The sampler lists no tracks, so this describes the playing one.
    control("Track info", Action::ShowInfo),
];

/// Buttons setting the range to slice, under the waveform.
pub const RANGE_BAR: &[Control] = &[
    control("Range in", Action::RangeIn),
    control("Range out", Action::RangeOut),
    control("Clear range", Action::SetRange(None)),
    control("Audition", Action::Audition),
];

/// Choosing a range end and moving it a column, under the waveform.
pub const EDGE_BAR: &[Control] = &[
    control("Move start", Action::PickEdge(Edge::Start)),
    control("Move end", Action::PickEdge(Edge::End)),
    control("Earlier", Action::MoveEdge(Nudge::Columns(-1))),
    control("Later", Action::MoveEdge(Nudge::Columns(1))),
];

/// Moving the cursor and editing the mark under it, under the waveform.
pub const MARK_BAR: &[Control] = &[
    control("Previous mark", Action::PickMark(false)),
    control("Next mark", Action::PickMark(true)),
    control("Mark earlier", Action::MoveMark(Nudge::Columns(-1))),
    control("Mark later", Action::MoveMark(Nudge::Columns(1))),
    control("Snap to rise", Action::SnapMark),
    control("Delete mark", Action::DeleteMark),
    control("Cursor to playhead", Action::SetCursor(None)),
];

/// Buttons slicing the region or range, under the waveform; beside them, a
/// count and a sensitivity choose equal and onset slices.
pub const SLICE_BAR: &[Control] = &[
    control("Slice region", Action::Slice(Slicing::Region)),
    control("Slice at marks", Action::Slice(Slicing::Marks)),
];

/// Hearing planned slices, then writing them, under the slice buttons; the
/// Edges menu sits between the two.
pub const PLAN_BAR: &[Control] = &[
    control("Previous slice", Action::AuditionSlice(false)),
    control("Next slice", Action::AuditionSlice(true)),
];

/// After the numbered loop buttons.
pub const LOOP_BAR: &[Control] = &[control("Clear loops", Action::ClearLoops)];

pub const WRITE_BAR: &[Control] = &[
    control("Write slices", Action::WriteSlices),
    control("Discard slices", Action::DiscardSlices),
];

/// Every table above.
pub const TABLES: &[&[Control]] = &[
    TRANSPORT,
    MARKS,
    FILE_MENU,
    VIEW_MENU,
    THEME_MENU,
    PLAYBACK_MENU,
    SLICE_MENU,
    HELP_MENU,
    LIBRARY_ROW,
    SELECTION_ROW,
    SELECTION_BAR,
    PLAYLIST_ROW,
    SAMPLER_BAR,
    MARK_BAR,
    RANGE_BAR,
    EDGE_BAR,
    SLICE_BAR,
    PLAN_BAR,
    LOOP_BAR,
    WRITE_BAR,
];

/// Actions whose value comes from how a control is used, by name: a slider's
/// position, a click on the progress bar, a row dragged, a file chosen.
pub const WITH_VALUES: &[(&str, &str)] = &[
    ("SetVolume", "the volume slider"),
    ("SetSpeed", "the speed slider"),
    ("SetMode", "the mode menu"),
    ("SetReplayGain", "Playback, ReplayGain"),
    ("SetSliceEdges", "the sampler's Edges menu"),
    ("SetColumns", "View, Columns"),
    ("SetSort", "a click on a column heading"),
    ("SeekTo", "a click on the progress bar or the waveform"),
    (
        "MarkAt",
        "a shift-click on the progress bar or the waveform",
    ),
    ("Zoom", "the mouse wheel over the waveform"),
    ("SetRange", "a drag across the waveform"),
    ("Snap", "the sampler's Snap to zero tick box"),
    ("Fit", "the sampler's Fit range tick box"),
    ("Loop", "the sampler's Loop range tick box"),
    ("LoopSlot", "the sampler's numbered loop buttons"),
    ("MoveTrack", "a selection row dragged to another place"),
    ("Add", "a library row's tick box"),
    ("Activate", "a double click on a row"),
    ("Open", "File, Open files and Open folder"),
    ("Scan", "File, Add folder to library"),
    (
        "MoveCursor",
        "the sampler's Cursor earlier and later buttons",
    ),
    ("MoveMarkTo", "a mark dragged along the waveform"),
    ("Rescan", "File, Rescan library"),
    ("Analyze", "File, Analyze library and Analyze folder"),
    ("ShowRoots", "File, Library directories"),
    ("ForgetRoot", "Forget, in File, Library directories"),
    ("Prune", "File, Remove missing files"),
];
