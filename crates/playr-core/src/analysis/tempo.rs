//! Tempo: one BPM estimate for a whole track, and how clear its pulse is.
//!
//! The novelty curve is spectral flux: the rise in log magnitude, summed over
//! bins, once per hop of about 11.6 ms. Its autocorrelation peaks at the beat
//! period. Lags are weighted by a log-Gaussian centred on 120 BPM, as in
//! Ellis, "Beat tracking by dynamic programming" (2007), which favours the
//! likelier of a tempo and its half or double without ruling either out.

use std::sync::Arc;

use realfft::{RealFftPlanner, RealToComplex};

/// Tempos considered, in BPM.
pub const SLOWEST: f32 = 40.0;
pub const FASTEST: f32 = 240.0;

/// The weighting's centre, in BPM, and its width, in octaves.
const PRIOR_BPM: f32 = 120.0;
const PRIOR_OCTAVES: f32 = 1.0;

/// Below this confidence the pulse is too weak to report a tempo.
///
/// Set against librosa on a 328-track library: see "Tempo" under Calibration
/// in `docs/dev/analyze.md`. It is the knee of that measurement, where
/// unrelated answers stop falling faster than coverage does. It applies when
/// a tempo is read, not when it is stored, so changing it needs no reanalysis.
pub const MIN_CONFIDENCE: f32 = 0.3;

/// Novelty frames needed for an estimate: about 8 s.
const MIN_SECONDS: f32 = 8.0;

/// Bins above this frequency add little to onsets and much to noise.
const TOP_HZ: f32 = 11_000.0;

/// Accumulates the novelty curve of one track.
pub struct Tempo {
    rate: u32,
    fft: Arc<dyn RealToComplex<f32>>,
    size: usize,
    hop: usize,
    /// Hann window, `size` long.
    window: Vec<f32>,
    /// Mono samples not yet consumed by a full transform.
    pending: Vec<f32>,
    frame: Vec<f32>,
    spectrum: Vec<realfft::num_complex::Complex<f32>>,
    /// Last frame's log magnitudes, up to `TOP_HZ`.
    last: Vec<f32>,
    novelty: Vec<f32>,
}

/// How strongly a track must pulse at twice the tempo chosen, against the
/// tempo chosen, for [`Estimate::alt`] to record it. Uncalibrated.
const ALT_SHARE: f32 = 0.5;

/// A tempo and how clearly the track pulses at it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    pub bpm: f32,
    /// The onset curve's autocorrelation at the chosen lag, normalised: 1
    /// for a strict pulse, near 0 for noise. Compare with [`MIN_CONFIDENCE`].
    pub confidence: f32,
    /// Double `bpm`, where the track pulses nearly as strongly there and
    /// `bpm` is slow enough for the prior to have halved it.
    ///
    /// A search matches either, so a track heard at 174 BPM is found by
    /// `bpm:174` even though it is recorded at 87.
    pub alt: Option<f32>,
}

impl Tempo {
    pub fn new(rate: u32) -> Tempo {
        // About 46 ms a transform and 11.6 ms a hop at any rate.
        let size = match rate {
            0..=48_000 => 2048,
            48_001..=96_000 => 4096,
            _ => 8192,
        };
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(size);
        let window = (0..size)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / size as f32).cos())
            .collect();
        let bins = ((TOP_HZ / (rate.max(1) as f32 / size as f32)) as usize).min(size / 2);
        Tempo {
            rate,
            spectrum: fft.make_output_vec(),
            frame: fft.make_input_vec(),
            fft,
            size,
            hop: size / 4,
            window,
            pending: Vec::new(),
            last: vec![0.0; bins],
            novelty: Vec::new(),
        }
    }

    /// Takes interleaved samples of `channels` channels.
    pub fn feed(&mut self, interleaved: &[f32], channels: usize) {
        let channels = channels.max(1);
        self.pending.extend(
            interleaved
                .chunks_exact(channels)
                .map(|f| f.iter().sum::<f32>() / channels as f32),
        );
        let mut start = 0;
        while self.pending.len() - start >= self.size {
            self.transform(start);
            start += self.hop;
        }
        self.pending.drain(..start);
    }

    fn transform(&mut self, start: usize) {
        let input = &self.pending[start..start + self.size];
        for ((out, x), w) in self.frame.iter_mut().zip(input).zip(&self.window) {
            *out = x * w;
        }
        if self
            .fft
            .process(&mut self.frame, &mut self.spectrum)
            .is_err()
        {
            return;
        }
        let scale = 1.0 / self.size as f32;
        let mut flux = 0.0;
        for (last, bin) in self.last.iter_mut().zip(&self.spectrum) {
            let level = (1.0 + 1000.0 * bin.norm() * scale).ln();
            flux += (level - *last).max(0.0);
            *last = level;
        }
        self.novelty.push(flux);
    }

    /// The estimate, or `None` for a track under about 8 s.
    pub fn finish(self) -> Option<Estimate> {
        estimate(&self.novelty, self.rate as f32 / self.hop as f32)
    }
}

