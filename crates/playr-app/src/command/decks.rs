//! Parsing and writing back the `:tape`, `:dj` and `:mix` commands, and the
//! delay and reverb words they share.

use super::*;

/// Parses the words of `:tape` or `:dj` that set the delay or the reverb:
/// `delay model|time|sync|feedback|tone|wow|return` and
/// `reverb model|size|decay|damp|return`.
pub(super) fn fx(words: &[&str]) -> Result<crate::fx::FxSetting, String> {
    use crate::fx::FxSetting as F;
    use playr_looper::{DECAY_SECONDS, DELAY_MIN, DELAY_SECONDS, FEEDBACK_MAX, TONE_HZ};
    let value = |w: &str, what: &str, lo: f32, hi: f32| {
        w.parse::<f32>()
            .ok()
            .filter(|v| (lo..=hi).contains(v))
            .ok_or_else(|| format!("{what} is {} to {}", number(lo.into()), number(hi.into())))
    };
    match words {
        ["delay", "model", m] => {
            choose(m, &crate::fx::DELAY_MODELS, "delay model").map(F::DelayModel)
        }
        ["delay", "time", ms] => {
            value(ms, "time", DELAY_MIN * 1000.0, DELAY_SECONDS * 1000.0).map(F::Time)
        }
        ["delay", "sync", "off"] => Ok(F::Sync(None)),
        ["delay", "sync", d] => crate::fx::Division::ALL
            .into_iter()
            .find(|x| x.name() == *d)
            .map(|d| F::Sync(Some(d)))
            .ok_or_else(|| "sync is off, 1/1, 1/2, 1/4, 1/8, 3/16 or 1/16".to_string()),
        ["delay", "feedback", v] => value(v, "feedback", 0.0, FEEDBACK_MAX).map(F::Feedback),
        ["delay", "tone", v] => value(v, "tone", TONE_HZ.0, TONE_HZ.1).map(F::Tone),
        ["delay", "wow", v] => value(v, "wow", 0.0, 1.0).map(F::Wow),
        ["delay", "return", v] => value(v, "return", 0.0, 1.0).map(F::DelayReturn),
        ["reverb", "model", m] => {
            choose(m, &crate::fx::REVERB_MODELS, "reverb model").map(F::ReverbModel)
        }
        ["reverb", "size", v] => value(v, "size", 0.0, 1.0).map(F::Size),
        ["reverb", "decay", v] => value(v, "decay", DECAY_SECONDS.0, DECAY_SECONDS.1).map(F::Decay),
        ["reverb", "damp", v] => value(v, "damp", 0.0, 1.0).map(F::Damp),
        ["reverb", "return", v] => value(v, "return", 0.0, 1.0).map(F::ReverbReturn),
        _ => Err(FX_USAGE.into()),
    }
}

/// What the delay and the reverb take, in `:tape` and `:dj`.
const FX_USAGE: &str = "usage: delay model clean|ping-pong|tape|time MS|\
     sync off|1/1|1/2|1/4|1/8|3/16|1/16|feedback F|tone HZ|wow W|return R | \
     reverb model freeverb|zita|jp|plate|size S|decay SEC|damp D|return R";

/// What follows `delay ` or `reverb ` in a command line for `f`.
pub(super) fn fx_line(f: &crate::fx::FxSetting) -> String {
    use crate::fx::{model_name, FxSetting as F, DELAY_MODELS, REVERB_MODELS};
    let n = |v: &f32| number(f64::from(*v));
    match f {
        F::DelayModel(m) => format!("delay model {}", model_name(&DELAY_MODELS, *m)),
        F::Time(v) => format!("delay time {}", n(v)),
        F::Sync(None) => "delay sync off".into(),
        F::Sync(Some(d)) => format!("delay sync {}", d.name()),
        F::Feedback(v) => format!("delay feedback {}", n(v)),
        F::Tone(v) => format!("delay tone {}", n(v)),
        F::Wow(v) => format!("delay wow {}", n(v)),
        F::DelayReturn(v) => format!("delay return {}", n(v)),
        F::ReverbModel(m) => format!("reverb model {}", model_name(&REVERB_MODELS, *m)),
        F::Size(v) => format!("reverb size {}", n(v)),
        F::Decay(v) => format!("reverb decay {}", n(v)),
        F::Damp(v) => format!("reverb damp {}", n(v)),
        F::ReverbReturn(v) => format!("reverb return {}", n(v)),
        F::Print(v) => format!("print-fx {}", n(v)),
    }
}

