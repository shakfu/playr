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
use std::time::Duration;

use crate::audio::decode::AudioStream;

/// How a track is cut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cut {
    /// The region between the marks either side of the playhead, as one slice.
    Region,
    /// The whole track, at every mark.
    Marks,
    /// The region, in this many equal parts.
    Equal(usize),
    /// The region, at onsets found with this sensitivity, from 0 to 1.
    Onsets(f32),
}

/// One export: which track, where the playhead and marks are, and how to cut.
#[derive(Debug, Clone)]
pub struct Job {
    pub path: PathBuf,
    /// The source rate that `at` and `marks` count frames in.
    pub rate: u32,
    /// Mark positions, in frames.
    pub marks: Vec<u64>,
    /// The playhead, in frames.
    pub at: u64,
    pub cut: Cut,
    /// The directory exports are written under.
    pub samples: PathBuf,
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

/// Runs `job`, returning the directory written and the slices in it.
pub fn export(job: &Job) -> Result<Exported, String> {
    write(job, &plan(job)?)
}

/// The slices `job` would write, without writing them. Finding onsets or the
/// length of an open region decodes the track, so this can take seconds.
pub fn plan(job: &Job) -> Result<Vec<Span>, String> {
    let (start, end) = region(&job.marks, job.at);
    let spans: Vec<Span> = match job.cut {
        Cut::Region => vec![(start, end)],
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
                None => Reader::open(job, start)?.count_to(None)?,
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
            let mono = Reader::open(job, start)?.mono_to(end)?;
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

    Ok(spans)
}

/// Writes `spans` of `job`'s track, as [`plan`] returned them, to a new
/// directory under `job.samples`.
pub fn write(job: &Job, spans: &[Span]) -> Result<Exported, String> {
    let Some(&(first, _)) = spans.first() else {
        return Err(EMPTY.into());
    };
    // Opened before the directory is made, so a track that will not open
    // leaves nothing behind.
    let reader = Reader::open(job, first)?;
    let stem = name_for(&job.path);
    fs::create_dir_all(&job.samples)
        .map_err(|e| format!("cannot create {}: {e}", job.samples.display()))?;
    let dir = unused_dir(&job.samples, &stem)?;
    let written = reader.write(spans, &dir, &stem);
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
    fs::write(
        dir.join("samples.json"),
        metadata(&job.path, job.rate, &slices),
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
    fn open(job: &Job, start: u64) -> Result<Reader, String> {
        let fail = |e: crate::audio::decode::DecodeError| format!("{}: {e}", job.path.display());
        let mut stream = AudioStream::open(&job.path).map_err(fail)?;
        if start > 0 {
            // Exact: a nanosecond of rounding is far below half a frame.
            let at = Duration::from_nanos(start * 1_000_000_000 / job.rate as u64);
            if stream.duration().is_some_and(|d| at >= d) {
                return Err(EMPTY.into());
            }
            stream.seek(at).map_err(fail)?;
        }
        Ok(Reader {
            stream,
            rate: job.rate,
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
    /// `None` runs to the end of the track. Returns the spans written, as far
    /// as the track went.
    fn write(mut self, spans: &[Span], dir: &Path, stem: &str) -> Result<Vec<(u64, u64)>, String> {
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
                for &s in &chunk[..take as usize * channels] {
                    let v = (s * 8_388_608.0).round().clamp(-8_388_608.0, 8_388_607.0);
                    writer.write_sample(v as i32).map_err(fail)?;
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
/// text in the file names, and ignores the provenance fields.
fn metadata(source: &Path, rate: u32, slices: &[(u64, u64)]) -> String {
    let entries: Vec<String> = slices
        .iter()
        .enumerate()
        .map(|(slot, (start, end))| {
            format!("    \"{slot:03}\": {{ \"start_frame\": {start}, \"end_frame\": {end} }}")
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
