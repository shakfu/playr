//! A track's spectrogram: the level in each of [`BANDS`] frequency bands for
//! every [`HOP`] frames, read in the same pass as its peaks.
//!
//! Each entry is a [`FFT`]-point Hann-windowed transform of the channels'
//! mean, centred on its hop. Bands are spaced evenly in log frequency from
//! [`LOWEST_HZ`] to half the sample rate. A band holding a bin keeps its
//! loudest, so a narrow tone is not averaged away. A band narrower than a
//! bin, below about 340 Hz at 44.1 kHz, reads the level interpolated between
//! the bins either side of its centre, so neighbouring bands do not repeat one
//! bin as stepped rows. A level is a byte, half a dB a step above [`FLOOR_DB`].
//!
//! Coarser scales keep the loudest of each pair, as the peaks do, so a view
//! at any zoom reads a few entries a column. A 4-minute track at 44.1 kHz
//! keeps about 2.6 MB at the finest scale and as much again above it.

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

/// Points in each transform: 21.5 Hz a bin at 44.1 kHz.
pub const FFT: usize = 2048;

/// Frames between entries: 11.6 ms at 44.1 kHz.
pub const HOP: u64 = 512;

/// Frequency bands an entry holds.
pub const BANDS: usize = 128;

/// The lowest band's lower edge.
pub const LOWEST_HZ: f32 = 20.0;

/// The level a stored 0 stands for, in dBFS; 255 is full scale.
pub const FLOOR_DB: f32 = -127.5;

/// The spectrogram of one track.
#[derive(Clone, PartialEq)]
pub struct Spectrogram {
    pub rate: u32,
    /// The [`BANDS`] + 1 band edges, in Hz.
    edges: Vec<f32>,
    /// `levels[k]` holds [`BANDS`] levels for every `HOP << k` frames, lowest
    /// band first.
    levels: Vec<Vec<u8>>,
    loudest: u8,
}

impl std::fmt::Debug for Spectrogram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spectrogram")
            .field("rate", &self.rate)
            .field("entries", &self.entries())
            .finish()
    }
}

impl Spectrogram {
    /// Entries at the finest scale.
    pub fn entries(&self) -> usize {
        self.levels.first().map_or(0, |l| l.len() / BANDS)
    }

    /// The loudest level of each band over frames `start..end`, lowest band
    /// first, in dBFS; `None` if the range holds no entry. The range is
    /// widened to whole hops.
    pub fn column(&self, start: u64, end: u64) -> Option<[f32; BANDS]> {
        let (mut lo, mut hi) = (
            (start / HOP) as usize,
            (end.div_ceil(HOP) as usize).min(self.entries()),
        );
        if start >= end || lo >= hi {
            return None;
        }
        // Whole hops, gathered from the coarsest scales that fit inside them.
        let mut out = [0u8; BANDS];
        let mut take = |level: &[u8], i: usize| {
            for (o, &v) in out.iter_mut().zip(&level[i * BANDS..(i + 1) * BANDS]) {
                *o = (*o).max(v);
            }
        };
        for level in &self.levels {
            if lo >= hi {
                break;
            }
            if lo % 2 == 1 {
                take(level, lo);
                lo += 1;
            }
            if hi % 2 == 1 && lo < hi {
                hi -= 1;
                take(level, hi);
            }
            lo /= 2;
            hi /= 2;
        }
        Some(out.map(db))
    }

    /// The loudest level anywhere in the track, in dBFS, for scaling a view.
    pub fn loudest(&self) -> f32 {
        db(self.loudest)
    }

    /// Where `hz` sits on the bands, from 0 at [`LOWEST_HZ`] to 1 at half the
    /// sample rate, each band an equal share; `None` outside them.
    pub fn height_of(&self, hz: f32) -> Option<f32> {
        let e = &self.edges;
        if !(e[0]..=e[BANDS]).contains(&hz) {
            return None;
        }
        let j = e.partition_point(|&x| x <= hz).clamp(1, BANDS) - 1;
        let within = (hz - e[j]) / (e[j + 1] - e[j]);
        Some((j as f32 + within) / BANDS as f32)
    }

    /// Band `band`'s lower and upper edge, in Hz.
    pub fn band_hz(&self, band: usize) -> (f32, f32) {
        (self.edges[band], self.edges[band + 1])
    }
}

/// A stored level in dBFS.
fn db(level: u8) -> f32 {
    FLOOR_DB + f32::from(level) / 2.0
}