/// LFO `a` or `b`, as 0 or 1.
pub(super) fn lfo_index(word: &str) -> Result<u8, String> {
    match word {
        "a" => Ok(0),
        "b" => Ok(1),
        _ => Err("LFOs are a and b".into()),
    }
}

/// Parses what follows `:tape`. Voices count from 1; windows take a time or
/// a percentage of the loop at each end.
pub(super) fn tape(rest: &str) -> Result<crate::tape::TapeAction, String> {
    use crate::tape::{FxSetting as F, TapeAction as T, VoiceSetting as V};
    use playr_looper::{BELL_DB, BELL_Q, FILTER_HZ};
    let usage = || TAPE_USAGE.to_string();
    let value = |w: &str, what: &str, lo: f32, hi: f32| {
        w.parse::<f32>()
            .ok()
            .filter(|v| (lo..=hi).contains(v))
            .ok_or_else(|| format!("{what} is {} to {}", number(lo.into()), number(hi.into())))
    };
    let on = |w: &str| match w {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(usage()),
    };
    let words: Vec<&str> = rest.split_whitespace().collect();
    match words.as_slice() {
        ["load"] => Ok(T::Load(None)),
        ["take"] => Ok(T::Take),
        ["load", n] => match n.parse::<u8>() {
            Ok(n) if (1..=playr_core::session::LOOP_SLOTS).contains(&n) => Ok(T::Load(Some(n))),
            _ => Err(format!("not a loop slot: {n}")),
        },
        ["play"] => Ok(T::Play),
        ["stop"] => Ok(T::Stop),
        ["reset"] => Ok(T::Reset),
        ["save"] => Ok(T::Save),
        ["rec"] => Ok(T::Record),
        ["bounce"] => Ok(T::Bounce(Some(crate::tape::Bounce::Pass))),
        ["bounce", "long"] => Ok(T::Bounce(Some(crate::tape::Bounce::Long))),
        ["bounce", "off"] => Ok(T::Bounce(None)),
        ["write", w] => on(w).map(T::Write),
        ["feedback", v] => value(v, "feedback", 0.0, 1.0).map(T::Feedback),
        ["wear", v] => value(v, "wear", 0.0, 1.0).map(T::Wear),
        ["thin", v] => value(v, "thin", 0.0, 1.0).map(T::Thin),
        ["window", a, b] => Ok(T::WriteWindow(tape_pos(a)?, tape_pos(b)?)),
        ["print-fx", v] => value(v, "print-fx", 0.0, 1.0).map(|v| T::Fx(F::Print(v))),
        ["delay" | "reverb", ..] => fx(&words).map(T::Fx),
        ["fx"] => Ok(T::Fold(None)),
        ["lfo", k, rest @ ..] => {
            use crate::tape::{LfoSetting as L, Shape, LFO_HZ};
            let k = lfo_index(k)?;
            match rest {
                ["sync", d] => crate::fx::Division::ALL
                    .into_iter()
                    .find(|x| x.name() == *d)
                    .map(|d| T::Lfo(k, L::Sync(d)))
                    .ok_or_else(|| "sync is 1/1, 1/2, 1/4, 1/8, 3/16 or 1/16".to_string()),
                [w] => match Shape::ALL.into_iter().find(|s| s.name() == *w) {
                    Some(shape) => Ok(T::Lfo(k, L::Shape(shape))),
                    None => value(w, "an LFO's rate in Hz", LFO_HZ.0, LFO_HZ.1)
                        .map(|hz| T::Lfo(k, L::Hz(hz))),
                },
                _ => Err(usage()),
            }
        }
        ["mod", n, rest @ ..] => {
            let slot = match n.parse::<u8>() {
                Ok(n) if (1..=crate::tape::SLOTS as u8).contains(&n) => n,
                _ => return Err(format!("slots are 1 to {}", crate::tape::SLOTS)),
            };
            let (lfo, dest, voice, depth) = match rest {
                ["off"] => return Ok(T::Route(slot, None)),
                [lfo, dest, v, depth] => (lfo, dest, v.parse::<u8>().ok(), depth),
                [lfo, dest, depth] => (lfo, dest, None, depth),
                _ => return Err(usage()),
            };
            let dest = crate::tape::dest_named(dest, voice)
                .ok_or_else(|| format!("no destination {dest}; see :help"))?;
            let depth = value(depth, "depth", -1.0, 1.0)?;
            let lfo = usize::from(lfo_index(lfo)?);
            Ok(T::Route(
                slot,
                Some(crate::tape::Route { lfo, dest, depth }),
            ))
        }
        [n, setting @ ..] if n.parse::<u8>().is_ok() => {
            let n: u8 = n.parse().expect("checked");
            if !(1..=playr_looper::VOICES as u8).contains(&n) {
                return Err(format!("voices are 1 to {}", playr_looper::VOICES));
            }
            let s = match setting {
                ["clear"] => return Ok(T::Clear(n)),
                ["eject"] => return Ok(T::Eject(n)),
                ["fold"] => return Ok(T::Fold(Some(n))),
                ["more"] => return Ok(T::Drawer(n)),
                ["share"] if usize::from(n) - 1 == playr_looper::WRITER => {
                    return Err(format!("voice {n} plays the write loop"))
                }
                ["share"] => return Ok(T::Share(n)),
                ["loop", id] => match id.parse::<i64>() {
                    Ok(id) => return Ok(T::Loop(n, id)),
                    Err(_) => return Err(format!("not a loop id: {id}")),
                },
                [w] => V::On(on(w)?),
                ["rate", r] => V::Rate(value(r, "rate", -4.0, 4.0)?),
                ["window", a, b] => V::Window(tape_pos(a)?, tape_pos(b)?),
                ["level", v] => V::Level(value(v, "level", 0.0, 1.0)?),
                ["pan", v] => V::Pan(value(v, "pan", -1.0, 1.0)?),
                ["send", v] => V::Send(value(v, "send", 0.0, 1.0)?),
                ["wear", v] => V::Wear(value(v, "wear", 0.0, 1.0)?),
                ["fade", ms] => V::Fade(value(ms, "fade", 0.0, 1000.0)?),
                ["ping", w] => V::Ping(on(w)?),
                ["slew", ms] => V::Slew(value(ms, "slew", 0.0, playr_looper::MAX_SLEW_MS)?),
                ["drive", v] => V::Drive(value(v, "drive", 0.0, 1.0)?),
                ["hp", "off"] => V::HighPass(FILTER_HZ.0),
                ["lp", "off"] => V::LowPass(FILTER_HZ.1),
                ["hp", v] => V::HighPass(value(v, "hp", FILTER_HZ.0, FILTER_HZ.1)?),
                ["lp", v] => V::LowPass(value(v, "lp", FILTER_HZ.0, FILTER_HZ.1)?),
                ["bell", v] => V::BellHz(value(v, "bell", FILTER_HZ.0, FILTER_HZ.1)?),
                ["bell-gain", v] => V::BellGain(value(v, "bell-gain", -BELL_DB, BELL_DB)?),
                ["bell-q", v] => V::BellQ(value(v, "bell-q", BELL_Q.0, BELL_Q.1)?),
                // The single filter of 0.18 became the high-pass and low-pass.
                ["filter", ..] => {
                    return Err("the filter is now hp HZ and lp HZ, with a bell: bell HZ, \
                         bell-gain DB, bell-q Q"
                        .into())
                }
                ["solo", w] => V::Solo(on(w)?),
                ["mute", w] => V::Mute(on(w)?),
                ["delay", v] => V::Delay(value(v, "delay", 0.0, 1.0)?),
                ["reverb", v] => V::Reverb(value(v, "reverb", 0.0, 1.0)?),
                _ => return Err(usage()),
            };
            Ok(T::Voice(n, s))
        }
        _ => Err(usage()),
    }
}

