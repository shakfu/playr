//! The tape looper in playr: `:tape` actions, loading the sampler's range,
//! and saving the loop and the mix as samples. The engine is `playr-looper`;
//! its design is in `docs/dev/looper-engine.md`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use playr_core::audio::bus::Handover;
use playr_core::audio::output::DeviceEvent;
use playr_core::audio::resample::Resample;
use playr_core::audio::{Attachment, Sources, State};
use playr_core::samples;
use playr_looper::{BusSetting, DelayParam, Handle, Loop, Looper, Returned, Setting, Window};

use crate::dispatch::Frontend;
use crate::message::Message;

mod actions;
mod controls;
mod saved;

pub use crate::fx::{
    model_name, Division, FxField, FxSetting, FxState, DELAY_MODELS, REVERB_MODELS,
};
pub use actions::{act, load_loop, poll, resume};
pub use controls::{fed, no_effect, settings, Control, Idle};
pub use playr_looper::modulation::{Dest, Route, Shape, Speed, LFOS, LFO_HZ, SLOTS};
pub use saved::{Reads, SavedTape};

/// How the Tape tab shows each lane: folded to its header and a strip, and
/// with its drawer of less-used fields open. A lane not folded or unfolded
/// by hand is folded while empty.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Layout {
    pub folded: [Option<bool>; playr_looper::VOICES],
    pub drawer: [bool; playr_looper::VOICES],
    /// The FX lane is open; it starts folded.
    pub fx: bool,
}

impl Layout {
    /// Whether lane `i` is folded, when it is `empty`.
    pub fn is_folded(&self, i: usize, empty: bool) -> bool {
        self.folded[i].unwrap_or(empty)
    }
}

/// A point in the loop, as a window names it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pos {
    Percent(f32),
    Time(Duration),
}

/// A setting of one voice. Ranges are checked where it is parsed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VoiceSetting {
    On(bool),
    /// Frames per frame; negative plays in reverse.
    Rate(f32),
    Window(Pos, Pos),
    Level(f32),
    Pan(f32),
    Send(f32),
    /// A low-pass on what the voice sends, 0 to 1.
    Wear(f32),
    /// The crossfade at a wrap, in ms.
    Fade(f32),
    /// Turn at the window's edges instead of wrapping.
    Ping(bool),
    /// How long a rate change takes, in ms.
    Slew(f32),
    /// Saturation on what the voice reads, 0 to 1.
    Drive(f32),
    /// The EQ: a high-pass and a low-pass in Hz, and a bell's centre in Hz,
    /// gain in dB and Q.
    HighPass(f32),
    LowPass(f32),
    BellHz(f32),
    BellGain(f32),
    BellQ(f32),
    /// Hear only the soloed voices; what they send is unchanged.
    Solo(bool),
    /// Silence what is heard of the voice; its head and send go on.
    Mute(bool),
    /// The sends to the delay and the reverb, 0 to 1, after the level.
    Delay(f32),
    Reverb(f32),
}

/// What `:tape` does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TapeAction {
    /// Load the sampler's range of the playing track, or its loop slot `n`.
    Load(Option<u8>),
    /// Load the sampler's range of the playing track and take over from the
    /// player: at its position inside the range, or at the range's start
    /// once the player reaches it.
    Take,
    /// Play, pausing the player.
    Play,
    Stop,
    /// Restore the loop as loaded.
    Reset,
    /// Silence the write loop, keeping its length, through voice `n`, from
    /// 1: voice 4, or a voice reading the write loop.
    Clear(u8),
    /// Empty voice `n`, so it reads nothing.
    Eject(u8),
    /// Voice `n` reads the write loop.
    Share(u8),
    /// Load loop `id` of the loop library into voice `n`.
    Loop(u8, i64),
    /// Save the loop as a sample.
    Save,
    /// Start or stop recording the mix.
    Record,
    /// A setting of voice `n`, from 1.
    Voice(u8, VoiceSetting),
    Write(bool),
    WriteWindow(Pos, Pos),
    Feedback(f32),
    Wear(f32),
    /// A high-pass on everything the write head records, 0 to 1.
    Thin(f32),
    Fx(FxSetting),
    /// Record the mix as heard into voice 4's loop, for one pass of the
    /// write window or until stopped; `None` stops either.
    Bounce(Option<Bounce>),
    /// Fold or unfold lane `n`, from 1; `None` is the FX lane.
    Fold(Option<u8>),
    /// Open or shut lane `n`'s drawer, from 1.
    Drawer(u8),
    /// A setting of LFO `k`: 0, a, or 1, b.
    Lfo(u8, LfoSetting),
    /// What modulation slot `n`, from 1, routes, or nothing.
    Route(u8, Option<Route>),
}

/// A setting of an LFO.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LfoSetting {
    Shape(Shape),
    Hz(f32),
    /// One cycle per this part of voice 1's window.
    Sync(Division),
}

/// A voice's destination, given the voice from 0.
pub type VoiceDest = fn(usize) -> Dest;

/// The names `:tape mod` gives destinations: a voice's, then the effects'.
pub const VOICE_DESTS: [(&str, VoiceDest); 11] = [
    ("rate", Dest::Rate),
    ("start", Dest::Start),
    ("pan", Dest::Pan),
    ("level", Dest::Level),
    ("send", Dest::Send),
    ("hp", Dest::HighPass),
    ("lp", Dest::LowPass),
    ("bell", Dest::BellHz),
    ("bell-gain", Dest::BellGain),
    ("delay", Dest::DelaySend),
    ("reverb", Dest::ReverbSend),
];
pub const FX_DESTS: [(&str, Dest); 5] = [
    ("delay-time", Dest::DelayTime),
    ("delay-feedback", Dest::DelayFeedback),
    ("delay-tone", Dest::DelayTone),
    ("reverb-damp", Dest::ReverbDamp),
    ("print-fx", Dest::PrintFx),
];

/// `dest`'s name, and its voice from 1 for a voice's.
pub fn dest_name(dest: Dest) -> (&'static str, Option<u8>) {
    match dest.voice() {
        Some(i) => {
            let name = VOICE_DESTS
                .iter()
                .find(|(_, f)| f(i) == dest)
                .map_or("", |d| d.0);
            (name, Some(i as u8 + 1))
        }
        None => (
            FX_DESTS.iter().find(|d| d.1 == dest).map_or("", |d| d.0),
            None,
        ),
    }
}

