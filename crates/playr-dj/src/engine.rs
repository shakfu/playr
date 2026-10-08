//! The mixer on the audio thread ([`Engine`]), and its controls on any
//! other ([`Handle`]): the command ring, the return ring and the status.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use rtrb::{Consumer, Producer, RingBuffer};

use crate::deck::{Deck, Range, HOT_CUES};
use crate::mixer::{CueOut, Mixer, Setting};
use crate::track::Track;
use crate::{Error, Side};

const COMMANDS: usize = 256;
/// One return per load; the handle drains them.
const RETURNS: usize = 8;

/// What the handle sends the callback.
#[derive(Debug)]
pub enum Cmd {
    Set(Setting),
    Load(Side, Box<Track>),
    /// Load onto a deck even while it plays, crossfading from the old track.
    Replace(Side, Box<Track>),
}

/// Tracks the callback hands back, since it must not free them.
#[derive(Debug)]
pub enum Returned {
    /// The track a load replaced.
    Replaced(Side, Box<Track>),
    /// A track sent to a playing deck, which refuses it.
    Refused(Side, Box<Track>),
    /// A replace took its track and is crossfading into it.
    Fading(Side),
    /// The track a replace faded out.
    Faded(Side, Box<Track>),
}

/// One deck's state, published once per callback.
#[derive(Debug)]
pub struct DeckStatus {
    playing: AtomicBool,
    synced: AtomicBool,
    pos: AtomicU64,
    cue: AtomicU64,
    rate: AtomicU64,
    pct: AtomicU64,
    range: AtomicU32,
    bpm: AtomicU64,
    phase: AtomicU64,
    hot: [AtomicU64; HOT_CUES],
    /// The loop's first frame and length; NaN without one.
    loop_start: AtomicU64,
    loop_len: AtomicU64,
    loads: AtomicU64,
}

fn load(a: &AtomicU64) -> f64 {
    f64::from_bits(a.load(Ordering::Relaxed))
}

fn store(a: &AtomicU64, v: f64) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

impl DeckStatus {
    fn new() -> Self {
        DeckStatus {
            playing: AtomicBool::new(false),
            synced: AtomicBool::new(false),
            pos: AtomicU64::new(0),
            cue: AtomicU64::new(0),
            rate: AtomicU64::new(1f64.to_bits()),
            pct: AtomicU64::new(0),
            range: AtomicU32::new(0),
            bpm: AtomicU64::new(f64::NAN.to_bits()),
            phase: AtomicU64::new(f64::NAN.to_bits()),
            hot: std::array::from_fn(|_| AtomicU64::new(f64::NAN.to_bits())),
            loop_start: AtomicU64::new(f64::NAN.to_bits()),
            loop_len: AtomicU64::new(f64::NAN.to_bits()),
            loads: AtomicU64::new(0),
        }
    }

    fn publish(&self, d: &Deck) {
        self.playing.store(d.playing(), Ordering::Relaxed);
        self.synced.store(d.synced(), Ordering::Relaxed);
        store(&self.pos, d.pos());
        store(&self.cue, d.cue());
        store(&self.rate, d.rate());
        store(&self.pct, d.pct());
        self.range.store(d.range() as u32, Ordering::Relaxed);
        store(&self.bpm, d.bpm().unwrap_or(f64::NAN));
        store(&self.phase, d.phase().unwrap_or(f64::NAN));
        for (a, h) in self.hot.iter().zip(d.hot) {
            store(a, h.unwrap_or(f64::NAN));
        }
        let (start, len) = d.looping().unwrap_or((f64::NAN, f64::NAN));
        store(&self.loop_start, start);
        store(&self.loop_len, len);
        self.loads.store(d.loads(), Ordering::Relaxed);
    }

    /// Tracks the deck has loaded, so a reader can tell which the rest is of.
    pub fn loads(&self) -> u64 {
        self.loads.load(Ordering::Relaxed)
    }

    /// The hot cues, in frames.
    pub fn hot_cues(&self) -> [Option<f64>; HOT_CUES] {
        std::array::from_fn(|i| Some(load(&self.hot[i])).filter(|h| !h.is_nan()))
    }