const DJ_USAGE: &str = "usage: :dj a|b load | take | play | pause | cue [down|up] | \
     sync [on|off] | rate PCT | range 8|16|50 | nudge +|-|off | gain DB | level L | \
     eq low|mid|high DB | kill low|mid|high on|off | filter K | hot N [clear] | \
     jump BEATS | loop BEATS|off | seek TIME|PCT% | mute on|off | unqueue | mark prev|next | \
     grid x2|/2|<|>|reset | grid offset MS | tap | delay S | reverb S; \
     :dj quantize on|off | strict on|off | cue a|b|off | cue-out split|3-4 | curve smooth|sharp | xfade X|a|b|centre | \
     delay model clean|ping-pong|tape|time MS|sync off|1/1|1/2|1/4|1/8|3/16|1/16|feedback F|tone HZ|wow W|return R | \
     reverb model freeverb|zita|jp|plate|size S|decay SEC|damp D|return R";

/// A deck's letter.
pub fn side_name(side: crate::dj::Side) -> &'static str {
    match side {
        crate::dj::Side::A => "a",
        crate::dj::Side::B => "b",
    }
}

/// Parses what follows `:dj`.
pub(super) fn dj(rest: &str) -> Result<crate::dj::DjAction, String> {
    use crate::dj::{Band, CueOut, Curve, DjAction as D, GridEdit as G, Nudge, Range, Side};
    let usage = || DJ_USAGE.to_string();
    let band = |w: &str| match w {
        "low" => Ok(Band::Low),
        "mid" => Ok(Band::Mid),
        "high" => Ok(Band::High),
        _ => Err(usage()),
    };
    let hot = |w: &str| match w.parse::<u8>() {
        Ok(n) if (1..=crate::dj::HOT_CUES as u8).contains(&n) => Ok(n),
        _ => Err(format!("hot cues are 1 to {}", crate::dj::HOT_CUES)),
    };
    let value = |w: &str, what: &str, lo: f32, hi: f32| {
        w.parse::<f32>()
            .ok()
            .filter(|v| (lo..=hi).contains(v))
            .ok_or_else(|| format!("{what} is {} to {}", number(lo.into()), number(hi.into())))
    };
    let on = |w: &str| match w {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(usage()),
    };
    let side = |w: &str| match w {
        "a" => Some(Side::A),
        "b" => Some(Side::B),
        _ => None,
    };
    let words: Vec<&str> = rest.split_whitespace().collect();
    match words.as_slice() {
        ["quantize", w] => on(w).map(D::Quantize),
        ["cue", "off"] => Ok(D::CueBus(None)),
        ["cue", w] if side(w).is_some() => Ok(D::CueBus(side(w))),
        ["xfade", "a"] => Ok(D::XfadeTo(Some(Side::A))),
        ["xfade", "b"] => Ok(D::XfadeTo(Some(Side::B))),
        ["xfade", "centre"] => Ok(D::XfadeTo(None)),
        ["xfade", x] => value(x, "xfade", 0.0, 1.0).map(D::Xfade),
        ["strict", w] => on(w).map(D::Strict),
        ["cue-out", "split"] => Ok(D::CueOut(CueOut::Split)),
        ["cue-out", "3-4"] => Ok(D::CueOut(CueOut::Channels)),
        ["curve", "smooth"] => Ok(D::Curve(Curve::Smooth)),
        ["curve", "sharp"] => Ok(D::Curve(Curve::Sharp)),
        ["delay" | "reverb", ..] => fx(&words).map(D::Fx),
        [s, setting @ ..] if side(s).is_some() => {
            let s = side(s).expect("checked");
            Ok(match setting {
                ["load"] => D::Load(s),
                ["take"] => D::Take(s),
                ["play"] => D::Play(s),
                ["pause"] => D::Pause(s),
                ["cue"] => D::Cue(s),
                ["cue", "down"] => D::CueHold(s, true),
                ["cue", "up"] => D::CueHold(s, false),
                ["sync"] => D::Sync(s, true),
                ["sync", w] => D::Sync(s, on(w)?),
                ["rate", r] => D::Rate(s, value(r, "rate", -50.0, 50.0)?),
                ["range", "8"] => D::Range(s, Range::Narrow),
                ["range", "16"] => D::Range(s, Range::Medium),
                ["range", "50"] => D::Range(s, Range::Wide),
                ["nudge", "+"] => D::Nudge(s, Nudge::Ahead),
                ["nudge", "-"] => D::Nudge(s, Nudge::Behind),
                ["nudge", "off"] => D::Nudge(s, Nudge::Off),
                ["gain", g] => D::Gain(s, value(g, "gain", -12.0, 12.0)?),
                ["level", l] => D::Level(s, value(l, "level", 0.0, 1.0)?),
                ["grid", "x2"] => D::Grid(s, G::Double),
                ["grid", "/2"] => D::Grid(s, G::Halve),
                ["grid", "<"] => D::Grid(s, G::Earlier),
                ["grid", ">"] => D::Grid(s, G::Later),
                ["grid", "reset"] => D::Grid(s, G::Reset),
                ["grid", "offset", ms] => {
                    D::Grid(s, G::Offset(value(ms, "offset", -1000.0, 1000.0)?))
                }
                ["tap"] => D::Grid(s, G::Tap),
                ["eq", b, db] => {
                    let (lo, hi) = crate::dj::EQ_DB;
                    D::Eq(s, band(b)?, value(db, "eq", lo as f32, hi as f32)?)
                }
                ["kill", b, w] => D::Kill(s, band(b)?, on(w)?),
                ["filter", k] => D::Filter(s, value(k, "filter", -1.0, 1.0)?),
                ["hot", n] => D::HotCue(s, hot(n)?),
                ["hot", n, "clear"] => D::HotClear(s, hot(n)?),
                ["jump", b] => D::Jump(s, value(b, "jump", -64.0, 64.0)?),
                ["seek", at] => D::Seek(s, tape_pos(at)?),
                ["mute", w] => D::Mute(s, on(w)?),
                ["delay", v] => D::DelaySend(s, value(v, "delay", 0.0, 1.0)?),
                ["reverb", v] => D::ReverbSend(s, value(v, "reverb", 0.0, 1.0)?),
                ["unqueue"] => D::Unqueue(s),
                ["mark", "next"] => D::Mark(s, true),
                ["mark", "prev"] => D::Mark(s, false),
                ["loop", "off"] => D::Loop(s, None),
                ["loop", b] => match b.parse::<f32>() {
                    Ok(b) if crate::dj::LOOP_BEATS.contains(&f64::from(b)) => D::Loop(s, Some(b)),
                    _ => return Err("loops are 0.25, 0.5, 1, 2, 4, 8, 16 or 32 beats".into()),
                },
                _ => return Err(usage()),
            })
        }
        _ => Err(usage()),
    }
}

