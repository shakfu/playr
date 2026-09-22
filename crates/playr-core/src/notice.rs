//! What an operation did, or why it did nothing, as data.
//!
//! A frontend words these itself: the terminal shows a line of text, a GUI
//! might show a toast or highlight a row, and a web page cannot call Rust's
//! formatting at all. Operations report a [`Notice`]; nothing here is text
//! meant for a person, except the error strings carried from lower layers.

use std::path::PathBuf;
use std::time::Duration;

use crate::audio::Mode;
use crate::db::Pruned;
use crate::scan::ScanReport;

/// A report of one operation.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    Done(Outcome),
    Refused(Refusal),
    /// The operation was attempted and failed; `error` is the cause.
    Failed {
        task: Task,
        error: String,
    },
    /// Playback skipped a track. `missed` more errors came before it was seen.
    PlaybackError {
        error: String,
        missed: u64,
    },
}

/// What an operation changed.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    AddedToSelection,
    AlreadyInSelection,
    RemovedFromSelection,
    /// A track removed from the selection by position, by its title.
    RemovedTrack {
        title: String,
    },
    SelectionCleared,
    Saved {
        name: String,
        tracks: usize,
        /// Selected tracks not in the library, which a playlist cannot hold.
        left_out: usize,
    },
    Renamed {
        from: String,
        to: String,
    },
    Deleted {
        name: String,
    },
    PlayingPlaylist {
        name: String,
    },
    Mode(Mode),
    /// An analysis finished: `failed` of `analysed` could not be decoded.
    Analysed {
        analysed: usize,
        failed: usize,
    },
    /// An analysis started, over `dir` or over every track for `None`.
    AnalysisStarted {
        dir: Option<std::path::PathBuf>,
    },
    /// An analysis has finished `done` of `total` files.
    Analysing {
        done: usize,
        total: usize,
    },
    ReplayGain(crate::gain::ReplayGain),
    /// A mark added at `at`; not `kept` when there is no library file.
    Marked {
        at: Duration,
        kept: bool,
    },
    MarkRemoved {
        at: Duration,
    },
    MarksCleared,
    /// Seeked to the mark at `at`.
    AtMark {
        at: Duration,
    },
    PlanStarted,
    Planned {
        slices: usize,
    },
    ExportStarted,
    Exported {
        dir: PathBuf,
        slices: usize,
    },
    ScanStarted {
        /// The directory being scanned, or `None` when every recorded root is.
        dir: Option<PathBuf>,
    },
    /// A scan in progress has seen `seen` files, `added` of them new or changed.
    Scanning {
        seen: usize,
        added: usize,
    },
    Scanned {
        /// The directory scanned, or `None` when every recorded root was.
        dir: Option<PathBuf>,
        report: ScanReport,
    },
    PruneStarted {
        /// The directory being pruned, or `None` when every recorded root is.
        dir: Option<PathBuf>,
    },
    Pruned {
        /// The directory pruned, or `None` when every recorded root was.
        dir: Option<PathBuf>,
        removed: Pruned,
    },
    /// A mark moved from one position to another.
    MarkMoved {
        from: Duration,
        to: Duration,
    },
    /// A root was forgotten, with everything the library held under it.
    Forgot {
        dir: PathBuf,
        removed: Pruned,
    },
    /// Paths are being gathered to play.
    Opening,
    /// `tracks` gathered and playing; `skipped` paths could not be.
    Opened {
        tracks: usize,
        skipped: usize,
    },
}

/// Why an operation did nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    NothingPlaying,
    SelectionEmpty,
    /// Playlists and marks would be lost on exit without a library file.
    NoLibraryFile,
    NameEmpty,
    NameUnchanged,
    NameTaken(String),
    /// Saving would replace the playlist of this name; ask, then save again.
    WouldReplace(String),
    /// No playlist, or more than one, has this name.
    NoPlaylistNamed(String),
    PlaylistEmpty,
    /// A mark is already this close to the one asked for.
    AlreadyMarked {
        at: Duration,
    },
    NoMarks,
    NoLaterMark,
    NoEarlierMark,
    /// The session has no library file set, so a scan has nowhere to write.
    NoLibraryPath,
    NotADirectory(PathBuf),
    /// One scan or prune at a time: a second would write the same file.
    ScanRunning,
    AnalysisRunning,
    /// A rescan needs at least one directory previously given to a scan.
    NoRoots,
    /// `:roots rm` was given a directory the library does not have as a root.
    NotARoot(PathBuf),
    /// Nothing is marked where the sampler's cursor is.
    NoMarkHere,
    /// A mark is already where this one would move to.
    MarkInTheWay {
        at: Duration,
    },
    /// The window around the mark holds no rise to snap to.
    NoOnsetNear,
}

/// An operation that can fail, for [`Notice::Failed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    Save,
    Rename,
    Mark,
    MoveMark,
    RemoveMark,
    ClearMarks,
    Slice,
    Export,
    Scan,
    Analyze,
    Prune,
    Open,
}

impl From<Outcome> for Notice {
    fn from(outcome: Outcome) -> Self {
        Notice::Done(outcome)
    }
}

impl From<Refusal> for Notice {
    fn from(refusal: Refusal) -> Self {
        Notice::Refused(refusal)
    }
}