    /// The loop playing: its first frame and its length.
    pub fn looping(&self) -> Option<(f64, f64)> {
        Some((load(&self.loop_start), load(&self.loop_len))).filter(|(s, _)| !s.is_nan())
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn synced(&self) -> bool {
        self.synced.load(Ordering::Relaxed)
    }

    /// The head, in frames.
    pub fn pos(&self) -> f64 {
        load(&self.pos)
    }

    /// The cue point, in frames.
    pub fn cue(&self) -> f64 {
        load(&self.cue)
    }

    /// The rate the head moves at, with any nudge.
    pub fn rate(&self) -> f64 {
        load(&self.rate)
    }

    /// The rate fader, in percent.
    pub fn pct(&self) -> f64 {
        load(&self.pct)
    }

    pub fn range(&self) -> Range {
        match self.range.load(Ordering::Relaxed) {
            0 => Range::Narrow,
            1 => Range::Medium,
            _ => Range::Wide,
        }
    }

    /// The tempo as played; none without a grid.
    pub fn bpm(&self) -> Option<f64> {
        Some(load(&self.bpm)).filter(|b| !b.is_nan())
    }

    /// Where in its beat the head is, 0 to 1; none without a grid.
    pub fn phase(&self) -> Option<f64> {
        Some(load(&self.phase)).filter(|p| !p.is_nan())
    }
}

/// What the callback publishes once per call.
#[derive(Debug)]
pub struct Status {
    decks: [DeckStatus; 2],
    peak: AtomicU32,
    cue_peak: AtomicU32,
    xfade: AtomicU64,
}

impl Status {
    pub fn deck(&self, side: Side) -> &DeckStatus {
        &self.decks[side.index()]
    }

    /// The crossfader where it is now, moving or not.
    pub fn xfade(&self) -> f64 {
        load(&self.xfade)
    }

    /// The largest magnitude of the main mix since the last call, which
    /// resets it. Measured before the master volume.
    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak.swap(0, Ordering::Relaxed))
    }

    /// The largest magnitude of the cue since the last call, which resets
    /// it. Measured before the headphones.
    pub fn take_cue_peak(&self) -> f32 {
        f32::from_bits(self.cue_peak.swap(0, Ordering::Relaxed))
    }
}

/// The mixer as the audio callback runs it. It never locks, allocates or
/// frees.
pub struct Engine {
    mixer: Mixer,
    commands: Consumer<Cmd>,
    returns: Producer<Returned>,
    status: Arc<Status>,
}

/// An engine at `sample_rate` and the handle that controls it.
pub fn new(sample_rate: u32) -> (Engine, Handle) {
    // The sinc's table, built here so the callback never allocates it.
    playr_dsp::interp::warm();
    let (cmd_tx, cmd_rx) = RingBuffer::new(COMMANDS);
    let (ret_tx, ret_rx) = RingBuffer::new(RETURNS);
    let status = Arc::new(Status {
        decks: [DeckStatus::new(), DeckStatus::new()],
        peak: AtomicU32::new(0),
        cue_peak: AtomicU32::new(0),
        xfade: AtomicU64::new(0.5f64.to_bits()),
    });
    let engine = Engine {
        mixer: Mixer::new(sample_rate),
        commands: cmd_rx,
        returns: ret_tx,
        status: status.clone(),
    };
    let handle = Handle {
        commands: cmd_tx,
        returns: ret_rx,
        status,
        sample_rate,
    };
    (engine, handle)
}

impl Engine {
    /// Applies pending commands, then fills `out`, interleaved stereo, and
    /// publishes the status.
    pub fn process(&mut self, out: &mut [f32]) {
        self.process_channels(out, 2);
    }

    /// As [`Engine::process`], with `channels` interleaved, as
    /// [`Mixer::process_channels`] fills them.
    pub fn process_channels(&mut self, out: &mut [f32], channels: usize) {
        self.commands();
        self.mixer.process_channels(out, channels);
        self.publish();
        self.hand_back();
    }

