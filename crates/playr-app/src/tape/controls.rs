//! Whether a control of the Tape tab would change what is heard, and the engine settings a `:tape` action becomes.

use super::*;

/// A control of the Tape tab, for [`no_effect`]. Voices count from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Window(usize),
    Rate(usize),
    Level(usize),
    Pan(usize),
    Send(usize),
    Wear(usize),
    Fade(usize),
    Ping(usize),
    Slew(usize),
    Drive(usize),
    HighPass(usize),
    LowPass(usize),
    BellHz(usize),
    BellGain(usize),
    BellQ(usize),
    Solo(usize),
    Mute(usize),
    DelaySend(usize),
    ReverbSend(usize),
    Write,
    WriteWindow,
    Feedback,
    WriteWear,
    Thin,
    PrintFx,
    /// A field of the delay or the reverb.
    Fx(crate::fx::FxField),
}

/// Why a control does nothing as the tape is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Idle {
    VoiceOff,
    WriteOff,
    NoSend,
    Silent,
    Muted,
    /// The bell's gain is 0 dB.
    Flat,
    Still,
    /// No pre-roll or post-roll for a crossfade to read.
    NoRoom,
    /// Writing on, with nothing that changes the loop: feedback 1, wear 0,
    /// thin 0 and every voice playing sending nothing.
    Unchanging,
    /// Ping turns the head at the window's edges, so it never wraps.
    Turns,
    /// No voice heard sends to the effect.
    NoFxSend,
    /// The effect's selected model has no such field.
    Model,
}

/// Why `control` has no effect with the tape set as `s`, over a loop of
/// shape `e`, or `None` when it has one.
pub fn no_effect(s: &TapeState, e: Extent, control: Control) -> Option<Idle> {
    let voice = |i: usize| &s.voices[i];
    let off = |i: usize| (!voice(i).on).then_some(Idle::VoiceOff);
    let write_off = || (!s.write).then_some(Idle::WriteOff);
    match control {
        Control::Window(i)
        | Control::Rate(i)
        | Control::Slew(i)
        | Control::Drive(i)
        | Control::HighPass(i)
        | Control::LowPass(i)
        | Control::BellGain(i) => off(i),
        Control::BellHz(i) | Control::BellQ(i) => {
            off(i).or((voice(i).bell_gain == 0.0).then_some(Idle::Flat))
        }
        Control::Ping(i) => off(i).or((voice(i).rate == 0.0).then_some(Idle::Still)),
        // A voice off still silences the others when soloed.
        Control::Solo(_) => None,
        Control::Mute(i) => off(i),
        Control::Level(i) => off(i).or(voice(i).mute.then_some(Idle::Muted)),
        Control::Pan(i) => off(i)
            .or(voice(i).mute.then_some(Idle::Muted))
            .or((voice(i).level == 0.0).then_some(Idle::Silent)),
        Control::Send(i) => off(i).or_else(write_off),
        Control::Wear(i) => off(i)
            .or_else(write_off)
            .or((voice(i).send == 0.0).then_some(Idle::NoSend)),
        Control::Fade(i) => {
            let v = voice(i);
            let x = playr_looper::crossfade(v.window, v.window, e.frames, v.rate, 1000.0, e.rate);
            off(i)
                .or((v.rate == 0.0).then_some(Idle::Still))
                .or(v.ping.then_some(Idle::Turns))
                .or((x.frames == 0).then_some(Idle::NoRoom))
        }
        Control::Write => {
            let sending = s.voices.iter().any(|v| v.on && v.send > 0.0);
            let still = s.feedback == 1.0 && s.wear == 0.0 && s.thin == 0.0;
            (s.write && still && !sending).then_some(Idle::Unchanging)
        }
        Control::WriteWindow | Control::Feedback | Control::WriteWear | Control::Thin => {
            write_off()
        }
        // Sends are after the level: an effect hears the voice as heard.
        Control::DelaySend(i) | Control::ReverbSend(i) => off(i)
            .or(voice(i).mute.then_some(Idle::Muted))
            .or((voice(i).level == 0.0).then_some(Idle::Silent)),
        Control::PrintFx => write_off().or_else(|| {
            let sent = |b| fed(s, b);
            (!sent(0) && !sent(1)).then_some(Idle::NoFxSend)
        }),
        Control::Fx(field) => crate::fx::idle(&s.fx, [fed(s, 0), fed(s, 1)], field),
    }
}

