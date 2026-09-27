//! A three-band tone control, applied as audio leaves the player: a low shelf
//! at 100 Hz, a wide peak at 1 kHz and a high shelf at 10 kHz, each cut or
//! boosted by up to [`RANGE_DB`]. The filters are the Audio EQ Cookbook's.
//!
//! A boost raises its band and nothing lowers the rest, so a large one can
//! clip a loud track. With every band at 0 the samples pass unchanged.

use std::sync::atomic::{AtomicU32, Ordering};

use super::meter::Biquad;

/// How far a band cuts or boosts, in dB either way.
pub const RANGE_DB: f32 = 12.0;

/// A band of the tone control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Bass,
    Mid,
    Treble,
}

impl Band {
    pub const ALL: [Band; 3] = [Band::Bass, Band::Mid, Band::Treble];

    /// Each band by the name commands and the settings use.
    pub const NAMES: [(&'static str, Band); 3] = [
        ("bass", Band::Bass),
        ("mid", Band::Mid),
        ("treble", Band::Treble),
    ];

    pub fn name(self) -> &'static str {
        Band::NAMES[self as usize].0
    }
}

/// Each band's gain in dB, set by the player and read by the callback.
#[derive(Default)]
pub struct Gains([AtomicU32; 3]);

impl Gains {
    pub fn get(&self) -> [f32; 3] {
        Band::ALL.map(|b| f32::from_bits(self.0[b as usize].load(Ordering::Relaxed)))
    }

    /// Sets `band` to `db`, kept within [`RANGE_DB`].
    pub fn set(&self, band: Band, db: f32) {
        let db = if db.is_finite() {
            db.clamp(-RANGE_DB, RANGE_DB)
        } else {
            0.0
        };
        self.0[band as usize].store(db.to_bits(), Ordering::Relaxed);
    }
}

/// Per-stream filter state. Built when a stream opens, so running it never
/// allocates.
pub struct Eq {
    rate: f64,
    gains: [f32; 3],
    stages: [Biquad; 3],
    /// Transposed direct form II state, `[stage][z1, z2]`, per channel.
    state: Vec<[[f64; 2]; 3]>,
    channel: usize,
}

impl Eq {
    pub fn new(rate: u32, channels: u16) -> Eq {
        let mut eq = Eq {
            rate: f64::from(rate.max(1)),
            gains: [f32::NAN; 3],
            stages: [PASS; 3],
            state: vec![[[0.0; 2]; 3]; channels.max(1) as usize],
            channel: 0,
        };
        eq.follow([0.0; 3]);
        eq
    }

    /// Takes `gains`, in dB by band, redesigning the filters when they change.
    pub fn follow(&mut self, gains: [f32; 3]) {
        if gains == self.gains {
            return;
        }
        self.gains = gains;
        let [bass, mid, treble] = gains.map(f64::from);
        self.stages = [
            shelf(self.rate, 100.0, bass, false),
            peak(self.rate, 1000.0, mid, 0.7),
            shelf(self.rate, 10_000.0, treble, true),
        ];
    }

    /// Whether every band is at 0, so [`Eq::process`] changes nothing.
    pub fn is_flat(&self) -> bool {
        self.gains == [0.0; 3]
    }

    /// Takes one interleaved sample and returns it filtered.
    pub fn process(&mut self, x: f32) -> f32 {
        let channel = self.channel;
        self.channel = (channel + 1) % self.state.len();
        if self.is_flat() {
            return x;
        }
        let y = self
            .stages
            .iter()
            .zip(self.state[channel].iter_mut())
            .fold(f64::from(x), |y, (stage, z)| stage.process(y, z));
        y as f32
    }
}

const PASS: Biquad = Biquad {
    b0: 1.0,
    b1: 0.0,
    b2: 0.0,
    a1: 0.0,
    a2: 0.0,
};

/// Divides by `a0` so the result is normalised as [`Biquad`] is.
fn normalised([b0, b1, b2, a0, a1, a2]: [f64; 6]) -> Biquad {
    Biquad {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

/// A shelf of `db` at `hz`, high or low, with a slope of 1.
fn shelf(rate: f64, hz: f64, db: f64, high: bool) -> Biquad {
    let a = 10f64.powf(db / 40.0);
    // Below the Nyquist frequency, for a low sample rate.
    let w = std::f64::consts::TAU * hz.min(rate * 0.45) / rate;
    let (cos, alpha) = (w.cos(), w.sin() / std::f64::consts::SQRT_2);
    let root = 2.0 * a.sqrt() * alpha;
    // A high shelf is a low one with the cosine's sign turned.
    let s = if high { -1.0 } else { 1.0 };
    normalised([
        a * ((a + 1.0) - s * (a - 1.0) * cos + root),
        s * 2.0 * a * ((a - 1.0) - s * (a + 1.0) * cos),
        a * ((a + 1.0) - s * (a - 1.0) * cos - root),
        (a + 1.0) + s * (a - 1.0) * cos + root,
        -s * 2.0 * ((a - 1.0) + s * (a + 1.0) * cos),
        (a + 1.0) + s * (a - 1.0) * cos - root,
    ])
}

/// A peak of `db` at `hz`, `q` wide.
fn peak(rate: f64, hz: f64, db: f64, q: f64) -> Biquad {
    let a = 10f64.powf(db / 40.0);
    let w = std::f64::consts::TAU * hz.min(rate * 0.45) / rate;
    let alpha = w.sin() / (2.0 * q);
    normalised([
        1.0 + alpha * a,
        -2.0 * w.cos(),
        1.0 - alpha * a,
        1.0 + alpha / a,
        -2.0 * w.cos(),
        1.0 - alpha / a,
    ])
}
