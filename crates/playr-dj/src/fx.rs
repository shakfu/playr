//! A deck's 3-band isolator EQ and its one-knob filter, after the trim and
//! before the channel fader.

use playr_dsp::{Ramp, Svf};

/// The crossovers, Mixxx's defaults.
const LOW_HZ: f32 = 246.0;
const HIGH_HZ: f32 = 2500.0;
/// How long a gain or knob change takes.
const SMOOTH_MS: f32 = 10.0;
/// The filter knob's span either side of centre where it passes everything.
const DEAD: f32 = 0.05;
/// The filter's cutoff range, on a log scale.
const FILTER_HZ: (f32, f32) = (20.0, 20_000.0);
/// The two sections of a 4th-order Butterworth filter, by Q.
const BW4: [f32; 2] = [0.541_196_1, 1.306_563];
/// An EQ band's travel, in dB; a kill goes below it, to silence.
pub const EQ_DB: (f64, f64) = (-24.0, 6.0);

/// An EQ band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Low,
    Mid,
    High,
}

impl Band {
    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// A Linkwitz-Riley 8th-order crossover: a 4th-order Butterworth filter
/// twice over, low and high. Its outputs sum to the input passed through
/// the Butterworth's all-pass.
#[derive(Debug, Clone, Copy)]
struct Crossover {
    /// The first section, shared by both sides.
    first: Svf,
    low: [Svf; 3],
    high: [Svf; 3],
}

impl Crossover {
    fn new(hz: f32, sample_rate: u32) -> Self {
        let tuned = |q: f32| {
            let mut s = Svf::new(q);
            s.tune(hz.min(0.45 * sample_rate as f32), sample_rate);
            s
        };
        let rest = || [tuned(BW4[1]), tuned(BW4[0]), tuned(BW4[1])];
        Crossover {
            first: tuned(BW4[0]),
            low: rest(),
            high: rest(),
        }
    }

    fn split(&mut self, ch: usize, x: f32) -> (f32, f32) {
        let t = self.first.tick(ch, x);
        let low = self.low.iter_mut().fold(t.low, |y, s| s.tick(ch, y).low);
        let high = self.high.iter_mut().fold(t.high, |y, s| s.tick(ch, y).high);
        (low, high)
    }
}

/// The all-pass a [`Crossover`] at the same frequency gives its sum.
#[derive(Debug, Clone, Copy)]
struct AllPass([Svf; 2]);

impl AllPass {
    fn new(hz: f32, sample_rate: u32) -> Self {
        AllPass(BW4.map(|q| {
            let mut s = Svf::new(q);
            s.tune(hz.min(0.45 * sample_rate as f32), sample_rate);
            s
        }))
    }

    fn tick(&mut self, ch: usize, x: f32) -> f32 {
        self.0.iter_mut().fold(x, |y, s| s.tick(ch, y).all())
    }
}

/// The isolator: low, mid and high bands split at [`LOW_HZ`] and
/// [`HIGH_HZ`], each scaled, then summed. While every band is at unity it
/// passes the input and the filters rest; moving a band fades the bands in
/// over 10 ms, which hides the filters starting from where they rested.
#[derive(Debug, Clone)]
pub(crate) struct Eq {
    low: Crossover,
    high: Crossover,
    /// The low band through the high crossover's all-pass, so the bands
    /// sum flat.
    align: AllPass,
    gains: [Ramp<f32>; 3],
    /// 1 while any band is off unity; the output fades to the input at 0.
    wet: Ramp<f32>,
    smooth: u32,
}

impl Eq {
    pub(crate) fn new(sample_rate: u32) -> Self {
        Eq {
            low: Crossover::new(LOW_HZ, sample_rate),
            high: Crossover::new(HIGH_HZ, sample_rate),
            align: AllPass::new(HIGH_HZ, sample_rate),
            gains: [Ramp::new(1.0); 3],
            wet: Ramp::new(0.0),
            smooth: (SMOOTH_MS * sample_rate as f32 / 1000.0) as u32,
        }
    }

    /// Sets each band's gain, as a ratio.
    pub(crate) fn set(&mut self, gains: [f32; 3]) {
        for (r, g) in self.gains.iter_mut().zip(gains) {
            r.set(g, self.smooth);
        }
        let flat = gains.iter().all(|&g| g == 1.0);
        self.wet.set(if flat { 0.0 } else { 1.0 }, self.smooth);
    }

    pub(crate) fn tick(&mut self, x: [f32; 2]) -> [f32; 2] {
        if self.wet.settled() && self.wet.value() == 0.0 {
            return x;
        }
        let g = self.gains.each_mut().map(|r| r.next());
        let wet = self.wet.next();
        let mut out = x;
        for (ch, o) in out.iter_mut().enumerate() {
            let (low, rest) = self.low.split(ch, x[ch]);
            let (mid, high) = self.high.split(ch, rest);
            let low = self.align.tick(ch, low);
            if wet > 0.0 {
                let eq = g[0] * low + g[1] * mid + g[2] * high;
                *o = x[ch] + wet * (eq - x[ch]);
            }
        }
        out
    }
}

/// One knob: a low-pass left of centre, a high-pass right of it, and
/// nothing within [`DEAD`] of centre.
#[derive(Debug, Clone)]
pub(crate) struct Filter {
    svf: Svf,
    knob: Ramp<f32>,
    sample_rate: u32,
    smooth: u32,
}

impl Filter {
    pub(crate) fn new(sample_rate: u32) -> Self {
        Filter {
            svf: Svf::butterworth(),
            knob: Ramp::new(0.0),
            sample_rate,
            smooth: (SMOOTH_MS * sample_rate as f32 / 1000.0) as u32,
        }
    }

    /// Sets the knob, -1 to 1.
    pub(crate) fn set(&mut self, knob: f32) {
        self.knob.set(knob.clamp(-1.0, 1.0), self.smooth);
    }

    pub(crate) fn tick(&mut self, x: [f32; 2]) -> [f32; 2] {
        let knob = self.knob.next();
        let t = ((knob.abs() - DEAD) / (1.0 - DEAD)).max(0.0);
        if t == 0.0 {
            return x;
        }
        let (lo, hi) = FILTER_HZ;
        // Fully open at the deadband's edge: 20 kHz for the low-pass, 20 Hz
        // for the high-pass.
        let hz = match knob < 0.0 {
            true => hi * (lo / hi).powf(t),
            false => lo * (hi / lo).powf(t),
        };
        self.svf
            .tune(hz.min(0.45 * self.sample_rate as f32), self.sample_rate);
        let mut out = x;
        for (ch, o) in out.iter_mut().enumerate() {
            let taps = self.svf.tick(ch, x[ch]);
            *o = match (t > 0.0, knob < 0.0) {
                (false, _) => x[ch],
                (true, true) => taps.low,
                (true, false) => taps.high,
            };
        }
        out
    }
}
