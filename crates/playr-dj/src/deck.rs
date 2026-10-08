//! One deck: a track, its read head, the transport and the CDJ-style cue.

use crate::fx::{Eq, Filter};
use crate::track::{Grid, Track};

pub(crate) type Ramp = playr_dsp::Ramp<f64>;

/// How long a rate or gain change takes.
const SMOOTH_MS: f64 = 10.0;
/// How long play and pause fade, and a jump crossfades.
const DECLICK_MS: f64 = 5.0;
/// How long a track replaced while playing crossfades into the new one.
pub const REPLACE_MS: f64 = 1000.0;
/// The rate factor a held nudge applies. A typical bend; not measured.
const NUDGE: f64 = 0.04;

/// The rate fader's travel either side of 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Range {
    #[default]
    Narrow,
    Medium,
    Wide,
}

impl Range {
    const ALL: [Range; 3] = [Range::Narrow, Range::Medium, Range::Wide];

    pub fn percent(self) -> f64 {
        match self {
            Range::Narrow => 8.0,
            Range::Medium => 16.0,
            Range::Wide => 50.0,
        }
    }

    /// This range or the first wider one that holds `pct`.
    fn fitting(self, pct: f64) -> Option<Range> {
        Range::ALL
            .into_iter()
            .find(|r| *r as u8 >= self as u8 && pct.abs() <= r.percent())
    }
}

/// A temporary bend of the rate, held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Nudge {
    #[default]
    Off,
    Ahead,
    Behind,
}

impl Nudge {
    fn factor(self) -> f64 {
        match self {
            Nudge::Off => 1.0,
            Nudge::Ahead => 1.0 + NUDGE,
            Nudge::Behind => 1.0 - NUDGE,
        }
    }
}

/// A track, a read head in `f64` frames, and the transport around it.
#[derive(Debug, Clone)]
pub struct Deck {
    sample_rate: f64,
    track: Box<Track>,
    pos: f64,
    playing: bool,
    /// Playing while CUE is held, to return to the cue point on release.
    previewing: bool,
    cue: f64,
    /// Where the head goes once a pause has faded out.
    pending: Option<f64>,
    gate: Ramp,
    /// The head a jump left, faded out over `fade_left` frames.
    from: f64,
    fade_left: u32,
    pct: f64,
    range: Range,
    nudge: Nudge,
    rate: Ramp,
    trim: Ramp,
    pub(crate) level: Ramp,
    pub(crate) eq: Eq,
    pub(crate) filter: Filter,
    /// The tempo multiple this deck follows the other at, while synced.
    pub(crate) synced: Option<f64>,
    /// The phase lock's rate factor, 1 when in phase.
    lock: f64,
    /// The phase, in beats, the lock holds this deck at against the other:
    /// where a nudge left it.
    pub(crate) lock_offset: f64,
    /// The hot cues, in frames.
    pub(crate) hot: [Option<f64>; HOT_CUES],
    /// The loop playing, as its first frame and its length.
    looping: Option<(f64, f64)>,
    /// Tracks loaded so far.
    loads: u64,
    /// A track replaced while playing, fading out from its head.
    outgoing: Option<Outgoing>,
    /// How the head reads between frames.
    pub(crate) interp: playr_dsp::Interp,
}

/// A track a replace took off a playing deck: it plays on from `pos`,
/// fading out over `left` more frames, then waits to be taken back.
#[derive(Debug, Clone)]
struct Outgoing {
    track: Box<Track>,
    pos: f64,
    left: u32,
}

/// Hot cues per deck.
pub const HOT_CUES: usize = 4;

impl Deck {
    pub(crate) fn new(sample_rate: u32) -> Self {
        Deck {
            sample_rate: sample_rate as f64,
            track: Box::new(Track::empty()),
            pos: 0.0,
            playing: false,
            previewing: false,
            cue: 0.0,
            pending: None,
            gate: Ramp::new(0.0),
            from: 0.0,
            fade_left: 0,
            pct: 0.0,
            range: Range::default(),
            nudge: Nudge::Off,
            rate: Ramp::new(1.0),
            trim: Ramp::new(1.0),
            level: Ramp::new(1.0),
            eq: Eq::new(sample_rate),
            filter: Filter::new(sample_rate),
            synced: None,
            lock: 1.0,
            lock_offset: 0.0,
            hot: [None; HOT_CUES],
            looping: None,
            loads: 0,
            outgoing: None,
            interp: playr_dsp::Interp::default(),
        }
    }

    fn frames(&self, ms: f64) -> u32 {
        (ms * self.sample_rate / 1000.0) as u32
    }

    pub fn track(&self) -> &Track {
        &self.track
    }

