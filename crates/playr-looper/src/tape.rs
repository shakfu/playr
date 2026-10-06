//! The loop buffer, its voices and its write head: all of the DSP, with no
//! device and no threads. See `docs/dev/looper-engine.md`.

use std::f32::consts::FRAC_PI_2;

use playr_dsp::{clip, flush, hermite, one_pole, soft_clip};

use crate::{Error, COLUMNS, VOICES};

type Ramp = playr_dsp::Ramp<f32>;

/// How long changes to level, pan, send, feedback and wear take.
const SMOOTH_MS: f32 = 20.0;
/// How long a rate change takes until a voice's slew is set.
pub const DEFAULT_SLEW_MS: f32 = SMOOTH_MS;
/// How long the write blends in and out at the write window's edges.
const EDGE_MS: f32 = 10.0;
/// The crossfade at a voice's wrap until set otherwise.
pub const DEFAULT_FADE_MS: f32 = 10.0;
/// The DC blocker's corner.
const DC_HZ: f32 = 10.0;
/// The wear low-pass's cutoff at wear 1; it falls there from the top of the
/// band on a log scale.
const WEAR_HZ: f32 = 500.0;
const WEAR_TOP_HZ: f32 = 20_000.0;
/// The thin high-pass's cutoff at thin 0 and at 1, on a log scale between.
const THIN_HZ: (f32, f32) = (20.0, 2000.0);
/// A voice filter's cutoff at 0 and at 1, on a log scale between.
const FILTER_HZ: (f32, f32) = (20.0, 20_000.0);
/// The gain a voice's drive reaches at 1, as a ratio: 24 dB.
const DRIVE_GAIN: f32 = 16.0;
/// Drive blends in from dry over its first `DRIVE_BLEND`, so leaving 0 does
/// not step on material above the clip's knee.
const DRIVE_BLEND: f32 = 0.05;
/// The knee of the clip on what the write head records, until set.
pub const DEFAULT_KNEE: f32 = 0.5;
/// The longest rate slew, in ms.
pub const MAX_SLEW_MS: f32 = 10_000.0;

/// The wear low-pass's one-pole coefficient: off at 0, falling on a log
/// scale from the top of the band to [`WEAR_HZ`] at 1.
fn wear_coefficient(wear: f32, sample_rate: u32) -> f32 {
    let top = WEAR_TOP_HZ.min(0.45 * sample_rate as f32);
    one_pole(top * (WEAR_HZ / top).powf(wear), sample_rate)
}

/// The thin high-pass's one-pole coefficient: 20 Hz at 0, rising on a log
/// scale to 2 kHz at 1.
fn thin_coefficient(thin: f32, sample_rate: u32) -> f32 {
    let (lo, hi) = THIN_HZ;
    one_pole(lo * (hi / lo).powf(thin), sample_rate)
}

/// Drive `d`, 0 to 1: gain up to [`DRIVE_GAIN`] into the soft clip, half of
/// it in dB taken back after, so quiet material gains up to 12 dB and loud
/// material is held under a quarter of full scale. 0 passes `x` unchanged.
fn drive(x: f32, d: f32) -> f32 {
    if d == 0.0 {
        return x;
    }
    let k = DRIVE_GAIN.powf(d);
    let y = clip(k * x) / k.sqrt();
    x + (y - x) * (d / DRIVE_BLEND).min(1.0)
}

/// What a voice's filter passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    Low,
    High,
    Band,
}

/// A voice's filter: a Butterworth state-variable filter over a cutoff of
/// 0 to 1, on a log scale across [`FILTER_HZ`].
#[derive(Debug, Clone, Copy)]
struct Svf {
    svf: playr_dsp::Svf,
    /// The cutoff, 0 to 1, the coefficients were computed for.
    at: f32,
}

impl Svf {
    fn new() -> Self {
        Svf {
            svf: playr_dsp::Svf::butterworth(),
            at: f32::NAN,
        }
    }

    fn tune(&mut self, cutoff: f32, sample_rate: u32) {
        if cutoff == self.at {
            return;
        }
        self.at = cutoff;
        let (lo, hi) = FILTER_HZ;
        let hz = (lo * (hi / lo).powf(cutoff)).min(0.45 * sample_rate as f32);
        self.svf.tune(hz, sample_rate);
    }

    /// Filters `x` in place: `morph` of the way from `kinds[0]` to
    /// `kinds[1]`, mixed `wet` with the input. The state runs while dry, so
    /// the filter does not start from silence.
    fn process(&mut self, x: &mut [f32; 2], kinds: [Filter; 2], morph: f32, wet: f32) {
        for (ch, x) in x.iter_mut().enumerate() {
            let t = self.svf.tick(ch, *x);
            let [a, b] = kinds.map(|k| match k {
                Filter::Low => t.low,
                Filter::High => t.high,
                Filter::Band => t.band,
            });
            *x += wet * (a + morph * (b - a) - *x);
        }
    }
}

