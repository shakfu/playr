//! The sampler view's state and geometry, which any frontend drawing a
//! waveform needs: the playing track's peaks as far as they are read, how the
//! view is zoomed and displayed, slices planned but not yet written, which
//! frames each column shows, and the words around the waveform. Drawing the
//! columns is the frontend's.
//!
//! Zoom halves the frames a column shows down to one, then doubles the
//! columns a frame takes, as far as a frontend allows. Columns of fewer than
//! [`DETAIL_BELOW`] frames are finer than the peaks hold, so they are drawn
//! from a few seconds of decoded frames, read as the view needs them.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::action::Nudge;
use playr_core::event::JobId;
use playr_core::samples::Plan;
use playr_core::spectrum::BANDS;
use playr_core::wave::{Detail, Extent, Peaks};

pub use crate::Display;

/// Columns of fewer frames than this are drawn from decoded frames. At this
/// many, two peak buckets, a column split in two, as a Braille cell is, still
/// covers whole buckets.
pub const DETAIL_BELOW: u64 = 2 * playr_core::wave::BUCKET;

/// How far past the view either side a detail read goes, so small moves need
/// no new read.
pub const DETAIL_MARGIN: Duration = Duration::from_secs(2);

/// How far a snap looks for a zero crossing, either side.
pub const SNAP_WITHIN: Duration = Duration::from_millis(10);

/// The level the dB display draws as empty, in dBFS.
pub const DB_FLOOR: f32 = -48.0;

/// How far below the track's loudest level the spectrogram reaches, in dB.
pub const SPECTRUM_RANGE_DB: f32 = 90.0;

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
    /// Whether moves and marks in the view snap to zero crossings.
    pub snap: bool,
    /// Whether the view centres on the range, once both ends are set, rather
    /// than on the playhead.
    pub fit: bool,
    /// Whether a fitted view centres on the picked end rather than the range.
    pub fit_edge: bool,
    /// The columns last drawn, which a nudge counts in.
    pub scale: Option<Scale>,
    /// Frames to slice instead of the region, while the track plays.
    pub range: Option<Range>,
    /// The range end that edge moves shift.
    pub edge: Edge,
    /// Frames decoded for a view finer than the peaks.
    pub detail: DetailRead,
    /// A frame the view points at, apart from the playhead. `None` follows the
    /// playhead, which is what the view did before there was a cursor.
    pub cursor: Option<u64>,
}

/// Frames of the playing track decoded for a close view.
#[derive(Debug, Clone, Default)]
pub enum DetailRead {
    #[default]
    None,
    Reading {
        path: PathBuf,
        job: JobId,
        start: u64,
        end: u64,
    },
    Ready {
        path: PathBuf,
        detail: Arc<Detail>,
    },
    /// A read of these frames failed; the view keeps drawing peaks there.
    Failed {
        path: PathBuf,
        start: u64,
        end: u64,
    },
}

impl DetailRead {
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            DetailRead::None => None,
            DetailRead::Reading { path, .. }
            | DetailRead::Ready { path, .. }
            | DetailRead::Failed { path, .. } => Some(path),
        }
    }

    /// Whether frames `a..b` of `path` are read, being read, or failed to read.
    pub fn answers(&self, path: &PathBuf, a: u64, b: u64) -> bool {
        match self {
            DetailRead::Reading {
                path: p,
                start,
                end,
                ..
            }
            | DetailRead::Failed {
                path: p,
                start,
                end,
            } => p == path && a >= *start && b <= *end,
            DetailRead::Ready { path: p, detail } => p == path && detail.covers(a, b),
            DetailRead::None => false,
        }
    }
}

impl Sampler {
    /// The frames decoded for `playing`, if any.
    pub fn detail(&self, playing: Option<&PathBuf>) -> Option<Arc<Detail>> {
        match &self.detail {
            DetailRead::Ready { path, detail } if Some(path) == playing => Some(detail.clone()),
            _ => None,
        }
    }