/// The destination `name` names, with `voice` from 1 for a voice's.
pub fn dest_named(name: &str, voice: Option<u8>) -> Option<Dest> {
    match voice {
        Some(n) if (1..=playr_looper::VOICES as u8).contains(&n) => VOICE_DESTS
            .iter()
            .find(|d| d.0 == name)
            .map(|d| d.1(usize::from(n) - 1)),
        Some(_) => None,
        None => FX_DESTS.iter().find(|d| d.0 == name).map(|d| d.1),
    }
}

/// An LFO's letter.
pub fn lfo_name(k: usize) -> &'static str {
    ["a", "b"].get(k).copied().unwrap_or("?")
}

/// The division a synced speed is a part of voice 1's window by.
pub fn speed_division(speed: Speed) -> Option<Division> {
    match speed {
        Speed::Sync(part) => Division::ALL
            .into_iter()
            .find(|d| d.fraction() as f32 == part),
        Speed::Hz(_) => None,
    }
}

/// How long a bounce records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bounce {
    /// One pass of the write window, into the write loop, making it as
    /// Write does when voice 4 is empty.
    Pass,
    /// Until stopped, up to [`LONG_BOUNCE`]: a new stereo loop of that
    /// length, which replaces voice 4's.
    Long,
}

/// The longest a bounce until stopped records.
pub const LONG_BOUNCE: Duration = Duration::from_secs(300);

/// What the tape reports.
#[derive(Debug, Clone, PartialEq)]
pub enum TapeMessage {
    Loading,
    Loaded {
        frames: usize,
        rate: u32,
    },
    /// A loop of its own is in voice `voice`, from 1.
    LoadedVoice {
        voice: u8,
        frames: usize,
        rate: u32,
    },
    /// A library loop asked for that the library no longer holds.
    NoSuchLoop,
    /// The action took effect.
    Done(TapeAction),
    /// A `:tape` action other than load before anything was loaded.
    NoTape,
    /// Voice `n` reads nothing, for an action that needs a loop.
    Empty(u8),
    /// A clear of voice `n`, which reads a loop of its own: only the write
    /// loop is cleared.
    OwnLoop(u8),
    /// Every voice is empty, so the tape is gone.
    Emptied,
    /// The tape kept at the last close is back, but for the loops in
    /// `lost`, whose files could not be read.
    Resumed {
        lost: Vec<PathBuf>,
    },
    /// `:tape load` with no range set.
    NoRange,
    EmptySlot(u8),
    /// A window whose start is not before its end.
    EmptyWindow,
    /// A save started while another runs.
    AlreadySaving,
    /// A save cut short by a load, a reset or a new write window.
    SaveAborted,
    /// The output device went away, and the tape with it.
    DeviceLost(String),
    /// The tape took over from the player.
    Took,
    /// The tape takes over when the player reaches the range's start.
    TakeWaiting,
    /// A take found the player past the range.
    PastRange,
    /// A take's player left the track, or stopped, before the tape took over.
    TakeDropped,
    Saving,
    /// A bounce finished its pass, or was stopped; a bounce until stopped
    /// is then in voice 4.
    Bounced,
    /// A bounce until stopped that recorded nothing: the tape was stopped.
    NothingBounced,
    /// Record and a bounce until stopped share one recorder.
    RecorderBusy,
    Saved(PathBuf),
    Recording(PathBuf),
    Recorded {
        path: PathBuf,
        frames: u64,
        rate: u32,
        dropped: u64,
    },
    Failed(String),
}

impl From<TapeMessage> for Message {
    fn from(m: TapeMessage) -> Self {
        Message::Tape(m)
    }
}

/// Where the tape plays.
enum Output {
    /// The player's stream, summed with what else plays there.
    Bus(Sources),
    /// Nowhere: [`Deck::process`] runs the looper, for tests.
    Manual,
}

/// The looper as a source on the player's stream.
struct Played(Looper);

impl playr_core::audio::bus::Source for Played {
    fn process(&mut self, main: &mut [f32], _cue: &mut [f32]) -> playr_core::audio::bus::Cue {
        self.0.process(main);
        playr_core::audio::bus::Cue::None
    }

    fn start(&mut self) {
        self.0.play();
    }
}

/// A loop read and ready to play.
struct Loaded {
    lp: Loop,
    rate: u32,
}

/// A load in progress: the decoding thread's result, and where it will play.
struct Loading {
    result: Receiver<Result<Loaded, String>>,
    name: String,
    /// Where it lies in its source, as `loops::span_text` gives it.
    span: String,
    /// The voice it is for, alone, from 0; `None` for every voice.
    voice: Option<usize>,
    /// The beats the loop library gives it.
    beats: Option<u32>,
    /// What it reads, for the tape to be resumed from.
    source: Source,
    /// A bounce's WAV, removed once the load is done or dropped.
    bounce: Option<TempFile>,
}

/// A file removed when dropped.
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        _ = std::fs::remove_file(&self.0);
    }
}

/// What a load reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    path: PathBuf,
    /// The rate `span` counts frames at.
    rate: u32,
    span: (u64, u64),
    /// Its rolls are its own other end, as for a file cut to loop, rather
    /// than the audio either side of it.
    circular: bool,
    name: String,
    voice: Option<usize>,
    beats: Option<u32>,
}

/// The tape playing, or ready to.
struct Live {
    handle: Handle,
    /// Where the looper plays on the player's stream; `None` for a manual deck.
    attached: Option<Attachment>,
    looper: Option<Looper>,
    /// The source track's name, for the directories saves go in.
    name: String,
    /// The write loop's shape, and that of every voice without its own.
    extent: Extent,
    /// The voices' own loops' shapes; `None` for a voice reading the write
    /// loop.
    own: [Option<Extent>; playr_looper::VOICES],
    /// Where voice 0's loop lies in its source, as `loops::span_text` gives it.
    span: String,
    /// The voices' own loops' names and spans, as `own`.
    own_names: [Option<(String, String)>; playr_looper::VOICES],
    /// The beats the loop library gives voice 0's loop.
    beats: Option<u32>,
    /// Where the snapshot asked for is to be saved.
    save_to: Option<PathBuf>,
    /// What each voice's own loop was read from, and at [`playr_looper::WRITER`],
    /// the write loop's; `None` for a voice reading the write loop, an empty
    /// one, or a write loop Write made.
    sources: [Option<Source>; playr_looper::VOICES],
    /// The write loop has been written or cleared since it was loaded or
    /// reset, so it differs from its source.
    printed: bool,
    /// A bounce sent, and whether the looper has reported it running.
    bounce: Option<bool>,
    /// A bounce until stopped: the WAV it records to, and when it began.
    long: Option<(PathBuf, Instant)>,
    /// A bounce until stopped's WAV, until its load takes it.
    bounced: Option<TempFile>,
    state: TapeState,
    /// The volume last sent to the looper.
    volume: f32,
    /// The interpolation last sent; `None` until one is.
    interp: Option<playr_dsp::Interp>,
}