/// Output frames a crossfade at a wrap takes: `fade_ms`, cut to half the time
/// the head takes to cross a window of `window` frames at `rate`.
pub fn fade_frames(fade_ms: f32, sample_rate: u32, window: usize, rate: f32) -> u32 {
    let fade = fade_ms * sample_rate as f32 / 1000.0;
    let cap = window as f32 / (2.0 * rate.abs().max(1.0));
    fade.min(cap) as u32
}

/// How a wrap crossfades, in loop frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossfade {
    /// How far before the window's edge the wrap starts: 0 when the leaving
    /// head fades out past the edge, `span` when the new head fades in from
    /// before the other edge instead.
    pub lead: f64,
    /// Loop frames each head reads while fading.
    pub span: f64,
    /// Output frames the fade lasts.
    pub frames: u32,
}

/// The crossfade at a wrap from window `from` into `to`, on a loop of
/// `frames`, at `rate`. The leaving head fades out over what follows `from`
/// when that much is there; otherwise the new head fades in over what comes
/// before `to`; otherwise the fade is cut to the longer of the two. Either
/// way the wrap stays one window long and reads nothing past the loop.
pub fn crossfade(
    from: Window,
    to: Window,
    frames: usize,
    rate: f32,
    fade_ms: f32,
    sample_rate: u32,
) -> Crossfade {
    let speed = f64::from(rate.abs());
    if speed == 0.0 {
        return Crossfade {
            lead: 0.0,
            span: 0.0,
            frames: 0,
        };
    }
    let want = f64::from(fade_frames(fade_ms, sample_rate, from.len(), rate)) * speed;
    let (after, before) = match rate > 0.0 {
        true => ((frames - from.end) as f64, to.start as f64),
        false => (from.start as f64, (frames - to.end) as f64),
    };
    let (ahead, room) = match () {
        _ if after >= want => (false, want),
        _ if before >= want => (true, want),
        _ if after >= before => (false, after),
        _ => (true, before),
    };
    let n = (room / speed).floor() as u32;
    let span = f64::from(n) * speed;
    Crossfade {
        lead: if ahead { span } else { 0.0 },
        span,
        frames: n,
    }
}

/// A part of the loop, in frames: `start` inclusive, `end` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: usize,
    pub end: usize,
}

impl Window {
    pub fn new(start: usize, end: usize) -> Self {
        Window { start, end }
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    fn contains(&self, pos: f64) -> bool {
        self.start as f64 <= pos && pos < self.end as f64
    }

    /// Itself when it lies inside `frames`, else `range`.
    fn fit(self, frames: usize, range: Window) -> Self {
        match !self.is_empty() && self.end <= frames {
            true => self,
            false => range,
        }
    }
}

/// A loop as the tape holds it: interleaved samples and the peak grid.
#[derive(Debug, Clone)]
pub struct Loop {
    samples: Box<[f32]>,
    channels: usize,
    peaks: [f32; COLUMNS],
    /// The part chosen to loop; the frames before it are pre-roll and those
    /// after it post-roll, which crossfades read past a window's edges.
    range: Window,
}

impl Loop {
    /// `samples` interleaved in 1 or 2 channels, at the rate of the tape it
    /// will be loaded into. Samples that are not finite become 0, since
    /// feedback would keep them forever.
    pub fn new(mut samples: Vec<f32>, channels: u16) -> Result<Self, Error> {
        let channels = channels as usize;
        if !(1..=2).contains(&channels) {
            return Err(Error::Channels(channels as u16));
        }
        if samples.is_empty() || !samples.len().is_multiple_of(channels) {
            return Err(Error::Length);
        }
        for s in samples.iter_mut().filter(|s| !s.is_finite()) {
            *s = 0.0;
        }
        let mut lp = Loop {
            samples: samples.into_boxed_slice(),
            channels,
            peaks: [0.0; COLUMNS],
            range: Window::new(0, 0),
        };
        lp.range = Window::new(0, lp.frames());
        for i in 0..lp.frames() {
            let col = lp.column(i);
            lp.peaks[col] = lp.peaks[col].max(lp.peak_at(i));
        }
        Ok(lp)
    }

    /// The loop with `range` as the part chosen, and the frames before and
    /// after it as pre-roll and post-roll. Windows start as the range.
    pub fn with_range(mut self, range: Window) -> Result<Self, Error> {
        if range.is_empty() || range.end > self.frames() {
            return Err(Error::Length);
        }
        self.range = range;
        Ok(self)
    }

