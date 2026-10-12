//! The send effects' settings, as the tape and the DJ decks share them: a
//! delay and a reverb, each with several models. The engines are
//! `playr_dsp::fx`; the design is in `docs/dev/fx-buses.md`.

use playr_dsp::fx::{BusSetting, DelayModel, DelayParam, ReverbModel, ReverbParam};

/// A part of a bar, which a synced delay's time follows: on the tape, of
/// voice 1's window; on the decks, of 4 beats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Division {
    Whole,
    Half,
    Quarter,
    Eighth,
    /// A dotted eighth.
    DottedEighth,
    Sixteenth,
}

impl Division {
    pub const ALL: [Division; 6] = [
        Division::Whole,
        Division::Half,
        Division::Quarter,
        Division::Eighth,
        Division::DottedEighth,
        Division::Sixteenth,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Division::Whole => "1/1",
            Division::Half => "1/2",
            Division::Quarter => "1/4",
            Division::Eighth => "1/8",
            Division::DottedEighth => "3/16",
            Division::Sixteenth => "1/16",
        }
    }

    pub fn fraction(self) -> f64 {
        match self {
            Division::Whole => 1.0,
            Division::Half => 0.5,
            Division::Quarter => 0.25,
            Division::Eighth => 0.125,
            Division::DottedEighth => 0.1875,
            Division::Sixteenth => 0.0625,
        }
    }
}

/// The delay's models by name, as commands and the saved tape write them.
pub const DELAY_MODELS: [(&str, DelayModel); 3] = [
    ("clean", DelayModel::Clean),
    ("ping-pong", DelayModel::PingPong),
    ("tape", DelayModel::Tape),
];

/// The reverb's models by name.
pub const REVERB_MODELS: [(&str, ReverbModel); 4] = [
    ("freeverb", ReverbModel::Freeverb),
    ("zita", ReverbModel::Zita),
    ("jp", ReverbModel::Jp),
    ("plate", ReverbModel::Plate),
];