/// How far each side of the range a load reads, for crossfades to fade into:
/// the longest crossfade at rate 1.
const ROLL: Duration = Duration::from_secs(1);

/// A loaded loop's shape: its frames with pre-roll and post-roll, the range chosen
/// within them, and its rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent {
    pub frames: usize,
    pub range: Window,
    pub rate: u32,
}

/// A voice's settings as last sent, for controls to show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceState {
    pub on: bool,
    pub rate: f32,
    pub window: Window,
    pub level: f32,
    pub pan: f32,
    pub send: f32,
    pub wear: f32,
    pub fade: f32,
    pub ping: bool,
    pub slew: f32,
    pub drive: f32,
    pub high_pass: f32,
    pub low_pass: f32,
    pub bell_hz: f32,
    pub bell_gain: f32,
    pub bell_q: f32,
    pub solo: bool,
    pub mute: bool,
    /// The sends to the delay and the reverb.
    pub delay: f32,
    pub reverb: f32,
}

/// The delay time, in ms, that `division` of voice 1's window makes, as
/// voice 1 plays it over a loop of shape `e`: `None` while it reads
/// nothing or stands still.
pub fn synced(division: Division, voice: &VoiceState, e: Extent) -> Option<f32> {
    let seconds = voice.window.len() as f64 / f64::from(e.rate) / f64::from(voice.rate.abs());
    let ms = (seconds * division.fraction() * 1000.0) as f32;
    let (lo, hi) = (playr_looper::DELAY_MIN, playr_looper::DELAY_SECONDS);
    (e.frames > 0 && voice.rate != 0.0).then(|| ms.clamp(lo * 1000.0, hi * 1000.0))
}

/// The tape's settings as last sent. The looper's own are not readable
/// from outside the audio thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TapeState {
    pub voices: [VoiceState; playr_looper::VOICES],
    /// Which voices read a loop of their own, rather than the write loop.
    pub own: [bool; playr_looper::VOICES],
    /// Which voices read nothing: ejected, or never loaded.
    pub empty: [bool; playr_looper::VOICES],
    pub playing: bool,
    pub write: bool,
    pub write_window: Window,
    pub feedback: f32,
    pub wear: f32,
    pub thin: f32,
    pub fx: FxState,
    /// Each LFO's wave and speed, and what each slot routes.
    pub lfos: [(Shape, Speed); LFOS],
    pub routes: [Option<Route>; SLOTS],
}

impl TapeState {
    /// A new looper's settings over a loop chosen as `range`: voice 1 on,
    /// writing off, as `playr_looper::Tape::new` starts.
    pub fn new(range: Window) -> Self {
        let whole = range;
        let voice = |on| VoiceState {
            on,
            rate: 1.0,
            window: whole,
            level: 1.0,
            pan: 0.0,
            send: 0.0,
            wear: 0.0,
            fade: playr_looper::DEFAULT_FADE_MS,
            ping: false,
            slew: playr_looper::DEFAULT_SLEW_MS,
            drive: 0.0,
            high_pass: playr_looper::FILTER_HZ.0,
            low_pass: playr_looper::FILTER_HZ.1,
            bell_hz: playr_looper::DEFAULT_BELL_HZ,
            bell_gain: 0.0,
            bell_q: playr_looper::DEFAULT_BELL_Q,
            solo: false,
            mute: false,
            delay: 0.0,
            reverb: 0.0,
        };
        TapeState {
            voices: std::array::from_fn(|i| voice(i == 0)),
            own: [false; playr_looper::VOICES],
            empty: [false; playr_looper::VOICES],
            playing: false,
            write: false,
            write_window: whole,
            feedback: 1.0,
            wear: 0.0,
            thin: 0.0,
            fx: FxState::default(),
            lfos: [(Shape::default(), Speed::default()); LFOS],
            routes: [None; SLOTS],
        }
    }

    fn apply(&mut self, s: Setting) {
        match s {
            Setting::Play => self.playing = true,
            Setting::Stop => self.playing = false,
            Setting::Write(on) => self.write = on,
            Setting::WriteWindow(w) => self.write_window = w,
            Setting::Feedback(x) => self.feedback = x,
            Setting::Wear(x) => self.wear = x,
            Setting::On(i, on) => self.voices[i].on = on,
            Setting::Rate(i, x) => self.voices[i].rate = x,
            Setting::Window(i, w) => self.voices[i].window = w,
            Setting::Level(i, x) => self.voices[i].level = x,
            Setting::Pan(i, x) => self.voices[i].pan = x,
            Setting::Send(i, x) => self.voices[i].send = x,
            Setting::Fade(i, x) => self.voices[i].fade = x,
            Setting::VoiceWear(i, x) => self.voices[i].wear = x,
            Setting::Ping(i, on) => self.voices[i].ping = on,
            Setting::Slew(i, x) => self.voices[i].slew = x,
            Setting::Drive(i, x) => self.voices[i].drive = x,
            Setting::HighPass(i, x) => self.voices[i].high_pass = x,
            Setting::LowPass(i, x) => self.voices[i].low_pass = x,
            Setting::BellHz(i, x) => self.voices[i].bell_hz = x,
            Setting::BellGain(i, x) => self.voices[i].bell_gain = x,
            Setting::BellQ(i, x) => self.voices[i].bell_q = x,
            Setting::Solo(i, on) => self.voices[i].solo = on,
            Setting::Mute(i, on) => self.voices[i].mute = on,
            Setting::Thin(x) => self.thin = x,
            Setting::DelaySend(i, x) => self.voices[i].delay = x,
            Setting::ReverbSend(i, x) => self.voices[i].reverb = x,
            Setting::Fx(b) => self.fx.apply(b),
            Setting::PrintFx(x) => self.fx.print = x,
            Setting::LfoShape(k, shape) if k < LFOS => self.lfos[k].0 = shape,
            Setting::LfoSpeed(k, speed) if k < LFOS => self.lfos[k].1 = speed,
            Setting::Route(k, route) if k < SLOTS => self.routes[k] = route,
            Setting::LfoShape(..) | Setting::LfoSpeed(..) | Setting::Route(..) => {}
            // From the settings file, a take and the mixer, and a bounce, which
            // the looper's status reports: not the tape's settings.
            Setting::Knee(_) | Setting::Head(..) | Setting::Interp(_) | Setting::Bounce(_) => {}
        }
    }
}

