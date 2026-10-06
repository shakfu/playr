//! The two decks, sync between them, and the mix with its split cue.

use std::f64::consts::FRAC_PI_2;

use playr_dsp::soft_clip;

use crate::deck::{Deck, Nudge, Ramp, Range, HOT_CUES};
use crate::fx::{Band, EQ_DB};
use crate::track::{Grid, Track};
use crate::Side;

/// How long crossfader and volume changes take.
const SMOOTH_MS: f64 = 10.0;
/// The trim's travel either side of 0 dB.
const TRIM_DB: f64 = 12.0;
/// The tempo multiples sync and quantize try: half, same, double.
const MULTIPLES: [f64; 3] = [0.5, 1.0, 2.0];
/// The phase lock, after Mixxx's tuning: no trim within `LOCK_DEAD` beats,
/// else `LOCK_K` times the error, up to `LOCK_MAX` of the rate.
const LOCK_DEAD: f64 = 0.01;
const LOCK_K: f64 = 0.7;
const LOCK_MAX: f64 = 0.05;
/// How long the crossfader takes to glide to a point, as to an end or the
/// centre by a button.
const GLIDE_MS: f64 = 400.0;
/// The sharp crossfader curve cuts a deck within this much of the far end.
const SHARP_EDGE: f64 = 0.05;
/// The master's soft clip passes levels below this unchanged, until set.
/// A knee at 0.5 bent every peak of one deck at unity.
pub const DEFAULT_KNEE: f32 = 0.9;
/// The loop lengths offered, in beats.
pub const LOOP_BEATS: [f64; 8] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];

/// Where the headphone cue goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CueOut {
    /// Channels 1 and 2: the main mix in mono on one, the cue on the other.
    #[default]
    Split,
    /// Channels 3 and 4, in stereo, with the main mix on 1 and 2. A
    /// stereo device gets the split cue instead.
    Channels,
}

/// The crossfader's curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Curve {
    /// Constant power: two uncorrelated tracks keep their summed level.
    #[default]
    Smooth,
    /// Both decks at full level except within 5% of either end, for cuts.
    Sharp,
}

/// A setting, as [`Mixer::set`] takes it. Values out of range are clamped;
/// values that are not finite are ignored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Setting {
    /// Plays from the head; during a cue preview, keeps playing on release.
    /// Moves the head to these frames and plays, out of phase with the
    /// other deck if need be: a handover from another player continues.
    PlayFrom(Side, f64),
    Play(Side),
    Pause(Side),
    /// CUE pressed (`true`) or released, as on a CDJ: pressed while playing,
    /// returns to the cue point and pauses; pressed while paused, sets the
    /// cue point there and plays until released, then returns to it.
    Cue(Side, bool),
    /// The rate fader, in percent. Turns sync off.
    Rate(Side, f64),
    Range(Side, Range),
    Nudge(Side, Nudge),
    /// On: follows the other deck's tempo, at half, the same or double,
    /// and moves into phase with it once. Refused without both grids or
    /// when no range holds the rate; [`Deck::synced`] tells.
    Sync(Side, bool),
    Grid(Side, Option<Grid>),
    /// The trim, in dB, +/-12.
    Gain(Side, f64),
    /// The channel fader, 0 to 1.
    Level(Side, f64),
    /// The crossfader, 0 for deck A only to 1 for deck B only.
    Xfade(f64),
    /// The crossfader moved over 400 ms rather than at once.
    XfadeGlide(f64),
    /// Play and cue start in phase with the other deck when it plays; a
    /// cue point set while paused falls on the deck's own nearest beat.
    Quantize(bool),
    /// The deck heard on the cue side of a split cue, or none for the
    /// main mix in stereo.
    CueBus(Option<Side>),
    /// Puts the cue on the left and the main mix on the right.
    CueSwap(bool),
    /// The master volume of the main mix, 0 to 1.
    Volume(f64),
    /// The cue's volume, 0 to 1, which the master volume leaves alone.
    Headphones(f64),
    /// The level, 0 to 0.99, below which the master's soft clip passes the
    /// mix unchanged.
    Knee(f32),
    /// An EQ band's gain in dB, -24 to 6.
    Eq(Side, Band, f64),
    /// Silences an EQ band, or brings it back at its gain.
    Kill(Side, Band, bool),
    /// The filter knob: -1 low-pass, 0 nothing, 1 high-pass.
    Filter(Side, f64),
    /// Hot cue `n`, from 0, pressed: set at the head when empty, else jump
    /// there and play. With quantize on it is set on a beat, and a jump
    /// while playing keeps the deck's phase.
    HotCue(Side, usize),
    HotClear(Side, usize),
    /// Moves the head this many beats, and a loop playing with it.
    BeatJump(Side, f64),
    /// Loops this many beats from the head, from the beat before it with
    /// quantize on; a loop playing keeps its start. `None` ends it.
    Loop(Side, Option<f64>),
    CueOut(CueOut),
    Curve(Curve),
    /// Moves the head to this frame. With quantize on, a playing deck keeps
    /// its phase. A loop playing ends unless the frame is inside it.
    Seek(Side, f64),
    /// Silences the deck in the main mix; the cue still hears it.
    Mute(Side, bool),
}