/// The tempo of a novelty curve sampled `fps` times a second.
pub fn estimate(novelty: &[f32], fps: f32) -> Option<Estimate> {
    if fps <= 0.0 || (novelty.len() as f32) < MIN_SECONDS * fps {
        return None;
    }
    // Onsets are rises above the local level, so a steady loud passage and a
    // quiet one weigh the same.
    let half = (fps * 0.25) as usize;
    let mut sums = Vec::with_capacity(novelty.len() + 1);
    sums.push(0.0f64);
    for &v in novelty {
        sums.push(sums.last().copied().unwrap_or(0.0) + v as f64);
    }
    let mut onsets: Vec<f32> = (0..novelty.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(half), (i + half + 1).min(novelty.len()));
            let local = (sums[b] - sums[a]) / (b - a) as f64;
            (novelty[i] - local as f32).max(0.0)
        })
        .collect();
    // A period of 34.5 hops puts onsets 34 and 35 hops apart in turn, which
    // splits the autocorrelation's peak between two lags and lets a lag of
    // twice the period, which does not split, win. Smoothing over a few hops
    // keeps each peak whole.
    const KERNEL: [f32; 5] = [0.25, 0.75, 1.0, 0.75, 0.25];
    let raw = std::mem::take(&mut onsets);
    onsets = (0..raw.len())
        .map(|i| {
            KERNEL
                .iter()
                .enumerate()
                .filter_map(|(k, w)| Some(w * raw.get((i + k).checked_sub(2)?)?))
                .sum()
        })
        .collect();
    // Centred, so the autocorrelation of an unpulsed curve is near zero at
    // every lag rather than its mean squared.
    let mean = onsets.iter().sum::<f32>() / onsets.len() as f32;
    for v in &mut onsets {
        *v -= mean;
    }

    let lag_of = |bpm: f32| fps * 60.0 / bpm;
    let (lo, hi) = (
        lag_of(FASTEST).floor() as usize,
        lag_of(SLOWEST).ceil() as usize,
    );
    let lo = lo.max(1);
    // The refinement reads up to four periods out.
    let longest = (hi * 4 + 4).min(onsets.len() - 1);
    if hi >= longest {
        return None;
    }
    let acf: Vec<f32> = (0..=longest)
        .map(|lag| {
            let n = onsets.len() - lag;
            let sum: f64 = onsets[..n]
                .iter()
                .zip(&onsets[lag..])
                .map(|(a, b)| (*a * *b) as f64)
                .sum();
            (sum / n as f64) as f32
        })
        .collect();

    let centre = lag_of(PRIOR_BPM);
    let weight = |lag: usize| {
        let octaves = (lag as f32 / centre).log2() / PRIOR_OCTAVES;
        (-0.5 * octaves * octaves).exp()
    };
    let best = (lo..=hi).max_by(|&a, &b| {
        (acf[a] * weight(a))
            .partial_cmp(&(acf[b] * weight(b)))
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;

    if acf[0] <= 0.0 {
        return None;
    }
    let confidence = acf[best] / acf[0];

    // A lag is whole hops, 2.8 BPM apart at 120 BPM. The peaks at two, three
    // and four periods locate the period to a half, a third and a quarter of
    // that, and each is refined between hops by a parabola.
    let mut sum = 0.0;
    let mut weights = 0.0;
    for k in 1..=4usize {
        let Some((lag, _)) = peak_near(&acf, best * k, k, longest) else {
            continue;
        };
        // Period estimates weighted by k, since a k-period lag locates the
        // period k times as finely: sum(k * lag / k) / sum(k).
        sum += lag;
        weights += k as f32;
    }
    let period = sum / weights;

    // The level above, kept when the track pulses nearly as strongly there.
    // Only below the prior's centre: a strict pulse correlates at every
    // multiple of its period, and the slower readings are the ones the prior
    // never had to choose against, so recording them would match searches for
    // tempos nothing in the track plays.
    let doubled = fps * 60.0 / period * 2.0;
    let halved = period > centre && (SLOWEST..=FASTEST).contains(&doubled);
    let alt = halved
        .then_some(best / 2)
        .filter(|lag| (lo..=hi).contains(lag))
        .and_then(|lag| peak_near(&acf, lag, 1, longest))
        .filter(|(_, height)| *height >= acf[best] * ALT_SHARE)
        .map(|(lag, _)| fps * 60.0 / lag);

    Some(Estimate {
        bpm: fps * 60.0 / period,
        confidence,
        alt,
    })
}

/// The highest autocorrelation within `window` hops of `centre`, as a lag
/// refined between hops by a parabola, and its height.
fn peak_near(acf: &[f32], centre: usize, window: usize, longest: usize) -> Option<(f32, f32)> {
    let (from, to) = (
        centre.saturating_sub(window).max(1),
        (centre + window).min(longest - 1),
    );
    if from > to {
        return None;
    }
    let peak = (from..=to).max_by(|&x, &y| {
        acf[x]
            .partial_cmp(&acf[y])
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;
    let (l, c, r) = (acf[peak - 1], acf[peak], acf[peak + 1]);
    let curve = l - 2.0 * c + r;
    let shift = match curve < 0.0 {
        true => (0.5 * (l - r) / curve).clamp(-0.5, 0.5),
        false => 0.0,
    };
    Some((peak as f32 + shift, c))
}
