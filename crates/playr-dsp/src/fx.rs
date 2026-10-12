//! The tape's send effects: a delay and a reverb, each with several models
//! to compare by ear. Stereo, wet only, a frame at a time; only the selected
//! model runs. Design in `docs/dev/fx-buses.md`.
//!
//! `Clean` and `Freeverb` are from `softcut-fx` in softcut-rs, by the same
//! author, under MIT. The rest are Faust ([`crate::faust`]).

use crate::faust::{self, Faust, ParamIndex};
use crate::{flush, one_pole, Ramp};

/// The longest delay, in seconds.
pub const DELAY_SECONDS: f32 = 2.0;
/// The shortest delay, in seconds.
pub const DELAY_MIN: f32 = 0.001;
pub const FEEDBACK_MAX: f32 = 0.95;
/// The delay's tone, a low-pass in its loop, in Hz.
pub const TONE_HZ: (f32, f32) = (500.0, 20000.0);
/// The reverb's decay to -60 dB, in seconds.
pub const DECAY_SECONDS: (f32, f32) = (0.1, 20.0);
/// A new [`Delay`]'s settings.
pub const DELAY_DEFAULTS: [(DelayParam, f32); 4] = [
    (DelayParam::Time, 0.375),
    (DelayParam::Feedback, 0.4),
    (DelayParam::Tone, 6000.0),
    (DelayParam::Wow, 0.3),
];
/// A new [`Reverb`]'s settings.
pub const REVERB_DEFAULTS: [(ReverbParam, f32); 3] = [
    (ReverbParam::Size, 0.5),
    (ReverbParam::Decay, 2.0),
    (ReverbParam::Damp, 0.3),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelayModel {
    /// Linear-interpolated repeats.
    Clean,
    /// Each repeat crosses to the other channel.
    PingPong,
    /// Wow and flutter on the read, a soft clip in the loop.
    Tape,
}

impl DelayModel {
    /// A new [`Delay`]'s model.
    pub const DEFAULT: DelayModel = DelayModel::Clean;
    pub const ALL: [DelayModel; 3] = [DelayModel::Clean, DelayModel::PingPong, DelayModel::Tape];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelayParam {
    /// Seconds, [`DELAY_MIN`] to [`DELAY_SECONDS`].
    Time,
    /// 0 to [`FEEDBACK_MAX`].
    Feedback,
    /// Hz, [`TONE_HZ`].
    Tone,
    /// 0 to 1; `Tape` only.
    Wow,
}

impl DelayParam {
    /// `value` kept in range, or `None` if not finite.
    pub fn clamp(self, value: f32) -> Option<f32> {
        let (lo, hi) = match self {
            DelayParam::Time => (DELAY_MIN, DELAY_SECONDS),
            DelayParam::Feedback => (0.0, FEEDBACK_MAX),
            DelayParam::Tone => TONE_HZ,
            DelayParam::Wow => (0.0, 1.0),
        };
        value.is_finite().then(|| value.clamp(lo, hi))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReverbModel {
    /// Jezar's Freeverb: 8 combs, 4 allpasses.
    Freeverb,
    /// Zita-rev1: an 8-line FDN.
    Zita,
    /// Julian Parker's modulated hall.
    Jp,
    /// Dattorro's plate.
    Plate,
}

impl ReverbModel {
    /// A new [`Reverb`]'s model.
    pub const DEFAULT: ReverbModel = ReverbModel::Zita;
    pub const ALL: [ReverbModel; 4] = [
        ReverbModel::Freeverb,
        ReverbModel::Zita,
        ReverbModel::Jp,
        ReverbModel::Plate,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReverbParam {
    /// 0 to 1; `Freeverb` and `Jp`.
    Size,
    /// Seconds to -60 dB, [`DECAY_SECONDS`]; not `Freeverb`.
    Decay,
    /// High-frequency damping, 0 to 1.
    Damp,
}

impl ReverbParam {
    /// `value` kept in range, or `None` if not finite.
    pub fn clamp(self, value: f32) -> Option<f32> {
        let (lo, hi) = match self {
            ReverbParam::Decay => DECAY_SECONDS,
            ReverbParam::Size | ReverbParam::Damp => (0.0, 1.0),
        };
        value.is_finite().then(|| value.clamp(lo, hi))
    }
}

/// Linear read `delay` frames (fractional) behind `write` in a power-of-two
/// ring. In f64: at f32 a position near 2^17 frames resolves only 1/128 of
/// a frame.
fn read(buf: &[[f32; 2]], write: usize, delay: f64, c: usize) -> f32 {
    let mask = buf.len() - 1;
    let pos = write as f64 - delay;
    let i = pos.floor();
    let frac = (pos - i) as f32;
    let i = (i as isize as usize) & mask;
    let (a, b) = (buf[i][c], buf[(i + 1) & mask][c]);
    a + (b - a) * frac
}

struct Clean {
    buf: Vec<[f32; 2]>,
    write: usize,
    rate: u32,
    /// Current delay in frames, gliding toward `target`. f64: in f32 the
    /// glide stalls short of the target, by ~9 frames at 2 s.
    time: f64,
    target: f64,
    glide: f64,
    feedback: f32,
    /// The loop's one-pole coefficient: 0 passes everything.
    damp: f32,
    lp: [f32; 2],
}

impl Clean {
    /// About 50 ms to settle.
    const GLIDE_SECONDS: f64 = 0.05;

    fn new(rate: u32) -> Self {
        let frames = ((DELAY_SECONDS * rate as f32) as usize + 2).next_power_of_two();
        Clean {
            buf: vec![[0.0; 2]; frames],
            write: 0,
            rate,
            time: 0.0,
            target: 0.0,
            glide: (-1.0 / (Self::GLIDE_SECONDS * f64::from(rate))).exp(),
            feedback: 0.0,
            damp: 0.0,
            lp: [0.0; 2],
        }
    }

    fn set(&mut self, param: DelayParam, value: f32) {
        match param {
            DelayParam::Time => self.target = f64::from(value * self.rate as f32),
            DelayParam::Feedback => self.feedback = value,
            DelayParam::Tone => self.damp = 1.0 - one_pole(value, self.rate),
            DelayParam::Wow => {}
        }
    }

    fn frame(&mut self, x: [f32; 2]) -> [f32; 2] {
        self.time = self.target + (self.time - self.target) * self.glide;
        // Land exactly: a residual fraction dulls the echo through the
        // linear interpolation.
        if (self.time - self.target).abs() < 1e-6 {
            self.time = self.target;
        }
        let mut out = [0.0; 2];
        for c in 0..2 {
            let wet = read(&self.buf, self.write, self.time, c);
            self.lp[c] = flush(wet + (self.lp[c] - wet) * self.damp);
            self.buf[self.write][c] = flush(x[c] + self.lp[c] * self.feedback);
            out[c] = wet;
        }
        self.write = (self.write + 1) & (self.buf.len() - 1);
        out
    }

    fn clear(&mut self) {
        self.buf.fill([0.0; 2]);
        self.lp = [0.0; 2];
        self.time = self.target;
    }
}

/// Freeverb's low-pass feedback comb.
struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

impl Comb {
    fn tick(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let y = self.buf[self.i];
        self.store = flush(y + (self.store - y) * damp);
        self.buf[self.i] = flush(x + self.store * feedback);
        self.i = (self.i + 1) % self.buf.len();
        y
    }
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

impl Allpass {
    fn tick(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = flush(x + b * 0.5);
        self.i = (self.i + 1) % self.buf.len();
        b - x
    }
}

/// Freeverb (Jezar at Dreampoint, public domain): 8 parallel combs into 4
/// series allpasses per channel, the right channel's 23 samples longer.
struct Freeverb {
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<Allpass>; 2],
    size: f32,
    damp: f32,
}

impl Freeverb {
    const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
    const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
    const SPREAD: usize = 23;
    const INPUT_GAIN: f32 = 0.015;
    const WET_GAIN: f32 = 3.0;

    fn new(rate: u32) -> Self {
        // Freeverb's lengths are for 44.1 kHz.
        let len = |n: usize| ((n as f32 * rate as f32 / 44100.0).round() as usize).max(1);
        Freeverb {
            combs: std::array::from_fn(|c| {
                Self::COMBS
                    .iter()
                    .map(|&n| Comb {
                        buf: vec![0.0; len(n + c * Self::SPREAD)],
                        i: 0,
                        store: 0.0,
                    })
                    .collect()
            }),
            allpasses: std::array::from_fn(|c| {
                Self::ALLPASSES
                    .iter()
                    .map(|&n| Allpass {
                        buf: vec![0.0; len(n + c * Self::SPREAD)],
                        i: 0,
                    })
                    .collect()
            }),
            size: 0.5,
            damp: 0.3,
        }
    }

    fn frame(&mut self, [l, r]: [f32; 2]) -> [f32; 2] {
        let input = (l + r) * 0.5 * Self::INPUT_GAIN;
        let feedback = self.size * 0.28 + 0.7;
        let damp = self.damp * 0.4;
        std::array::from_fn(|c| {
            let mut y: f32 = self.combs[c]
                .iter_mut()
                .map(|comb| comb.tick(input, feedback, damp))
                .sum();
            for ap in &mut self.allpasses[c] {
                y = ap.tick(y);
            }
            y * Self::WET_GAIN
        })
    }

    fn clear(&mut self) {
        for comb in self.combs.iter_mut().flatten() {
            comb.buf.fill(0.0);
            comb.store = 0.0;
        }
        for ap in self.allpasses.iter_mut().flatten() {
            ap.buf.fill(0.0);
        }
    }
}

/// The delay bus's models, all built, one running.
pub struct Delay {
    model: DelayModel,
    clean: Clean,
    pingpong: Faust<faust::pingpong::Dsp>,
    tape: Faust<faust::tape_delay::Dsp>,
    /// Each Faust model's indices for Time, Feedback and Tone; Wow last.
    pingpong_params: [ParamIndex; 3],
    tape_params: [ParamIndex; 4],
}

impl Delay {
    /// Allocates about 10 MB at 192 kHz; build it off the audio thread.
    pub fn new(rate: u32) -> Self {
        type P = faust::pingpong::Dsp;
        type T = faust::tape_delay::Dsp;
        let mut delay = Delay {
            model: DelayModel::DEFAULT,
            clean: Clean::new(rate),
            pingpong: Faust::new(rate),
            tape: Faust::new(rate),
            pingpong_params: ["time", "feedback", "tone"].map(Faust::<P>::param),
            tape_params: ["time", "feedback", "tone", "wow"].map(Faust::<T>::param),
        };
        for (param, value) in DELAY_DEFAULTS {
            delay.set(param, value);
        }
        delay.clean.clear();
        delay
    }

    pub fn model(&self) -> DelayModel {
        self.model
    }

    /// Selects a model, which starts silent.
    pub fn set_model(&mut self, model: DelayModel) {
        if model != self.model {
            self.model = model;
            self.clear();
        }
    }

    /// Sets `param` on every model, so a switch keeps the settings. A value
    /// not finite is ignored.
    pub fn set(&mut self, param: DelayParam, value: f32) {
        let Some(value) = param.clamp(value) else {
            return;
        };
        self.clean.set(param, value);
        let i = param as usize;
        if let Some(&p) = self.pingpong_params.get(i) {
            self.pingpong.set(p, value);
        }
        self.tape.set(self.tape_params[i], value);
    }

    /// Puts the time at its target without a glide, for a bus resuming
    /// from silence. The Faust models do so after every clear.
    pub fn settle(&mut self) {
        self.clean.time = self.clean.target;
    }

    /// Silences the running model's tail.
    pub fn clear(&mut self) {
        match self.model {
            DelayModel::Clean => self.clean.clear(),
            DelayModel::PingPong => self.pingpong.clear(),
            DelayModel::Tape => self.tape.clear(),
        }
    }

    /// The wet signal for one frame in.
    pub fn frame(&mut self, x: [f32; 2]) -> [f32; 2] {
        match self.model {
            DelayModel::Clean => self.clean.frame(x),
            DelayModel::PingPong => self.pingpong.frame(x),
            DelayModel::Tape => self.tape.frame(x),
        }
    }
}

/// The reverb bus's models, all built, one running.
pub struct Reverb {
    model: ReverbModel,
    freeverb: Freeverb,
    zita: Faust<faust::zita::Dsp>,
    jp: Faust<faust::jp::Dsp>,
    plate: Faust<faust::plate::Dsp>,
    /// Decay and Damp for each Faust model; JP's Size last.
    zita_params: [ParamIndex; 2],
    jp_params: [ParamIndex; 3],
    plate_params: [ParamIndex; 2],
}

impl Reverb {
    /// JP's size at Size 0 and 1.
    const JP_SIZE: (f32, f32) = (0.5, 5.0);

    /// Allocates; build it off the audio thread.
    pub fn new(rate: u32) -> Self {
        let names = ["decay", "damp"];
        let mut reverb = Reverb {
            model: ReverbModel::DEFAULT,
            freeverb: Freeverb::new(rate),
            zita: Faust::new(rate),
            jp: Faust::new(rate),
            plate: Faust::new(rate),
            zita_params: names.map(Faust::<faust::zita::Dsp>::param),
            jp_params: ["decay", "damp", "size"].map(Faust::<faust::jp::Dsp>::param),
            plate_params: names.map(Faust::<faust::plate::Dsp>::param),
        };
        for (param, value) in REVERB_DEFAULTS {
            reverb.set(param, value);
        }
        reverb
    }

    pub fn model(&self) -> ReverbModel {
        self.model
    }

    /// Selects a model, which starts silent.
    pub fn set_model(&mut self, model: ReverbModel) {
        if model != self.model {
            self.model = model;
            self.clear();
        }
    }

    /// Sets `param` on every model that has it. A value not finite is
    /// ignored.
    pub fn set(&mut self, param: ReverbParam, value: f32) {
        let Some(value) = param.clamp(value) else {
            return;
        };
        match param {
            ReverbParam::Size => {
                self.freeverb.size = value;
                let (lo, hi) = Self::JP_SIZE;
                self.jp.set(self.jp_params[2], lo + (hi - lo) * value);
            }
            ReverbParam::Decay | ReverbParam::Damp => {
                let i = (param == ReverbParam::Damp) as usize;
                if param == ReverbParam::Damp {
                    self.freeverb.damp = value;
                }
                self.zita.set(self.zita_params[i], value);
                self.jp.set(self.jp_params[i], value);
                self.plate.set(self.plate_params[i], value);
            }
        }
    }

    /// Silences the running model's tail.
    pub fn clear(&mut self) {
        match self.model {
            ReverbModel::Freeverb => self.freeverb.clear(),
            ReverbModel::Zita => self.zita.clear(),
            ReverbModel::Jp => self.jp.clear(),
            ReverbModel::Plate => self.plate.clear(),
        }
    }

    /// The wet signal for one frame in.
    pub fn frame(&mut self, x: [f32; 2]) -> [f32; 2] {
        match self.model {
            ReverbModel::Freeverb => self.freeverb.frame(x),
            ReverbModel::Zita => self.zita.frame(x),
            ReverbModel::Jp => self.jp.frame(x),
            ReverbModel::Plate => self.plate.frame(x),
        }
    }
}

/// A bus with no input and its output below -120 dB for this long is
/// skipped, and its tail cleared: the dry signal is then bit-exact.
const BUS_QUIET_SECONDS: f32 = 1.0;
const BUS_FLOOR: f32 = 1e-6;

/// A setting of [`Buses`]. Values out of range are clamped; values not
/// finite are ignored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BusSetting {
    /// Selects the delay's model; the new one starts silent.
    DelayModel(DelayModel),
    Delay(DelayParam, f32),
    /// The delay's level into the mix, 0 to 1.
    DelayReturn(f32),
    ReverbModel(ReverbModel),
    Reverb(ReverbParam, f32),
    ReverbReturn(f32),
}

/// A delay and a reverb, each a send bus with a return level, as the tape
/// and the DJ mixer share them. A bus that hears nothing is skipped.
pub struct Buses {
    delay: Delay,
    reverb: Reverb,
    /// Each bus's level into the mix.
    ret: [Ramp<f32>; 2],
    /// Frames each bus has had no input and its output below [`BUS_FLOOR`].
    quiet: [u32; 2],
    /// Quiet frames after which a bus is skipped.
    idle: u32,
}

impl Buses {
    /// Allocates megabytes; build them off the audio thread.
    pub fn new(sample_rate: u32) -> Self {
        let idle = (BUS_QUIET_SECONDS * sample_rate as f32) as u32;
        Buses {
            delay: Delay::new(sample_rate),
            reverb: Reverb::new(sample_rate),
            ret: [Ramp::new(1.0); 2],
            quiet: [idle; 2],
            idle,
        }
    }

    /// Applies `s`; a return moves over `ramp` frames.
    pub fn set(&mut self, s: BusSetting, ramp: u32) {
        let unit = |v: f32| v.is_finite().then(|| v.clamp(0.0, 1.0));
        match s {
            BusSetting::DelayModel(m) => self.delay.set_model(m),
            BusSetting::Delay(p, v) => self.delay.set(p, v),
            BusSetting::ReverbModel(m) => self.reverb.set_model(m),
            BusSetting::Reverb(p, v) => self.reverb.set(p, v),
            BusSetting::DelayReturn(v) | BusSetting::ReverbReturn(v) => {
                let b = matches!(s, BusSetting::ReverbReturn(_)) as usize;
                if let Some(v) = unit(v) {
                    self.ret[b].set(v, ramp);
                }
            }
        }
    }

    /// Each bus's return for one frame in, delay then reverb, at its level;
    /// silence from a skipped bus.
    pub fn frame(&mut self, input: [[f32; 2]; 2]) -> [[f32; 2]; 2] {
        let mut out = [[0.0; 2]; 2];
        for (b, x) in input.into_iter().enumerate() {
            let level = self.ret[b].next();
            let loud = x != [0.0; 2];
            let idle = self.quiet[b] >= self.idle;
            if !loud && idle {
                continue;
            }
            if idle && b == 0 {
                self.delay.settle();
            }
            let wet = match b {
                0 => self.delay.frame(x),
                _ => self.reverb.frame(x),
            };
            match loud || wet.iter().any(|w| w.abs() > BUS_FLOOR) {
                true => self.quiet[b] = 0,
                false => {
                    self.quiet[b] += 1;
                    if self.quiet[b] == self.idle {
                        match b {
                            0 => self.delay.clear(),
                            _ => self.reverb.clear(),
                        }
                    }
                }
            }
            out[b] = [wet[0] * level, wet[1] * level];
        }
        out
    }
}

impl std::fmt::Debug for Buses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Buses")
            .field("delay", &self.delay.model())
            .field("reverb", &self.reverb.model())
            .field("quiet", &self.quiet)
            .finish_non_exhaustive()
    }
}