    pub fn range(&self) -> Window {
        self.range
    }

    fn empty() -> Self {
        Loop {
            samples: Box::new([]),
            channels: 1,
            peaks: [0.0; COLUMNS],
            range: Window::new(0, 0),
        }
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn into_samples(self) -> Box<[f32]> {
        self.samples
    }

    pub fn channels(&self) -> u16 {
        self.channels as u16
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }

    /// The largest magnitude in each of [`COLUMNS`] equal parts of the loop.
    pub fn peaks(&self) -> &[f32; COLUMNS] {
        &self.peaks
    }

    fn column(&self, frame: usize) -> usize {
        (frame as u64 * COLUMNS as u64 / self.frames() as u64) as usize
    }

    fn peak_at(&self, frame: usize) -> f32 {
        let c = self.channels;
        self.samples[frame * c..frame * c + c]
            .iter()
            .fold(0.0, |m, s| m.max(s.abs()))
    }

    /// Frame `pos`, cubic Hermite interpolated, into `out[..channels]`.
    /// Neighbours wrap around the whole loop.
    fn read(&self, pos: f64, out: &mut [f32; 2]) {
        let frames = self.frames() as isize;
        let i = pos.floor();
        let t = (pos - i) as f32;
        let i = i as isize;
        let c = self.channels;
        let at = |k: isize, ch: usize| self.samples[(k.rem_euclid(frames) as usize) * c + ch];
        for (ch, o) in out.iter_mut().enumerate().take(c) {
            let x0 = at(i, ch);
            if t == 0.0 {
                *o = x0;
                continue;
            }
            *o = hermite(at(i - 1, ch), x0, at(i + 1, ch), at(i + 2, ch), t);
        }
    }
}

/// A setting of the tape, as [`Tape::set`] takes it. Voices count from 0.
/// Values out of range are clamped; settings that are not finite, or name no
/// voice, are ignored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Setting {
    Play,
    /// Silences the output and holds every head.
    Stop,
    On(usize, bool),
    /// Frames advanced per output frame, -4 to 4.
    Rate(usize, f32),
    Window(usize, Window),
    /// Puts the voice's head at this frame of the loop, inside its window;
    /// while playing it jumps there, crossfading.
    Head(usize, f64),
    Level(usize, f32),
    Pan(usize, f32),
    Send(usize, f32),
    /// The crossfade at a wrap, 0 to 1000 ms.
    Fade(usize, f32),
    /// The low-pass on the voice's send, 0 to 1 as [`Setting::Wear`].
    VoiceWear(usize, f32),
    /// Turn at the window's edges instead of wrapping.
    Ping(usize, bool),
    /// How long a rate change takes, 0 to [`MAX_SLEW_MS`] ms.
    Slew(usize, f32),
    /// Saturation on what the voice reads, 0 to 1.
    Drive(usize, f32),
    /// The voice filter's cutoff, 0 to 1: 20 Hz to 20 kHz on a log scale.
    Cutoff(usize, f32),
    Filter(usize, Filter),
    /// Hear only the soloed voices, while any is; sends are unchanged.
    Solo(usize, bool),
    Write(bool),
    WriteWindow(Window),
    Feedback(f32),
    Wear(f32),
    /// A high-pass on everything the write head records, 0 to 1.
    Thin(f32),
    /// The level, 0 to 0.99, below which the clip on what the write head
    /// records passes the signal unchanged.
    Knee(f32),
}

/// One read head, and a second while a wrap crossfades.
#[derive(Debug, Clone)]
struct Voice {
    on: bool,
    gate: Ramp,
    rate: Ramp,
    level: Ramp,
    pan: Ramp,
    send: Ramp,
    /// A low-pass on the send alone, with its coefficient and state.
    wear: Ramp,
    wear_at: f32,
    wear_a: f32,
    wear_lp: [f32; 2],
    drive: Ramp,
    cutoff: Ramp,
    filter: Filter,
    /// The filter faded from, and how far the fade to `filter` is.
    was: Filter,
    morph: Ramp,
    /// How much of the filter is heard: 0 where it would pass the input
    /// unchanged, low at 1 or high at 0, so the switch is a crossfade.
    wet: Ramp,
    svf: Svf,
    /// What is heard of the voice under solo, 0 or 1.
    heard: Ramp,
    solo: bool,
    ping: bool,
    /// The direction Ping has turned the head to, as a sign on its rate.
    dir: f64,
    slew_ms: f32,
    fade_ms: f32,
    window: Window,
    /// A window set while the head was inside the current one; it applies
    /// at the next wrap.
    next: Option<Window>,
    pos: f64,
    /// The head has not moved since it was put at its window's start, so a
    /// rate set now picks the end that suits its direction.
    homed: bool,
    /// The head fading out, and how far through the fade it is.
    old: Option<f64>,
    faded: u32,
    fade_len: u32,
}

impl Voice {
    fn new(on: bool) -> Self {
        let g = if on { 1.0 } else { 0.0 };
        Voice {
            on,
            gate: Ramp::new(g),
            rate: Ramp::new(1.0),
            level: Ramp::new(1.0),
            pan: Ramp::new(0.0),
            send: Ramp::new(0.0),
            wear: Ramp::new(0.0),
            wear_at: 0.0,
            wear_a: 1.0,
            wear_lp: [0.0; 2],
            drive: Ramp::new(0.0),
            cutoff: Ramp::new(1.0),
            filter: Filter::Low,
            was: Filter::Low,
            morph: Ramp::new(1.0),
            wet: Ramp::new(0.0),
            svf: Svf::new(),
            heard: Ramp::new(1.0),
            solo: false,
            ping: false,
            dir: 1.0,
            slew_ms: DEFAULT_SLEW_MS,
            fade_ms: DEFAULT_FADE_MS,
            window: Window::new(0, 0),
            next: None,
            pos: 0.0,
            homed: false,
            old: None,
            faded: 0,
            fade_len: 0,
        }
    }

