//! Modulation: two LFOs routed through four slots to the voices', the EQs'
//! and the effects' settings, at a control rate. See "Modulation" in
//! `docs/dev/looper-engine.md`.

use crate::VOICES;

/// LFOs, and routes from them.
pub const LFOS: usize = 2;
pub const SLOTS: usize = 4;
/// Frames between control ticks. Each value ramps over the next tick.
pub const CONTROL: usize = 32;
/// Octaves either way a frequency moves at depth 1, and a rate.
pub const HZ_OCTAVES: f32 = 4.0;
pub const RATE_OCTAVES: f32 = 1.0;
/// The slowest and fastest free LFO, in Hz.
pub const LFO_HZ: (f32, f32) = (0.01, 20.0);

/// An LFO's wave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    #[default]
    Sine,
    Triangle,
    Square,
    /// Sample and hold: a new random value each cycle.
    Hold,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Sine, Shape::Triangle, Shape::Square, Shape::Hold];

    pub fn name(self) -> &'static str {
        match self {
            Shape::Sine => "sine",
            Shape::Triangle => "triangle",
            Shape::Square => "square",
            Shape::Hold => "hold",
        }
    }
}

/// How fast an LFO runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Speed {
    /// Free, in Hz, within [`LFO_HZ`].
    Hz(f32),
    /// One cycle per this part of voice 1's window, in phase with its head,
    /// as a synced delay follows the window.
    Sync(f32),
}

impl Default for Speed {
    fn default() -> Self {
        Speed::Hz(1.0)
    }
}

/// A setting an LFO moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dest {
    /// A voice's rate, by octaves; its window's start, by its length.
    Rate(usize),
    Start(usize),
    Pan(usize),
    /// Gains dip from their setting, never above it.
    Level(usize),
    Send(usize),
    /// EQ frequencies, by octaves; the bell's gain, by dB.
    HighPass(usize),
    LowPass(usize),
    BellHz(usize),
    BellGain(usize),
    DelaySend(usize),
    ReverbSend(usize),
    DelayTime,
    DelayFeedback,
    DelayTone,
    ReverbDamp,
    PrintFx,
}

impl Dest {
    /// The voice it belongs to, if any.
    pub fn voice(self) -> Option<usize> {
        use Dest::*;
        match self {
            Rate(v) | Start(v) | Pan(v) | Level(v) | Send(v) | HighPass(v) | LowPass(v)
            | BellHz(v) | BellGain(v) | DelaySend(v) | ReverbSend(v) => Some(v),
            DelayTime | DelayFeedback | DelayTone | ReverbDamp | PrintFx => None,
        }
    }
}

/// A route: LFO `lfo` moves `dest` by `depth`, -1 to 1; negative inverts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Route {
    pub lfo: usize,
    pub dest: Dest,
    pub depth: f32,
}

/// One LFO's state.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Lfo {
    pub shape: Shape,
    pub speed: Speed,
    /// 0 to 1.
    phase: f64,
    held: f32,
}

impl Lfo {
    /// Advances a control tick and returns the value, -1 to 1. `synced` is
    /// voice 1's place in its window, 0 to 1, for a synced LFO.
    pub fn tick(&mut self, sample_rate: u32, synced: Option<f64>, noise: &mut u32) -> f32 {
        let was = self.phase;
        self.phase = match self.speed {
            Speed::Hz(hz) => {
                let hz = f64::from(hz.clamp(LFO_HZ.0, LFO_HZ.1));
                (self.phase + hz * CONTROL as f64 / f64::from(sample_rate)).fract()
            }
            Speed::Sync(part) => match synced {
                Some(at) if part > 0.0 => (at / f64::from(part)).rem_euclid(1.0),
                _ => self.phase,
            },
        };
        // A new cycle, or a jump back, holds a new value.
        if self.phase < was || self.held == 0.0 {
            *noise ^= *noise << 13;
            *noise ^= *noise >> 17;
            *noise ^= *noise << 5;
            self.held = (*noise as f32 / u32::MAX as f32) * 2.0 - 1.0;
        }
        let p = self.phase as f32;
        match self.shape {
            Shape::Sine => (std::f32::consts::TAU * p).sin(),
            Shape::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
            Shape::Square => match p < 0.5 {
                true => 1.0,
                false => -1.0,
            },
            Shape::Hold => self.held,
        }
    }
}

/// What the routes come to on one tick, by destination: a sum of depth
/// times LFO for most, a product of [`dip`]s for gains.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Offsets {
    /// Rate, start, pan, level, send, high-pass, low-pass, bell Hz, bell
    /// gain, delay send and reverb send, by voice.
    pub voice: [[f32; 11]; VOICES],
    /// Delay time, feedback and tone, reverb damp, and print.
    pub fx: [f32; 5],
}

/// The voice slots and effect slots that are gains.
const VOICE_GAINS: [usize; 4] = [3, 4, 9, 10];
const FX_GAINS: [usize; 1] = [4];

impl Offsets {
    /// No change anywhere.
    pub fn new() -> Self {
        let mut voice = [[0.0; 11]; VOICES];
        for v in &mut voice {
            for k in VOICE_GAINS {
                v[k] = 1.0;
            }
        }
        let mut fx = [0.0; 5];
        for k in FX_GAINS {
            fx[k] = 1.0;
        }
        Offsets { voice, fx }
    }

    /// Takes route `depth` at LFO value `lfo` into `dest`.
    pub fn add(&mut self, dest: Dest, depth: f32, lfo: f32) {
        use Dest::*;
        let (voice, k) = match dest {
            Rate(v) => (Some(v), 0),
            Start(v) => (Some(v), 1),
            Pan(v) => (Some(v), 2),
            Level(v) => (Some(v), 3),
            Send(v) => (Some(v), 4),
            HighPass(v) => (Some(v), 5),
            LowPass(v) => (Some(v), 6),
            BellHz(v) => (Some(v), 7),
            BellGain(v) => (Some(v), 8),
            DelaySend(v) => (Some(v), 9),
            ReverbSend(v) => (Some(v), 10),
            DelayTime => (None, 0),
            DelayFeedback => (None, 1),
            DelayTone => (None, 2),
            ReverbDamp => (None, 3),
            PrintFx => (None, 4),
        };
        let (slot, gain) = match voice {
            Some(v) if v < VOICES => (&mut self.voice[v][k], VOICE_GAINS.contains(&k)),
            Some(_) => return,
            None => (&mut self.fx[k], FX_GAINS.contains(&k)),
        };
        match gain {
            true => *slot *= dip(depth, lfo),
            false => *slot += depth * lfo,
        }
    }
}

/// A gain's factor from a route of `depth` at LFO value `lfo`: 1 at the
/// LFO's peak, or its trough for a negative depth, falling to `1 - |depth|`
/// at the other, so a gain never rises above its setting.
pub(crate) fn dip(depth: f32, lfo: f32) -> f32 {
    let d = depth.clamp(-1.0, 1.0);
    1.0 - d.abs() * 0.5 * (1.0 - d.signum() * lfo)
}