/// What [`Deck::poll`] found.
enum Found {
    Note(TapeMessage),
    /// A file written, to add to the library.
    Wrote(TapeMessage, PathBuf),
    /// A loop written, to add to the loop library, with the beats it holds.
    WroteLoop(TapeMessage, PathBuf, Option<u32>),
}

/// A save's result: the file written and the beats it holds, or why not.
type Saved = Result<(PathBuf, Option<u32>), String>;

/// The looper's state on the interface's side.
pub struct Deck {
    /// The knee of the clip on what the write head records.
    knee: f32,
    output: Output,
    live: Option<Live>,
    loading: Option<Loading>,
    /// Saves finishing on their threads, and the stream's events.
    saved: (Sender<Saved>, Receiver<Saved>),
    errors: (Sender<DeviceEvent>, Receiver<DeviceEvent>),
    /// The volume the mixer gives the tape, which a new looper starts at.
    volume: f32,
    /// A take under way, until the tape takes over or it is dropped.
    taking: Option<Taking>,
    /// A resume under way, until its loops are read and its settings sent.
    restoring: Option<Restoring>,
    layout: Layout,
}

/// A resume: the tape kept, the loads still to read, last first, and the
/// files that could not be read.
struct Restoring {
    saved: SavedTape,
    queue: Vec<Source>,
    lost: Vec<PathBuf>,
}

/// A take: the track and range loaded, and how far it has gone.
#[derive(Debug, Clone)]
struct Taking {
    path: PathBuf,
    /// The track's rate, which `span` counts frames at.
    rate: u32,
    span: (u64, u64),
    /// The loop is in.
    loaded: bool,
    /// The player is before the range, and was told so.
    waiting: bool,
    /// The player holds a handover to the tape.
    armed: bool,
}

/// How far ahead of the device a take arms its handover from inside the
/// range: room for the callback to see it before its frame plays.
const TAKE_LEAD: f64 = 0.1;

impl Deck {
    /// A deck playing on the player's stream through `sources`, with the
    /// write head's clip at `knee`.
    pub fn new(sources: Sources, knee: f32) -> Self {
        Deck::with(Output::Bus(sources), knee)
    }

    /// A deck with no device, which plays only as [`Deck::process`] is called.
    pub fn manual() -> Self {
        Deck::with(Output::Manual, playr_looper::DEFAULT_KNEE)
    }

    fn with(output: Output, knee: f32) -> Self {
        Deck {
            knee,
            output,
            live: None,
            loading: None,
            saved: channel(),
            errors: channel(),
            volume: 1.0,
            taking: None,
            restoring: None,
            layout: Layout::default(),
        }
    }

    /// Sends the looper `volume` when it changed.
    fn follow_volume(&mut self, volume: f32) {
        self.volume = volume;
        if let Some(live) = self.live.as_mut().filter(|l| l.volume != volume) {
            if live.handle.set_volume(volume).is_ok() {
                live.volume = volume;
            }
        }
    }

    /// Sends the heads' interpolation, when it changed. A new looper has
    /// none sent, so it gets it too.
    fn follow_interp(&mut self, interp: playr_dsp::Interp) {
        if let Some(live) = self.live.as_mut().filter(|l| l.interp != Some(interp)) {
            if live.handle.set(Setting::Interp(interp)).is_ok() {
                live.interp = Some(interp);
            }
        }
    }

    /// How the Tape tab shows each lane.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    /// Whether a load is being read.
    pub fn loading(&self) -> bool {
        self.loading.is_some()
    }

    /// The tape to keep for the next run: `None` with no tape, or every voice
    /// empty. While a resume is under way, the tape it resumes.
    pub fn saved(&self) -> Option<SavedTape> {
        if let Some(r) = &self.restoring {
            return Some(r.saved.clone());
        }
        let live = self.live.as_ref()?;
        let mut reads: [Reads; playr_looper::VOICES] = Default::default();
        for (i, r) in reads.iter_mut().enumerate().take(playr_looper::WRITER) {
            *r = match (live.own[i], &live.sources[i]) {
                (None, _) => Reads::Write,
                (Some(e), Some(src)) if e.frames > 0 => Reads::File(src.clone()),
                _ => Reads::Empty,
            };
        }
        let w = playr_looper::WRITER;
        let mut shape = None;
        reads[w] = match (&live.sources[w], live.printed, live.extent.frames) {
            (_, _, 0) => Reads::Empty,
            (Some(src), false, _) => Reads::File(src.clone()),
            _ => {
                let channels = live.handle.channels().unwrap_or(1);
                shape = Some((live.extent.frames, live.extent.range, channels));
                Reads::Write
            }
        };
        if reads[w] == Reads::Empty {
            for r in &mut reads {
                if *r == Reads::Write {
                    *r = Reads::Empty;
                }
            }
        }
        let mut state = live.state;
        state.write = false;
        for (i, r) in reads.iter().enumerate() {
            state.own[i] = i != w && matches!(r, Reads::File(_));
            state.empty[i] = *r == Reads::Empty;
        }
        let saved = SavedTape {
            rate: live.handle.sample_rate(),
            state,
            reads,
            shape,
            layout: self.layout,
        };
        (saved.files() > 0 || saved.shape.is_some()).then_some(saved)
    }

    /// Starts the next load of a resume, or finishes it.
    fn restore_next(&mut self) -> Vec<Found> {
        while let Some(src) = self.restoring.as_mut().and_then(|r| r.queue.pop()) {
            let path = src.path.clone();
            match self.load(src) {
                Ok(()) => return Vec::new(),
                Err(_) => self
                    .restoring
                    .as_mut()
                    .expect("popped above")
                    .lost
                    .push(path),
            }
        }
        let Some(r) = self.restoring.take() else {
            return Vec::new();
        };
        // Nothing read: a tape at the kept rate holds the silent write loop.
        if self.live.is_none() && r.saved.shape.is_some() {
            let rate = match &self.output {
                Output::Bus(sources) => sources.rate_for(r.saved.rate),
                Output::Manual => r.saved.rate,
            };
            let silent = Loaded {
                lp: Loop::empty(),
                rate,
            };
            if let Err(e) = self.start(String::new(), String::new(), None, None, silent) {
                return vec![Found::Note(TapeMessage::Failed(e))];
            }
        }
        let done = match self.live.as_mut() {
            Some(live) => live.restore(&r.saved).map_err(|e| e.to_string()),
            None => Ok(()),
        };
        vec![Found::Note(match done {
            Ok(()) => TapeMessage::Resumed { lost: r.lost },
            Err(e) => TapeMessage::Failed(e),
        })]
    }

