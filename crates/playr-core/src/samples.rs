//! Samples: regions of a track, cut into slices and written as WAV files.
//!
//! A slice is read from the source file, not captured from the output, so it
//! carries no volume, varispeed or resampling. Positions are source frames,
//! as marks are. Each export writes one directory in the layout rtrack's
//! sample loader reads: `000-name_S00.wav`, `001-name_S01.wav`, and a
//! `samples.json` recording where each slice came from.
//!
//! Files are 24-bit integer WAV at the source's rate and channel count. The
//! decoder divides 16- and 24-bit samples by 2^15 and 2^23 on the way to f32,
//! so scaling back by 2^23 restores them exactly. Float and 32-bit sources are
//! quantised to 24 bits without dither.
//!
//! Onset detection is adapted from rtrack's `detect_transients_range`, with the
//! channels averaged to mono instead of assuming stereo.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::audio::decode::AudioStream;

/// How a track is cut. The region is the job's range when it has one, and
/// otherwise the stretch between the marks either side of the playhead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cut {
    /// The region, as one slice.
    Region,
    /// The whole track, or the range, at every mark in it.
    Marks,
    /// The region, in this many equal parts.
    Equal(usize),
    /// The region, at onsets found with this sensitivity, from 0 to 1.
    Onsets(f32),
}

/// What an export does at each slice's edges, against clicks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Edges {
    /// Cut where the edge falls: an exact copy, which may click.
    #[default]
    Exact,
    /// Move each edge to the nearest zero crossing, as the sampler's snap
    /// does, while planning. Still an exact copy between the edges.
    Zero,
    /// Fade each slice in and out as it is written.
    Fade,
}

impl Edges {
    /// Every choice, by the name the settings and `:slice-edges` use.
    pub const NAMES: [(&'static str, Edges); 3] = [
        ("exact", Edges::Exact),
        ("zero", Edges::Zero),
        ("fade", Edges::Fade),
    ];

    pub fn name(self) -> &'static str {
        Edges::NAMES
            .iter()
            .find(|n| n.1 == self)
            .map_or("exact", |n| n.0)
    }
}

/// How long [`Edges::Fade`] fades each end of a slice. Linear, and at most
/// half the slice at either end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fades {
    pub fade_in: Duration,
    pub fade_out: Duration,
}

impl Default for Fades {
    /// A millisecond in, which leaves a hit's attack; 5 ms out, as rtrack
    /// fades a slice's tail when it plays one.
    fn default() -> Self {
        Fades {
            fade_in: Duration::from_millis(1),
            fade_out: Duration::from_millis(5),
        }
    }
}

/// One export: which track, where the playhead and marks are, and how to cut.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub path: PathBuf,
    /// The source rate that `at` and `marks` count frames in.
    pub rate: u32,
    /// Mark positions, in frames.
    pub marks: Vec<u64>,
    /// The playhead, in frames.
    pub at: u64,
    pub cut: Cut,
    /// Frames to cut instead of the region around `at`, end exclusive.
    pub range: Option<(u64, u64)>,
    /// The directory exports are written under.
    pub samples: PathBuf,
    pub edges: Edges,
    pub fades: Fades,
    /// The range is being looped and cut as one slice: `samples.json` marks
    /// that slice to loop whole, and its edges are left as they are.
    pub loops: bool,
}

/// What an export wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct Exported {
    pub dir: PathBuf,
    /// Source frames of each slice written, end exclusive.
    pub slices: Vec<(u64, u64)>,
}

const EMPTY: &str = "nothing to export: the region is empty";

/// Most slices in one export: rtrack's sample bank has 256 slots.
pub const MAX_SLICES: usize = 256;

/// Most frames read into memory to find onsets: about 23 minutes at 48 kHz.
pub const MAX_ONSET_FRAMES: usize = 1 << 26;

/// The region around frame `at`: from the last mark at or before it, or the
/// start, to the first mark after it, or the end (`None`).
pub fn region(marks: &[u64], at: u64) -> (u64, Option<u64>) {
    let start = marks
        .iter()
        .copied()
        .filter(|&m| m <= at)
        .max()
        .unwrap_or(0);
    let end = marks.iter().copied().filter(|&m| m > at).min();
    (start, end)
}

/// Frames read either side of a mark when snapping it to an onset.
pub const SNAP_WINDOW: u64 = 2;