    /// The range's start and end on `playing`, either of which may be unset.
    pub fn range_ends(&self, playing: Option<&PathBuf>) -> (Option<u64>, Option<u64>) {
        match &self.range {
            Some(r) if Some(&r.path) == playing => (r.start, r.end),
            _ => (None, None),
        }
    }

    /// The range on `playing`, once both ends are set.
    pub fn range(&self, playing: Option<&PathBuf>) -> Option<(u64, u64)> {
        match self.range_ends(playing) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        }
    }

    /// The frame the view centres on while fitting: the picked end, or the
    /// range's middle. `None` for the playhead.
    pub fn centre(&self, playing: Option<&PathBuf>) -> Option<u64> {
        if !self.fit {
            return None;
        }
        let (start, end) = self.range_ends(playing);
        match (self.fit_edge, self.edge) {
            (true, Edge::Start) => start,
            (true, Edge::End) => end,
            (false, _) => start.zip(end).map(|(a, b)| a + (b - a) / 2),
        }
    }

    /// Sets the range's start on `path`, dropping an end not after it.
    pub fn set_range_start(&mut self, path: &PathBuf, frame: u64) {
        let (_, end) = self.range_ends(Some(path));
        self.range = Some(Range {
            path: path.clone(),
            start: Some(frame),
            end: end.filter(|&e| e > frame),
        });
    }

    /// Sets the range's end on `path`, dropping a start not before it.
    pub fn set_range_end(&mut self, path: &PathBuf, frame: u64) {
        let (start, _) = self.range_ends(Some(path));
        self.range = Some(Range {
            path: path.clone(),
            start: start.filter(|&s| s < frame),
            end: Some(frame),
        });
    }
}

/// One end of the range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Edge {
    #[default]
    Start,
    End,
}

impl Edge {
    /// The name `:edge` takes.
    pub fn name(self) -> &'static str {
        match self {
            Edge::Start => "start",
            Edge::End => "end",
        }
    }
}

/// A stretch of one track to slice, in source frames, end exclusive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub path: PathBuf,
    pub start: Option<u64>,
    pub end: Option<u64>,
}

/// The columns of a drawn sampler view: from frame `start`, `per_column`
/// frames a column or `per_frame` columns a frame, one of them 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scale {
    pub start: u64,
    pub per_column: u64,
    pub per_frame: u64,
    pub columns: u64,
}

impl Scale {
    /// The frames `nudge` moves; at least one, and a percentage at least a column.
    pub fn frames(self, nudge: Nudge) -> i64 {
        let frames = |columns: i64| {
            let f = columns * self.per_column as i64 / self.per_frame.max(1) as i64;
            f.max(1)
        };
        match nudge {
            Nudge::Columns(n) => n.signum() * frames(n.abs()),
            Nudge::Percent(p) => {
                let shown = self.columns.max(1) as i64;
                let columns = (shown * p.abs() / 100).clamp(1, shown);
                p.signum() * frames(columns)
            }
        }
    }

    /// The frames shown, end exclusive, unclamped to the track.
    pub fn shown(self) -> (u64, u64) {
        (
            self.start,
            self.start + frames_shown(self.columns, self.per_column, self.per_frame),
        )
    }

    /// Whether columns this fine need decoded frames.
    pub fn needs_detail(self) -> bool {
        self.per_column < DETAIL_BELOW
    }
}

/// Frames `columns` columns show at `per_column` frames a column or
/// `per_frame` columns a frame.
fn frames_shown(columns: u64, per_column: u64, per_frame: u64) -> u64 {
    if per_frame > 1 {
        columns.div_ceil(per_frame)
    } else {
        per_column * columns
    }
}

/// `time` into a track at `rate`, in frames, rounded as marks are.
pub fn frame_of(time: Duration, rate: u32) -> u64 {
    (time.as_secs_f64() * rate as f64).round() as u64
}

