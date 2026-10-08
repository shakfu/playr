//! The read heads' interpolation: a frame between two stored frames, as a
//! set of weights over the frames around it. [`Interp::Hermite`] is the
//! 4-point cubic; [`Interp::Sinc`] a Kaiser-windowed sinc, stretched by the
//! read rate above 1 so its cutoff falls with the rate. `docs/dev/aliasing.md`
//! has the measurements behind the choice.

use std::sync::OnceLock;

/// How a read head finds a frame between stored frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interp {
    /// The 4-point cubic Hermite: cheap, with images and folding above
    /// about 5 kHz at any rate but 1.
    Hermite,
    /// A windowed sinc of [`TAPS`] taps at rate 1 and up to [`MAX_STRETCH`]
    /// times as many above it.
    #[default]
    Sinc,
}

impl Interp {
    pub const NAMES: [(&str, Interp); 2] = [("sinc", Interp::Sinc), ("hermite", Interp::Hermite)];

    pub fn name(self) -> &'static str {
        match self {
            Interp::Hermite => "hermite",
            Interp::Sinc => "sinc",
        }
    }

    /// The weights that read the frame at `pos` while reading at `rate`
    /// frames a frame, negative in reverse.
    pub fn kernel(self, pos: f64, rate: f64) -> Kernel {
        let i = pos.floor();
        let t = pos - i;
        let i = i as isize;
        // A whole frame at rate 1 needs no filter: the stored frame is it.
        if t == 0.0 && (self == Interp::Hermite || rate.abs() == 1.0) {
            return Kernel::one(i);
        }
        match self {
            Interp::Hermite => Kernel::hermite(i, t as f32),
            Interp::Sinc => Kernel::sinc(i, t, rate.abs()),
        }
    }
}

/// Taps either side of the read position at rate 1.
pub const HALF: usize = 16;
/// Taps of the sinc at rate 1.
pub const TAPS: usize = 2 * HALF;
/// The most the sinc is stretched: the tape's top rate. Faster reads keep
/// this cutoff, and fold what lies above it.
pub const MAX_STRETCH: f64 = 4.0;
/// The Kaiser window's shape: about 80 dB down outside the passband.
const BETA: f64 = 8.0;
/// Where the passband ends, as a share of the stored rate: the window's
/// transition then ends near half the stored rate.
const CUTOFF: f64 = 0.42;
/// Table points per frame of distance.
const STEPS: usize = 512;
/// The most weights a kernel holds.
const MAX_WEIGHTS: usize = 2 * HALF * MAX_STRETCH as usize + 2;

/// Weights over consecutive frames from `first`.
#[derive(Debug, Clone, Copy)]
pub struct Kernel {
    first: isize,
    len: usize,
    weights: [f32; MAX_WEIGHTS],
}

impl Kernel {
    fn one(i: isize) -> Kernel {
        let mut weights = [0.0; MAX_WEIGHTS];
        weights[0] = 1.0;
        Kernel {
            first: i,
            len: 1,
            weights,
        }
    }

    fn hermite(i: isize, t: f32) -> Kernel {
        let (t2, t3) = (t * t, t * t * t);
        let mut weights = [0.0; MAX_WEIGHTS];
        weights[..4].copy_from_slice(&[
            -0.5 * t + t2 - 0.5 * t3,
            1.0 - 2.5 * t2 + 1.5 * t3,
            0.5 * t + 2.0 * t2 - 1.5 * t3,
            -0.5 * t2 + 0.5 * t3,
        ]);
        Kernel {
            first: i - 1,
            len: 4,
            weights,
        }
    }

    /// The sinc at `t` past frame `i`, stretched by `rate` when above 1, and
    /// scaled so its weights sum to 1.
    fn sinc(i: isize, t: f64, rate: f64) -> Kernel {
        let table = table();
        let stretch = rate.clamp(1.0, MAX_STRETCH);
        let reach = (HALF as f64 * stretch).ceil() as isize;
        let first = i - reach + 1;
        let len = (2 * reach) as usize;
        let mut weights = [0.0; MAX_WEIGHTS];
        let mut sum = 0.0;
        for (k, w) in weights[..len].iter_mut().enumerate() {
            let d = ((first + k as isize - i) as f64 - t).abs() / stretch;
            *w = lookup(table, d);
            sum += *w;
        }
        if sum != 0.0 {
            weights[..len].iter_mut().for_each(|w| *w /= sum);
        }
        Kernel {
            first,
            len,
            weights,
        }
    }

    /// The first frame the weights fall on.
    pub fn first(&self) -> isize {
        self.first
    }

    pub fn weights(&self) -> &[f32] {
        &self.weights[..self.len]
    }

    /// The weighted sum of `at(frame)` over the kernel's frames.
    pub fn apply(&self, at: impl Fn(isize) -> f32) -> f32 {
        (self.first..)
            .zip(self.weights())
            .map(|(k, w)| at(k) * w)
            .sum()
    }
}

/// The kernel's value at distance `d` frames, linearly between table points.
fn lookup(table: &[f32], d: f64) -> f32 {
    let x = d * STEPS as f64;
    let j = x as usize;
    if j + 1 >= table.len() {
        return 0.0;
    }
    let f = (x - j as f64) as f32;
    table[j] + (table[j + 1] - table[j]) * f
}

/// One side of the kernel, from distance 0 to [`HALF`], [`STEPS`] points a
/// frame. Built once; [`warm`] builds it before an audio thread needs it.
fn table() -> &'static [f32] {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..=HALF * STEPS + 1)
            .map(|j| {
                let d = j as f64 / STEPS as f64;
                let x = d / HALF as f64;
                if x > 1.0 {
                    return 0.0;
                }
                let sinc = match d {
                    0.0 => 1.0,
                    d => {
                        (std::f64::consts::PI * 2.0 * CUTOFF * d).sin()
                            / (std::f64::consts::PI * 2.0 * CUTOFF * d)
                    }
                };
                let window = bessel_i0(BETA * (1.0 - x * x).sqrt()) / bessel_i0(BETA);
                (sinc * window) as f32
            })
            .collect()
    })
}

/// Builds the sinc's table, which allocates, so an audio thread's first
/// read does not.
pub fn warm() {
    table();
}

/// The modified Bessel function of the first kind, order 0, by its series.
fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term) = (1.0, 1.0);
    for k in 1..50 {
        term *= (x / (2.0 * k as f64)).powi(2);
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}