/// The onset nearest `at` in the track at `path`, within [`SNAP_WINDOW`]
/// seconds either side, or `None` when the window holds none.
///
/// The window's own start does not count as an onset: [`onsets`] always opens
/// a slice there, and a mark dragged to the edge of the window is not a snap.
///
/// Only the window is decoded, so this costs milliseconds whatever the track's
/// length. Onsets are found in the window alone, so a rise just outside it is
/// not a candidate; that is the point of snapping to what is in view.
pub fn nearest_onset(
    path: &Path,
    rate: u32,
    at: u64,
    sensitivity: f32,
) -> Result<Option<u64>, String> {
    let span = SNAP_WINDOW * rate.max(1) as u64;
    let start = at.saturating_sub(span);
    let detail = crate::wave::Detail::read(path, rate, start, at + span)?;
    let held = detail.end.min(at + span);
    let mono: Vec<f32> = (start..held).filter_map(|f| detail.mean(f)).collect();
    // `onsets` opens the first slice at 0 whether or not anything rises there,
    // so the window's own start is not a candidate: snapping to it would drag
    // the mark to wherever the window happened to begin.
    Ok(onsets(&mono, rate, sensitivity)
        .into_iter()
        .skip_while(|&i| i == 0)
        .map(|i| start + i as u64)
        .min_by_key(|&f| f.abs_diff(at)))
}

/// `len` frames in `n` equal spans; the last takes the remainder.
pub fn equal_spans(len: u64, n: usize) -> Vec<(u64, u64)> {
    let size = len / n.max(1) as u64;
    if n == 0 || size == 0 {
        return Vec::new();
    }
    (0..n as u64)
        .map(|i| {
            let start = i * size;
            let end = if i + 1 == n as u64 { len } else { start + size };
            (start, end)
        })
        .collect()
}

/// Spans from each point to the next, and from the last to `len`.
pub fn spans_at(points: &[u64], len: u64) -> Vec<(u64, u64)> {
    let ends = points.iter().skip(1).copied().chain([len]);
    points
        .iter()
        .copied()
        .zip(ends)
        .filter(|(start, end)| start < end)
        .collect()
}

/// Onsets in `mono` audio at `rate`, as frame offsets, always starting with 0.
///
/// The detector works on the energy envelope in dB, so a rise from -60 to
/// -50 dB counts as much as one from -20 to -10 dB. Each rise is compared
/// with the rises around it, not the loudest in the region, so one loud hit
/// does not hide quieter ones. Only the crest of a rise counts, onsets are at
/// least 50 ms apart, so a hit that close to the start stays in the first
/// slice. Each onset is moved back to the quietest frame within one 5 ms
/// window, so a slice starts up to 10 ms before its attack rather than
/// partway up it. Higher `sensitivity` finds more.
pub fn onsets(mono: &[f32], rate: u32, sensitivity: f32) -> Vec<usize> {
    let rate = rate as usize;
    let window = (rate / 200).max(16);
    let hop = (window / 2).max(1);

    let mut energies = Vec::new();
    let mut pos = 0;
    while pos + window <= mono.len() {
        let sum: f32 = mono[pos..pos + window].iter().map(|s| s * s).sum();
        let rms = (sum / window as f32).sqrt();
        // Floored at -120 dB, so silence gives no infinite rises.
        energies.push(20.0 * rms.max(1e-6).log10());
        pos += hop;
    }
    if energies.len() < 3 {
        return vec![0];
    }

    let mut flux = vec![0.0f32];
    flux.extend(energies.windows(2).map(|w| (w[1] - w[0]).max(0.0)));

    let quiet = (1.0 - sensitivity.clamp(0.0, 1.0)).powi(2);
    let alpha = 1.0 + 3.0 * quiet;
    let floor_db = 1.0 + 12.0 * quiet;
    let avg_half = (rate / 10 / hop).max(2);
    let min_gap = (rate / 20).max(1);

    let mut points = vec![0];
    for i in 1..flux.len() - 1 {
        let f = flux[i];
        if f < floor_db || f < flux[i - 1] || f < flux[i + 1] {
            continue;
        }
        let lo = i.saturating_sub(avg_half);
        let hi = (i + avg_half + 1).min(flux.len());
        let local_mean = flux[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
        if f < local_mean * alpha {
            continue;
        }
        let frame = i * hop;
        let previous = *points.last().expect("starts with 0");
        if frame <= previous + min_gap || frame >= mono.len() {
            continue;
        }
        let limit = frame.saturating_sub(window).max(previous + 1);
        let onset = (limit..=frame)
            .min_by(|&a, &b| mono[a].abs().total_cmp(&mono[b].abs()))
            .unwrap_or(frame);
        if onset > previous + min_gap / 2 {
            points.push(onset);
        }
    }
    points
}

/// A slice's first frame and its end, exclusive, or `None` for the end of the track.
pub type Span = (u64, Option<u64>);

/// Slices planned for a job, to be written with [`write`] or dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub job: Job,
    pub spans: Vec<Span>,
}