/// Frame `frame` of a track at `rate`, as a time.
pub fn time_of(frame: u64, rate: u32) -> Duration {
    Duration::from_secs_f64(frame as f64 / rate.max(1) as f64)
}

/// `frame`, moved to the nearest zero crossing within [`SNAP_WITHIN`], if any.
pub fn snap(peaks: &Peaks, frame: u64) -> u64 {
    let within = frame_of(SNAP_WITHIN, peaks.rate);
    peaks
        .crossing(frame.saturating_sub(within), frame + within, frame)
        .unwrap_or(frame)
}

/// Where the playhead lands moving `step` frames from `from`, inside the
/// track. With `snap`, it lands on the crossing nearest that point within
/// [`SNAP_WITHIN`] and past `from`, so repeated nudges cannot stay put.
pub fn nudge(peaks: &Peaks, from: u64, step: i64, snap: bool) -> u64 {
    let to = from
        .saturating_add_signed(step)
        .min(peaks.frames.saturating_sub(1));
    if !snap || step == 0 {
        return to;
    }
    let within = frame_of(SNAP_WITHIN, peaks.rate);
    let (lo, hi) = if step > 0 {
        ((from + 1).max(to.saturating_sub(within)), to + within)
    } else {
        (
            to.saturating_sub(within),
            (to + within).min(from.saturating_sub(1)),
        )
    };
    peaks.crossing(lo, hi, to).unwrap_or(to)
}

/// The first frame shown, the frames a column shows, and the columns a frame
/// takes, for `width` columns of a track of `frames` zoomed `zoom` steps,
/// centred on frame `at`. Past one frame a column, each step doubles the
/// columns a frame takes, up to `most_per_frame`, a power of two. Returns the
/// zoom clamped to the steps that change the view.
///
/// Columns drawn from peaks start on whole buckets, so each column's peaks
/// hold its own frames and no others.
pub fn window(
    frames: u64,
    width: u64,
    zoom: u32,
    at: u64,
    most_per_frame: u64,
) -> (u64, u64, u64, u32) {
    let bucket = playr_core::wave::BUCKET;
    let width = width.max(1);
    let fit = frames.div_ceil(width).max(1);
    // Steps to one frame a column, then to the most columns a frame.
    let to_one = fit.ilog2();
    let zoom = zoom.min(to_one + most_per_frame.max(1).ilog2());
    let (per_column, per_frame) = match zoom.checked_sub(to_one) {
        Some(past) if past > 0 => (1, 1 << past),
        _ => match fit >> zoom {
            fine if fine < DETAIL_BELOW => (fine, 1),
            coarse => (coarse.next_multiple_of(bucket), 1),
        },
    };
    let shown = frames_shown(width, per_column, per_frame);
    (
        start_at(frames, shown, per_column, at),
        per_column,
        per_frame,
        zoom,
    )
}

/// The first frame of `shown` frames of a track of `frames`, centred on `at`.
fn start_at(frames: u64, shown: u64, per_column: u64, at: u64) -> u64 {
    if shown >= frames {
        return 0;
    }
    let start = at.saturating_sub(shown / 2).min(frames - shown);
    if per_column >= DETAIL_BELOW {
        let bucket = playr_core::wave::BUCKET;
        start / bucket * bucket
    } else {
        start
    }
}

/// The deepest zoom at which `width` columns of a track of `frames` show
/// `len` frames centred, with a column to spare either side for bucket
/// alignment. Drawing clamps it as it clamps any zoom.
pub fn zoom_to_fit(frames: u64, width: u64, len: u64) -> u32 {
    let width = width.max(1);
    let mut zoom = 0;
    loop {
        let (_, per_column, per_frame, next) = window(frames, width, zoom + 1, 0, u64::MAX);
        let shown = frames_shown(width, per_column, per_frame);
        if next == zoom || shown < len + 2 * per_column {
            return zoom;
        }
        zoom = next;
    }
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
    /// The first frame shown, the frames each column shows, and the columns
    /// each frame takes; one of the last two is 1.
    pub start: u64,
    pub per_column: u64,
    pub per_frame: u64,
    /// The zoom the view shows, clamped to the steps that change it.
    pub zoom: u32,
    /// The playhead, in frames.
    pub at: u64,
    /// Marks in the track, in frames.
    pub marks: Vec<u64>,
    /// The range when both ends are set, or else the region around the
    /// playhead, end exclusive.
    pub region: (u64, u64),
    /// The range's ends, as set.
    pub range: (Option<u64>, Option<u64>),
    /// The loudest sample in the track, which the linear displays scale to.
    pub loudest: f32,
    /// Decoded frames, which columns finer than the peaks read where they cover.
    pub detail: Option<Arc<Detail>>,
}

