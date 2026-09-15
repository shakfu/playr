//! The sampler view's state and geometry, which any frontend drawing a
//! waveform needs: the playing track's peaks as far as they are read, how the
//! view is zoomed and displayed, slices planned but not yet written, which
//! frames each column shows, and the words around the waveform. Drawing the
//! columns is the frontend's.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

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
    /// The job planning slices on another thread. Only its plan is shown, so a
    /// slicing replaced before it finishes shows nothing.
    pub planning: Option<JobId>,
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

/// The peaks of the playing track, or why they cannot be drawn yet, in words.
pub fn peaks_of(sampler: &Sampler, playing: Option<&PathBuf>) -> Result<Arc<Peaks>, String> {
    match (&sampler.wave, playing) {
        (_, None) => Err("Nothing is playing. Play a track to see its waveform.".into()),
        (Wave::Ready { path, peaks }, Some(playing)) if path == playing => Ok(peaks.clone()),
        (Wave::Failed { error, .. }, _) => Err(format!("Cannot read the waveform: {error}")),
        _ => Err("Reading the waveform...".into()),
    }
}

/// What the planned slices are doing, for the line under the waveform: empty
/// when nothing is planned.
pub fn plan_text(sampler: &Sampler) -> String {
    match (&sampler.pending, sampler.planning) {
        (_, Some(_)) => "planning slices".into(),
        (Some(p), _) => format!(
            "{} slices planned: enter writes, esc discards",
            p.spans.len()
        ),
        (None, _) => String::new(),
    }
}

/// One frame of a waveform view `columns` wide: which frames each column
/// shows, where the playhead, marks and region fall, and the scale.
#[derive(Debug, Clone)]
pub struct Layout {
    pub peaks: Arc<Peaks>,
    pub rate: u32,
    pub columns: u64,
    /// The first frame shown, and the frames each column shows.
    pub start: u64,
    pub per_column: u64,
    /// The zoom the view shows, clamped to the steps that change it.
    pub zoom: u32,
    /// The playhead, in frames.
    pub at: u64,
    /// Marks in the track, in frames.
    pub marks: Vec<u64>,
    /// The region around the playhead, end exclusive.
    pub region: (u64, u64),
    /// The loudest sample in the track, which the linear displays scale to.
    pub loudest: f32,
}

impl Layout {
    /// The layout of `columns` columns of `peaks`, zoomed `zoom` steps around
    /// `position`, with `marks` as times into the track.
    pub fn new(
        peaks: Arc<Peaks>,
        columns: u64,
        zoom: u32,
        position: Duration,
        marks: &[Duration],
    ) -> Layout {
        let rate = peaks.rate.max(1);
        let frame_of = |d: Duration| (d.as_secs_f64() * rate as f64).round() as u64;
        let at = frame_of(position);
        let columns = columns.max(1);
        let (start, per_column, zoom) = window(peaks.frames, columns, zoom, at);
        let marks: Vec<u64> = marks.iter().map(|&d| frame_of(d)).collect();
        let (region_start, region_end) = playr_core::samples::region(&marks, at);
        let region = (region_start, region_end.unwrap_or(peaks.frames));
        let loudest = peaks.loudest().max(f32::MIN_POSITIVE);
        Layout {
            peaks,
            rate,
            columns,
            start,
            per_column,
            zoom,
            at,
            marks,
            region,
            loudest,
        }
    }

    /// The frames column `c` shows, end exclusive.
    pub fn span_of(&self, c: usize) -> (u64, u64) {
        let a = self.start + c as u64 * self.per_column;
        (a, a + self.per_column)
    }

    /// The column showing `frame`, if one does.
    pub fn column_of(&self, frame: u64) -> Option<usize> {
        (frame >= self.start && frame < self.start + self.per_column * self.columns)
            .then(|| ((frame - self.start) / self.per_column) as usize)
    }

    /// The column under the playhead, if it is in view.
    pub fn playhead(&self) -> Option<usize> {
        self.column_of(self.at)
    }

    /// Whether column `c` shows any of the region.
    pub fn in_region(&self, c: usize) -> bool {
        let (a, b) = self.span_of(c);
        a < self.region.1 && b > self.region.0
    }

    /// The last frame shown, exclusive.
    pub fn end(&self) -> u64 {
        (self.start + self.per_column * self.columns).min(self.peaks.frames)
    }

    /// Column `c`'s RMS and peak levels, as heights from 0 to 1: scaled to the
    /// loudest sample for the envelope, and on the dB scale for the dB display.
    pub fn heights(&self, display: Display, c: usize) -> (f32, f32) {
        let height = |magnitude: f32| match display {
            Display::Decibels => db_height(magnitude),
            _ => magnitude / self.loudest,
        };
        let (a, b) = self.span_of(c);
        self.peaks.range(a, b).map_or((0.0, 0.0), |e| {
            (height(e.rms), height(e.min.abs().max(e.max.abs())))
        })
    }

    /// The lowest and highest sample in frames `a` to `b`, from -1 to 1 of the
    /// loudest sample, or `(1, -1)`, an empty extent, where there are none.
    pub fn extent(&self, a: u64, b: u64) -> (f32, f32) {
        self.peaks.range(a, b).map_or((1.0, -1.0), |e| {
            (e.min / self.loudest, e.max / self.loudest)
        })
    }

    /// The time at `columns` columns from the left edge, which may fall
    /// between columns.
    pub fn time_at(&self, columns: f32) -> Duration {
        let frame = self.start as f64 + columns.max(0.0) as f64 * self.per_column as f64;
        let frame = frame.min(self.peaks.frames as f64);
        Duration::from_secs_f64(frame / self.rate as f64)
    }

    /// The time a column shows: `1.5 ms`, `12 ms`.
    pub fn scale(&self) -> String {
        let ms = self.per_column as f64 * 1000.0 / self.rate as f64;
        if ms < 10.0 {
            format!("{ms:.1} ms")
        } else {
            format!("{ms:.0} ms")
        }
    }

    /// The times shown: `0:48.000-1:12.000`.
    pub fn shown(&self) -> String {
        format!(
            "{}-{}",
            fmt_frames(self.start, self.rate),
            fmt_frames(self.end(), self.rate)
        )
    }

    /// The region and the number of marks: `region 0:52.310-1:04.870 (12.560 s)  marks 2`.
    pub fn region_text(&self) -> String {
        let (a, b) = self.region;
        format!(
            "region {}-{} ({:.3} s)  marks {}",
            fmt_frames(a, self.rate),
            fmt_frames(b, self.rate),
            (b - a) as f64 / self.rate as f64,
            self.marks.len(),
        )
    }
}

/// The first and last frame of each span `plan` holds, the edges drawn.
pub fn edges(plan: &Plan) -> impl Iterator<Item = u64> + '_ {
    plan.spans.iter().flat_map(|&(a, b)| [Some(a), b]).flatten()
}