/// Frames `start..end` of the track at `path`, which counts frames at `rate`,
/// interleaved, with the channel count. Fewer where the track ends first.
pub(crate) fn read_frames(
    path: &Path,
    rate: u32,
    start: u64,
    end: u64,
) -> Result<(Vec<f32>, usize), String> {
    Reader::open(path, rate, start)?.frames_to(end)
}

/// Runs `job`, returning the directory written and the slices in it.
pub fn export(job: &Job) -> Result<Exported, String> {
    write(job, &plan(job)?)
}

/// The audio onsets were last found in, averaged to mono, so finding them
/// again at another sensitivity reads nothing: what makes onset slicing
/// follow a slider.
#[derive(Debug, Default)]
pub struct OnsetAudio(Mutex<Option<Read>>);

/// Frames `start..end` of the track at `path`, as mono.
#[derive(Debug)]
struct Read {
    path: PathBuf,
    start: u64,
    end: Option<u64>,
    mono: Arc<Vec<f32>>,
}

impl OnsetAudio {
    /// Frames `start..end` of the track at `path`, as mono, read once.
    fn mono(
        &self,
        path: &Path,
        rate: u32,
        start: u64,
        end: Option<u64>,
    ) -> Result<Arc<Vec<f32>>, String> {
        // Held while reading, so a second job for the same audio waits for it.
        let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(read) = held.as_ref() {
            if (read.path.as_path(), read.start, read.end) == (path, start, end) {
                return Ok(read.mono.clone());
            }
        }
        let mono = Arc::new(Reader::open(path, rate, start)?.mono_to(end)?);
        *held = Some(Read {
            path: path.to_path_buf(),
            start,
            end,
            mono: mono.clone(),
        });
        Ok(mono)
    }
}

/// The slices `job` would write, without writing them. Finding onsets or the
/// length of an open region decodes the track, so this can take seconds.
pub fn plan(job: &Job) -> Result<Vec<Span>, String> {
    plan_with(job, &OnsetAudio::default())
}

/// As [`plan`], finding onsets in `audio` when it holds the region already.
pub fn plan_with(job: &Job, audio: &OnsetAudio) -> Result<Vec<Span>, String> {
    let (start, end) = match job.range {
        Some((a, b)) => (a, Some(b)),
        None => region(&job.marks, job.at),
    };
    let spans: Vec<Span> = match job.cut {
        Cut::Region => vec![(start, end)],
        Cut::Marks if job.range.is_some() => {
            let mut points: Vec<u64> = job
                .marks
                .iter()
                .copied()
                .filter(|&m| m > start && end.is_none_or(|e| m < e))
                .collect();
            if points.is_empty() {
                return Err("no marks in the range".into());
            }
            points.sort_unstable();
            points.dedup();
            points.insert(0, start);
            let ends = points.iter().skip(1).map(|&e| Some(e)).chain([end]);
            points.iter().copied().zip(ends).collect()
        }
        Cut::Marks => {
            let mut points: Vec<u64> = job.marks.iter().copied().filter(|&m| m > 0).collect();
            if points.is_empty() {
                return Err("no marks in this track".into());
            }
            points.sort_unstable();
            points.dedup();
            points.insert(0, 0);
            let ends = points.iter().skip(1).map(|&e| Some(e)).chain([None]);
            points.iter().copied().zip(ends).collect()
        }
        Cut::Equal(n) => {
            let len = match end {
                Some(end) => end - start,
                None => Reader::open(&job.path, job.rate, start)?.count_to(None)?,
            };
            let spans = equal_spans(len, n);
            if spans.is_empty() {
                return Err(format!("the region is too short for {n} slices"));
            }
            spans
                .into_iter()
                .map(|(s, e)| (start + s, Some(start + e)))
                .collect()
        }
        Cut::Onsets(sensitivity) => {
            let mono = audio.mono(&job.path, job.rate, start, end)?;
            let points: Vec<u64> = onsets(&mono, job.rate, sensitivity)
                .into_iter()
                .map(|p| p as u64)
                .collect();
            spans_at(&points, mono.len() as u64)
                .into_iter()
                .map(|(s, e)| (start + s, Some(start + e)))
                .collect()
        }
    };
    if spans.len() > MAX_SLICES {
        return Err(format!(
            "{} slices is more than the {MAX_SLICES} a sample bank holds",
            spans.len()
        ));
    }
    if job.edges == Edges::Zero && !job.loops {
        return snap_edges(job, spans);
    }
    Ok(spans)
}

