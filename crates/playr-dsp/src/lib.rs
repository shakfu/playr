//! The DSP `playr-looper` and `playr-dj` share: linear ramps, a
//! state-variable filter, a one-pole coefficient, a soft clip and the
//! interpolation of their read heads ([`interp`]). No threads, and no
//! allocation once [`interp::warm`] has built the sinc's table.

use std::ops::{Add, Div, Mul, Sub};

pub mod interp;
pub use interp::{Interp, Kernel};

/// What a [`Ramp`] moves: `f32` or `f64`.
pub trait Value:
    Copy + PartialEq + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> + Div<Output = Self>
{
    const ZERO: Self;
    fn of(n: u32) -> Self;
}

impl Value for f32 {
    const ZERO: Self = 0.0;
    fn of(n: u32) -> Self {
        n as f32
    }
}

impl Value for f64 {
    const ZERO: Self = 0.0;
    fn of(n: u32) -> Self {
        f64::from(n)
    }
}

/// A value that moves linearly to its target over a set number of frames.
/// Each value is computed back from the target, not accumulated, so an
/// `f32` ramp of many frames neither drifts nor stalls.
#[derive(Debug, Clone, Copy)]
pub struct Ramp<T> {
    value: T,
    target: T,
    step: T,
    left: u32,
}

impl<T: Value> Ramp<T> {
    pub fn new(value: T) -> Self {
        Ramp {
            value,
            target: value,
            step: T::ZERO,
            left: 0,
        }
    }

    pub fn set(&mut self, target: T, frames: u32) {
        self.target = target;
        self.left = frames;
        match frames {
            0 => self.value = target,
            n => self.step = (target - self.value) / T::of(n),
        }
    }

    /// Advances one frame and returns the value there.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> T {
        if self.left > 0 {
            self.left -= 1;
            self.value = self.target - self.step * T::of(self.left);
        }
        self.value
    }

    pub fn value(&self) -> T {
        self.value
    }

    pub fn target(&self) -> T {
        self.target
    }

    /// Whether it has reached its target.
    pub fn settled(&self) -> bool {
        self.left == 0
    }

    /// How far the sum of the remaining values falls short of the target's.
    /// A head moving at this ramp ends `lag` frames behind one moving at the
    /// target all along.
    pub fn lag(&self) -> T {
        let n = T::of(self.left);
        let one = T::of(1);
        self.step * n * (n - one) / T::of(2)
    }
}

/// The three outputs of a [`Svf`] for one sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Taps {
    pub low: f32,
    /// The band-pass, at unity gain at the cutoff.
    pub band: f32,
    pub high: f32,
}

impl Taps {
    /// The all-pass: the input's level at every frequency, its phase turned
    /// as the low-pass's poles turn it.
    pub fn all(&self) -> f32 {
        self.low + self.high - self.band
    }
}

/// A 2-pole state-variable filter per channel, in its topology-preserving
/// form, which stays stable as the cutoff moves.
#[derive(Debug, Clone, Copy)]
pub struct Svf {
    ic1: [f32; 2],
    ic2: [f32; 2],
    a: [f32; 3],
    /// The damping, 1/Q.
    k: f32,
    /// The cutoff the coefficients were computed for.
    at: f32,
}

impl Svf {
    /// Damped as `q`.
    pub fn new(q: f32) -> Self {
        Svf {
            ic1: [0.0; 2],
            ic2: [0.0; 2],
            a: [0.0; 3],
            k: 1.0 / q,
            at: f32::NAN,
        }
    }

    /// The Butterworth response, Q 1/sqrt(2).
    pub fn butterworth() -> Self {
        Svf::new(std::f32::consts::FRAC_1_SQRT_2)
    }

    /// Sets the cutoff to `hz`, kept within 1 Hz to 0.49 of `sample_rate`:
    /// at 0 the state freezes, and past Nyquist it diverges. A cutoff not
    /// finite is ignored.
    pub fn tune(&mut self, hz: f32, sample_rate: u32) {
        if hz == self.at || !hz.is_finite() {
            return;
        }
        self.at = hz;
        let hz = hz.clamp(1.0, 0.49 * sample_rate as f32);
        let g = (std::f32::consts::PI * hz / sample_rate as f32).tan();
        let a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a = [a1, g * a1, g * g * a1];
    }

    /// Clears the state, as of silence.
    pub fn reset(&mut self) {
        self.ic1 = [0.0; 2];
        self.ic2 = [0.0; 2];
    }

    /// Filters `x` on channel `ch`, 0 or 1. A state made not finite, as by
    /// an input not finite, starts again from silence.
    pub fn tick(&mut self, ch: usize, x: f32) -> Taps {
        let [a1, a2, a3] = self.a;
        let v3 = x - self.ic2[ch];
        let v1 = a1 * self.ic1[ch] + a2 * v3;
        let v2 = self.ic2[ch] + a2 * self.ic1[ch] + a3 * v3;
        self.ic1[ch] = flush(2.0 * v1 - self.ic1[ch]);
        self.ic2[ch] = flush(2.0 * v2 - self.ic2[ch]);
        if !(self.ic1[ch] + self.ic2[ch]).is_finite() {
            self.ic1[ch] = 0.0;
            self.ic2[ch] = 0.0;
        }
        Taps {
            low: v2,
            band: self.k * v1,
            high: x - self.k * v1 - v2,
        }
    }
}

/// `x`, or 0 below -600 dB. A decaying state would otherwise reach
/// subnormals, which are slow on x86.
pub fn flush(x: f32) -> f32 {
    match x.abs() < 1e-30 {
        true => 0.0,
        false => x,
    }
}

/// A one-pole low-pass coefficient for a cutoff of `hz`.
pub fn one_pole(hz: f32, sample_rate: u32) -> f32 {
    1.0 - (-std::f32::consts::TAU * hz / sample_rate as f32).exp()
}

/// Passes `|x| <= 0.5` unchanged and bends larger values towards 1.
pub fn clip(x: f32) -> f32 {
    soft_clip(x, 0.5)
}

/// Passes `|x| <= knee` unchanged and bends larger values towards 1, with
/// slope 1 at the knee. `knee` is below 1.
pub fn soft_clip(x: f32, knee: f32) -> f32 {
    let a = x.abs();
    let room = 1.0 - knee;
    match a <= knee {
        true => x,
        false => (knee + room * ((a - knee) / room).tanh()).copysign(x),
    }
}

/// The cubic Hermite curve through `x0` and `x1` at `t`, 0 to 1, with
/// slopes from their neighbours `xm` and `x2`.
pub fn hermite(xm: f32, x0: f32, x1: f32, x2: f32, t: f32) -> f32 {
    let c1 = 0.5 * (x1 - xm);
    let c2 = xm - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
    let c3 = 0.5 * (x2 - xm) + 1.5 * (x0 - x1);
    ((c3 * t + c2) * t + c1) * t + x0
}