/// What follows `dj ` in the command line for `d`.
pub(super) fn dj_line(d: &crate::dj::DjAction) -> String {
    use crate::dj::{Band, CueOut, Curve, DjAction as D, GridEdit as G, Nudge, Range};
    let band = |b: &Band| match b {
        Band::Low => "low",
        Band::Mid => "mid",
        Band::High => "high",
    };
    let n = |v: &f32| number(f64::from(*v));
    let on = |b: &bool| if *b { "on" } else { "off" };
    let deck = side_name;
    match d {
        D::Quantize(b) => format!("quantize {}", on(b)),
        D::CueBus(None) => "cue off".into(),
        D::CueBus(Some(s)) => format!("cue {}", deck(*s)),
        D::Xfade(x) => format!("xfade {}", n(x)),
        D::XfadeTo(None) => "xfade centre".into(),
        D::XfadeTo(Some(s)) => format!("xfade {}", deck(*s)),
        D::Load(s) => format!("{} load", deck(*s)),
        D::Take(s) => format!("{} take", deck(*s)),
        D::Play(s) => format!("{} play", deck(*s)),
        D::Pause(s) => format!("{} pause", deck(*s)),
        D::Cue(s) => format!("{} cue", deck(*s)),
        D::CueHold(s, true) => format!("{} cue down", deck(*s)),
        D::CueHold(s, false) => format!("{} cue up", deck(*s)),
        D::Sync(s, true) => format!("{} sync", deck(*s)),
        D::Sync(s, false) => format!("{} sync off", deck(*s)),
        D::Rate(s, r) => format!("{} rate {}", deck(*s), n(r)),
        D::Range(s, r) => format!(
            "{} range {}",
            deck(*s),
            match r {
                Range::Narrow => 8,
                Range::Medium => 16,
                Range::Wide => 50,
            }
        ),
        D::Nudge(s, k) => format!(
            "{} nudge {}",
            deck(*s),
            match k {
                Nudge::Ahead => "+",
                Nudge::Behind => "-",
                Nudge::Off => "off",
            }
        ),
        D::Gain(s, g) => format!("{} gain {}", deck(*s), n(g)),
        D::Level(s, l) => format!("{} level {}", deck(*s), n(l)),
        D::Grid(s, G::Tap) => format!("{} tap", deck(*s)),
        D::Eq(s, b, db) => format!("{} eq {} {}", deck(*s), band(b), n(db)),
        D::Kill(s, b, k) => format!("{} kill {} {}", deck(*s), band(b), on(k)),
        D::Filter(s, k) => format!("{} filter {}", deck(*s), n(k)),
        D::HotCue(s, h) => format!("{} hot {h}", deck(*s)),
        D::HotClear(s, h) => format!("{} hot {h} clear", deck(*s)),
        D::Jump(s, b) => format!("{} jump {}", deck(*s), n(b)),
        D::Loop(s, None) => format!("{} loop off", deck(*s)),
        D::Loop(s, Some(b)) => format!("{} loop {}", deck(*s), n(b)),
        D::Seek(s, crate::tape::Pos::Percent(p)) => format!("{} seek {}%", deck(*s), n(p)),
        D::Seek(s, crate::tape::Pos::Time(t)) => {
            format!("{} seek {}", deck(*s), number(t.as_secs_f64()))
        }
        D::Mute(s, m) => format!("{} mute {}", deck(*s), on(m)),
        D::Unqueue(s) => format!("{} unqueue", deck(*s)),
        D::Mark(s, true) => format!("{} mark next", deck(*s)),
        D::Mark(s, false) => format!("{} mark prev", deck(*s)),
        D::Strict(b) => format!("strict {}", on(b)),
        D::DelaySend(s, v) => format!("{} delay {}", deck(*s), n(v)),
        D::ReverbSend(s, v) => format!("{} reverb {}", deck(*s), n(v)),
        D::Fx(f) => fx_line(f),
        D::CueOut(CueOut::Split) => "cue-out split".into(),
        D::CueOut(CueOut::Channels) => "cue-out 3-4".into(),
        D::Curve(Curve::Smooth) => "curve smooth".into(),
        D::Curve(Curve::Sharp) => "curve sharp".into(),
        D::Grid(s, g) => format!(
            "{} grid {}",
            deck(*s),
            match g {
                G::Double => "x2".into(),
                G::Halve => "/2".into(),
                G::Earlier => "<".into(),
                G::Later => ">".into(),
                G::Reset => "reset".into(),
                G::Offset(ms) => format!("offset {}", n(ms)),
                G::Tap => unreachable!("matched above"),
            }
        ),
    }
}