/// `spans` with each edge on the nearest zero crossing within
/// [`crate::wave::SNAP_WITHIN`], found as the sampler's snap finds it. The
/// track's own start and end stay. An edge whose crossing would reach a
/// neighbouring edge stays too, so every planned slice is written.
fn snap_edges(job: &Job, spans: Vec<Span>) -> Result<Vec<Span>, String> {
    let mut edges: Vec<u64> = spans
        .iter()
        .flat_map(|&(start, end)| [Some(start), end])
        .flatten()
        .filter(|&e| e > 0)
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let reach = crate::wave::snap_reach(job.rate);
    let mut moved = Vec::with_capacity(edges.len());
    let mut before = 0;
    for (i, &edge) in edges.iter().enumerate() {
        let after = edges.get(i + 1).copied().unwrap_or(u64::MAX);
        let to = Some(crossing_near(job, edge, reach)?)
            .filter(|&to| to > before && to < after)
            .unwrap_or(edge);
        moved.push(to);
        before = to;
    }
    let to = |e: u64| match edges.binary_search(&e) {
        Ok(i) => moved[i],
        Err(_) => e,
    };
    Ok(spans.into_iter().map(|(s, e)| (to(s), e.map(to))).collect())
}

/// The zero crossing nearest `edge` within `reach` frames, or `edge`.
fn crossing_near(job: &Job, edge: u64, reach: u64) -> Result<u64, String> {
    // From the frame before the first that can cross, which it is compared with.
    let first = edge.saturating_sub(reach).max(1) - 1;
    let (samples, channels) = read_frames(&job.path, job.rate, first, edge + reach + 1)?;
    let signs: Vec<bool> = samples
        .chunks_exact(channels.max(1))
        .map(crate::wave::non_negative)
        .collect();
    let Some(last) = (signs.len() as u64).checked_sub(1).map(|n| first + n) else {
        return Ok(edge);
    };
    let hi = (edge + reach).min(last);
    let at = |f: u64| signs[(f - first) as usize];
    Ok(crate::wave::nearest_crossing(first + 1, hi, edge, at).unwrap_or(edge))
}

