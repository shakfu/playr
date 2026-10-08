//! The tape looper in playr: `:tape` actions, loading the sampler's range,
//! and saving the loop and the mix as samples. The engine is `playr-looper`;
//! its design is in `docs/dev/looper-engine.md`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::time::Duration;

use playr_core::audio::bus::Handover;
use playr_core::audio::output::DeviceEvent;
use playr_core::audio::resample::Resample;
use playr_core::audio::{Attachment, Sources, State};
use playr_core::samples;
pub use playr_looper::Filter;
use playr_looper::{Handle, Loop, Looper, Returned, Setting, Window};

use crate::dispatch::Frontend;
use crate::message::Message;

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
    /// The filter's cutoff, 0 to 1.
    Cutoff(f32),
    Filter(Filter),
    /// Hear only the soloed voices; what they send is unchanged.
    Solo(bool),
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
}

/// What the tape reports.
#[derive(Debug, Clone, PartialEq)]
pub enum TapeMessage {
    Loading,
    Loaded {
        frames: usize,
        rate: u32,
    },
    /// The action took effect.
    Done(TapeAction),
    /// A `:tape` action other than load before anything was loaded.
    NoTape,
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
}

/// The tape playing, or ready to.
struct Live {
    handle: Handle,
    /// Where the looper plays on the player's stream; `None` for a manual deck.
    attached: Option<Attachment>,
    looper: Option<Looper>,
    /// The source track's name, for the directories saves go in.
    name: String,
    extent: Extent,
    /// Where the snapshot asked for is to be saved.
    save_to: Option<PathBuf>,
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
    pub cutoff: f32,
    pub filter: Filter,
    pub solo: bool,
}

/// The tape's settings as last sent. The looper's own are not readable
/// from outside the audio thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TapeState {
    pub voices: [VoiceState; playr_looper::VOICES],
    pub playing: bool,
    pub write: bool,
    pub write_window: Window,
    pub feedback: f32,
    pub wear: f32,
    pub thin: f32,
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
            cutoff: 1.0,
            filter: Filter::Low,
            solo: false,
        };
        TapeState {
            voices: [voice(true), voice(false), voice(false)],
            playing: false,
            write: false,
            write_window: whole,
            feedback: 1.0,
            wear: 0.0,
            thin: 0.0,
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
            Setting::Cutoff(i, x) => self.voices[i].cutoff = x,
            Setting::Filter(i, f) => self.voices[i].filter = f,
            Setting::Solo(i, on) => self.voices[i].solo = on,
            Setting::Thin(x) => self.thin = x,
            // From the settings file, a take and the mixer: not the tape's controls.
            Setting::Knee(_) | Setting::Head(..) | Setting::Interp(_) => {}
        }
    }
}

/// What [`Deck::poll`] found.
enum Found {
    Note(TapeMessage),
    /// A file written, to add to the library.
    Wrote(TapeMessage, PathBuf),
}