impl Setting {
    fn finite(&self) -> bool {
        match *self {
            Setting::Rate(_, v)
            | Setting::Gain(_, v)
            | Setting::Level(_, v)
            | Setting::Xfade(v)
            | Setting::XfadeGlide(v)
            | Setting::Volume(v)
            | Setting::Headphones(v)
            | Setting::Eq(_, _, v)
            | Setting::Filter(_, v)
            | Setting::BeatJump(_, v)
            | Setting::Loop(_, Some(v))
            | Setting::Seek(_, v)
            | Setting::PlayFrom(_, v) => v.is_finite(),
            Setting::Knee(v) => v.is_finite(),
            _ => true,
        }
    }
}

/// The decks and the mix. [`Mixer::process`] is the whole signal path and
/// is deterministic, so tests drive it directly.
#[derive(Debug, Clone)]
pub struct Mixer {
    sample_rate: u32,
    decks: [Deck; 2],
    quantize: bool,
    xfade: Ramp,
    cue_bus: Option<Side>,
    cue_swap: bool,
    volume: Ramp,
    headphones: Ramp,
    /// The largest magnitude of the main mix before the volume, since taken.
    peak: f32,
    knee: f32,
    /// Each deck's EQ bands, in dB, and whether each is killed.
    bands: [[(f64, bool); 3]; 2],
    cue_out: CueOut,
    curve: Curve,
    mutes: [Ramp; 2],
}

