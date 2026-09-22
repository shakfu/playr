//! The frequency a recording's content stops at, and how sharply it stops.
//!
//! An encoder's lowpass leaves a cliff: tens of dB lost within a few hundred
//! Hz, at 16 to 20 kHz for MP3 and AAC. A lossless file with such a cliff was
//! probably made from a lossy one; a high-rate file whose cliff sits near
//! 22 kHz was probably made from a 44.1 kHz one. A dark master falls slowly,
//! so the slope, not the cutoff alone, tells them apart.

use std::sync::Arc;

use realfft::{RealFftPlanner, RealToComplex};

/// Transforms averaged across a track.
const WINDOWS: u64 = 24;

/// Width compared on each side of a candidate cutoff, at 44.1 kHz. It
/// scales with the rate, as a resampler's transition band does.
const SIDE_HZ: f32 = 500.0;

/// Cutoffs below this are not searched: content usually thins out there anyway.
const LOWEST_HZ: f32 = 10_000.0;

/// A window quieter than this, in RMS, is left out, so silence between tracks
/// does not count as a cutoff everywhere.
const QUIET: f32 = 1e-4;

pub struct Cutoff {
    rate: u32,
    fft: Arc<dyn RealToComplex<f32>>,
    size: usize,
    window: Vec<f32>,
    /// Frames between the starts of transforms.
    stride: u64,
    /// Mono frames seen so far.
    seen: u64,
    /// The frame the next transform starts at.
    next: u64,
    /// Mono samples of the transform being collected.
    collecting: Vec<f32>,
    frame: Vec<f32>,
    spectrum: Vec<realfft::num_complex::Complex<f32>>,
    /// Power summed per bin over the transforms taken.
    power: Vec<f64>,
    taken: u32,
}

/// Where content stops, and the fall across it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    pub hz: u32,
    /// Mean level just below `hz` less that just above, in dB: 500 Hz each
    /// side at 44.1 kHz, wider at higher rates.
    pub fall_db: f32,
}

impl Cutoff {
    /// For a track of about `frames` frames at `rate`; unknown lengths take a
    /// transform every 10 s.
    pub fn new(rate: u32, frames: Option<u64>) -> Cutoff {
        let size = if rate > 96_000 { 8192 } else { 4096 };
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(size);
        let window = (0..size)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / size as f32).cos())
            .collect();
        let stride = frames
            .map_or(rate as u64 * 10, |f| f / WINDOWS)
            .max(size as u64);
        Cutoff {
            rate,
            power: vec![0.0; size / 2 + 1],
            spectrum: fft.make_output_vec(),
            frame: fft.make_input_vec(),
            fft,
            size,
            window,
            stride,
            seen: 0,
            // Half a stride in, so the windows are centred across the track.
            next: stride / 2,
            collecting: Vec::new(),
            taken: 0,
        }
    }

    pub fn feed(&mut self, interleaved: &[f32], channels: usize) {
        let channels = channels.max(1);
        for f in interleaved.chunks_exact(channels) {
            if self.seen >= self.next {
                self.collecting
                    .push(f.iter().sum::<f32>() / channels as f32);
                if self.collecting.len() == self.size {
                    self.transform();
                    self.collecting.clear();
                    self.next += self.stride;
                }
            }
            self.seen += 1;
        }
    }

    fn transform(&mut self) {
        let rms = (self.collecting.iter().map(|x| x * x).sum::<f32>() / self.size as f32).sqrt();
        if rms < QUIET {
            return;
        }
        for ((out, x), w) in self
            .frame
            .iter_mut()
            .zip(&self.collecting)
            .zip(&self.window)
        {
            *out = x * w;
        }
        if self
            .fft
            .process(&mut self.frame, &mut self.spectrum)
            .is_err()
        {
            return;
        }
        for (p, bin) in self.power.iter_mut().zip(&self.spectrum) {
            *p += bin.norm_sqr() as f64;
        }
        self.taken += 1;
    }

    /// The steepest fall above 10 kHz; `None` when fewer than three windows
    /// held sound, or the rate leaves no room above 10 kHz.
    pub fn finish(self) -> Option<Measured> {
        if self.taken < 3 {
            return None;
        }
        let hz_per_bin = self.rate as f32 / self.size as f32;
        let db: Vec<f32> = self
            .power
            .iter()
            .map(|p| (10.0 * (p / self.taken as f64 + 1e-30).log10()) as f32)
            .collect();
        let side_hz = SIDE_HZ * (self.rate as f32 / 44_100.0).max(1.0);
        let side = ((side_hz / hz_per_bin).round() as usize).max(3);
        let first = ((LOWEST_HZ / hz_per_bin) as usize).max(side);
        let last = db.len().checked_sub(side)?;
        if first >= last {
            return None;
        }
        let mean = |r: std::ops::Range<usize>| db[r.clone()].iter().sum::<f32>() / r.len() as f32;
        let (bin, fall) = (first..last)
            .map(|b| (b, mean(b - side..b) - mean(b..b + side)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))?;
        Some(Measured {
            hz: (bin as f32 * hz_per_bin).round() as u32,
            fall_db: fall,
        })
    }
}
