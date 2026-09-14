//! The sampler view's state and geometry, which any frontend drawing a
//! waveform needs: the playing track's peaks as far as they are read, how the
//! view is zoomed and displayed, slices planned but not yet written, and which
//! frames each column shows. Drawing the columns is the frontend's.

use std::path::PathBuf;
use std::sync::Arc;

use playr_core::event::JobId;
use playr_core::samples::Plan;
use playr_core::wave::Peaks;

pub use crate::Display;

/// Fewest frames a column shows: two peak buckets, so a column split in two,
/// as a Braille cell is, still covers whole buckets.
pub const MIN_FRAMES_PER_COLUMN: u64 = 2 * playr_core::wave::BUCKET;

/// The level the dB display draws as empty, in dBFS.
pub const DB_FLOOR: f32 = -48.0;

/// A sample magnitude as a height from 0 at [`DB_FLOOR`] to 1 at full scale.
pub fn db_height(magnitude: f32) -> f32 {
    if magnitude <= 0.0 {
        return 0.0;
    }
    ((20.0 * magnitude.log10() - DB_FLOOR) / -DB_FLOOR).clamp(0.0, 1.0)
}

/// The waveform of the playing track, as far as it has been read.
#[derive(Debug, Clone, Default)]
pub enum Wave {
    #[default]
    None,
    /// Being read by the session, as background job `job`.
    Reading {
        path: PathBuf,
        job: JobId,
    },
    Ready {
        path: PathBuf,
        peaks: Arc<Peaks>,
    },
    Failed {
        path: PathBuf,
        error: String,
    },
}

impl Wave {
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            Wave::None => None,
            Wave::Reading { path, .. } | Wave::Ready { path, .. } | Wave::Failed { path, .. } => {
                Some(path)
            }
        }
    }
}

/// Everything the sampler view keeps between frames.
#[derive(Debug, Clone, Default)]
pub struct Sampler {
    pub wave: Wave,
    pub display: Display,
    /// How many times the view is zoomed in from the whole track, each step
    /// halving the frames a column shows. Drawing clamps it.
    pub zoom: u32,
    /// Whether slices are being planned on another thread.
    pub planning: bool,
    /// Slices planned for the track, shown until written or discarded.
    pub pending: Option<Plan>,
}

/// The first frame shown and the frames a column shows, for `width` columns
/// of a track of `frames` zoomed `zoom` steps, centred on frame `at`.
/// Returns the zoom clamped to the steps that change the view.
///
/// Columns start on whole peak buckets, so each column's peaks hold its own
/// frames and no others; a track too short for that is shown frame by frame.
pub fn window(frames: u64, width: u64, zoom: u32, at: u64) -> (u64, u64, u32) {
    let bucket = playr_core::wave::BUCKET;
    let width = width.max(1);
    let fit = frames.div_ceil(width).max(1);
    if fit < MIN_FRAMES_PER_COLUMN {
        return (0, fit, 0);
    }
    let deepest = (0..63)
        .find(|&z| fit >> z <= MIN_FRAMES_PER_COLUMN)
        .unwrap_or(63);
    let zoom = zoom.min(deepest);
    let per_column = (fit >> zoom)
        .max(MIN_FRAMES_PER_COLUMN)
        .next_multiple_of(bucket);
    let shown = per_column * width;
    let start = if shown >= frames {
        0
    } else {
        at.saturating_sub(shown / 2).min(frames - shown) / bucket * bucket
    };
    (start, per_column, zoom)
}

/// `frames` at `rate` as `m:ss.mmm`.
pub fn fmt_frames(frames: u64, rate: u32) -> String {
    let ms = frames * 1000 / rate.max(1) as u64;
    format!("{}:{:02}.{:03}", ms / 60_000, ms / 1000 % 60, ms % 1000)
}