    /// Fades the filter in or out as its kind and cutoff's target make it
    /// pass the input unchanged or not.
    fn retarget_wet(&mut self, frames: u32) {
        let c = self.cutoff.target();
        let dry = match self.filter {
            Filter::Low => c >= 1.0,
            Filter::High => c <= 0.0,
            Filter::Band => false,
        };
        let want = if dry { 0.0 } else { 1.0 };
        if self.wet.target() != want {
            self.wet.set(want, frames);
        }
    }

    /// Where a head starts in `w`: its first frame, or its last in reverse.
    fn home(&self, w: Window) -> f64 {
        match self.rate.target() < 0.0 {
            true => (w.end - 1) as f64,
            false => w.start as f64,
        }
    }

    fn jump(&mut self, to: f64, rate: f32, sample_rate: u32) {
        self.fade_len = fade_frames(self.fade_ms, sample_rate, self.window.len(), rate);
        self.faded = 0;
        self.old = (self.fade_len > 0).then_some(self.pos);
        self.pos = to;
    }

    fn set_window(&mut self, w: Window, playing: bool, sample_rate: u32) {
        if !playing {
            self.window = w;
            self.next = None;
            self.pos = self.home(w);
            self.homed = true;
            self.old = None;
            self.dir = 1.0;
        } else if w.contains(self.pos) {
            self.next = Some(w);
        } else {
            self.window = w;
            self.next = None;
            self.jump(self.home(w), self.rate.value(), sample_rate);
        }
    }

    /// Puts the head at `at`, kept inside the window: a jump while playing,
    /// else a move.
    fn place(&mut self, at: f64, playing: bool, sample_rate: u32) {
        let w = self.window;
        let at = at.clamp(w.start as f64, (w.end - 1) as f64);
        if playing {
            self.jump(at, self.rate.value(), sample_rate);
        } else {
            self.pos = at;
            self.homed = false;
            self.old = None;
        }
    }

    /// Reads one frame into `mix` (stereo) and `send` (the loop's channels),
    /// then advances. Returns whether the voice sent anything.
    fn render(
        &mut self,
        lp: &Loop,
        sample_rate: u32,
        mix: &mut [f32; 2],
        send: &mut [f32; 2],
    ) -> bool {
        let gate = self.gate.next();
        if gate == 0.0 && !self.on {
            return false;
        }
        let rate = self.rate.next();
        let level = self.level.next() * gate * self.heard.next();
        let pan = self.pan.next();
        let sends = self.send.next() * gate;
        let wear = self.wear.next();
        let drive_d = self.drive.next();
        let cutoff = self.cutoff.next();
        self.svf.tune(cutoff, sample_rate);
        if wear != self.wear_at {
            self.wear_at = wear;
            self.wear_a = wear_coefficient(wear, sample_rate);
        }

        let mut s = [0.0; 2];
        lp.read(self.pos, &mut s);
        if let Some(old) = self.old {
            let phi = FRAC_PI_2 * self.faded as f32 / self.fade_len as f32;
            let mut o = [0.0; 2];
            lp.read(old, &mut o);
            let (gin, gout) = (phi.sin(), phi.cos());
            for (s, o) in s.iter_mut().zip(o) {
                *s = *s * gin + o * gout;
            }
        }
        // Drive, then the filter, which can take off the edge drive adds.
        for s in s.iter_mut().take(lp.channels) {
            *s = drive(*s, drive_d);
        }
        let (morph, wet) = (self.morph.next(), self.wet.next());
        self.svf
            .process(&mut s, [self.was, self.filter], morph, wet);

        let [l, r] = pan_frame(s, lp.channels, pan);
        mix[0] += l * level;
        mix[1] += r * level;
        // The send's low-pass tracks its input while off, so it resumes without a step.
        for ((o, s), y) in send.iter_mut().zip(s).zip(&mut self.wear_lp) {
            *y = match wear > 0.0 {
                true => flush(*y + self.wear_a * (s - *y)),
                false => s,
            };
            *o += *y * sends;
        }

        self.advance(rate, sample_rate, lp.frames());
        sends != 0.0
    }