/// Writes `spans` of `job`'s track, as [`plan`] returned them, to a new
/// directory under `job.samples`.
pub fn write(job: &Job, spans: &[Span]) -> Result<Exported, String> {
    let Some(&(first, _)) = spans.first() else {
        return Err(EMPTY.into());
    };
    // Opened before the directory is made, so a track that will not open
    // leaves nothing behind.
    let reader = Reader::open(&job.path, job.rate, first)?;
    let stem = name_for(&job.path);
    fs::create_dir_all(&job.samples)
        .map_err(|e| format!("cannot create {}: {e}", job.samples.display()))?;
    let fading = job.edges == Edges::Fade && !job.loops;
    // A fade out needs to know where the slice ends.
    let mut spans = spans.to_vec();
    if fading {
        if let Some(last) = spans.last_mut().filter(|s| s.1.is_none()) {
            let frames = Reader::open(&job.path, job.rate, last.0)?.count_to(None)?;
            last.1 = Some(last.0 + frames);
        }
    }
    let fades = fading.then(|| {
        let frames = |d: Duration| (d.as_secs_f64() * job.rate as f64).round() as u64;
        (frames(job.fades.fade_in), frames(job.fades.fade_out))
    });
    let dir = unused_dir(&job.samples, &stem)?;
    let written = reader.write(&spans, &dir, &stem, fades);
    let slices = match written {
        Ok(slices) if slices.is_empty() => Err(EMPTY.into()),
        other => other,
    };
    let slices = match slices {
        Ok(slices) => slices,
        Err(e) => {
            // A partial export would load into rtrack as if it were whole.
            let _ = fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    let looped = job.loops && slices.len() == 1;
    fs::write(
        dir.join("samples.json"),
        metadata(&job.path, job.rate, &slices, looped),
    )
    .map_err(|e| format!("cannot write samples.json: {e}"))?;
    Ok(Exported { dir, slices })
}

/// A decoder positioned at a source frame, handing out frames in order.
struct Reader {
    stream: AudioStream,
    rate: u32,
    /// The source frame of the next frame handed out.
    frame: u64,
    /// The chunk being handed out, copied so the decoder can be asked its spec.
    buf: Vec<f32>,
}

impl Reader {
    /// The track at `path`, which counts frames at `rate`, from frame `start`.
    fn open(path: &Path, rate: u32, start: u64) -> Result<Reader, String> {
        let fail = |e: crate::audio::decode::DecodeError| format!("{}: {e}", path.display());
        let mut stream = AudioStream::open(path).map_err(fail)?;
        if start > 0 {
            // Exact: a nanosecond of rounding is far below half a frame.
            let at = Duration::from_nanos(start * 1_000_000_000 / rate as u64);
            if stream.duration().is_some_and(|d| at >= d) {
                return Err(EMPTY.into());
            }
            stream.seek(at).map_err(fail)?;
        }
        Ok(Reader {
            stream,
            rate,
            frame: start,
            buf: Vec::new(),
        })
    }

    /// Calls `each` with every chunk up to `end`, or the end of the track, and
    /// the channel count. Stops early when `each` returns false.
    fn chunks(
        &mut self,
        end: Option<u64>,
        mut each: impl FnMut(u64, &[f32], usize) -> Result<bool, String>,
    ) -> Result<(), String> {
        while end.is_none_or(|end| self.frame < end) {
            match self.stream.next_chunk().map_err(|e| e.to_string())? {
                Some(chunk) => {
                    self.buf.clear();
                    self.buf.extend_from_slice(chunk);
                }
                None => return Ok(()),
            }
            let spec = self.stream.spec();
            if spec.rate != self.rate {
                return Err(format!(
                    "the file plays at {} Hz, but its marks count {} Hz",
                    spec.rate, self.rate
                ));
            }
            let channels = spec.channels.max(1) as usize;
            let frames = (self.buf.len() / channels) as u64;
            let take = end.map_or(frames, |end| frames.min(end - self.frame));
            let chunk = &self.buf[..take as usize * channels];
            let at = self.frame;
            self.frame += take;
            if !each(at, chunk, channels)? {
                return Ok(());
            }
        }
        Ok(())
    }

    /// The frames from here to `end`, or the end of the track, interleaved,
    /// with the channel count.
    fn frames_to(mut self, end: u64) -> Result<(Vec<f32>, usize), String> {
        let (mut samples, mut count) = (Vec::new(), 1);
        self.chunks(Some(end), |_, chunk, channels| {
            samples.extend_from_slice(chunk);
            count = channels;
            Ok(true)
        })?;
        Ok((samples, count))
    }

    /// Frames from here to `end`, or the end of the track.
    fn count_to(mut self, end: Option<u64>) -> Result<u64, String> {
        let start = self.frame;
        self.chunks(end, |_, _, _| Ok(true))?;
        Ok(self.frame - start)
    }

    /// The audio from here to `end`, with its channels averaged.
    fn mono_to(mut self, end: Option<u64>) -> Result<Vec<f32>, String> {
        let mut mono = Vec::new();
        self.chunks(end, |_, chunk, channels| {
            if mono.len() + chunk.len() / channels > MAX_ONSET_FRAMES {
                return Err("the region is too long to find onsets in; mark a shorter one".into());
            }
            let scale = 1.0 / channels as f32;
            mono.extend(
                chunk
                    .chunks_exact(channels)
                    .map(|f| f.iter().sum::<f32>() * scale),
            );
            Ok(true)
        })?;
        Ok(mono)
    }

    /// Writes each span to its own file in `dir`, in one pass. A span ending at
    /// `None` runs to the end of the track. With `fades`, frames to fade in and
    /// out, every span must have an end. Returns the spans written, as far as
    /// the track went.
    fn write(
        mut self,
        spans: &[Span],
        dir: &Path,
        stem: &str,
        fades: Option<(u64, u64)>,
    ) -> Result<Vec<(u64, u64)>, String> {
        let mut written: Vec<(u64, u64)> = Vec::new();
        let mut open: Option<hound::WavWriter<std::io::BufWriter<fs::File>>> = None;
        let mut index = 0;
        let fail = |e: hound::Error| format!("cannot write a slice: {e}");
        let last_end = spans.last().and_then(|s| s.1);
        let rate = self.rate;

        self.chunks(last_end, |mut at, mut chunk, channels| {
            while !chunk.is_empty() && index < spans.len() {
                let (start, end) = spans[index];
                if at < start {
                    let skip = ((start - at) as usize).min(chunk.len() / channels);
                    chunk = &chunk[skip * channels..];
                    at += skip as u64;
                    continue;
                }
                let frames = (chunk.len() / channels) as u64;
                let take = end.map_or(frames, |end| frames.min(end - at));
                if open.is_none() {
                    let slot = written.len();
                    let file = dir.join(format!("{slot:03}-{stem}_S{slot:02}.wav"));
                    let spec = hound::WavSpec {
                        channels: channels as u16,
                        sample_rate: rate,
                        bits_per_sample: 24,
                        sample_format: hound::SampleFormat::Int,
                    };
                    open = Some(hound::WavWriter::create(file, spec).map_err(fail)?);
                    written.push((start, start));
                }
                let writer = open.as_mut().expect("opened above");
                let (first, len) = (at - start, end.map_or(0, |e| e - start));
                for (k, frame) in chunk[..take as usize * channels]
                    .chunks_exact(channels)
                    .enumerate()
                {
                    let gain = fades.map_or(1.0, |f| fade_gain(first + k as u64, len, f));
                    for &s in frame {
                        let v = (s * gain * 8_388_608.0)
                            .round()
                            .clamp(-8_388_608.0, 8_388_607.0);
                        writer.write_sample(v as i32).map_err(fail)?;
                    }
                }
                at += take;
                chunk = &chunk[take as usize * channels..];
                written.last_mut().expect("pushed above").1 = at;
                if end == Some(at) {
                    open.take()
                        .expect("opened above")
                        .finalize()
                        .map_err(fail)?;
                    index += 1;
                }
            }
            Ok(index < spans.len())
        })?;
        if let Some(writer) = open {
            writer.finalize().map_err(fail)?;
        }
        Ok(written)
    }
}

/// The gain of frame `i` of a slice `len` frames long, fading in over
/// `fade_in` frames from 0 and out over `fade_out` frames to 0, each at most
/// half the slice.
pub(crate) fn fade_gain(i: u64, len: u64, (fade_in, fade_out): (u64, u64)) -> f32 {
    let ramp = |from_edge: u64, fade: u64| {
        let fade = fade.min(len / 2);
        if from_edge < fade {
            from_edge as f32 / fade as f32
        } else {
            1.0
        }
    };
    ramp(i, fade_in).min(ramp(len.saturating_sub(1 + i), fade_out))
}

/// The track's file name without its extension, safe as part of a file name.
fn name_for(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let name: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " _.-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    match name.trim() {
        "" => "slice".into(),
        name => name.into(),
    }
}

/// `parent/name`, or `parent/name-2` and so on, whichever does not exist yet.
fn unused_dir(parent: &Path, name: &str) -> Result<PathBuf, String> {
    for n in 1.. {
        let dir = match n {
            1 => parent.join(name),
            n => parent.join(format!("{name}-{n}")),
        };
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("cannot create {}: {e}", dir.display())),
        }
    }
    unreachable!("an unused name exists")
}

/// `samples.json`: rtrack reads the `samples` keys, which must match the slot
/// text in the file names, and the loop fields, counted in the slice's own
/// frames; it ignores the provenance fields. With `looped`, each slice loops
/// whole.
fn metadata(source: &Path, rate: u32, slices: &[(u64, u64)], looped: bool) -> String {
    let entries: Vec<String> = slices
        .iter()
        .enumerate()
        .map(|(slot, (start, end))| {
            let lp = match looped {
                true => format!(
                    ", \"loop_enabled\": true, \"loop_start\": 0, \"loop_end\": {}",
                    end - start
                ),
                false => String::new(),
            };
            format!("    \"{slot:03}\": {{ \"start_frame\": {start}, \"end_frame\": {end}{lp} }}")
        })
        .collect();
    format!(
        "{{\n  \"source\": {},\n  \"sample_rate\": {rate},\n  \"samples\": {{\n{}\n  }}\n}}\n",
        json_string(&source.to_string_lossy()),
        entries.join(",\n")
    )
}

fn json_string(s: &str) -> String {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