/// A save's result: the file written, or why not.
type Saved = Result<PathBuf, String>;

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

    /// Whether a load is being read.
    pub fn loading(&self) -> bool {
        self.loading.is_some()
    }

    /// The loaded loop's shape.
    pub fn loaded(&self) -> Option<Extent> {
        self.live.as_ref().map(|l| l.extent)
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

    pub fn saving(&self) -> bool {
        self.live.as_ref().is_some_and(|l| l.save_to.is_some())
    }

    /// Runs a manual deck's looper for `out`, interleaved stereo.
    pub fn process(&mut self, out: &mut [f32]) {
        if let Some(looper) = self.live.as_mut().and_then(|l| l.looper.as_mut()) {
            looper.process(out);
        }
    }

    /// Starts reading `start..end` of `path`, which counts frames at `rate`.
    fn load(&mut self, path: PathBuf, rate: u32, (start, end): (u64, u64)) -> Result<(), String> {
        let to = match &self.output {
            Output::Bus(sources) => sources.rate_for(rate),
            Output::Manual => rate,
        };
        let (tx, rx) = channel();
        let name = samples::name_for(&path);
        std::thread::spawn(move || _ = tx.send(read(&path, rate, (start, end), to)));
        self.loading = Some(Loading { result: rx, name });
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
            // A recording of the old tape is finished before it goes.
            if let Some(f) = self.live.as_mut().and_then(stop_recording) {
                found.push(f);
            }
            let started = result.and_then(|l| self.start(loading, l));
            match (&started, &mut self.taking) {
                (Ok(_), Some(t)) => t.loaded = true,
                (Err(_), _) => self.taking = None,
                _ => {}
            }
            found.push(Found::Note(started.unwrap_or_else(TapeMessage::Failed)));
        }
        if let Some(live) = &mut self.live {
            while let Some(r) = live.handle.poll() {
                match (r, live.save_to.take()) {
                    (Returned::Snapshot(s), Some(dir)) => {
                        let to = (live.name.clone(), live.extent, live.handle.channels());
                        let tx = self.saved.0.clone();
                        std::thread::spawn(move || _ = tx.send(save(&dir, to, &s)));
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
                Ok(path) => Found::Wrote(TapeMessage::Saved(path.clone()), path),
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
    fn start(&mut self, loading: Loading, loaded: Loaded) -> Result<TapeMessage, String> {
        let extent = Extent {
            frames: loaded.lp.frames(),
            range: loaded.lp.range(),
            rate: loaded.rate,
        };
        let (looper, mut handle) = playr_looper::new(extent.rate);
        handle.load(loaded.lp).map_err(|e| e.to_string())?;
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
            name: loading.name,
            extent,
            save_to: None,
            state: TapeState::new(extent.range),
            volume: self.volume,
            interp: None,
        });
        Ok(TapeMessage::Loaded {
            frames: extent.range.len(),
            rate: extent.rate,
        })
    }
}

/// Frames `start..end` of `path` as a loop at `to`, with up to [`ROLL`] of
/// the track either side as pre-roll and post-roll: at most 2 channels, the first two of
/// more, resampled from `rate` when the device needs it.
fn read(path: &Path, rate: u32, (start, end): (u64, u64), to: u32) -> Result<Loaded, String> {
    let roll = (ROLL.as_secs_f64() * f64::from(rate)) as u64;
    let before = roll.min(start);
    let (mut audio, channels) = samples::read_frames(path, rate, start - before, end + roll)?;
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

/// Takes in what the deck finished since the last frame: loads, saves,
/// recordings and device errors. Files written are added to the library.
pub fn poll(f: &mut impl Frontend) {
    for found in f.tape().poll() {
        report(f, found);
    }
    take_over(f);
    let volume = f.mix().gain(crate::mix::Strip::Tape);
    f.tape().follow_volume(volume);
    let interp = f.mix().interp();
    f.tape().follow_interp(interp);
}

fn report(f: &mut impl Frontend, found: Found) {
    match found {
        Found::Note(m) => f.notify(m.into()),
        Found::Wrote(m, path) => match f.session_mut().add_to_library(&path) {
            Ok(()) => f.notify(m.into()),
            Err(e) => f.notify(TapeMessage::Failed(e).into()),
        },
    }
}

/// Does `action`.
pub fn act(f: &mut impl Frontend, action: TapeAction) {
    match action {
        TapeAction::Load(slot) => return load(f, slot),
        TapeAction::Take => return take(f),
        TapeAction::Record => return record(f),
        _ => {}
    }
    let samples = f.session().samples_dir().to_path_buf();
    let playing = f.session().player().status().state == State::Playing;
    let sent = match f.tape().live.as_mut() {
        Some(live) => send(live, action, samples),
        None => Err(TapeMessage::NoTape),
    };
    if let Err(m) = sent {
        return f.notify(m.into());
    }
    // The tape plays alone; the player stays paused until asked.
    if action == TapeAction::Play && playing {
        f.session().send(playr_core::audio::Cmd::TogglePause);
    }
    if action != TapeAction::Save {
        f.notify(TapeMessage::Done(action).into());
    }
}

/// Sends `action` to the looper.
fn send(live: &mut Live, action: TapeAction, samples: PathBuf) -> Result<(), TapeMessage> {
    let failed = |e: playr_looper::Error| TapeMessage::Failed(e.to_string());
    match action {
        TapeAction::Reset => live.handle.reset().map_err(failed),
        TapeAction::Save if live.save_to.is_some() => Err(TapeMessage::AlreadySaving),
        TapeAction::Save => {
            live.handle.snapshot().map_err(failed)?;
            live.save_to = Some(samples);
            Ok(())
        }
        _ => settings(action, live.extent)?
            .into_iter()
            .try_for_each(|s| {
                live.handle.set(s)?;
                live.state.apply(s);
                Ok(())
            })
            .map_err(failed),
    }
}

fn load(f: &mut impl Frontend, slot: Option<u8>) {
    if let Some((path, rate, span)) = range(f, slot) {
        drop_take(f);
        start_load(f, path, rate, span);
    }
}

/// Loads the range as `load` does, to take over from the player once in.
fn take(f: &mut impl Frontend) {
    if let Some((path, rate, span)) = range(f, None) {
        drop_take(f);
        f.tape().taking = Some(Taking {
            path: path.clone(),
            rate,
            span,
            loaded: false,
            waiting: false,
            armed: false,
        });
        start_load(f, path, rate, span);
    }
}

/// Drops a take under way, and the handover it armed.
fn drop_take(f: &mut impl Frontend) {
    if f.tape().taking.take().is_some_and(|t| t.armed) {
        f.session().player().cancel_handover();
    }
}

/// The playing track, its rate, and the span `slot` or the sampler's range
/// marks in it; or says why there is none.
fn range(f: &mut impl Frontend, slot: Option<u8>) -> Option<(PathBuf, u32, (u64, u64))> {
    let (path, rate) = match f.session().playing_track() {
        Ok(track) => track,
        Err(refusal) => {
            f.notify(refusal.into());
            return None;
        }
    };
    let span = match slot {
        Some(n) => f.session_mut().loops_for(Some(&path))[usize::from(n) - 1],
        None => f.sampler().range(Some(&path)),
    };
    match (span, slot) {
        (Some(span), _) => Some((path, rate, span)),
        (None, Some(n)) => {
            f.notify(TapeMessage::EmptySlot(n).into());
            None
        }
        (None, None) => {
            f.notify(TapeMessage::NoRange.into());
            None
        }
    }
}

fn start_load(f: &mut impl Frontend, path: PathBuf, rate: u32, span: (u64, u64)) {
    match f.tape().load(path, rate, span) {
        Ok(()) => f.notify(TapeMessage::Loading.into()),
        Err(e) => {
            f.tape().taking = None;
            f.notify(TapeMessage::Failed(e).into());
        }
    }
}

/// Hands the player over to the tape once a take's loop is in: voice 1 at
/// the player's position inside the range, or at its start once the player
/// reaches it. On the player's stream the handover is on one frame, which
/// the player's callback keeps; a paused player leaves the tape cued there.
fn take_over(f: &mut impl Frontend) {
    let Some(t) = f.tape().taking.clone().filter(|t| t.loaded) else {
        return;
    };
    let player = f.session().player();
    let state = player.status().state;
    let on_track = f.session().playing_track().is_ok_and(|(p, _)| p == t.path);
    let gone = !on_track || state == State::Stopped;
    if t.armed {
        match f.session().player().handover() {
            Handover::Done => return took(f),
            Handover::Armed if !gone => return,
            _ => {}
        }
        f.session().player().cancel_handover();
        f.tape().taking.as_mut().expect("taken above").armed = false;
    }
    if gone {
        f.tape().taking = None;
        return f.notify(TapeMessage::TakeDropped.into());
    }
    let player = f.session().player();
    let status = player.status();
    let rate = f64::from(t.rate);
    let now = player.position().as_secs_f64() * rate;
    let (start, end) = (t.span.0 as f64, t.span.1 as f64);
    let bus = f
        .tape()
        .live
        .as_ref()
        .and_then(|l| l.attached.as_ref().map(Attachment::id));
    // The frame the tape starts on: now, or ahead by the lead on the bus.
    let lead = match (bus, state) {
        (Some(_), State::Playing) => TAKE_LEAD * playr_core::audio::speed_at(status.cents) * rate,
        _ => 0.0,
    };
    let from = start.max(now + lead);
    if from >= end {
        f.tape().taking = None;
        return f.notify(TapeMessage::PastRange.into());
    }
    if now < start && !t.waiting {
        f.tape().taking.as_mut().expect("taken above").waiting = true;
        f.notify(TapeMessage::TakeWaiting.into());
    }
    if now < start && (bus.is_none() || state != State::Playing) {
        return;
    }
    let Some(live) = f.tape().live.as_mut() else {
        return;
    };
    let e = live.extent;
    let head = e.range.start as f64 + (from - start) * f64::from(e.rate) / rate;
    if let Err(err) = live.handle.set(Setting::Head(0, head)) {
        f.tape().taking = None;
        return f.notify(TapeMessage::Failed(err.to_string()).into());
    }
    match (bus, state) {
        (Some(id), State::Playing) => {
            // Refused while a seek settles or before a track change: tried
            // again next frame.
            let at = Duration::from_secs_f64(from / rate);
            if f.session().player().hand_over(id, at) {
                f.tape().taking.as_mut().expect("taken above").armed = true;
            }
        }
        (None, State::Playing) => {
            // Play pauses the player, as the tape's Play does.
            f.tape().taking = None;
            act(f, TapeAction::Play);
            f.notify(TapeMessage::Took.into());
        }
        _ => {
            f.tape().taking = None;
            f.notify(TapeMessage::Took.into());
        }
    }
}

/// Finishes a take the player's callback handed over: the tape plays and
/// the player paused, on one frame.
fn took(f: &mut impl Frontend) {
    f.tape().taking = None;
    if let Some(live) = f.tape().live.as_mut() {
        live.state.apply(Setting::Play);
    }
    f.notify(TapeMessage::Took.into());
}

fn record(f: &mut impl Frontend) {
    let samples = f.session().samples_dir().to_path_buf();
    let Some(live) = f.tape().live.as_mut() else {
        return f.notify(TapeMessage::NoTape.into());
    };
    if let Some(found) = stop_recording(live) {
        return report(f, found);
    }
    let started =
        new_file(&samples, &live.name, "mix").and_then(|path| match live.handle.record(&path) {
            Ok(()) => Ok(path),
            Err(e) => {
                _ = std::fs::remove_dir(path.parent().expect("in its directory"));
                Err(e.to_string())
            }
        });
    match started {
        Ok(path) => f.notify(TapeMessage::Recording(path).into()),
        Err(e) => f.notify(TapeMessage::Failed(e).into()),
    }
}

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
    Cutoff(usize),
    Filter(usize),
    Solo(usize),
    Write,
    WriteWindow,
    Feedback,
    WriteWear,
    Thin,
}

/// Why a control does nothing as the tape is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Idle {
    VoiceOff,
    WriteOff,
    NoSend,
    Silent,
    Still,
    /// No pre-roll or post-roll for a crossfade to read.
    NoRoom,
    /// Writing on, with nothing that changes the loop: feedback 1, wear 0,
    /// thin 0 and every voice playing sending nothing.
    Unchanging,
    /// Ping turns the head at the window's edges, so it never wraps.
    Turns,
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
        | Control::Cutoff(i)
        | Control::Filter(i) => off(i),
        Control::Ping(i) => off(i).or((voice(i).rate == 0.0).then_some(Idle::Still)),
        // A voice off still silences the others when soloed.
        Control::Solo(_) => None,
        Control::Level(i) => off(i),
        Control::Pan(i) => off(i).or((voice(i).level == 0.0).then_some(Idle::Silent)),
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
    }
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
                VoiceSetting::Cutoff(v) => Setting::Cutoff(i, v),
                VoiceSetting::Filter(f) => Setting::Filter(i, f),
                VoiceSetting::Solo(on) => Setting::Solo(i, on),
            }
        }
        TapeAction::Load(_)
        | TapeAction::Take
        | TapeAction::Reset
        | TapeAction::Save
        | TapeAction::Record => return Ok(Vec::new()),
    }])
}