    /// The write loop's shape, once there is a tape; no frames until Write
    /// first makes one.
    pub fn loaded(&self) -> Option<Extent> {
        self.live.as_ref().map(|l| l.extent)
    }

    /// The shape of the loop voice `i`, from 0, reads.
    pub fn voice_extent(&self, i: usize) -> Option<Extent> {
        self.live.as_ref().map(|l| l.extent_of(i))
    }

    /// The name of the loop voice `i`, from 0, reads: its own, or voice 0's
    /// source.
    pub fn voice_name(&self, i: usize) -> Option<&str> {
        let live = self.live.as_ref().filter(|l| !l.state.empty[i])?;
        Some(match live.own_names.get(i)? {
            Some((name, _)) => name,
            None => &live.name,
        })
    }

    /// Where the loop voice `i`, from 0, reads lies in its source, as
    /// `loops::span_text` gives it.
    pub fn voice_span(&self, i: usize) -> Option<&str> {
        let live = self.live.as_ref().filter(|l| !l.state.empty[i])?;
        Some(match live.own_names.get(i)? {
            Some((_, span)) => span,
            None => &live.span,
        })
    }

    /// The settings as last sent, once a loop is loaded.
    pub fn state(&self) -> Option<&TapeState> {
        self.live.as_ref().map(|l| &l.state)
    }

    /// What the looper publishes: its heads, peak grid and level.
    pub fn status(&self) -> Option<&playr_looper::Status> {
        self.live.as_ref().map(|l| &**l.handle.status())
    }

    pub fn recording(&self) -> bool {
        self.live.as_ref().is_some_and(|l| l.handle.recording())
    }

    /// Whether a bounce runs: one pass, as the looper last reported, or one
    /// until stopped.
    pub fn bouncing(&self) -> bool {
        self.live
            .as_ref()
            .is_some_and(|l| l.long.is_some() || l.handle.status().bouncing())
    }

    pub fn saving(&self) -> bool {
        self.live.as_ref().is_some_and(|l| l.save_to.is_some())
    }

    /// Runs a manual deck's looper for `out`, interleaved stereo.
    pub fn process(&mut self, out: &mut [f32]) {
        if let Some(looper) = self.live.as_mut().and_then(|l| l.looper.as_mut()) {
            looper.process(out);
        }
    }

    /// Starts reading `src`. A load for one voice goes into the tape playing,
    /// at its rate; with none playing it is a load for every voice.
    fn load(&mut self, mut src: Source) -> Result<(), String> {
        let live_rate = self.live.as_ref().map(|l| l.handle.sample_rate());
        // With no tape, voice 1's loop starts one.
        if live_rate.is_none() && src.voice == Some(0) {
            src.voice = None;
        }
        let to = match (src.voice, live_rate, &self.output) {
            (Some(_), Some(r), _) => r,
            (_, _, Output::Bus(sources)) => sources.rate_for(src.rate),
            (_, _, Output::Manual) => src.rate,
        };
        let (tx, rx) = channel();
        let (name, voice, beats) = (src.name.clone(), src.voice, src.beats);
        let span = crate::loops::span_text(src.span.0, src.span.1, src.rate);
        let source = Source {
            voice: None,
            ..src.clone()
        };
        let bounce = self
            .live
            .as_mut()
            .and_then(|l| l.bounced.take_if(|b| b.0 == src.path));
        std::thread::spawn(move || _ = tx.send(read(&src, to)));
        self.loading = Some(Loading {
            result: rx,
            name,
            span,
            voice,
            beats,
            source,
            bounce,
        });
        Ok(())
    }

    /// Takes in what finished since the last call.
    fn poll(&mut self) -> Vec<Found> {
        let mut found = Vec::new();
        let done = match self.loading.as_ref().map(|l| l.result.try_recv()) {
            Some(Ok(result)) => Some(result),
            Some(Err(TryRecvError::Disconnected)) => Some(Err("the read stopped".into())),
            Some(Err(TryRecvError::Empty)) | None => None,
        };
        if let Some(result) = done {
            let loading = self.loading.take().expect("polled above");
            let path = loading.source.path.clone();
            let into_voice = loading.voice.filter(|_| self.live.is_some());
            // A recording of the old tape is finished before it goes.
            if into_voice.is_none() {
                if let Some(f) = self.live.as_mut().and_then(stop_recording) {
                    found.push(f);
                }
            }
            let started = match (into_voice, loading.voice) {
                (Some(i), _) => result.and_then(|l| self.load_voice(i, loading, l)),
                // Into another voice with no tape: an empty tape, then the voice.
                (None, Some(i)) => result.and_then(|l| {
                    let silent = Loaded {
                        lp: Loop::empty(),
                        rate: l.rate,
                    };
                    self.start(String::new(), String::new(), None, None, silent)?;
                    self.load_voice(i, loading, l)
                }),
                (None, None) => result.and_then(|l| {
                    let source = Some(loading.source);
                    self.start(loading.name, loading.span, loading.beats, source, l)
                }),
            };
            match (&started, &mut self.taking) {
                (Ok(_), Some(t)) => t.loaded = true,
                (Err(_), _) => self.taking = None,
                _ => {}
            }
            match self.restoring.as_mut() {
                // A resume reports once, when done.
                Some(r) => {
                    if started.is_err() {
                        r.lost.push(path);
                    }
                    found.extend(self.restore_next());
                }
                None => found.push(Found::Note(started.unwrap_or_else(TapeMessage::Failed))),
            }
        }
        if let Some(live) = &mut self.live {
            // Reported once seen running, so a bounce not yet started is
            // not taken for one finished.
            match (live.bounce, live.handle.status().bouncing()) {
                (Some(_), true) => live.bounce = Some(true),
                (Some(true), false) => {
                    live.bounce = None;
                    found.push(Found::Note(TapeMessage::Bounced));
                }
                _ => {}
            }
            while let Some(r) = live.handle.poll() {
                match (r, live.save_to.take()) {
                    (Returned::Snapshot(s), Some(dir)) => {
                        let to = (live.name.clone(), live.extent, live.handle.channels());
                        let (tx, beats) = (self.saved.0.clone(), live.beats);
                        std::thread::spawn(move || {
                            _ = tx.send(save(&dir, to, &s).map(|p| (p, beats)))
                        });
                        found.push(Found::Note(TapeMessage::Saving));
                    }
                    (Returned::Aborted(_), Some(_)) => {
                        found.push(Found::Note(TapeMessage::SaveAborted));
                    }
                    // An old loop is dropped here.
                    (_, dir) => live.save_to = dir,
                }
            }
        }
        while let Ok(r) = self.saved.1.try_recv() {
            found.push(match r {
                Ok((path, beats)) => {
                    Found::WroteLoop(TapeMessage::Saved(path.clone()), path, beats)
                }
                Err(e) => Found::Note(TapeMessage::Failed(e)),
            });
        }
        while let Ok(e) = self.errors.1.try_recv() {
            match e {
                DeviceEvent::Rerouted => {}
                DeviceEvent::Error(e) => found.push(Found::Note(TapeMessage::Failed(e))),
                // The loop played in the stream's callback, and went with it.
                DeviceEvent::Lost(e) => {
                    if let Some(f) = self.live.as_mut().and_then(stop_recording) {
                        found.push(f);
                    }
                    self.live = None;
                    found.push(Found::Note(TapeMessage::DeviceLost(e)));
                }
            }
        }
        // The player's stream lost its device, and the looper with it.
        let lost = |l: &Live| l.attached.as_ref().is_some_and(Attachment::lost);
        if self.live.as_ref().is_some_and(lost) {
            if let Some(f) = self.live.as_mut().and_then(stop_recording) {
                found.push(f);
            }
            self.live = None;
            found.push(Found::Note(TapeMessage::DeviceLost(
                "the output device went away".into(),
            )));
        }
        found
    }

