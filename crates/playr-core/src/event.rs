//! Events: what a frontend learns without having asked just then.
//!
//! Background work (reading peaks, planning and writing slices) and the
//! engine report through one [`EventSink`], a function each frontend supplies.
//! The terminal sends events to a channel it drains every frame; an egui app
//! would also ask for a repaint, and a Tauri app would emit them to its page.
//! Position and levels change continuously, so they are read, not sent.

use std::path::PathBuf;
use std::sync::Arc;

use crate::audio::State;
use crate::db::Pruned;
use crate::samples::{Exported, Plan};
use crate::scan::{Playable, ScanReport};
use crate::wave::Peaks;

/// How many files a scan sees between [`Event::ScanProgress`] events.
pub const SCAN_PROGRESS_EVERY: usize = 100;

/// Names a piece of background work, so its event can be matched to it.
pub type JobId = u64;

/// Receives events. Called from the engine and worker threads, so it must be
/// quick and must not call back into the session.
pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

/// A sink that drops every event, for a session nobody watches.
pub fn ignore() -> EventSink {
    Arc::new(|_| {})
}

#[derive(Debug, Clone)]
pub enum Event {
    /// The engine moved to track `index` of its queue, at `path` if the queue
    /// holds one there.
    TrackChanged { index: usize, path: Option<PathBuf> },
    /// The engine started, paused or stopped.
    StateChanged(State),
    /// A track would not play and was skipped.
    PlaybackError(String),
    /// Peaks of `track` finished reading. A read that was cancelled sends nothing.
    Peaks {
        job: JobId,
        track: PathBuf,
        result: Result<Arc<Peaks>, String>,
    },
    /// Frames of `track` decoded for a close view.
    Detail {
        job: JobId,
        track: PathBuf,
        result: Result<Arc<crate::wave::Detail>, String>,
    },
    /// Slices of `track` were planned.
    Planned {
        job: JobId,
        track: PathBuf,
        result: Result<Plan, String>,
    },
    /// Slices were written.
    Exported {
        job: JobId,
        result: Result<Exported, String>,
    },
    /// A scan has seen `seen` files so far, `added` of them new or changed.
    /// Sent every [`SCAN_PROGRESS_EVERY`] files.
    ScanProgress {
        job: JobId,
        seen: usize,
        added: usize,
    },
    /// A scan of `dir` finished, or of every recorded root when `dir` is
    /// `None`. The session reads the library again once told with
    /// `Session::scanned`.
    Scanned {
        job: JobId,
        dir: Option<PathBuf>,
        result: Result<ScanReport, String>,
    },
    /// Tracks and marks under `dir` whose files are gone were removed, or
    /// under every recorded root when `dir` is `None`. The session reads the
    /// library again once told with `Session::pruned`.
    Pruned {
        job: JobId,
        dir: Option<PathBuf>,
        result: Result<Pruned, String>,
    },
    /// The onset nearest a mark was looked for, to snap it to. `to` is `None`
    /// when the window around it held none.
    Snapped {
        job: JobId,
        track: PathBuf,
        from: u64,
        result: Result<Option<u64>, String>,
    },
    /// Paths given to `Session::open` are gathered into tracks to play.
    Opened { job: JobId, playable: Playable },
}
