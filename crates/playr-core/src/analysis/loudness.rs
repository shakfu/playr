//! Integrated loudness per ITU-R BS.1770-4 and EBU R128, and sample peak.
//!
//! The gating blocks are [`Meter`]'s momentary readings: 400 ms, every 100 ms.
//! Blocks below -70 LUFS are dropped, then those more than 10 LU below the
//! mean of the rest. An album pools its tracks' blocks, which each track
//! keeps as a [`Histogram`] so the album needs no second decode.
//!
//! Every channel weighs 1.0, as in the meter: right for stereo, while BS.1770
//! weighs surround channels 1.41 and leaves out LFE.

use crate::audio::meter::{Meter, SILENCE_LUFS};

/// The histogram's lowest bin starts here, at the absolute gate.
const LOWEST_LUFS: f32 = SILENCE_LUFS;

/// One bin per LU from -70 to +5 LUFS; louder blocks share the top bin.
pub const BINS: usize = 76;

/// Bytes of [`Histogram::to_bytes`]: a `u32` count and an `f64` energy a bin.
pub const HISTOGRAM_BYTES: usize = BINS * 12;

/// A block's mean square, K-weighted and summed over channels, from its loudness.
fn energy(lufs: f32) -> f64 {
    10f64.powf((lufs as f64 + 0.691) / 10.0)
}

/// Loudness from a mean square.
fn lufs(energy: f64) -> f32 {
    (-0.691 + 10.0 * energy.log10()) as f32
}

/// Gating blocks by loudness, 1 LU a bin: how many, and their summed energy.
///
/// The energy sums make a pooled mean exact. Only the relative gate is
/// approximate, for blocks in the bin it falls in.
#[derive(Debug, Clone, PartialEq)]
pub struct Histogram {
    pub bins: [(u32, f64); BINS],
}

impl Default for Histogram {
    fn default() -> Self {
        Histogram {
            bins: [(0, 0.0); BINS],
        }
    }
}

impl Histogram {
    fn add(&mut self, lufs: f32) {
        let bin = ((lufs - LOWEST_LUFS).floor() as usize).min(BINS - 1);
        self.bins[bin].0 += 1;
        self.bins[bin].1 += energy(lufs);
    }

    /// Adds `other`'s blocks, as an album pools its tracks.
    pub fn pool(&mut self, other: &Histogram) {
        for (mine, theirs) in self.bins.iter_mut().zip(&other.bins) {
            mine.0 += theirs.0;
            mine.1 += theirs.1;
        }
    }

    /// Integrated loudness of the blocks held; `None` when none pass the gate.
    ///
    /// The bin holding the relative gate counts when its centre is above it.
    pub fn integrated(&self) -> Option<f32> {
        let (n, e) = self
            .bins
            .iter()
            .fold((0u64, 0.0), |(n, e), b| (n + b.0 as u64, e + b.1));
        if n == 0 {
            return None;
        }
        let gate = lufs(e / n as f64) - 10.0;
        let (n, e) = self
            .bins
            .iter()
            .enumerate()
            .filter(|(i, _)| LOWEST_LUFS + *i as f32 + 0.5 >= gate)
            .fold((0u64, 0.0), |(n, e), (_, b)| (n + b.0 as u64, e + b.1));
        (n > 0).then(|| lufs(e / n as f64))
    }

    /// Little-endian, bin by bin: the count, then the energy.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HISTOGRAM_BYTES);
        for (n, e) in &self.bins {
            out.extend_from_slice(&n.to_le_bytes());
            out.extend_from_slice(&e.to_le_bytes());
        }
        out
    }

    /// The inverse of [`Histogram::to_bytes`]; `None` for any other length.
    pub fn from_bytes(bytes: &[u8]) -> Option<Histogram> {
        if bytes.len() != HISTOGRAM_BYTES {
            return None;
        }
        let mut h = Histogram::default();
        let (chunks, _) = bytes.as_chunks::<12>();
        for (bin, chunk) in h.bins.iter_mut().zip(chunks) {
            bin.0 = u32::from_le_bytes(chunk[..4].try_into().ok()?);
            bin.1 = f64::from_le_bytes(chunk[4..].try_into().ok()?);
        }
        Some(h)
    }
}

/// Measures one track, fed its interleaved samples.
pub struct Loudness {
    meter: Meter,
    /// Readings to drop: the meter's first three cover less than 400 ms.
    warmup: u8,
    /// Every block's loudness that passed the absolute gate.
    blocks: Vec<f32>,
    histogram: Histogram,
}

/// What [`Loudness`] measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    /// Integrated loudness in LUFS; `None` for silence or under 400 ms.
    pub lufs: Option<f32>,
    /// Sample peak, linear.
    pub peak: f32,
    pub histogram: Histogram,
}

impl Loudness {
    pub fn new(rate: u32, channels: u16) -> Loudness {
        Loudness {
            meter: Meter::new(rate, channels),
            warmup: 3,
            blocks: Vec::new(),
            histogram: Histogram::default(),
        }
    }

    pub fn feed(&mut self, interleaved: &[f32]) {
        for &x in interleaved {
            let Some(lufs) = self.meter.sample(x) else {
                continue;
            };
            if self.warmup > 0 {
                self.warmup -= 1;
            } else if lufs >= LOWEST_LUFS {
                self.blocks.push(lufs);
                self.histogram.add(lufs);
            }
        }
    }

    /// Integrated loudness, gated exactly from the blocks rather than the
    /// histogram.
    pub fn finish(mut self) -> Measured {
        let mean = |blocks: &mut dyn Iterator<Item = &f32>| {
            let (n, e) = blocks.fold((0usize, 0.0), |(n, e), &l| (n + 1, e + energy(l)));
            (n > 0).then(|| lufs(e / n as f64))
        };
        let lufs = mean(&mut self.blocks.iter()).and_then(|ungated| {
            let gate = ungated - 10.0;
            mean(&mut self.blocks.iter().filter(|&&l| l >= gate))
        });
        Measured {
            lufs,
            peak: self.meter.take_peak(),
            histogram: self.histogram,
        }
    }
}