/// `25%` of the loop's range, or a time into it: `1.5`, `0:02`. Below 0% or
/// past 100% reaches into the handles either side.
pub(super) fn tape_pos(word: &str) -> Result<crate::tape::Pos, String> {
    use crate::tape::Pos;
    match word.strip_suffix('%') {
        Some(p) => p
            .parse::<f32>()
            .ok()
            .filter(|p| (-100.0..=200.0).contains(p))
            .map(Pos::Percent)
            .ok_or_else(|| format!("not a percentage: {word}")),
        None => parse_time(word).map(Pos::Time),
    }
}

const MIX_USAGE: &str = "usage: :mix [master|player|tape|decks|headphones \
     PERCENT|+N|-N|mute [on|off]] | law [db|cubic] | rec";

/// Parses what follows `:mix`.
pub(super) fn mix(rest: &str) -> Result<crate::mix::MixAction, String> {
    use crate::mix::{Law, MixAction as M, Strip};
    let usage = || MIX_USAGE.to_string();
    let words: Vec<&str> = rest.split_whitespace().collect();
    match words.as_slice() {
        [] => Ok(M::Show),
        ["law"] => Ok(M::Law(None)),
        ["rec"] => Ok(M::Record),
        ["law", w] => Law::named(w).map(|l| M::Law(Some(l))).ok_or_else(usage),
        [s, rest @ ..] => {
            let strip = Strip::named(s).ok_or_else(usage)?;
            match rest {
                ["mute"] => Ok(M::Mute(strip, None)),
                ["mute", "on"] => Ok(M::Mute(strip, Some(true))),
                ["mute", "off"] => Ok(M::Mute(strip, Some(false))),
                [n] => {
                    let (by, sign, n) = amount(n);
                    let n = n
                        .parse::<f32>()
                        .ok()
                        .filter(|n| n.is_finite())
                        .ok_or_else(|| format!("not a percentage: {n}"))?;
                    match (by, sign as f32 * n) {
                        (true, d) => Ok(M::By(strip, d / 100.0)),
                        (false, p) if (0.0..=100.0).contains(&p) => Ok(M::Set(strip, p / 100.0)),
                        _ => Err(format!("{s} is 0 to 100")),
                    }
                }
                _ => Err(usage()),
            }
        }
    }
}