/// Whether a voice that is heard sends to bus `b`: 0 the delay, 1 the reverb.
pub fn fed(s: &TapeState, b: usize) -> bool {
    s.voices.iter().zip(s.empty).any(|(v, empty)| {
        let amount = [v.delay, v.reverb][b];
        !empty && v.on && !v.mute && v.level > 0.0 && amount > 0.0
    })
}

/// The looper settings `action` makes on a loop of shape `e`. A window's
/// ends count from the range's start: a percentage of the range, or a time;
/// past either end of the range they reach into the pre-roll or post-roll.
pub fn settings(action: TapeAction, e: Extent) -> Result<Vec<Setting>, TapeMessage> {
    let frame = |p: Pos| {
        let f = e.range.start as f64
            + match p {
                Pos::Percent(p) => f64::from(p) / 100.0 * e.range.len() as f64,
                Pos::Time(d) => d.as_secs_f64() * f64::from(e.rate),
            };
        (f.round().max(0.0) as usize).min(e.frames)
    };
    let window = |a, b| match (frame(a), frame(b)) {
        (a, b) if a < b => Ok(Window::new(a, b)),
        _ => Err(TapeMessage::EmptyWindow),
    };
    Ok(vec![match action {
        TapeAction::Play => Setting::Play,
        TapeAction::Stop => Setting::Stop,
        TapeAction::Write(on) => Setting::Write(on),
        TapeAction::WriteWindow(a, b) => Setting::WriteWindow(window(a, b)?),
        TapeAction::Feedback(v) => Setting::Feedback(v),
        TapeAction::Wear(v) => Setting::Wear(v),
        TapeAction::Thin(v) => Setting::Thin(v),
        TapeAction::Voice(n, s) => {
            let i = usize::from(n) - 1;
            match s {
                VoiceSetting::On(on) => Setting::On(i, on),
                VoiceSetting::Rate(r) => Setting::Rate(i, r),
                VoiceSetting::Window(a, b) => Setting::Window(i, window(a, b)?),
                VoiceSetting::Level(v) => Setting::Level(i, v),
                VoiceSetting::Pan(v) => Setting::Pan(i, v),
                VoiceSetting::Send(v) => Setting::Send(i, v),
                VoiceSetting::Wear(v) => Setting::VoiceWear(i, v),
                VoiceSetting::Fade(ms) => Setting::Fade(i, ms),
                VoiceSetting::Ping(on) => Setting::Ping(i, on),
                VoiceSetting::Slew(ms) => Setting::Slew(i, ms),
                VoiceSetting::Drive(v) => Setting::Drive(i, v),
                VoiceSetting::HighPass(v) => Setting::HighPass(i, v),
                VoiceSetting::LowPass(v) => Setting::LowPass(i, v),
                VoiceSetting::BellHz(v) => Setting::BellHz(i, v),
                VoiceSetting::BellGain(v) => Setting::BellGain(i, v),
                VoiceSetting::BellQ(v) => Setting::BellQ(i, v),
                VoiceSetting::Solo(on) => Setting::Solo(i, on),
                VoiceSetting::Mute(on) => Setting::Mute(i, on),
                VoiceSetting::Delay(v) => Setting::DelaySend(i, v),
                VoiceSetting::Reverb(v) => Setting::ReverbSend(i, v),
            }
        }
        // Sync sets the time from voice 1's window where the tape is known.
        TapeAction::Fx(FxSetting::Sync(_)) => return Ok(Vec::new()),
        TapeAction::Fx(FxSetting::Print(v)) => Setting::PrintFx(v),
        TapeAction::Fx(fx) => Setting::Fx(fx.bus().expect("a bus setting")),
        TapeAction::Lfo(k, LfoSetting::Shape(shape)) => Setting::LfoShape(usize::from(k), shape),
        TapeAction::Lfo(k, LfoSetting::Hz(hz)) => Setting::LfoSpeed(usize::from(k), Speed::Hz(hz)),
        TapeAction::Lfo(k, LfoSetting::Sync(d)) => {
            Setting::LfoSpeed(usize::from(k), Speed::Sync(d.fraction() as f32))
        }
        TapeAction::Route(n, route) => Setting::Route(usize::from(n).saturating_sub(1), route),
        TapeAction::Load(_)
        | TapeAction::Take
        | TapeAction::Reset
        | TapeAction::Clear(_)
        | TapeAction::Eject(_)
        | TapeAction::Share(_)
        | TapeAction::Loop(..)
        | TapeAction::Save
        | TapeAction::Record
        | TapeAction::Bounce(_)
        | TapeAction::Fold(_)
        | TapeAction::Drawer(_) => return Ok(Vec::new()),
    }])
}