    /// Where a manual deck's device events are reported, by whatever drives
    /// it. A deck on the player's stream learns of a loss from the player.
    pub fn device_events(&self) -> Sender<DeviceEvent> {
        self.errors.0.clone()
    }

    /// Plays `loaded` on a new looper, in place of the old one.
    fn start(
        &mut self,
        name: String,
        span: String,
        beats: Option<u32>,
        source: Option<Source>,
        loaded: Loaded,
    ) -> Result<TapeMessage, String> {
        let extent = Extent {
            frames: loaded.lp.frames(),
            range: loaded.lp.range(),
            rate: loaded.rate,
        };
        let (looper, mut handle) = playr_looper::new(extent.rate);
        // No write loop until Write makes one; voice 1 holds this loop, and
        // the other lanes nothing.
        handle.load(Loop::empty()).map_err(|e| e.to_string())?;
        handle.load_voice(0, loaded.lp).map_err(|e| e.to_string())?;
        for i in 1..playr_looper::WRITER {
            handle.eject(i).map_err(|e| e.to_string())?;
        }
        handle
            .set(Setting::Knee(self.knee))
            .map_err(|e| e.to_string())?;
        handle.set_volume(self.volume).map_err(|e| e.to_string())?;
        // The old looper is detached as its `Live` goes.
        let (attached, looper) = match &self.output {
            Output::Bus(sources) => (
                Some(sources.attach(extent.rate, Box::new(Played(looper)))),
                None,
            ),
            Output::Manual => (None, Some(looper)),
        };
        self.live = Some(Live {
            handle,
            attached,
            looper,
            name: name.clone(),
            span: span.clone(),
            extent: empty(extent.rate),
            own: [Some(empty(extent.rate)); playr_looper::VOICES],
            own_names: Default::default(),
            beats,
            save_to: None,
            sources: Default::default(),
            printed: false,
            bounce: None,
            long: None,
            bounced: None,
            state: TapeState::new(Window::new(0, 0)),
            volume: self.volume,
            interp: None,
        });
        let live = self.live.as_mut().expect("set above");
        live.own[0] = Some(extent);
        live.own_names[0] = Some((name, span));
        live.sources[0] = source;
        // Voice 4 plays the write loop, which there is none of yet.
        live.own[playr_looper::WRITER] = None;
        live.state.voices[playr_looper::WRITER].window = Window::new(0, 0);
        live.state.voices[0].window = extent.range;
        live.refresh();
        Ok(TapeMessage::Loaded {
            frames: extent.range.len(),
            rate: extent.rate,
        })
    }
}

impl Live {
    /// The shape of the loop voice `i` reads.
    fn extent_of(&self, i: usize) -> Extent {
        self.own.get(i).copied().flatten().unwrap_or(self.extent)
    }

    /// Sets which voices read a loop of their own, and which read nothing,
    /// from the loops' shapes.
    fn refresh(&mut self) {
        for i in 0..playr_looper::VOICES {
            self.state.empty[i] = self.extent_of(i).frames == 0;
            self.state.own[i] = self.own[i].is_some_and(|e| e.frames > 0);
        }
        self.resync();
    }

    /// Sets a synced delay's time from voice 1's window as it now plays.
    fn resync(&mut self) {
        let fx = self.state.fx;
        let Some(ms) = fx
            .sync
            .and_then(|d| synced(d, &self.state.voices[0], self.extent_of(0)))
        else {
            return;
        };
        let s = Setting::Fx(BusSetting::Delay(DelayParam::Time, ms / 1000.0));
        // A full ring drops it; the next change sends it again.
        if ms != fx.time && self.handle.set(s).is_ok() {
            self.state.apply(s);
        }
    }

    /// The voices reading the write loop.
    fn readers(&self) -> Vec<usize> {
        (0..playr_looper::VOICES)
            .filter(|j| self.own[*j].is_none())
            .collect()
    }

