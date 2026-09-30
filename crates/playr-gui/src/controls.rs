//! The window's fixed controls and the action each performs.
//!
//! Menus, row menus and buttons are drawn from these tables, and the parity
//! test reads them, so a control cannot perform one thing and be counted as
//! another.

use std::time::Duration;

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

/// Playback, after ReplayGain.
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
    control("Stop after this track", Action::StopAfter),
    control(
        "Stop in 30 minutes",
        Action::StopIn(Some(Duration::from_secs(1800))),
    ),
    control("Sleep timer off", Action::StopIn(None)),
    control("Next track", Action::Next),
    control("Previous track", Action::Prev),
    control("Next mode", Action::CycleMode(true)),
    control("Previous mode", Action::CycleMode(false)),
    control("Normal speed", Action::SetSpeed(0)),
];

/// In the EQ dialog, under its sliders.
pub const EQ: &[Control] = &[control("Flat", Action::FlatEq)];

/// Sampler, Slice: slicing the playing track; in the sampler view these plan
/// instead of write.
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
    control("Play next", Action::Enqueue(true)),
    control("Add to queue", Action::Enqueue(false)),
    control("Track info", Action::ShowInfo),
];

pub const SELECTION_ROW: &[Control] = &[
    control("Play from here", Action::Activate),
    control("Remove", Action::Remove),
    control("Play next", Action::Enqueue(true)),
    control("Add to queue", Action::Enqueue(false)),
    control("Track info", Action::ShowInfo),
    control("Move up", Action::MoveTrack(-1)),
    control("Move down", Action::MoveTrack(1)),
];

pub const QUEUE_ROW: &[Control] = &[
    control("Play from here", Action::Activate),
    control("Remove from queue", Action::Remove),
    control("Add to selection", Action::Add),
    control("Track info", Action::ShowInfo),
    control("Move up", Action::MoveTrack(-1)),
    control("Move down", Action::MoveTrack(1)),
];

/// Buttons above the selection.
pub const SELECTION_BAR: &[Control] = &[
    control("Save as playlist", Action::StartSave),
    control("Clear selection", Action::ClearSelection),
    control("Queue the selection", Action::EnqueueAll),
];

/// Buttons above the queue.
pub const QUEUE_BAR: &[Control] = &[
    control("Save as playlist", Action::StartSave),
    control("Clear queue", Action::ClearQueue),
];

/// Buttons above search results.
pub const RESULTS_BAR: &[Control] = &[
    control("Queue all results", Action::EnqueueAll),
    control("Save search", Action::StartSaveSearch),
];

pub const SEARCH_ROW: &[Control] = &[
    control("Show results", Action::Activate),
    control("Add to selection", Action::Add),
    control("Play next", Action::Enqueue(true)),
    control("Add to queue", Action::Enqueue(false)),
    control("Edit", Action::EditPlaylist),
    control("Delete", Action::DeletePlaylist),
];

pub const PLAYLIST_ROW: &[Control] = &[
    control("Play", Action::Activate),
    control("Add to selection", Action::Add),
    control("Play next", Action::Enqueue(true)),
    control("Add to queue", Action::Enqueue(false)),
    control("Edit", Action::EditPlaylist),
    control("Rename", Action::StartRename),
    control("Delete", Action::DeletePlaylist),
];

/// Buttons right of the sampler's header, drawn as `+`, `-`, U+2194 and `i`.
pub const SAMPLER_HEADER: &[Control] = &[
    control("Zoom in", Action::Zoom(Zoom::In)),
    control("Zoom out", Action::Zoom(Zoom::Out)),
    control("Whole track", Action::Zoom(Zoom::All)),
    // The sampler lists no tracks, so this describes the playing one.
    control("Track info", Action::ShowInfo),
];

/// The header's drop-down: how the waveform is drawn.
pub const DISPLAYS: &[Control] = &[
    control("Envelope", Action::Display(Some(Display::Envelope))),
    control("dB", Action::Display(Some(Display::Decibels))),
    control("Spectrogram", Action::Display(Some(Display::Spectrogram))),
    control("Waveform", Action::Display(Some(Display::Braille))),
];

/// Buttons under the waveform; Loop, Save loop, Snap and Fit follow them.
pub const SAMPLER_BAR: &[Control] = &[
    control("Mark", Action::Mark),
    control("Audition", Action::Audition),
    control("Clear range", Action::SetRange(None)),
];