impl Layout {
    /// The layout of `columns` columns of `peaks`, zoomed `zoom` steps around
    /// `position`, with `marks` as times into the track, and at most
    /// `most_per_frame` columns a frame.
    pub fn new(
        peaks: Arc<Peaks>,
        columns: u64,
        zoom: u32,
        position: Duration,
        marks: &[Duration],
        most_per_frame: u64,
    ) -> Layout {
        let rate = peaks.rate.max(1);
        let frame_of = |d: Duration| frame_of(d, rate);
        let at = frame_of(position);
        let columns = columns.max(1);
        let (start, per_column, per_frame, zoom) =
            window(peaks.frames, columns, zoom, at, most_per_frame);
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
            per_frame,
            zoom,
            at,
            marks,
            region,
            range: (None, None),
            loudest,
            detail: None,
        }
    }

    /// The layout reading `detail` where it covers a column.
    pub fn with_detail(mut self, detail: Option<Arc<Detail>>) -> Layout {
        self.detail = detail;
        self
    }

    /// The layout centred on `centre` rather than the playhead, when given.
    pub fn with_centre(mut self, centre: Option<u64>) -> Layout {
        if let Some(at) = centre {
            let shown = frames_shown(self.columns, self.per_column, self.per_frame);
            self.start = start_at(self.peaks.frames, shown, self.per_column, at);
        }
        self
    }

    /// The layout with the range's ends; a range with both replaces the region.
    pub fn with_range(mut self, (start, end): (Option<u64>, Option<u64>)) -> Layout {
        self.range = (start, end);
        if let (Some(a), Some(b)) = (start, end) {
            self.region = (a, b);
        }
        self
    }

    /// The columns, for a nudge to count in.
    pub fn columns(&self) -> Scale {
        Scale {
            start: self.start,
            per_column: self.per_column,
            per_frame: self.per_frame,
            columns: self.columns,
        }
    }

    /// The frames column `c` shows, end exclusive: one frame for several
    /// columns when a frame takes more than one.
    pub fn span_of(&self, c: usize) -> (u64, u64) {
        if self.per_frame > 1 {
            let f = self.start + c as u64 / self.per_frame;
            return (f, f + 1);
        }
        let a = self.start + c as u64 * self.per_column;
        (a, a + self.per_column)
    }

    /// The column showing `frame`, the first of its columns, if one does.
    pub fn column_of(&self, frame: u64) -> Option<usize> {
        let shown = frames_shown(self.columns, self.per_column, self.per_frame);
        (frame >= self.start && frame < self.start + shown)
            .then(|| ((frame - self.start) * self.per_frame / self.per_column) as usize)
    }

    /// The column under the playhead, if it is in view.
    pub fn playhead(&self) -> Option<usize> {
        self.column_of(self.at)
    }

    /// Which side of the view the playhead is past: `Less` before it,
    /// `Greater` after it, `None` in view.
    pub fn playhead_off(&self) -> Option<std::cmp::Ordering> {
        if self.at < self.start {
            Some(std::cmp::Ordering::Less)
        } else if self.playhead().is_none() {
            Some(std::cmp::Ordering::Greater)
        } else {
            None
        }
    }

    /// Whether column `c` shows any of the region.
    pub fn in_region(&self, c: usize) -> bool {
        let (a, b) = self.span_of(c);
        a < self.region.1 && b > self.region.0
    }

    /// The last frame shown, exclusive.
    pub fn end(&self) -> u64 {
        let shown = frames_shown(self.columns, self.per_column, self.per_frame);
        (self.start + shown).min(self.peaks.frames)
    }

    /// The extent of frames `a..b`: exact from the decoded frames where they
    /// cover a column finer than the peaks, and from the peaks otherwise.
    fn range(&self, a: u64, b: u64) -> Option<Extent> {
        match &self.detail {
            Some(d) if self.per_column < DETAIL_BELOW && d.covers(a, b) => d.range(a, b),
            _ => self.peaks.range(a, b),
        }
    }

    /// Column `c`'s RMS and peak levels, as heights from 0 to 1: scaled to the
    /// loudest sample for the envelope, and on the dB scale for the dB display.
    pub fn heights(&self, display: Display, c: usize) -> (f32, f32) {
        let height = |magnitude: f32| match display {
            Display::Decibels => db_height(magnitude),
            _ => magnitude / self.loudest,
        };
        let (a, b) = self.span_of(c);
        self.range(a, b).map_or((0.0, 0.0), |e| {
            (height(e.rms), height(e.min.abs().max(e.max.abs())))
        })
    }

    /// The lowest and highest sample in frames `a` to `b`, from -1 to 1 of the
    /// loudest sample, or `(1, -1)`, an empty extent, where there are none.
    pub fn extent(&self, a: u64, b: u64) -> (f32, f32) {
        self.range(a, b).map_or((1.0, -1.0), |e| {
            (e.min / self.loudest, e.max / self.loudest)
        })
    }

    /// Column `c`'s spectrogram in `rows` rows, lowest frequency first, each
    /// the loudest of its bands from 0, [`SPECTRUM_RANGE_DB`] below the
    /// track's loudest level, to 1 at it. Empty where the column holds no frames.
    pub fn spectrum(&self, c: usize, rows: usize) -> Vec<f32> {
        let spectrum = &self.peaks.spectrum;
        let (a, b) = self.span_of(c);
        let Some(bands) = spectrum.column(a, b.min(self.peaks.frames)) else {
            return Vec::new();
        };
        let top = spectrum.loudest();
        (0..rows)
            .map(|r| {
                let lo = r * BANDS / rows;
                let hi = ((r + 1) * BANDS / rows).max(lo + 1);
                let db = bands[lo..hi].iter().copied().fold(f32::MIN, f32::max);
                ((db - top) / SPECTRUM_RANGE_DB + 1.0).clamp(0.0, 1.0)
            })
            .collect()
    }

    /// The time at `columns` columns from the left edge, which may fall
    /// between columns.
    pub fn time_at(&self, columns: f32) -> Duration {
        let frame = self.start as f64
            + columns.max(0.0) as f64 * self.per_column as f64 / self.per_frame as f64;
        let frame = frame.min(self.peaks.frames as f64);
        Duration::from_secs_f64(frame / self.rate as f64)
    }

    /// The time a column shows: `12 ms`, `1.5 ms`, `32 frames`, `1 frame`, or
    /// the share of a frame: `1/8 frame`.
    pub fn scale(&self) -> String {
        let ms = self.per_column as f64 * 1000.0 / self.rate as f64;
        if self.per_frame > 1 {
            format!("1/{} frame", self.per_frame)
        } else if self.per_column == 1 {
            "1 frame".into()
        } else if self.per_column < DETAIL_BELOW {
            format!("{} frames", self.per_column)
        } else if ms < 10.0 {
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

    /// The region or range and the number of marks:
    /// `region 0:52.310-1:04.870 (12.560 s)  marks 2`.
    pub fn region_text(&self) -> String {
        let (a, b) = self.region;
        let name = match self.range {
            (Some(_), Some(_)) => "range",
            _ => "region",
        };
        format!(
            "{name} {}-{} ({:.3} s)  marks {}",
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