    fn advance(&mut self, rate: f32, sample_rate: u32, frames: usize) {
        let r = rate as f64 * self.dir;
        self.pos += r;
        self.homed = false;
        if let Some(old) = &mut self.old {
            *old += r;
            self.faded += 1;
            if self.faded >= self.fade_len {
                self.old = None;
            }
        }
        if rate == 0.0 {
            return;
        }
        if self.ping {
            return self.turn(sample_rate);
        }
        let (w, to) = (self.window, self.next.unwrap_or(self.window));
        let x = crossfade(w, to, frames, rate, self.fade_ms, sample_rate);
        let forward = rate > 0.0;
        let trigger = match forward {
            true => w.end as f64 - x.lead,
            false => w.start as f64 + x.lead,
        };
        if (forward && self.pos < trigger) || (!forward && self.pos >= trigger) {
            return;
        }
        self.next = None;
        self.window = to;
        let len = to.len() as f64;
        let at = match forward {
            true => to.start as f64 - x.lead + (self.pos - trigger).rem_euclid(len),
            false => {
                let d = (trigger - self.pos).rem_euclid(len);
                to.end as f64 + x.lead - if d == 0.0 { len } else { d }
            }
        };
        self.fade_len = x.frames;
        self.faded = 0;
        self.old = (x.frames > 0).then_some(self.pos);
        self.pos = at;
    }
}

impl Voice {
    /// Under Ping: a head past either end of its window comes back from it,
    /// reversed, with no jump to fade over. A window set meanwhile applies here.
    fn turn(&mut self, sample_rate: u32) {
        let w = self.window;
        let (first, last) = (w.start as f64, (w.end - 1) as f64);
        let back = match () {
            _ if self.pos > last => 2.0 * last - self.pos,
            _ if self.pos < first => 2.0 * first - self.pos,
            _ => return,
        };
        self.dir = -self.dir;
        self.pos = back.clamp(first, last);
        if let Some(to) = self.next.take() {
            self.window = to;
            if !to.contains(self.pos) {
                let at = self.pos.clamp(to.start as f64, (to.end - 1) as f64);
                self.jump(at, self.rate.value(), sample_rate);
            }
        }
    }
}

/// One frame of the loop's `channels`, panned to stereo. Mono pans with equal
/// power. Stereo keeps the near channel and pans the far one into it with
/// equal power, so a hard pan keeps both channels' content. Both are unity
/// at the centre.
fn pan_frame(s: [f32; 2], channels: usize, pan: f32) -> [f32; 2] {
    if channels == 1 {
        return [s[0] * (1.0 - pan).sqrt(), s[0] * (1.0 + pan).sqrt()];
    }
    let phi = FRAC_PI_2 * pan.abs();
    let (stay, cross) = (phi.cos().max(0.0), phi.sin());
    match pan < 0.0 {
        true => [s[0] + s[1] * cross, s[1] * stay],
        false => [s[0] * stay, s[1] + s[0] * cross],
    }
}

/// The write path's filters: the wear low-pass and the DC blocker.
#[derive(Debug, Clone, Copy, Default)]
struct Filters {
    lp: [f32; 2],
    /// The thin high-pass's low-pass, which it takes from its input.
    hp: [f32; 2],
    dc_x: [f32; 2],
    dc_y: [f32; 2],
}

/// What the write head writes with on one frame.
struct Pass {
    feedback: f32,
    wear: f32,
    lp_a: f32,
    /// How much of the thin high-pass is applied, 0 to 1.
    thin_mix: f32,
    hp_a: f32,
    knee: f32,
    dc_g: f32,
    dc_r: f32,
    send: [f32; 2],
    sending: bool,
    /// The blend from the old content to the new.
    g: f32,
}

/// A run of frames the write head writes: its window, or a roll it writes as
/// the range's continuation. Each has its own filters and peak column.
#[derive(Debug, Clone, Copy)]
struct Lane {
    filters: Filters,
    /// The peak column the lane is in, the peak written in it so far, and
    /// whether the lane will have rewritten all of it on leaving.
    col: usize,
    col_peak: f32,
    col_whole: bool,
}

impl Lane {
    fn new() -> Self {
        Lane {
            filters: Filters::default(),
            col: usize::MAX,
            col_peak: 0.0,
            col_whole: false,
        }
    }