/// The command line for `m`.
pub(super) fn mix_line(m: &crate::mix::MixAction) -> String {
    use crate::mix::MixAction as M;
    let on = |b: &bool| if *b { "on" } else { "off" };
    match m {
        M::Show => "mix".into(),
        M::Set(s, p) => format!("mix {} {}", s.name(), number(f64::from(*p) * 100.0)),
        M::By(s, d) => format!(
            "mix {} {}{}",
            s.name(),
            if *d < 0.0 { "-" } else { "+" },
            number(f64::from(d.abs()) * 100.0)
        ),
        M::Mute(s, None) => format!("mix {} mute", s.name()),
        M::Mute(s, Some(b)) => format!("mix {} mute {}", s.name(), on(b)),
        M::Law(None) => "mix law".into(),
        M::Record => "mix rec".into(),
        M::Law(Some(l)) => format!("mix law {}", l.name()),
    }
}

/// What follows `tape ` in the command line for `t`.
pub(super) fn tape_line(t: &crate::tape::TapeAction) -> String {
    use crate::tape::{Pos, TapeAction as T, VoiceSetting as V};
    let n = |v: &f32| number(f64::from(*v));
    let pos = |p: &Pos| match p {
        Pos::Percent(p) => format!("{}%", n(p)),
        Pos::Time(d) => number(d.as_secs_f64()),
    };
    let on = |b: &bool| if *b { "on" } else { "off" };
    match t {
        T::Load(None) => "load".into(),
        T::Take => "take".into(),
        T::Load(Some(slot)) => format!("load {slot}"),
        T::Play => "play".into(),
        T::Stop => "stop".into(),
        T::Reset => "reset".into(),
        T::Clear(n) => format!("{n} clear"),
        T::Fold(None) => "fx".into(),
        T::Fold(Some(n)) => format!("{n} fold"),
        T::Drawer(n) => format!("{n} more"),
        T::Eject(n) => format!("{n} eject"),
        T::Share(n) => format!("{n} share"),
        T::Loop(n, id) => format!("{n} loop {id}"),
        T::Save => "save".into(),
        T::Record => "rec".into(),
        T::Bounce(Some(crate::tape::Bounce::Pass)) => "bounce".into(),
        T::Bounce(Some(crate::tape::Bounce::Long)) => "bounce long".into(),
        T::Bounce(None) => "bounce off".into(),
        T::Write(b) => format!("write {}", on(b)),
        T::WriteWindow(a, b) => format!("window {} {}", pos(a), pos(b)),
        T::Feedback(v) => format!("feedback {}", n(v)),
        T::Wear(v) => format!("wear {}", n(v)),
        T::Thin(v) => format!("thin {}", n(v)),
        T::Voice(i, s) => match s {
            V::On(b) => format!("{i} {}", on(b)),
            V::Rate(v) => format!("{i} rate {}", n(v)),
            V::Window(a, b) => format!("{i} window {} {}", pos(a), pos(b)),
            V::Level(v) => format!("{i} level {}", n(v)),
            V::Pan(v) => format!("{i} pan {}", n(v)),
            V::Send(v) => format!("{i} send {}", n(v)),
            V::Wear(v) => format!("{i} wear {}", n(v)),
            V::Fade(v) => format!("{i} fade {}", n(v)),
            V::Ping(b) => format!("{i} ping {}", on(b)),
            V::Slew(v) => format!("{i} slew {}", n(v)),
            V::Drive(v) => format!("{i} drive {}", n(v)),
            V::HighPass(v) => format!("{i} hp {}", n(v)),
            V::LowPass(v) => format!("{i} lp {}", n(v)),
            V::BellHz(v) => format!("{i} bell {}", n(v)),
            V::BellGain(v) => format!("{i} bell-gain {}", n(v)),
            V::BellQ(v) => format!("{i} bell-q {}", n(v)),
            V::Solo(b) => format!("{i} solo {}", on(b)),
            V::Mute(b) => format!("{i} mute {}", on(b)),
            V::Delay(v) => format!("{i} delay {}", n(v)),
            V::Reverb(v) => format!("{i} reverb {}", n(v)),
        },
        T::Fx(f) => fx_line(f),
        T::Lfo(k, l) => {
            let k = crate::tape::lfo_name(usize::from(*k));
            match l {
                crate::tape::LfoSetting::Shape(shape) => format!("lfo {k} {}", shape.name()),
                crate::tape::LfoSetting::Hz(hz) => format!("lfo {k} {}", n(hz)),
                crate::tape::LfoSetting::Sync(d) => format!("lfo {k} sync {}", d.name()),
            }
        }
        T::Route(slot, None) => format!("mod {slot} off"),
        T::Route(slot, Some(r)) => {
            let lfo = crate::tape::lfo_name(r.lfo);
            match crate::tape::dest_name(r.dest) {
                (dest, Some(v)) => format!("mod {slot} {lfo} {dest} {v} {}", n(&r.depth)),
                (dest, None) => format!("mod {slot} {lfo} {dest} {}", n(&r.depth)),
            }
        }
    }
}