/// The name `DELAY_MODELS` or `REVERB_MODELS` gives `model`.
pub fn model_name<M: PartialEq>(table: &[(&'static str, M)], model: M) -> &'static str {
    table.iter().find(|m| m.1 == model).expect("every model").0
}

/// A setting of the send effects. Ranges are checked where it is parsed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FxSetting {
    DelayModel(DelayModel),
    /// The delay's time in ms; sets Sync off.
    Time(f32),
    /// The delay's time as a part of voice 1's window, or off.
    Sync(Option<Division>),
    Feedback(f32),
    /// The low-pass in the delay's loop, in Hz.
    Tone(f32),
    /// The tape model's wow and flutter, 0 to 1.
    Wow(f32),
    DelayReturn(f32),
    ReverbModel(ReverbModel),
    Size(f32),
    /// Seconds to -60 dB.
    Decay(f32),
    Damp(f32),
    ReverbReturn(f32),
    /// How much of the returns the tape's write head records, 0 to 1.
    Print(f32),
}

impl FxSetting {
    /// The engine setting it is, but for Sync and Print, which are not the
    /// buses'.
    pub fn bus(self) -> Option<BusSetting> {
        Some(match self {
            FxSetting::DelayModel(m) => BusSetting::DelayModel(m),
            FxSetting::Time(ms) => BusSetting::Delay(DelayParam::Time, ms / 1000.0),
            FxSetting::Feedback(v) => BusSetting::Delay(DelayParam::Feedback, v),
            FxSetting::Tone(hz) => BusSetting::Delay(DelayParam::Tone, hz),
            FxSetting::Wow(v) => BusSetting::Delay(DelayParam::Wow, v),
            FxSetting::DelayReturn(v) => BusSetting::DelayReturn(v),
            FxSetting::ReverbModel(m) => BusSetting::ReverbModel(m),
            FxSetting::Size(v) => BusSetting::Reverb(ReverbParam::Size, v),
            FxSetting::Decay(v) => BusSetting::Reverb(ReverbParam::Decay, v),
            FxSetting::Damp(v) => BusSetting::Reverb(ReverbParam::Damp, v),
            FxSetting::ReverbReturn(v) => BusSetting::ReverbReturn(v),
            FxSetting::Sync(_) | FxSetting::Print(_) => return None,
        })
    }
}

/// The send effects' settings as last sent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FxState {
    pub delay_model: DelayModel,
    /// In ms.
    pub time: f32,
    pub sync: Option<Division>,
    pub feedback: f32,
    pub tone: f32,
    pub wow: f32,
    pub delay_return: f32,
    pub reverb_model: ReverbModel,
    pub size: f32,
    pub decay: f32,
    pub damp: f32,
    pub reverb_return: f32,
    pub print: f32,
}

impl Default for FxState {
    /// As `playr_dsp::fx::Buses::new` starts.
    fn default() -> Self {
        let mut fx = FxState {
            delay_model: DelayModel::DEFAULT,
            time: 0.0,
            sync: None,
            feedback: 0.0,
            tone: 0.0,
            wow: 0.0,
            delay_return: 1.0,
            reverb_model: ReverbModel::DEFAULT,
            size: 0.0,
            decay: 0.0,
            damp: 0.0,
            reverb_return: 1.0,
            print: 0.0,
        };
        for (p, v) in playr_looper::DELAY_DEFAULTS {
            fx.apply(BusSetting::Delay(p, v));
        }
        for (p, v) in playr_looper::REVERB_DEFAULTS {
            fx.apply(BusSetting::Reverb(p, v));
        }
        fx
    }
}

impl FxState {
    pub fn apply(&mut self, s: BusSetting) {
        match s {
            BusSetting::DelayModel(m) => self.delay_model = m,
            BusSetting::Delay(DelayParam::Time, x) => self.time = x * 1000.0,
            BusSetting::Delay(DelayParam::Feedback, x) => self.feedback = x,
            BusSetting::Delay(DelayParam::Tone, x) => self.tone = x,
            BusSetting::Delay(DelayParam::Wow, x) => self.wow = x,
            BusSetting::DelayReturn(x) => self.delay_return = x,
            BusSetting::ReverbModel(m) => self.reverb_model = m,
            BusSetting::Reverb(ReverbParam::Size, x) => self.size = x,
            BusSetting::Reverb(ReverbParam::Decay, x) => self.decay = x,
            BusSetting::Reverb(ReverbParam::Damp, x) => self.damp = x,
            BusSetting::ReverbReturn(x) => self.reverb_return = x,
        }
    }

    /// The settings that make a new tape's or mixer's effects these; Print
    /// FX is the tape's alone.
    pub fn settings(&self) -> [BusSetting; 11] {
        [
            BusSetting::DelayModel(self.delay_model),
            BusSetting::Delay(DelayParam::Time, self.time / 1000.0),
            BusSetting::Delay(DelayParam::Feedback, self.feedback),
            BusSetting::Delay(DelayParam::Tone, self.tone),
            BusSetting::Delay(DelayParam::Wow, self.wow),
            BusSetting::DelayReturn(self.delay_return),
            BusSetting::ReverbModel(self.reverb_model),
            BusSetting::Reverb(ReverbParam::Size, self.size),
            BusSetting::Reverb(ReverbParam::Decay, self.decay),
            BusSetting::Reverb(ReverbParam::Damp, self.damp),
            BusSetting::ReverbReturn(self.reverb_return),
        ]
    }
}

/// A field of the delay or the reverb, for [`idle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxField {
    DelayModel,
    Time,
    Sync,
    Feedback,
    Tone,
    Wow,
    DelayReturn,
    ReverbModel,
    Size,
    Decay,
    Damp,
    ReverbReturn,
}

/// Why `field` does nothing with the effects set as `fx`, when `fed` says
/// whether anything heard sends to the delay and to the reverb.
pub fn idle(fx: &FxState, fed: [bool; 2], field: FxField) -> Option<crate::tape::Idle> {
    use crate::tape::Idle;
    use FxField as F;
    let quiet = |b: usize| (!fed[b]).then_some(Idle::NoFxSend);
    match field {
        F::DelayModel | F::Time | F::Sync | F::Feedback | F::Tone | F::DelayReturn => quiet(0),
        F::Wow => quiet(0).or((fx.delay_model != DelayModel::Tape).then_some(Idle::Model)),
        F::ReverbModel | F::Damp | F::ReverbReturn => quiet(1),
        F::Size => quiet(1).or(
            matches!(fx.reverb_model, ReverbModel::Zita | ReverbModel::Plate)
                .then_some(Idle::Model),
        ),
        F::Decay => quiet(1).or((fx.reverb_model == ReverbModel::Freeverb).then_some(Idle::Model)),
    }
}