    /// Writes frame `i`. The lane's frames end at `bound`.
    fn write(&mut self, lp: &mut Loop, i: usize, p: &Pass, bound: usize) {
        let f = &mut self.filters;
        let c = lp.channels;
        for (ch, slot) in lp.samples[i * c..i * c + c].iter_mut().enumerate() {
            let old = *slot;
            let mut x = old * p.feedback + p.send[ch];
            // Bypassed filters track their input, so they resume without a step.
            if p.wear > 0.0 {
                f.lp[ch] += p.lp_a * (x - f.lp[ch]);
                x = f.lp[ch];
            } else {
                f.lp[ch] = x;
            }
            // Its low-pass runs while off, so thin starts from where it is.
            f.hp[ch] = flush(f.hp[ch] + p.hp_a * (x - f.hp[ch]));
            x -= p.thin_mix * f.hp[ch];
            if p.sending {
                let y = flush(p.dc_g * (x - f.dc_x[ch]) + p.dc_r * f.dc_y[ch]);
                f.dc_x[ch] = x;
                f.dc_y[ch] = y;
                x = soft_clip(y, p.knee);
            } else {
                f.dc_x[ch] = x;
                f.dc_y[ch] = x;
            }
            *slot = flush(match p.g {
                1.0 => x,
                g => old + g * (x - old),
            });
        }
        self.peak(lp, i, bound);
    }

    /// Keeps the peak grid in step with what the lane writes.
    fn peak(&mut self, lp: &mut Loop, i: usize, bound: usize) {
        let col = lp.column(i);
        if col != self.col {
            self.leave(lp);
            self.col = col;
            self.col_peak = 0.0;
            // Whole when entered at its first frame and it ends inside the lane.
            let first = (col as u64 * lp.frames() as u64).div_ceil(COLUMNS as u64) as usize;
            let end = ((col as u64 + 1) * lp.frames() as u64).div_ceil(COLUMNS as u64) as usize;
            self.col_whole = i == first && end <= bound;
        }
        self.col_peak = self.col_peak.max(lp.peak_at(i));
        lp.peaks[col] = lp.peaks[col].max(self.col_peak);
    }

    fn leave(&mut self, lp: &mut Loop) {
        if self.col_whole && self.col < COLUMNS {
            lp.peaks[self.col] = self.col_peak;
        }
    }
}

/// The write head and its lanes.
#[derive(Debug, Clone)]
struct Writer {
    on: bool,
    gate: Ramp,
    feedback: Ramp,
    wear: Ramp,
    thin: Ramp,
    /// Whether thin is on, 0 or 1, ramped: its lowest setting, 20 Hz, is
    /// not transparent, so turning it on or off is a crossfade.
    thin_on: Ramp,
    knee: f32,
    window: Window,
    pos: usize,
    /// The wear low-pass's coefficient, and the wear it was computed for.
    lp_a: f32,
    lp_wear: f32,
    /// The thin high-pass's coefficient, and the thin it was computed for.
    hp_a: f32,
    hp_thin: f32,
    main: Lane,
    /// The post-roll and pre-roll, written while the window is the range.
    post: Lane,
    pre: Lane,
}

impl Writer {
    fn new() -> Self {
        Writer {
            on: false,
            gate: Ramp::new(0.0),
            feedback: Ramp::new(1.0),
            wear: Ramp::new(0.0),
            thin: Ramp::new(0.0),
            thin_on: Ramp::new(0.0),
            knee: DEFAULT_KNEE,
            window: Window::new(0, 0),
            pos: 0,
            lp_a: 1.0,
            lp_wear: 0.0,
            hp_a: 0.0,
            hp_thin: f32::NAN,
            main: Lane::new(),
            post: Lane::new(),
            pre: Lane::new(),
        }
    }

    fn rewind(&mut self) {
        self.pos = self.window.start;
        self.main = Lane::new();
        self.post = Lane::new();
        self.pre = Lane::new();
    }

    /// Blend gain at the head: 0 to 1 over the window's first `edge` frames
    /// and back over its last, or 1 when the window is the whole loop or
    /// the range, whose rolls are written with it.
    fn edge(&self, lp: &Loop, edge: usize) -> f32 {
        let w = self.window;
        if w.len() == lp.frames() || w == lp.range || w.len() < 2 * edge || edge == 0 {
            return 1.0;
        }
        let i = self.pos - w.start;
        let d = i.min(w.len() - 1 - i);
        (d as f32 / edge as f32).min(1.0)
    }