/// Sampler, Range: its ends at the playhead, then choosing an end and moving
/// it a column. A drag across the waveform does both by pointer.
pub const RANGE_MENU: &[Control] = &[
    control("Range in", Action::RangeIn),
    control("Range out", Action::RangeOut),
    control("Move start", Action::PickEdge(Edge::Start)),
    control("Move end", Action::PickEdge(Edge::End)),
    control("Earlier", Action::MoveEdge(Nudge::Columns(-1))),
    control("Later", Action::MoveEdge(Nudge::Columns(1))),
];

/// Sampler, Marks: moving the cursor and editing the mark under it.
pub const MARK_MENU: &[Control] = &[
    control("Select previous mark", Action::PickMark(false)),
    control("Select next mark", Action::PickMark(true)),
    control("Mark earlier", Action::MoveMark(Nudge::Columns(-1))),
    control("Mark later", Action::MoveMark(Nudge::Columns(1))),
    control("Snap to rise", Action::SnapMark),
    control("Delete mark", Action::DeleteMark),
    control("Cursor earlier", Action::MoveCursor(Nudge::Columns(-1))),
    control("Cursor later", Action::MoveCursor(Nudge::Columns(1))),
    control("Cursor to playhead", Action::SetCursor(None)),
];

/// A mark's entries in the waveform's menu; the cursor moves to it first.
pub const MARK_ROW: &[Control] = &[
    control("Snap to rise", Action::SnapMark),
    control("Delete mark", Action::DeleteMark),
];

/// Hearing planned slices, then writing them: in the slice row while a plan
/// waits, drawn as `<`, `>`, Write and Discard, and under Sampler, Slice.
pub const PLAN_BAR: &[Control] = &[
    control("Previous slice", Action::AuditionSlice(false)),
    control("Next slice", Action::AuditionSlice(true)),
    control("Write slices", Action::WriteSlices),
    control("Discard slices", Action::DiscardSlices),
];

/// Sampler, Loops, after the numbered loops.
pub const LOOP_MENU: &[Control] = &[control("Clear loops", Action::ClearLoops)];

/// Every table above.
pub const TABLES: &[&[Control]] = &[
    TRANSPORT,
    MARKS,
    FILE_MENU,
    VIEW_MENU,
    THEME_MENU,
    PLAYBACK_MENU,
    EQ,
    SLICE_MENU,
    HELP_MENU,
    LIBRARY_ROW,
    SELECTION_ROW,
    SELECTION_BAR,
    QUEUE_ROW,
    QUEUE_BAR,
    RESULTS_BAR,
    PLAYLIST_ROW,
    SEARCH_ROW,
    SAMPLER_HEADER,
    DISPLAYS,
    SAMPLER_BAR,
    RANGE_MENU,
    MARK_MENU,
    MARK_ROW,
    PLAN_BAR,
    LOOP_MENU,
];

/// Actions whose value comes from how a control is used, by name: a slider's
/// position, a click on the progress bar, a row dragged, a file chosen.
pub const WITH_VALUES: &[(&str, &str)] = &[
    ("SetVolume", "the volume slider"),
    ("SetSpeed", "the speed slider"),
    ("SetEq", "the sliders in the EQ dialog"),
    ("SetMode", "the mode menu"),
    ("SetReplayGain", "Playback, ReplayGain"),
    (
        "SetSliceEdges",
        "Sampler, Slice, Edges, and the sampler's Edges drop-down",
    ),
    ("Convert", "Sampler, Slice, Convert to"),
    ("SetColumns", "View, Columns"),
    ("SetSort", "a click on a column heading"),
    ("SeekTo", "a click on the progress bar or the waveform"),
    (
        "MarkAt",
        "a shift-click on the progress bar or the waveform, and the waveform's menu",
    ),
    ("Zoom", "the mouse wheel over the waveform"),
    ("SetRange", "a drag across the waveform, and its menu"),
    ("Slice", "the sampler's Slice drop-down"),
    ("Snap", "the sampler's Snap button"),
    ("Fit", "the sampler's Fit button"),
    ("Loop", "the sampler's Loop button"),
    (
        "LoopSlot",
        "a loop under the waveform, Save loop, and Sampler, Loops",
    ),
    ("MoveTrack", "a selection row dragged to another place"),
    ("Add", "a library row's tick box"),
    ("Activate", "a double click on a row"),
    ("Open", "File, Open files and Open folder"),
    ("Scan", "File, Add folder to library"),
    ("MoveMarkTo", "a mark dragged along the waveform"),
    ("Rescan", "File, Rescan library"),
    ("Analyze", "File, Analyze library and Analyze folder"),
    ("ShowRoots", "File, Library directories"),
    ("ForgetRoot", "Forget, in File, Library directories"),
    ("Prune", "File, Remove missing files"),
];
