//! Momentary loudness and sample peak, measured as audio leaves the player.
//!
//! Loudness follows ITU-R BS.1770-4: each channel is K-weighted by two biquads,
//! a high shelf that models the head and a high-pass that discards the lowest
//! frequencies, then its mean square is taken. Momentary loudness is the mean
//! over the last 400 ms, kept as four 100 ms blocks per EBU Tech 3341, so it
//! updates ten times a second.
//!
//! All channels are weighted 1.0, which is right for stereo. BS.1770 weights
//! surround channels 1.41 and excludes LFE, but the device's channel layout is
//! not known here.

/// Loudness below this is reported as silence. EBU R128 uses it as the
/// absolute gate.
pub const SILENCE_LUFS: f32 = -70.0;

/// Coefficients of one biquad, normalised so `a0` is 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

/// The two K-weighting stages for `rate`, pre-filter first.
///
/// The formulas are libebur128's. They reproduce BS.1770-4's 48 kHz table and
/// extend it to other rates by the bilinear transform.
pub fn k_weighting(rate: u32) -> [Biquad; 2] {
    let rate = rate as f64;

    let (f0, gain_db, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
    let k = (std::f64::consts::PI * f0 / rate).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b0: (vh + vb * k / q + k * k) / a0,
        b1: 2.0 * (k * k - vh) / a0,
        b2: (vh - vb * k / q + k * k) / a0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
    };

    let (f0, q) = (38.13547087602444, 0.5003270373238773);
    let k = (std::f64::consts::PI * f0 / rate).tan();
    let a0 = 1.0 + k / q + k * k;
    let high_pass = Biquad {
        b0: 1.0,
        b1: -2.0,
        b2: 1.0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
    };

    [shelf, high_pass]
}

/// Per-stream metering state. Built when a stream opens, so feeding it never
/// allocates.
pub struct Meter {
    stages: [Biquad; 2],
    /// Transposed direct form II state, `[stage][z1, z2]`, per channel.
    state: Vec<[[f64; 2]; 2]>,
    channels: usize,
    /// Channel the next sample belongs to.
    channel: usize,
    /// Frames in a 100 ms block.
    block_frames: usize,
    frames: usize,
    /// Sum of weighted squares in the current block.
    sum: f64,
    /// Mean square of the last four blocks, oldest overwritten first.
    blocks: [f64; 4],
    next_block: usize,
    filled: usize,
    peak: f32,
}

impl Meter {
    pub fn new(rate: u32, channels: u16) -> Self {
        let channels = channels.max(1) as usize;
        Meter {
            stages: k_weighting(rate),
            state: vec![[[0.0; 2]; 2]; channels],
            channels,
            channel: 0,
            block_frames: (rate as usize / 10).max(1),
            frames: 0,
            sum: 0.0,
            blocks: [0.0; 4],
            next_block: 0,
            filled: 0,
            peak: 0.0,
        }
    }

    /// Takes one interleaved sample. Returns the momentary loudness in LUFS
    /// when it completes a 100 ms block.
    pub fn sample(&mut self, x: f32) -> Option<f32> {
        self.peak = self.peak.max(x.abs());
        let mut y = x as f64;
        for (stage, z) in self.stages.iter().zip(self.state[self.channel].iter_mut()) {
            let out = stage.b0 * y + z[0];
            z[0] = stage.b1 * y - stage.a1 * out + z[1];
            z[1] = stage.b2 * y - stage.a2 * out;
            // Decaying state would reach denormals in silence, which are slow on x86.
            for v in z.iter_mut() {
                if v.abs() < 1e-30 {
                    *v = 0.0;
                }
            }
            y = out;
        }
        self.sum += y * y;

        self.channel += 1;
        if self.channel < self.channels {
            return None;
        }
        self.channel = 0;
        self.frames += 1;
        if self.frames < self.block_frames {
            return None;
        }
        self.blocks[self.next_block] = self.sum / self.frames as f64;
        self.next_block = (self.next_block + 1) % self.blocks.len();
        self.filled = (self.filled + 1).min(self.blocks.len());
        self.frames = 0;
        self.sum = 0.0;

        // Channel weights are all 1.0, so the channel sum is the block's sum.
        let power = self.blocks[..self.filled].iter().sum::<f64>() / self.filled as f64;
        Some((-0.691 + 10.0 * power.log10()) as f32)
    }

    /// The largest sample magnitude since the last call, which resets it.
    pub fn take_peak(&mut self) -> f32 {
        std::mem::take(&mut self.peak)
    }
}