    /// Writes the frame under the head and advances. While the window is the
    /// range, the frames a crossing head reads past its edges are written as
    /// the range's continuation: their own content at the same feedback, plus
    /// the send written one range-length away. Each roll then keeps the
    /// range's level and carries its sends, so a crossfade into it does not
    /// meet the loop as loaded.
    fn write(
        &mut self,
        lp: &mut Loop,
        sample_rate: u32,
        edge: usize,
        dc_r: f32,
        send: [f32; 2],
        sending: bool,
    ) {
        let gate = self.gate.next();
        let feedback = self.feedback.next();
        let wear = self.wear.next();
        let thin = self.thin.next();
        let thin_mix = self.thin_on.next();
        if gate == 0.0 && !self.on {
            self.advance();
            return;
        }
        if wear != self.lp_wear {
            self.lp_wear = wear;
            self.lp_a = wear_coefficient(wear, sample_rate);
        }
        if thin != self.hp_thin {
            self.hp_thin = thin;
            self.hp_a = thin_coefficient(thin, sample_rate);
        }
        let p = Pass {
            feedback,
            wear,
            lp_a: self.lp_a,
            thin_mix,
            hp_a: self.hp_a,
            knee: self.knee,
            dc_g: (1.0 + dc_r) / 2.0,
            dc_r,
            send,
            sending,
            g: self.edge(lp, edge) * gate,
        };
        let (i, r) = (self.pos, lp.range);
        if self.window == r {
            let n = r.len();
            // Each roll starts from the range's filters where it joins the range.
            if i + n < lp.frames() {
                if i == r.start {
                    self.post.filters = self.main.filters;
                }
                self.post.write(lp, i + n, &p, lp.frames().min(r.end + n));
            }
            if i >= n {
                if i == r.start.max(n) {
                    self.pre.filters = self.main.filters;
                }
                self.pre.write(lp, i - n, &p, r.start);
            }
        }
        self.main.write(lp, i, &p, self.window.end);
        self.advance();
    }

    fn advance(&mut self) {
        self.pos += 1;
        if self.pos >= self.window.end {
            self.pos = self.window.start;
        }
    }
}

/// The loop, its voices and its write head. [`Tape::process`] is the whole
/// signal path and is deterministic, so tests drive it directly.
#[derive(Debug, Clone)]
pub struct Tape {
    sample_rate: u32,
    lp: Box<Loop>,
    voices: [Voice; VOICES],
    writer: Writer,
    playing: bool,
    smooth: u32,
    edge: usize,
    dc_r: f32,
}

impl Tape {
    /// An empty, stopped tape at `sample_rate`. Voice 0 is on; writing is off.
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate as f32;
        Tape {
            sample_rate,
            lp: Box::new(Loop::empty()),
            voices: [Voice::new(true), Voice::new(false), Voice::new(false)],
            writer: Writer::new(),
            playing: false,
            smooth: (SMOOTH_MS * sr / 1000.0) as u32,
            edge: (EDGE_MS * sr / 1000.0) as usize,
            dc_r: (-std::f32::consts::TAU * DC_HZ / sr).exp(),
        }
    }

    /// Replaces the loop and returns the old one. Windows that no longer fit
    /// become the whole loop, and every head goes to its window's start.
    pub fn load(&mut self, lp: Box<Loop>) -> Box<Loop> {
        let old = std::mem::replace(&mut self.lp, lp);
        let (frames, range) = (self.lp.frames(), self.lp.range);
        for v in &mut self.voices {
            let w = v.window.fit(frames, range);
            v.set_window(w, false, self.sample_rate);
        }
        self.writer.window = self.writer.window.fit(frames, range);
        self.writer.rewind();
        old
    }