    /// Empties voice `i`, or the write loop for [`playr_looper::TAPE`]; the
    /// voices reading it go silent too, and writing goes off.
    fn eject(&mut self, i: usize) -> Result<(), playr_looper::Error> {
        self.handle.eject(i)?;
        let followers = match i {
            playr_looper::TAPE => {
                self.extent = empty(self.extent.rate);
                self.sources[playr_looper::WRITER] = None;
                self.printed = false;
                self.state.write_window = Window::new(0, 0);
                self.handle.set(Setting::Write(false))?;
                self.state.apply(Setting::Write(false));
                self.readers()
            }
            _ => {
                self.own[i] = Some(empty(self.extent.rate));
                self.own_names[i] = None;
                self.sources[i] = None;
                vec![i]
            }
        };
        for j in followers {
            self.state.voices[j].window = Window::new(0, 0);
        }
        self.refresh();
        Ok(())
    }

    /// The first of voices 1 to 3, from 0, holding a loop: the shape the
    /// first Write gives the write loop.
    fn first_loop(&self) -> Option<usize> {
        (0..playr_looper::WRITER).find(|&i| self.own[i].is_some_and(|e| e.frames > 0))
    }

    /// Makes the write loop, silent and the shape of the first lane's loop,
    /// named after it for saves; the voices reading it take its range.
    fn blank_tape(&mut self) -> Result<(), TapeMessage> {
        // A tape with voice 4 empty holds a loop in another lane.
        let i = self.first_loop().ok_or(TapeMessage::NoTape)?;
        let shape = self.own[i].expect("found");
        let failed = |e: playr_looper::Error| TapeMessage::Failed(e.to_string());
        self.handle.blank_tape(i).map_err(failed)?;
        self.extent = shape;
        self.sources[playr_looper::WRITER] = None;
        self.printed = false;
        if let Some((name, span)) = self.own_names[i].clone() {
            (self.name, self.span) = (name, span);
        }
        let mut settings = vec![
            Setting::WriteWindow(shape.range),
            Setting::On(playr_looper::WRITER, true),
        ];
        settings.extend(
            self.readers()
                .into_iter()
                .map(|j| Setting::Window(j, shape.range)),
        );
        for s in settings {
            self.handle.set(s).map_err(failed)?;
            self.state.apply(s);
        }
        self.refresh();
        Ok(())
    }

    /// Voice `i` reads the write loop.
    fn share(&mut self, i: usize) -> Result<(), playr_looper::Error> {
        if self.own[i].is_none() {
            return Ok(());
        }
        self.handle.unload(i)?;
        self.own[i] = None;
        self.own_names[i] = None;
        self.sources[i] = None;
        let range = self.extent.range;
        match range.is_empty() {
            true => self.state.voices[i].window = range,
            false => {
                self.handle.set(Setting::Window(i, range))?;
                self.state.apply(Setting::Window(i, range));
            }
        }
        self.refresh();
        Ok(())
    }
}

impl Live {
    /// Gives the loops read for a resume the rest of `saved`: the silent write
    /// loop, the voices reading it, and the settings. Windows kept at another
    /// rate are scaled to the tape's, and cut to their loops.
    fn restore(&mut self, saved: &SavedTape) -> Result<(), playr_looper::Error> {
        let rate = self.handle.sample_rate();
        let scale =
            |f: usize| (f as f64 * f64::from(rate) / f64::from(saved.rate)).round() as usize;
        let fit = |w: Window, frames: usize| {
            let w = Window::new(scale(w.start).min(frames), scale(w.end).min(frames));
            (!w.is_empty()).then_some(w)
        };
        let w = playr_looper::WRITER;
        if let (Reads::Write, Some((frames, range, channels))) = (&saved.reads[w], saved.shape) {
            let frames = scale(frames);
            let range = fit(range, frames).unwrap_or(Window::new(0, frames));
            let lp = Loop::new(vec![0.0; frames * usize::from(channels)], channels)?
                .with_range(range)?;
            self.handle.load_voice(playr_looper::TAPE, lp)?;
            self.extent = Extent {
                frames,
                range,
                rate,
            };
            // Named after the first lane's loop for saves, as Write names it.
            if let Some((name, span)) = self.first_loop().and_then(|i| self.own_names[i].clone()) {
                (self.name, self.span) = (name, span);
            }
            self.printed = true;
        }
        if self.extent.frames > 0 {
            for i in 0..w {
                if saved.reads[i] == Reads::Write {
                    self.share(i)?;
                }
            }
        }
        let mut settings = Vec::new();
        for (i, v) in saved.state.voices.iter().enumerate() {
            let e = self.extent_of(i);
            if e.frames == 0 {
                continue;
            }
            if let Some(window) = fit(v.window, e.frames) {
                settings.push(Setting::Window(i, window));
            }
            settings.extend([
                Setting::Rate(i, v.rate),
                Setting::Slew(i, v.slew),
                Setting::Level(i, v.level),
                Setting::Pan(i, v.pan),
                Setting::Send(i, v.send),
                Setting::VoiceWear(i, v.wear),
                Setting::Fade(i, v.fade),
                Setting::Ping(i, v.ping),
                Setting::Drive(i, v.drive),
                Setting::HighPass(i, v.high_pass),
                Setting::LowPass(i, v.low_pass),
                Setting::BellHz(i, v.bell_hz),
                Setting::BellGain(i, v.bell_gain),
                Setting::BellQ(i, v.bell_q),
                Setting::Solo(i, v.solo),
                Setting::Mute(i, v.mute),
                Setting::DelaySend(i, v.delay),
                Setting::ReverbSend(i, v.reverb),
                Setting::On(i, v.on),
            ]);
        }
        let s = &saved.state;
        if let Some(window) = fit(s.write_window, self.extent.frames) {
            settings.push(Setting::WriteWindow(window));
        }
        settings.extend([
            Setting::Feedback(s.feedback),
            Setting::Wear(s.wear),
            Setting::Thin(s.thin),
        ]);
        settings.extend(s.fx.settings().map(Setting::Fx));
        settings.push(Setting::PrintFx(s.fx.print));
        for (k, (shape, speed)) in s.lfos.iter().enumerate() {
            settings.extend([Setting::LfoShape(k, *shape), Setting::LfoSpeed(k, *speed)]);
        }
        settings.extend(
            s.routes
                .iter()
                .enumerate()
                .map(|(k, r)| Setting::Route(k, *r)),
        );
        self.state.fx.sync = s.fx.sync;
        if s.playing {
            settings.push(Setting::Play);
        }
        for s in settings {
            self.handle.set(s)?;
            self.state.apply(s);
        }
        self.refresh();
        Ok(())
    }
}

/// The shape of an empty voice's loop.
fn empty(rate: u32) -> Extent {
    Extent {
        frames: 0,
        range: Window::new(0, 0),
        rate,
    }
}