    /// The read head, in frames.
    pub fn pos(&self) -> f64 {
        self.pos
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn previewing(&self) -> bool {
        self.previewing
    }

    /// The cue point, in frames.
    pub fn cue(&self) -> f64 {
        self.cue
    }

    /// The rate fader, in percent.
    pub fn pct(&self) -> f64 {
        self.pct
    }

    pub fn range(&self) -> Range {
        self.range
    }

    pub fn synced(&self) -> bool {
        self.synced.is_some()
    }

    /// The rate the head moves at now, with any nudge.
    pub fn rate(&self) -> f64 {
        self.rate.value()
    }

    /// The rate the fader sets, without the nudge.
    pub(crate) fn base(&self) -> f64 {
        1.0 + self.pct / 100.0
    }

    /// The rate the head is moving towards, with any nudge.
    pub(crate) fn target(&self) -> f64 {
        self.rate.target()
    }

    pub fn grid(&self) -> Option<Grid> {
        self.track.grid()
    }

    /// The tempo as played: the grid's at the current rate.
    pub fn bpm(&self) -> Option<f64> {
        self.grid().map(|g| g.bpm * self.rate())
    }

    /// Where in its beat the head is, 0 to 1.
    pub fn phase(&self) -> Option<f64> {
        self.grid()
            .map(|g| g.beats(self.pos / self.sample_rate).rem_euclid(1.0))
    }

    /// The head as if every rate ramp under way had finished at once. Two
    /// decks in phase by this stay in phase once the ramps end.
    pub(crate) fn settled(&self) -> f64 {
        match self.playing {
            true => self.pos - self.rate.lag(),
            false => self.pos,
        }
    }

    /// Where the head stops: the cue point while a cue return fades out.
    pub(crate) fn resting(&self) -> f64 {
        self.pending.unwrap_or(self.pos)
    }

    pub(crate) fn seconds(&self, frames: f64) -> f64 {
        frames / self.sample_rate
    }

    /// Swaps in `track` with the head and cue at its start, and its hot cues.
    pub(crate) fn load(&mut self, track: Box<Track>) -> Box<Track> {
        self.pos = 0.0;
        self.cue = 0.0;
        self.pending = None;
        self.fade_left = 0;
        self.synced = None;
        self.hot = track.hot_cues();
        self.looping = None;
        self.loads += 1;
        self.set_lock(1.0);
        std::mem::replace(&mut self.track, track)
    }

    /// Swaps in `track` from its start while this one plays on, fading out
    /// over [`REPLACE_MS`] as the new one fades in. A paused deck loads it as
    /// [`Deck::load`] does and hands back the old track; a deck still fading
    /// one out refuses, handing `track` back.
    pub(crate) fn replace(&mut self, track: Box<Track>) -> Replace {
        if !self.playing {
            return Replace::Loaded(self.load(track));
        }
        if self.outgoing.is_some() {
            return Replace::Refused(track);
        }
        let pos = self.pos;
        let old = self.load(track);
        self.outgoing = Some(Outgoing {
            track: old,
            pos,
            left: self.frames(REPLACE_MS).max(1),
        });
        Replace::Fading
    }

    /// The track a replace faded out, once it has.
    pub(crate) fn take_faded(&mut self) -> Option<Box<Track>> {
        self.outgoing.take_if(|o| o.left == 0).map(|o| o.track)
    }

    pub(crate) fn set_grid(&mut self, grid: Option<Grid>) {
        self.track.set_grid(grid);
    }

    /// Moves the head to `to`, crossfading from where it was if audible.
    pub(crate) fn jump(&mut self, to: f64) {
        if self.gate.value() > 0.0 {
            self.from = self.pos;
            self.fade_left = self.frames(DECLICK_MS);
        }
        self.place(to);
    }

    /// Puts the head at `to`, ending a loop it leaves; else the loop would
    /// pull it back a loop length a frame.
    fn place(&mut self, to: f64) {
        if self
            .looping
            .is_some_and(|(start, len)| !(start..start + len).contains(&to))
        {
            self.looping = None;
        }
        self.pos = to;
    }

    /// Moves the head to `to`, in place of any cue return under way.
    pub(crate) fn seek(&mut self, to: f64) {
        self.pending = None;
        self.jump(to);
    }

    /// Starts playing, from a cue return still fading out if there is one.
    pub(crate) fn play(&mut self) {
        if let Some(p) = self.pending.take() {
            self.jump(p);
        }
        self.playing = true;
        self.gate.set(1.0, self.frames(DECLICK_MS));
    }

    pub(crate) fn pause(&mut self) {
        self.playing = false;
        self.previewing = false;
        self.gate.set(0.0, self.frames(DECLICK_MS));
    }

    /// Pauses and puts the head on the cue point once faded out.
    pub(crate) fn back_to_cue(&mut self) {
        self.pause();
        if self.gate.value() == 0.0 {
            self.place(self.cue);
        } else {
            self.pending = Some(self.cue);
        }
    }

    /// Sets the cue point and the head to `at`, then plays until released.
    pub(crate) fn preview(&mut self, at: f64) {
        self.pending = None;
        self.cue = at;
        if at != self.pos {
            self.jump(at);
        }
        self.play();
        self.previewing = true;
    }

    /// Ends a preview, as Play during one does.
    pub(crate) fn latch(&mut self) {
        self.previewing = false;
    }

    pub(crate) fn set_pct(&mut self, pct: f64) {
        self.pct = pct.clamp(-self.range.percent(), self.range.percent());
        self.retarget();
    }

    /// Sets the range, clamping the fader into it. False if that moved it.
    pub(crate) fn set_range(&mut self, range: Range) -> bool {
        self.range = range;
        let pct = self.pct;
        self.set_pct(pct);
        self.pct == pct
    }

    /// Sets the fader to `pct`, widening the range to fit. False, and no
    /// change, if no range holds it.
    pub(crate) fn fit_pct(&mut self, pct: f64) -> bool {
        match self.range.fitting(pct) {
            Some(r) => {
                self.range = r;
                if pct != self.pct {
                    self.set_pct(pct);
                }
                true
            }
            None => false,
        }
    }

    pub(crate) fn set_nudge(&mut self, n: Nudge) {
        self.nudge = n;
        self.retarget();
    }

    pub(crate) fn nudge(&self) -> Nudge {
        self.nudge
    }

    /// Sets the phase lock's rate factor.
    pub(crate) fn set_lock(&mut self, lock: f64) {
        if lock != self.lock {
            self.lock = lock;
            self.retarget();
        }
    }

    /// Tracks loaded so far, so a reader can tell which track a status is of.
    pub fn loads(&self) -> u64 {
        self.loads
    }

    /// The hot cues, in frames.
    pub fn hot(&self) -> [Option<f64>; HOT_CUES] {
        self.hot
    }

    /// The loop playing: its first frame and its length.
    pub fn looping(&self) -> Option<(f64, f64)> {
        self.looping
    }

    pub(crate) fn set_loop(&mut self, l: Option<(f64, f64)>) {
        self.looping = l;
    }

    pub(crate) fn set_trim(&mut self, gain: f64) {
        let n = self.frames(SMOOTH_MS);
        self.trim.set(gain, n);
    }

    pub(crate) fn set_level(&mut self, level: f64) {
        let n = self.frames(SMOOTH_MS);
        self.level.set(level, n);
    }

    fn retarget(&mut self) {
        let n = self.frames(SMOOTH_MS);
        self.rate
            .set(self.base() * self.nudge.factor() * self.lock, n);
    }

    /// The next frame, after the trim, the EQ and the filter, before the
    /// channel fader, and advances the head.
    pub(crate) fn next(&mut self) -> [f32; 2] {
        let s = self.read();
        let s = self.eq.tick(s);
        self.filter.tick(s)
    }

    fn read(&mut self) -> [f32; 2] {
        let trim = self.trim.next() as f32;
        let rate = self.rate.next();
        if !self.playing && self.gate.value() == 0.0 {
            return [0.0; 2];
        }
        let mut s = self.track.read(self.pos, rate, self.interp);
        if self.fade_left > 0 {
            let w = self.fade_left as f32 / self.frames(DECLICK_MS) as f32;
            let o = self.track.read(self.from, rate, self.interp);
            s = [0, 1].map(|c| s[c] * (1.0 - w) + o[c] * w);
            self.fade_left -= 1;
            self.from += rate;
        }
        // Equal power: the tracks are unrelated, so their powers add.
        let total = self.frames(REPLACE_MS).max(1) as f32;
        if let Some(o) = self.outgoing.as_mut().filter(|o| o.left > 0) {
            let w = o.left as f32 / total;
            let (out, into) = (w * std::f32::consts::FRAC_PI_2).sin_cos();
            let old = o.track.read(o.pos, rate, self.interp);
            s = [0, 1].map(|c| s[c] * into + old[c] * out);
            o.pos += rate;
            o.left -= 1;
        }
        let g = self.gate.next() as f32 * trim;
        self.pos += rate;
        if let Some((start, len)) = self.looping {
            if self.pos >= start + len {
                self.jump(self.pos - len);
            }
        }
        if !self.playing && self.gate.value() == 0.0 {
            if let Some(p) = self.pending.take() {
                self.place(p);
            }
        }
        if self.playing && self.pos >= self.track.frames() as f64 {
            self.pause();
        }
        s.map(|x| x * g)
    }
}

/// What [`Deck::replace`] did.
#[derive(Debug)]
pub(crate) enum Replace {
    /// The deck was paused: loaded, with the old track handed back.
    Loaded(Box<Track>),
    /// Crossfading; the old track comes back through [`Deck::take_faded`].
    Fading,
    Refused(Box<Track>),
}