    /// As [`Engine::process`], filling the main mix and the cue apart, as
    /// [`Mixer::process_buses`] does, for a caller that routes them.
    pub fn process_buses(&mut self, main: &mut [f32], cue: &mut [f32]) -> Option<(CueOut, bool)> {
        self.commands();
        let routed = self.mixer.process_buses(main, cue);
        self.publish();
        self.hand_back();
        routed
    }

    fn publish(&mut self) {
        for side in [Side::A, Side::B] {
            self.status.deck(side).publish(self.mixer.deck(side));
        }
        store(&self.status.xfade, self.mixer.xfade());
        // The reader resets it, so a plain store could undo a higher peak.
        // A magnitude is never negative, and such floats order as their bits.
        let peak = self.mixer.take_peak();
        self.status
            .peak
            .fetch_max(peak.to_bits(), Ordering::Relaxed);
        let cue = self.mixer.take_cue_peak();
        self.status
            .cue_peak
            .fetch_max(cue.to_bits(), Ordering::Relaxed);
    }

    pub fn mixer(&self) -> &Mixer {
        &self.mixer
    }

    /// Returns the tracks replaces faded out; one the ring has no room for
    /// waits, silent, for the next callback.
    fn hand_back(&mut self) {
        for side in [Side::A, Side::B] {
            if self.returns.slots() == 0 {
                return;
            }
            if let Some(old) = self.mixer.take_faded(side) {
                _ = self.returns.push(Returned::Faded(side, old));
            }
        }
    }

    fn commands(&mut self) {
        while let Ok(cmd) = self.commands.peek() {
            // Left in the ring until the handle has drained room for its return.
            if matches!(cmd, Cmd::Load(..) | Cmd::Replace(..)) && self.returns.slots() == 0 {
                return;
            }
            let Ok(cmd) = self.commands.pop() else { return };
            match cmd {
                Cmd::Set(s) => self.mixer.set(s),
                Cmd::Load(side, track) => {
                    _ = self.returns.push(match self.mixer.load(side, track) {
                        Ok(old) => Returned::Replaced(side, old),
                        Err(new) => Returned::Refused(side, new),
                    });
                }
                Cmd::Replace(side, track) => {
                    use crate::deck::Replace;
                    _ = self.returns.push(match self.mixer.replace(side, track) {
                        Replace::Loaded(old) => Returned::Replaced(side, old),
                        Replace::Fading => Returned::Fading(side),
                        Replace::Refused(new) => Returned::Refused(side, new),
                    });
                }
            }
        }
    }
}

/// Controls an [`Engine`] from outside the audio thread.
pub struct Handle {
    commands: Producer<Cmd>,
    returns: Consumer<Returned>,
    status: Arc<Status>,
    sample_rate: u32,
}

impl Handle {
    /// Sends `cmd`, or hands it back when the ring is full.
    pub fn send(&mut self, cmd: Cmd) -> Result<(), Cmd> {
        self.commands
            .push(cmd)
            .map_err(|rtrb::PushError::Full(c)| c)
    }

    pub fn set(&mut self, s: Setting) -> Result<(), Error> {
        self.send(Cmd::Set(s)).map_err(|_| Error::Full)
    }

    /// Loads `track` onto a paused deck. The replaced or refused track
    /// arrives through [`Handle::poll`].
    pub fn load(&mut self, side: Side, track: Track) -> Result<(), Error> {
        self.send(Cmd::Load(side, Box::new(track)))
            .map_err(|_| Error::Full)
    }

    /// Loads `track` onto `side`, crossfading from the track playing there;
    /// a paused deck loads it as [`Handle::load`] does.
    pub fn replace(&mut self, side: Side, track: Track) -> Result<(), Error> {
        self.send(Cmd::Replace(side, Box::new(track)))
            .map_err(|_| Error::Full)
    }

    /// The next track the callback has handed back.
    pub fn poll(&mut self) -> Option<Returned> {
        self.returns.pop().ok()
    }

    /// Shared, so a drawing thread can read it without the handle.
    pub fn status(&self) -> &Arc<Status> {
        &self.status
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}