impl Deck {
    /// Puts `loaded` in voice `i` of the tape playing: its own loop, or for
    /// voice 4 the write loop. The voices it changes take its range as their
    /// window, and voice `i` is turned on.
    fn load_voice(
        &mut self,
        i: usize,
        loading: Loading,
        loaded: Loaded,
    ) -> Result<TapeMessage, String> {
        let live = self.live.as_mut().expect("checked by the caller");
        let extent = Extent {
            frames: loaded.lp.frames(),
            range: loaded.lp.range(),
            rate: loaded.rate,
        };
        let failed = |e: playr_looper::Error| e.to_string();
        let mut bounced = false;
        // Voice 4's loop is the write loop, which every voice without its own
        // reads, and Save loop saves.
        let changed = match i {
            playr_looper::WRITER => {
                live.handle
                    .load_voice(playr_looper::TAPE, loaded.lp)
                    .map_err(failed)?;
                live.extent = extent;
                // A bounce keeps the tape's name for saves; its file is read,
                // and goes, so the write loop resumes silent, as printed.
                match loading.bounce.is_some() {
                    true => {
                        live.sources[i] = None;
                        live.printed = true;
                        bounced = true;
                    }
                    false => {
                        (live.name, live.span, live.beats) =
                            (loading.name, loading.span, loading.beats);
                        live.sources[i] = Some(loading.source);
                        live.printed = false;
                    }
                }
                live.state.write_window = extent.range;
                live.handle
                    .set(Setting::WriteWindow(extent.range))
                    .map_err(failed)?;
                live.readers()
            }
            _ => {
                live.handle.load_voice(i, loaded.lp).map_err(failed)?;
                if i == 0 {
                    live.beats = loading.beats;
                }
                live.own[i] = Some(extent);
                live.own_names[i] = Some((loading.name, loading.span));
                live.sources[i] = Some(loading.source);
                vec![i]
            }
        };
        let mut settings: Vec<Setting> = changed
            .iter()
            .map(|&j| Setting::Window(j, extent.range))
            .collect();
        settings.push(Setting::On(i, true));
        for s in settings {
            live.handle.set(s).map_err(failed)?;
            live.state.apply(s);
        }
        live.refresh();
        if bounced {
            return Ok(TapeMessage::Bounced);
        }
        Ok(TapeMessage::LoadedVoice {
            voice: i as u8 + 1,
            frames: extent.range.len(),
            rate: extent.rate,
        })
    }
}

/// `src` as a loop at `to`: at most 2 channels, the first two of more,
/// resampled from its rate when the device needs it. Up to [`ROLL`] either
/// side is pre-roll and post-roll: the track's audio there, or for a
/// circular source its own other end.
fn read(src: &Source, to: u32) -> Result<Loaded, String> {
    let (path, rate, (start, end)) = (src.path.as_path(), src.rate, src.span);
    let roll = (ROLL.as_secs_f64() * f64::from(rate)) as u64;
    let (before, mut audio, channels) = match src.circular {
        true => {
            let (audio, channels) = samples::read_frames(path, rate, start, end)?;
            let c = channels.max(1);
            let n = (audio.len() / c) as u64;
            let r = roll.min(n) as usize;
            let mut rolled = Vec::with_capacity(audio.len() + 2 * r * c);
            rolled.extend_from_slice(&audio[audio.len() - r * c..]);
            rolled.extend_from_slice(&audio);
            rolled.extend_from_slice(&audio[..r * c]);
            (r as u64, rolled, channels)
        }
        false => {
            let before = roll.min(start);
            let (audio, channels) = samples::read_frames(path, rate, start - before, end + roll)?;
            (before, audio, channels)
        }
    };
    if channels > 2 {
        audio = audio
            .chunks_exact(channels)
            .flat_map(|f| [f[0], f[1]])
            .collect();
    }
    let channels = channels.min(2) as u16;
    if to != rate {
        let mut r = Resample::new(rate, to, channels, 1.0)
            .ok_or_else(|| format!("cannot resample {rate} Hz to {to} Hz"))?;
        let mut out = Vec::with_capacity(audio.len() * to as usize / rate as usize + 64);
        r.push(&audio, &mut out);
        r.flush(&mut out);
        audio = out;
    }
    // The range, in the loop's frames at `to`; fewer where the track ended.
    let frames = audio.len() / usize::from(channels);
    let at = |f: u64| ((f as f64 * f64::from(to) / f64::from(rate)).round() as usize).min(frames);
    let range = Window::new(at(before), at(before + end - start));
    let lp = Loop::new(audio, channels)
        .and_then(|lp| lp.with_range(range))
        .map_err(|e| e.to_string())?;
    Ok(Loaded { lp, rate: to })
}

/// A new `<name>-tape` directory under `samples`, and a file in it named
/// after it, ending in `what`.
fn new_file(samples: &Path, name: &str, what: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(samples)
        .map_err(|e| format!("cannot create {}: {e}", samples.display()))?;
    let dir = samples::unused_dir(samples, &format!("{name}-tape"))?;
    let stem = dir.file_name().unwrap_or_default().to_string_lossy();
    Ok(dir.join(format!("{stem}-{what}.wav")))
}

/// Writes the range of a snapshot of the loop under `samples`, without its
/// pre-roll and post-roll: exactly the loop's length.
fn save(
    samples: &Path,
    (name, extent, channels): (String, Extent, Option<u16>),
    audio: &[f32],
) -> Result<PathBuf, String> {
    let path = new_file(samples, &name, "loop")?;
    let c = usize::from(channels.unwrap_or(1));
    let range = &audio[extent.range.start * c..extent.range.end * c];
    playr_looper::write_wav(&path, range, c as u16, extent.rate)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Stops `live`'s recording, if it has one, and reports it.
fn stop_recording(live: &mut Live) -> Option<Found> {
    if !live.handle.recording() {
        return None;
    }
    // A bounce until stopped goes with the tape it would replace a loop of.
    if let Some((path, _)) = live.long.take() {
        _ = live.handle.stop_recording();
        _ = std::fs::remove_file(path);
        return None;
    }
    Some(match live.handle.stop_recording() {
        Ok(r) => Found::Wrote(
            TapeMessage::Recorded {
                path: r.path.clone(),
                frames: r.frames,
                rate: live.extent.rate,
                dropped: r.dropped,
            },
            r.path,
        ),
        Err(e) => Found::Note(TapeMessage::Failed(e.to_string())),
    })
}
