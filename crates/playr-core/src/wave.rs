//! Waveform peaks: the lowest and highest sample and the mean square in each
//! stretch of a track, kept at several scales so a view at any zoom reads a
//! few values a column.
//!
//! The finest scale holds one entry per [`BUCKET`] frames, across all channels;
//! each coarser scale halves the count. An entry is 12 bytes, so a 4-minute
//! track at 44.1 kHz keeps about 4 MB at the finest scale and as much again
//! above it. Peaks show where a signal reaches; the mean square gives its RMS,
//! which shows loudness where a mastered track's peaks are all near full scale.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::audio::decode::AudioStream;

/// Frames per entry at the finest scale.
pub const BUCKET: u64 = 32;

/// The extremes and RMS of some stretch of frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent {
    pub min: f32,
    pub max: f32,
    /// Root mean square across all channels.
    pub rms: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Entry {
    min: f32,
    max: f32,
    mean_square: f32,
}

/// The peaks of one track.
#[derive(Debug, Clone, PartialEq)]
pub struct Peaks {
    pub rate: u32,
    pub frames: u64,
    /// `levels[k]` holds one entry per `BUCKET << k` frames.
    levels: Vec<Vec<Entry>>,
}

impl Peaks {
    /// The peaks of interleaved `samples` with `channels` channels.
    pub fn from_interleaved(samples: &[f32], channels: usize, rate: u32) -> Peaks {
        let mut builder = Builder::new(rate);
        builder.push(samples, channels.max(1));
        builder.finish()
    }

    /// Decodes the track at `path`. Returns `Ok(None)` if `cancel` is set
    /// before it finishes, as when the track changes.
    pub fn read(path: &Path, cancel: &AtomicBool) -> Result<Option<Peaks>, String> {
        let fail = |e: crate::audio::decode::DecodeError| format!("{}: {e}", path.display());
        let mut stream = AudioStream::open(path).map_err(fail)?;
        let mut builder = None;
        // Copied out, so the decoder can be asked its spec: rate and channels
        // can be unknown until the first chunk decodes.
        let mut chunk = Vec::new();
        while let Some(decoded) = stream.next_chunk().map_err(fail)? {
            chunk.clear();
            chunk.extend_from_slice(decoded);
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let spec = stream.spec();
            builder
                .get_or_insert_with(|| Builder::new(spec.rate))
                .push(&chunk, spec.channels.max(1) as usize);
        }
        let rate = stream.spec().rate;
        Ok(Some(builder.unwrap_or_else(|| Builder::new(rate)).finish()))
    }

    /// The extremes and RMS of frames `start..end`, or `None` if the range
    /// holds no frames. The range is widened to whole [`BUCKET`]s, by at most
    /// `BUCKET - 1` frames at either end.
    pub fn range(&self, start: u64, end: u64) -> Option<Extent> {
        let end = end.min(self.frames);
        if start >= end {
            return None;
        }
        // Whole buckets at the finest scale, gathered from the coarsest scales
        // that fit inside them: a few entries a range, and nothing outside it.
        let (mut lo, mut hi) = ((start / BUCKET) as usize, end.div_ceil(BUCKET) as usize);
        let mut total = Total::default();
        for (k, level) in self.levels.iter().enumerate() {
            if lo >= hi {
                break;
            }
            if lo % 2 == 1 {
                total.add(level[lo], frames_in(self.frames, k, lo));
                lo += 1;
            }
            if hi % 2 == 1 && lo < hi {
                hi -= 1;
                total.add(level[hi], frames_in(self.frames, k, hi));
            }
            lo /= 2;
            hi /= 2;
        }
        total.extent()
    }

    /// The largest sample magnitude in the track, for scaling a view.
    pub fn loudest(&self) -> f32 {
        self.levels
            .last()
            .into_iter()
            .flatten()
            .fold(0.0f32, |m, e| m.max(-e.min).max(e.max))
    }
}

/// Frames covered by entry `i` at scale `k` of a track of `frames`:
/// `BUCKET << k`, or fewer for the entry at the end.
fn frames_in(frames: u64, k: usize, i: usize) -> u64 {
    let size = BUCKET << k;
    let start = i as u64 * size;
    (start + size).min(frames).saturating_sub(start)
}

/// Entries combined, each mean square weighted by the frames it covers.
#[derive(Default)]
struct Total {
    min: f32,
    max: f32,
    squares: f64,
    frames: u64,
}

impl Total {
    fn add(&mut self, e: Entry, frames: u64) {
        if self.frames == 0 {
            (self.min, self.max) = (e.min, e.max);
        } else {
            (self.min, self.max) = (self.min.min(e.min), self.max.max(e.max));
        }
        self.squares += f64::from(e.mean_square) * frames as f64;
        self.frames += frames;
    }

    fn extent(&self) -> Option<Extent> {
        (self.frames > 0).then(|| Extent {
            min: self.min,
            max: self.max,
            rms: (self.squares / self.frames as f64).sqrt() as f32,
        })
    }

    fn entry(&self) -> Entry {
        Entry {
            min: self.min,
            max: self.max,
            mean_square: (self.squares / self.frames.max(1) as f64) as f32,
        }
    }
}

struct Builder {
    rate: u32,
    frames: u64,
    base: Vec<Entry>,
    /// The bucket being filled: extremes, sum of squares, and frames in it.
    min: f32,
    max: f32,
    squares: f64,
    filled: u64,
}

impl Builder {
    fn new(rate: u32) -> Self {
        Builder {
            rate,
            frames: 0,
            base: Vec::new(),
            min: f32::INFINITY,
            max: f32::NEG_INFINITY,
            squares: 0.0,
            filled: 0,
        }
    }

    fn push(&mut self, samples: &[f32], channels: usize) {
        let per_channel = 1.0 / channels as f64;
        for frame in samples.chunks_exact(channels) {
            let mut square = 0.0f64;
            for &s in frame {
                self.min = self.min.min(s);
                self.max = self.max.max(s);
                square += f64::from(s) * f64::from(s);
            }
            self.squares += square * per_channel;
            self.filled += 1;
            self.frames += 1;
            if self.filled == BUCKET {
                self.close();
            }
        }
    }

    fn close(&mut self) {
        self.base.push(Entry {
            min: self.min,
            max: self.max,
            mean_square: (self.squares / self.filled as f64) as f32,
        });
        (self.min, self.max, self.squares, self.filled) =
            (f32::INFINITY, f32::NEG_INFINITY, 0.0, 0);
    }

    fn finish(mut self) -> Peaks {
        if self.filled > 0 {
            self.close();
        }
        let frames = self.frames;
        let mut levels = vec![std::mem::take(&mut self.base)];
        while levels.last().is_some_and(|l| l.len() > 1) {
            let k = levels.len() - 1;
            let coarser = levels[k]
                .chunks(2)
                .enumerate()
                .map(|(pair, entries)| {
                    let mut total = Total::default();
                    for (j, &e) in entries.iter().enumerate() {
                        total.add(e, frames_in(frames, k, 2 * pair + j));
                    }
                    total.entry()
                })
                .collect();
            levels.push(coarser);
        }
        Peaks {
            rate: self.rate,
            frames,
            levels,
        }
    }
}