    pub fn set(&mut self, s: Setting) {
        let ramp = if self.playing { self.smooth } else { 0 };
        let frames = self.lp.frames();
        let sr = self.sample_rate;
        let finite = |v: f32| v.is_finite();
        match s {
            Setting::Play => self.playing = true,
            Setting::Stop => self.playing = false,
            Setting::Write(on) => {
                self.writer.on = on;
                self.writer.gate.set(on as u8 as f32, ramp);
            }
            Setting::WriteWindow(w) => {
                if w.is_empty() || w.end > frames {
                    return;
                }
                self.writer.window = w;
                if !(w.start..w.end).contains(&self.writer.pos) {
                    self.writer.pos = w.start;
                }
            }
            Setting::Feedback(v) if finite(v) => self.writer.feedback.set(v.clamp(0.0, 1.0), ramp),
            Setting::Wear(v) if finite(v) => self.writer.wear.set(v.clamp(0.0, 1.0), ramp),
            Setting::Thin(v) if finite(v) => {
                let v = v.clamp(0.0, 1.0);
                self.writer.thin.set(v, ramp);
                self.writer
                    .thin_on
                    .set(if v > 0.0 { 1.0 } else { 0.0 }, ramp);
            }
            Setting::Knee(v) if finite(v) => self.writer.knee = v.clamp(0.0, 0.99),
            Setting::On(i, on) if i < VOICES => {
                let v = &mut self.voices[i];
                v.on = on;
                v.gate.set(on as u8 as f32, ramp);
            }
            Setting::Rate(i, r) if i < VOICES && finite(r) => {
                let v = &mut self.voices[i];
                let slew = match self.playing {
                    true => (v.slew_ms * sr as f32 / 1000.0) as u32,
                    false => 0,
                };
                v.rate.set(r.clamp(-4.0, 4.0), slew);
                if v.homed {
                    v.pos = v.home(v.window);
                }
            }
            Setting::Level(i, v) if i < VOICES && finite(v) => {
                self.voices[i].level.set(v.clamp(0.0, 1.0), ramp)
            }
            Setting::Pan(i, v) if i < VOICES && finite(v) => {
                self.voices[i].pan.set(v.clamp(-1.0, 1.0), ramp)
            }
            Setting::Send(i, v) if i < VOICES && finite(v) => {
                self.voices[i].send.set(v.clamp(0.0, 1.0), ramp)
            }
            Setting::VoiceWear(i, v) if i < VOICES && finite(v) => {
                self.voices[i].wear.set(v.clamp(0.0, 1.0), ramp)
            }
            Setting::Fade(i, ms) if i < VOICES && finite(ms) => {
                self.voices[i].fade_ms = ms.clamp(0.0, 1000.0)
            }
            Setting::Ping(i, on) if i < VOICES => {
                let v = &mut self.voices[i];
                v.ping = on;
                v.dir = 1.0;
            }
            Setting::Slew(i, ms) if i < VOICES && finite(ms) => {
                self.voices[i].slew_ms = ms.clamp(0.0, MAX_SLEW_MS)
            }
            Setting::Drive(i, v) if i < VOICES && finite(v) => {
                self.voices[i].drive.set(v.clamp(0.0, 1.0), ramp)
            }
            Setting::Cutoff(i, c) if i < VOICES && finite(c) => {
                let v = &mut self.voices[i];
                v.cutoff.set(c.clamp(0.0, 1.0), ramp);
                v.retarget_wet(ramp);
            }
            Setting::Filter(i, kind) if i < VOICES => {
                let v = &mut self.voices[i];
                if kind != v.filter {
                    v.was = v.filter;
                    v.filter = kind;
                    v.morph = Ramp::new(0.0);
                    v.morph.set(1.0, ramp);
                }
                v.retarget_wet(ramp);
            }
            Setting::Solo(i, on) if i < VOICES => {
                self.voices[i].solo = on;
                let any = self.voices.iter().any(|v| v.solo);
                for v in &mut self.voices {
                    v.heard.set((!any || v.solo) as u8 as f32, ramp);
                }
            }
            Setting::Window(i, w) if i < VOICES && !w.is_empty() && w.end <= frames => {
                self.voices[i].set_window(w, self.playing, sr)
            }
            Setting::Head(i, at) if i < VOICES && at.is_finite() => {
                self.voices[i].place(at, self.playing, sr)
            }
            _ => {}
        }
    }

    /// Fills `out`, interleaved stereo, with the next frames, and writes
    /// the loop as it goes. Silence while stopped or empty.
    pub fn process(&mut self, out: &mut [f32]) {
        if !self.playing || self.lp.frames() == 0 {
            out.fill(0.0);
            return;
        }
        let sr = self.sample_rate;
        for frame in out.as_chunks_mut::<2>().0 {
            let mut mix = [0.0; 2];
            let mut send = [0.0; 2];
            let mut sending = false;
            for v in &mut self.voices {
                sending |= v.render(&self.lp, sr, &mut mix, &mut send);
            }
            self.writer
                .write(&mut self.lp, sr, self.edge, self.dc_r, send, sending);
            frame.copy_from_slice(&mix);
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn buffer(&self) -> &Loop {
        &self.lp
    }

    /// Voice `i`'s head, in frames.
    pub fn voice(&self, i: usize) -> f64 {
        self.voices[i].pos
    }

    pub fn write_head(&self) -> usize {
        self.writer.pos
    }

    pub fn write_window(&self) -> Window {
        self.writer.window
    }
}
