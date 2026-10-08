//! The mixer: the master volume, a level and a mute for each source, and the
//! headphones. It holds fader positions; a [`Law`] turns them into the gains
//! the player, the tape and the decks apply. `docs/dev/mixer.md` has the
//! design.

use playr_core::audio::Cmd;

use crate::dispatch::Frontend;
use crate::message::Message;

/// The range the `db` law spans, from the top of a fader to just above 0.
pub const RANGE_DB: f32 = 60.0;

/// How a fader position, 0 to 1, maps to a gain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Law {
    /// Even in decibels: each 1% of travel is 0.6 dB, and 0 is silent.
    #[default]
    Db,
    /// The position cubed, as PulseAudio's volume.
    Cubic,
}

impl Law {
    pub const NAMES: [(&'static str, Law); 2] = [("db", Law::Db), ("cubic", Law::Cubic)];

    pub fn name(self) -> &'static str {
        match self {
            Law::Db => "db",
            Law::Cubic => "cubic",
        }
    }

    pub fn named(name: &str) -> Option<Law> {
        Self::NAMES.iter().find(|n| n.0 == name).map(|n| n.1)
    }

    /// The gain at position `p`, both 0 to 1.
    pub fn gain(self, p: f32) -> f32 {
        let p = p.clamp(0.0, 1.0);
        match self {
            Law::Db if p == 0.0 => 0.0,
            Law::Db => 10f32.powf(RANGE_DB * (p - 1.0) / 20.0),
            Law::Cubic => p * p * p,
        }
    }

    /// The position that gives `gain`. Under `db`, a gain below the range is
    /// position 0.
    pub fn position(self, gain: f32) -> f32 {
        let g = gain.clamp(0.0, 1.0);
        match self {
            Law::Db if g == 0.0 => 0.0,
            Law::Db => (1.0 + 20.0 * g.log10() / RANGE_DB).max(0.0),
            Law::Cubic => g.cbrt(),
        }
    }
}

/// A fader with a mute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strip {
    Master,
    Player,
    Tape,
    Decks,
    /// The DJ cue, which the master leaves alone.
    Headphones,
}

impl Strip {
    pub const ALL: [Strip; 5] = [
        Strip::Master,
        Strip::Player,
        Strip::Tape,
        Strip::Decks,
        Strip::Headphones,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Strip::Master => "master",
            Strip::Player => "player",
            Strip::Tape => "tape",
            Strip::Decks => "decks",
            Strip::Headphones => "headphones",
        }
    }

    pub fn named(name: &str) -> Option<Strip> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The faders, as positions from 0 to 1, and the mutes.
#[derive(Debug, Clone, PartialEq)]
pub struct Mix {
    law: Law,
    level: [f32; 5],
    muted: [bool; 5],
    /// The player's EQ bands, in dB, while `:eq bypass` holds them flat.
    eq_held: Option<[f32; 3]>,
    /// How the decks' and the tape's heads read between frames.
    interp: playr_dsp::Interp,
}

impl Mix {
    /// Every fader at the top and unmuted, but the master at `master`.
    pub fn new(law: Law, master: f32) -> Mix {
        let mut mix = Mix {
            law,
            level: [1.0; 5],
            muted: [false; 5],
            eq_held: None,
            interp: playr_dsp::Interp::default(),
        };
        mix.set_level(Strip::Master, master);
        mix
    }

    pub fn law(&self) -> Law {
        self.law
    }

    pub fn set_law(&mut self, law: Law) {
        self.law = law;
    }

    pub fn level(&self, s: Strip) -> f32 {
        self.level[s.index()]
    }

    /// Sets a position, clamped to 0 to 1. A value that is not a number is
    /// ignored.
    pub fn set_level(&mut self, s: Strip, p: f32) {
        if !p.is_nan() {
            self.level[s.index()] = p.clamp(0.0, 1.0);
        }
    }

    pub fn muted(&self, s: Strip) -> bool {
        self.muted[s.index()]
    }

    pub fn set_muted(&mut self, s: Strip, on: bool) {
        self.muted[s.index()] = on;
    }

    /// How the decks' and the tape's heads read between frames; they follow
    /// it on their next poll.
    pub fn interp(&self) -> playr_dsp::Interp {
        self.interp
    }

    pub fn set_interp(&mut self, interp: playr_dsp::Interp) {
        self.interp = interp;
    }

    /// The EQ bands a bypass holds, which `:eq bypass` restores.
    pub fn eq_held(&self) -> Option<[f32; 3]> {
        self.eq_held
    }

    pub fn hold_eq(&mut self, bands: Option<[f32; 3]>) {
        self.eq_held = bands;
    }

    /// The gain `s` plays at: its own fader's, and for a source the master's
    /// too. 0 while muted.
    pub fn gain(&self, s: Strip) -> f32 {
        match s {
            Strip::Master | Strip::Headphones => self.fader_gain(s),
            Strip::Player | Strip::Tape | Strip::Decks => {
                self.fader_gain(Strip::Master) * self.fader_gain(s)
            }
        }
    }

    /// The gain of `s`'s own fader and mute, without the master's.
    pub fn fader_gain(&self, s: Strip) -> f32 {
        match self.muted(s) {
            true => 0.0,
            false => self.law.gain(self.level(s)),
        }
    }
}

/// What `:mix` does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MixAction {
    /// Report every fader.
    Show,
    /// Set a fader's position, 0 to 1.
    Set(Strip, f32),
    /// Move a fader by this much of its travel.
    By(Strip, f32),
    /// Mute or unmute, or toggle for `None`.
    Mute(Strip, Option<bool>),
    /// Use this law, or the other one for `None`.
    Law(Option<Law>),
    /// Start recording the master, or stop the recording under way.
    Record,
}

/// Does `action`. The player takes its gain here; the tape and the decks
/// take theirs as they poll.
pub fn act(f: &mut impl Frontend, action: MixAction) {
    let mix = f.mix();
    match action {
        MixAction::Show => {
            let shown = mix.clone();
            f.notify(Message::Mix(shown));
            return;
        }
        MixAction::Set(s, p) => mix.set_level(s, p),
        MixAction::By(s, d) => mix.set_level(s, mix.level(s) + d),
        MixAction::Mute(s, on) => {
            let on = on.unwrap_or(!mix.muted(s));
            mix.set_muted(s, on);
        }
        MixAction::Record => return record(f),
        MixAction::Law(law) => {
            let law = law.unwrap_or(match mix.law() {
                Law::Db => Law::Cubic,
                Law::Cubic => Law::Db,
            });
            mix.set_law(law);
            f.notify(Message::FaderLaw(law));
        }
    }
    send_player(f);
}

/// Starts recording the master, or stops the recording under way.
fn record(f: &mut impl Frontend) {
    if f.session_mut().stop_master() {
        return f.notify(Message::MasterStopping);
    }
    match f.session_mut().start_master() {
        Ok(path) => f.notify(Message::MasterRecording(path)),
        Err(e) => f.notify(Message::MasterFailed(e)),
    }
}

/// Reports the master's recording once its file is written.
pub fn poll(f: &mut impl Frontend) {
    match f.session_mut().master_done() {
        Some(Ok(done)) => f.notify(Message::MasterRecorded(done)),
        Some(Err(e)) => f.notify(Message::MasterFailed(e)),
        None => {}
    }
}

/// Sends the player its gain.
pub fn send_player(f: &mut impl Frontend) {
    let gain = f.mix().gain(Strip::Player);
    f.session().send(Cmd::SetVolume(gain));
}