impl Mixer {
    pub fn new(sample_rate: u32) -> Self {
        Mixer {
            sample_rate,
            decks: [Deck::new(sample_rate), Deck::new(sample_rate)],
            quantize: false,
            xfade: Ramp::new(0.5),
            cue_bus: None,
            cue_swap: false,
            volume: Ramp::new(1.0),
            headphones: Ramp::new(1.0),
            peak: 0.0,
            knee: DEFAULT_KNEE,
            bands: [[(0.0, false); 3]; 2],
            cue_out: CueOut::default(),
            curve: Curve::default(),
            mutes: [Ramp::new(1.0); 2],
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn deck(&self, side: Side) -> &Deck {
        &self.decks[side.index()]
    }

    fn deck_mut(&mut self, side: Side) -> &mut Deck {
        &mut self.decks[side.index()]
    }

    pub fn quantize(&self) -> bool {
        self.quantize
    }

    /// The crossfader where it is now, moving or not.
    pub fn xfade(&self) -> f64 {
        self.xfade.value()
    }

    /// The largest magnitude of the main mix before the volume since the
    /// last call, which resets it.
    pub fn take_peak(&mut self) -> f32 {
        std::mem::take(&mut self.peak)
    }

    fn smooth(&self) -> u32 {
        (SMOOTH_MS * self.sample_rate as f64 / 1000.0) as u32
    }

    /// Loads `track` onto a paused deck and returns the one it replaced. A
    /// deck synced to it stops following. A playing deck refuses, and
    /// `track` comes back as the error.
    pub fn load(&mut self, side: Side, track: Box<Track>) -> Result<Box<Track>, Box<Track>> {
        let d = self.deck_mut(side);
        if d.playing() {
            return Err(track);
        }
        let old = d.load(track);
        self.deck_mut(side.other()).synced = None;
        Ok(old)
    }

    pub fn set(&mut self, s: Setting) {
        if !s.finite() {
            return;
        }
        match s {
            Setting::Play(d) => self.play(d),
            Setting::PlayFrom(d, at) => {
                let deck = self.deck_mut(d);
                let at = at.clamp(0.0, deck.track().frames() as f64);
                deck.seek(at);
                deck.play();
            }
            Setting::Pause(d) => self.deck_mut(d).pause(),
            Setting::Cue(d, true) => self.cue_down(d),
            Setting::Cue(d, false) => {
                if self.deck(d).previewing() {
                    self.deck_mut(d).back_to_cue();
                }
            }
            Setting::Rate(d, pct) => {
                let deck = self.deck_mut(d);
                deck.synced = None;
                deck.set_pct(pct);
            }
            Setting::Range(d, r) => {
                let deck = self.deck_mut(d);
                if !deck.set_range(r) {
                    deck.synced = None;
                }
            }
            Setting::Nudge(d, n) => self.deck_mut(d).set_nudge(n),
            Setting::Sync(d, true) => self.sync(d),
            Setting::Sync(d, false) => self.deck_mut(d).synced = None,
            Setting::Grid(d, g) => {
                self.deck_mut(d).set_grid(g);
                for side in [Side::A, Side::B] {
                    self.repick(side);
                }
            }
            Setting::Gain(d, db) => {
                let gain = 10f64.powf(db.clamp(-TRIM_DB, TRIM_DB) / 20.0);
                self.deck_mut(d).set_trim(gain);
            }
            Setting::Level(d, l) => self.deck_mut(d).set_level(l.clamp(0.0, 1.0)),
            Setting::Xfade(x) => {
                let n = self.smooth();
                self.xfade.set(x.clamp(0.0, 1.0), n);
            }
            Setting::XfadeGlide(x) => {
                let n = (GLIDE_MS * self.sample_rate as f64 / 1000.0) as u32;
                self.xfade.set(x.clamp(0.0, 1.0), n);
            }
            Setting::Quantize(on) => self.quantize = on,
            Setting::CueBus(d) => self.cue_bus = d,
            Setting::CueSwap(on) => self.cue_swap = on,
            Setting::Volume(v) => {
                let n = self.smooth();
                self.volume.set(v.clamp(0.0, 1.0), n);
            }
            Setting::Headphones(v) => {
                let n = self.smooth();
                self.headphones.set(v.clamp(0.0, 1.0), n);
            }
            Setting::Knee(v) => self.knee = v.clamp(0.0, 0.99),
            Setting::Eq(d, b, db) => {
                self.bands[d.index()][b.index()].0 = db.clamp(EQ_DB.0, EQ_DB.1);
                self.retune(d);
            }
            Setting::Kill(d, b, on) => {
                self.bands[d.index()][b.index()].1 = on;
                self.retune(d);
            }
            Setting::Filter(d, k) => self.deck_mut(d).filter.set(k as f32),
            Setting::HotCue(d, n) if n < HOT_CUES => self.hot_cue(d, n),
            Setting::HotClear(d, n) if n < HOT_CUES => self.deck_mut(d).hot[n] = None,
            Setting::HotCue(..) | Setting::HotClear(..) => {}
            Setting::BeatJump(d, beats) => self.beat_jump(d, beats),
            Setting::Loop(d, beats) => self.set_loop(d, beats),
            Setting::CueOut(c) => self.cue_out = c,
            Setting::Curve(c) => self.curve = c,
            Setting::Seek(d, to) => self.seek(d, to),
            Setting::Mute(d, on) => {
                let n = self.smooth();
                self.mutes[d.index()].set(if on { 0.0 } else { 1.0 }, n);
            }
        }
    }

    fn seek(&mut self, side: Side, to: f64) {
        let d = self.deck(side);
        let to = to.clamp(0.0, d.track().frames() as f64);
        let to = match (self.quantize, d.playing()) {
            (true, true) => self.snap(side, to) + d.pos() - self.snap(side, d.pos()),
            _ => to,
        };
        let to = self.in_track(side, to);
        self.deck_mut(side).seek(to);
    }

    /// Sends `side`'s EQ bands to its deck as gains.
    fn retune(&mut self, side: Side) {
        let gains = self.bands[side.index()].map(|(db, kill)| match kill {
            true => 0.0,
            false => 10f32.powf(db as f32 / 20.0),
        });
        self.deck_mut(side).eq.set(gains);
    }

    /// `frames` on `side`'s nearest beat with quantize on and a grid, else
    /// as it is.
    fn snap(&self, side: Side, frames: f64) -> f64 {
        let d = self.deck(side);
        match (self.quantize, d.grid()) {
            (true, Some(g)) => {
                let beat = g.beats(d.seconds(frames)).round();
                (g.t0 + beat * g.period()) * self.sample_rate as f64
            }
            _ => frames,
        }
    }

    /// `frames`, moved whole beats later if before the track's start, so a
    /// quantized position keeps its phase without playing silence first.
    fn in_track(&self, side: Side, frames: f64) -> f64 {
        match (frames < 0.0, self.beat(side)) {
            (true, Some(beat)) => frames + (-frames / beat).ceil() * beat,
            (true, None) => 0.0,
            (false, _) => frames,
        }
    }

    /// Frames per beat on `side`'s grid.
    fn beat(&self, side: Side) -> Option<f64> {
        let g = self.deck(side).grid()?;
        Some(g.period() * self.sample_rate as f64)
    }

    fn hot_cue(&mut self, side: Side, n: usize) {
        let d = self.deck(side);
        let Some(at) = d.hot[n] else {
            let at = self.in_track(side, self.snap(side, d.resting()));
            self.deck_mut(side).hot[n] = Some(at);
            return;
        };
        let to = match d.playing() {
            // As far past the cue as the head is past its nearest beat.
            true => at + d.pos() - self.snap(side, d.pos()),
            false => at,
        };
        let to = self.in_track(side, to);
        self.deck_mut(side).seek(to);
        self.play(side);
    }

    fn beat_jump(&mut self, side: Side, beats: f64) {
        let Some(beat) = self.beat(side) else {
            return;
        };
        let by = beats * beat;
        let d = self.deck_mut(side);
        // The loop moves first, so the head does not leave it.
        if let Some((start, len)) = d.looping() {
            d.set_loop(Some((start + by, len)));
        }
        let to = d.resting() + by;
        d.seek(to);
    }

    fn set_loop(&mut self, side: Side, beats: Option<f64>) {
        let (Some(beats), Some(beat)) = (beats.filter(|b| *b > 0.0), self.beat(side)) else {
            return self.deck_mut(side).set_loop(None);
        };
        let d = self.deck(side);
        let len = beats * beat;
        let pos = d.resting();
        let start = match (d.looping(), self.quantize, d.grid()) {
            (Some((start, _)), _, _) => start,
            (None, true, Some(g)) => {
                let before = g.beats(d.seconds(pos)).floor();
                (g.t0 + before * g.period()) * self.sample_rate as f64
            }
            (None, _, _) => pos,
        };
        // A loop shorter than the way back to its start begins where the
        // head is in it.
        let start = start + ((pos - start) / len).floor().max(0.0) * len;
        self.deck_mut(side).set_loop(Some((start, len)));
    }

    fn play(&mut self, side: Side) {
        let d = self.deck_mut(side);
        if d.playing() {
            d.latch();
            return;
        }
        d.play();
        self.quantize_start(side);
    }

    fn cue_down(&mut self, side: Side) {
        let d = self.deck(side);
        if d.previewing() {
            return;
        }
        if d.playing() {
            self.deck_mut(side).back_to_cue();
            return;
        }
        let at = self.in_track(side, self.snap(side, d.resting()));
        self.deck_mut(side).preview(at);
        self.quantize_start(side);
    }

    /// With quantize on, or the deck synced, and the other deck playing,
    /// moves a deck that has just started into phase with it.
    fn quantize_start(&mut self, side: Side) {
        let (f, l) = (self.deck(side), self.deck(side.other()));
        if !(self.quantize || f.synced()) || !l.playing() {
            return;
        }
        let (Some(gf), Some(gl)) = (f.grid(), l.grid()) else {
            return;
        };
        let m = multiple(gf.bpm * f.target(), gl.bpm * l.target());
        if let Some(by) = self.offset(side, m) {
            let d = self.deck_mut(side);
            let to = match d.looping() {
                // Kept in the loop: moving into phase does not leave it.
                Some((start, len)) => start + (d.pos() + by - start).rem_euclid(len),
                None => d.pos() + by,
            };
            d.jump(to);
        }
    }

    fn sync(&mut self, side: Side) {
        let (f, l) = (self.deck(side), self.deck(side.other()));
        let (Some(gf), Some(gl)) = (f.grid(), l.grid()) else {
            self.deck_mut(side).synced = None;
            return;
        };
        let m = multiple(gf.bpm, gl.bpm * l.base());
        self.deck_mut(side.other()).synced = None;
        self.deck_mut(side.other()).set_lock(1.0);
        let d = self.deck_mut(side);
        d.synced = Some(m);
        d.lock_offset = 0.0;
        if !self.follow(side) {
            return;
        }
        if let Some(by) = self.offset(side, m) {
            let d = self.deck_mut(side);
            let to = match d.looping() {
                // Kept in the loop: moving into phase does not leave it.
                Some((start, len)) => start + (d.pos() + by - start).rem_euclid(len),
                None => d.pos() + by,
            };
            d.jump(to);
        }
    }

    /// After a grid edit, picks the multiple that keeps a synced deck's
    /// rate nearest where it is, so an octave fix does not move it.
    fn repick(&mut self, side: Side) {
        let (f, l) = (self.deck(side), self.deck(side.other()));
        let (Some(_), Some(gf), Some(gl)) = (f.synced, f.grid(), l.grid()) else {
            return;
        };
        let m = multiple(gf.bpm * f.base(), gl.bpm * l.base());
        self.deck_mut(side).synced = Some(m);
    }

    /// Sets a synced deck's fader to the other deck's tempo. False, with
    /// sync off, if the grids are gone or no range holds the rate.
    fn follow(&mut self, side: Side) -> bool {
        let (f, l) = (self.deck(side), self.deck(side.other()));
        let Some(m) = f.synced else {
            return false;
        };
        let (Some(gf), Some(gl)) = (f.grid(), l.grid()) else {
            self.deck_mut(side).synced = None;
            return false;
        };
        let pct = (gl.bpm * l.base() / (m * gf.bpm) - 1.0) * 100.0;
        let d = self.deck_mut(side);
        if !d.fit_pct(pct) {
            d.synced = None;
            return false;
        }
        true
    }

    /// The frames `side` must move for its beats, counted `m` to each beat
    /// of its grid, to fall in phase with the other deck's. At most half a
    /// beat either way.
    fn offset(&self, side: Side, m: f64) -> Option<f64> {
        let (f, l) = (self.deck(side), self.deck(side.other()));
        let (gf, gl) = (f.grid()?, l.grid()?);
        let at_f = gf.beats(f.seconds(f.settled())) * m;
        let at_l = gl.beats(l.seconds(l.settled()));
        let mut e = (at_l - at_f).rem_euclid(1.0);
        if e >= 0.5 {
            e -= 1.0;
        }
        Some(e * gf.period() / m * self.sample_rate as f64)
    }

    /// Keeps a synced deck in phase with the other while both play: a rate
    /// trim in proportion to the phase error. A nudge moves the phase it
    /// holds instead.
    fn lock(&mut self, side: Side) {
        let (f, l) = (self.deck(side), self.deck(side.other()));
        let (Some(m), Some(g)) = (f.synced, f.grid()) else {
            return self.deck_mut(side).set_lock(1.0);
        };
        let nudging = f.nudge() != Nudge::Off;
        let by = self.offset(side, m).filter(|_| f.playing() && l.playing());
        let Some(by) = by.filter(|_| !nudging) else {
            let d = self.deck_mut(side);
            d.set_lock(1.0);
            // Held from where a nudge leaves it. A deck stopped holds none,
            // or the phase the leader happens to start in would be held.
            d.lock_offset = if nudging { f64::NAN } else { 0.0 };
            return;
        };
        let e = by / (g.period() / m * self.sample_rate as f64);
        let d = self.deck_mut(side);
        if d.lock_offset.is_nan() {
            d.lock_offset = e;
        }
        let err = (e - d.lock_offset + 0.5).rem_euclid(1.0) - 0.5;
        let trim = match err.abs() < LOCK_DEAD {
            true => 0.0,
            false => (LOCK_K * err).clamp(-LOCK_MAX, LOCK_MAX),
        };
        d.set_lock(1.0 + trim);
    }

    /// The crossfader's gains for decks A and B at `x`, 0 to 1.
    fn xfade_gains(&self, x: f64) -> [f32; 2] {
        match self.curve {
            Curve::Smooth => {
                let x = x * FRAC_PI_2;
                [x.cos() as f32, x.sin() as f32]
            }
            Curve::Sharp => [
                ((1.0 - x) / SHARP_EDGE).min(1.0) as f32,
                (x / SHARP_EDGE).min(1.0) as f32,
            ],
        }
    }

    /// Fills `out`, interleaved stereo.
    pub fn process(&mut self, out: &mut [f32]) {
        self.process_channels(out, 2);
    }

    /// Fills `out`, interleaved with `channels`, 2 or more: the main mix on
    /// the first two, or the split cue; with [`CueOut::Channels`] and 4 or
    /// more, the cue in stereo on the third and fourth. Channels past those
    /// are silent. In a split, each side is the mean of the clipped stereo.
    pub fn process_channels(&mut self, out: &mut [f32], channels: usize) {
        const FRAMES: usize = 512;
        let ch = channels.max(2);
        let (mut main, mut cue) = ([0.0f32; FRAMES * 2], [0.0f32; FRAMES * 2]);
        for block in out.chunks_mut(FRAMES * ch) {
            let n = block.len() / ch * 2;
            let routed = self.process_buses(&mut main[..n], &mut cue[..n]);
            let frames = block.chunks_exact_mut(ch);
            let pairs = main[..n]
                .as_chunks::<2>()
                .0
                .iter()
                .zip(cue[..n].as_chunks::<2>().0);
            for (o, (m, c)) in frames.zip(pairs) {
                o.fill(0.0);
                match routed {
                    Some((out, swap)) if out == CueOut::Split || ch < 4 => {
                        let (l, r) = ((m[0] + m[1]) / 2.0, (c[0] + c[1]) / 2.0);
                        (o[0], o[1]) = if swap { (r, l) } else { (l, r) };
                    }
                    routed => {
                        o[..2].copy_from_slice(m);
                        if routed.is_some() {
                            o[2..4].copy_from_slice(c);
                        }
                    }
                }
            }
        }
    }

    /// Fills `main_out` with the main mix and `cue_out` with the cued deck,
    /// interleaved stereo, each after the soft clip and its volume. Returns
    /// where the cue goes and whether its sides swap; `None` with no deck cued,
    /// when `cue_out` stays silent.
    pub fn process_buses(
        &mut self,
        main_out: &mut [f32],
        cue_out: &mut [f32],
    ) -> Option<(CueOut, bool)> {
        for side in [Side::A, Side::B] {
            self.follow(side);
            self.lock(side);
        }
        let knee = self.knee;
        let frames = main_out.as_chunks_mut::<2>().0.iter_mut();
        for (mo, co) in frames.zip(cue_out.as_chunks_mut::<2>().0) {
            let x = self.xfade.next();
            let gains = self.xfade_gains(x);
            let vol = self.volume.next() as f32;
            let phones = self.headphones.next() as f32;
            let mut main = [0.0f32; 2];
            let mut cue = [0.0f32; 2];
            for (i, d) in self.decks.iter_mut().enumerate() {
                let s = d.next();
                if self.cue_bus.is_some_and(|c| c.index() == i) {
                    cue = s;
                }
                let g = d.level.next() as f32 * gains[i] * self.mutes[i].next() as f32;
                main[0] += s[0] * g;
                main[1] += s[1] * g;
            }
            let m = [soft_clip(main[0], knee), soft_clip(main[1], knee)];
            self.peak = self.peak.max(m[0].abs()).max(m[1].abs());
            *mo = [m[0] * vol, m[1] * vol];
            *co = match self.cue_bus {
                Some(_) => [
                    soft_clip(cue[0], knee) * phones,
                    soft_clip(cue[1], knee) * phones,
                ],
                None => [0.0; 2],
            };
        }
        self.cue_bus.map(|_| (self.cue_out, self.cue_swap))
    }
}

/// The rate fader, in percent, that sync sets on a deck whose grid reads
/// `follower` BPM to follow `leader` BPM as played. Beyond +/-50 sync refuses.
pub fn sync_pct(follower: f64, leader: f64) -> f64 {
    let m = multiple(follower, leader);
    (leader / (m * follower) - 1.0) * 100.0
}

/// Of half, the same and double, the multiple of `f` nearest `l` in log terms.
fn multiple(f: f64, l: f64) -> f64 {
    let off = |m: f64| (m * f / l).ln().abs();
    MULTIPLES
        .into_iter()
        .min_by(|a, b| off(*a).total_cmp(&off(*b)))
        .expect("not empty")
}