/// The [`BANDS`] + 1 band edges for a track at `rate`, in Hz.
fn edges(rate: u32) -> Vec<f32> {
    let top = (rate as f32 / 2.0).max(2.0 * LOWEST_HZ);
    (0..=BANDS)
        .map(|j| LOWEST_HZ * (top / LOWEST_HZ).powf(j as f32 / BANDS as f32))
        .collect()
}

/// Where a band reads its level from.
#[derive(Clone, Copy)]
enum Read {
    /// The loudest of bins `lo..hi`.
    Loudest(usize, usize),
    /// Between bin `k` and the next, `at` of the way, in dB.
    Between(usize, f32),
}

/// Zeros before the first frame, so entry `i` is centred on hop `i`.
const LEAD: usize = FFT / 2 - HOP as usize / 2;

pub(crate) struct Builder {
    rate: u32,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    edges: Vec<f32>,
    /// Where each band reads its level from.
    bins: Vec<Read>,
    /// The channels' mean over the frames the next transform reads.
    history: Vec<f32>,
    input: Vec<f32>,
    output: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    frames: u64,
    base: Vec<u8>,
}

impl Builder {
    pub(crate) fn new(rate: u32) -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FFT);
        let window = (0..FFT)
            .map(|i| {
                let x = std::f32::consts::TAU * i as f32 / FFT as f32;
                0.5 - 0.5 * x.cos()
            })
            .collect();
        let bin_hz = rate.max(1) as f32 / FFT as f32;
        let top = FFT / 2;
        let edges = edges(rate);
        let bins = edges
            .windows(2)
            .map(|e| {
                let lo = ((e[0] / bin_hz).ceil() as usize).min(top);
                let hi = ((e[1] / bin_hz).ceil() as usize).min(top + 1);
                if lo < hi {
                    Read::Loudest(lo, hi)
                } else {
                    let at = (e[0] * e[1]).sqrt() / bin_hz;
                    let k = (at as usize).min(top - 1);
                    Read::Between(k, at - k as f32)
                }
            })
            .collect();
        Builder {
            rate,
            input: fft.make_input_vec(),
            output: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            window,
            edges,
            bins,
            history: vec![0.0; LEAD],
            frames: 0,
            base: Vec::new(),
        }
    }

    /// Takes one frame, as the channels' mean.
    pub(crate) fn push(&mut self, mean: f32) {
        self.frames += 1;
        self.history.push(mean);
        if self.history.len() == FFT {
            self.analyse();
        }
    }

    /// Transforms the history into one entry and moves on a hop.
    fn analyse(&mut self) {
        for ((i, &s), &w) in self.input.iter_mut().zip(&self.history).zip(&self.window) {
            *i = s * w;
        }
        self.history.drain(..HOP as usize);
        self.fft
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .expect("buffers from the plan");
        // A full-scale sine peaks at FFT / 4 through a Hann window.
        let scale = (4.0 / FFT as f32).powi(2);
        let db = |power: f32| 10.0 * (power * scale).max(1e-30).log10();
        for &read in &self.bins {
            let db = match read {
                Read::Loudest(lo, hi) => db(self.output[lo..hi]
                    .iter()
                    .fold(0.0f32, |m, c| m.max(c.norm_sqr()))),
                Read::Between(k, at) => {
                    let (a, b) = (
                        db(self.output[k].norm_sqr()),
                        db(self.output[k + 1].norm_sqr()),
                    );
                    a + (b - a) * at
                }
            };
            self.base
                .push(((db - FLOOR_DB) * 2.0).round().clamp(0.0, 255.0) as u8);
        }
    }

    pub(crate) fn finish(mut self) -> Spectrogram {
        // Zeros after the last frame, until every hop has its entry.
        let wanted = self.frames.div_ceil(HOP) as usize;
        while self.base.len() / BANDS < wanted {
            self.history.resize(FFT, 0.0);
            self.analyse();
        }
        let loudest = self.base.iter().copied().max().unwrap_or(0);
        let mut levels = vec![std::mem::take(&mut self.base)];
        while levels.last().is_some_and(|l| l.len() > BANDS) {
            let finer = levels.last().expect("one level at least");
            let coarser = finer
                .chunks(2 * BANDS)
                .flat_map(|pair| {
                    let (a, b) = pair.split_at(BANDS.min(pair.len()));
                    (0..BANDS).map(move |i| a[i].max(b.get(i).copied().unwrap_or(0)))
                })
                .collect();
            levels.push(coarser);
        }
        Spectrogram {
            rate: self.rate,
            edges: self.edges,
            levels,
            loudest,
        }
    }
}
