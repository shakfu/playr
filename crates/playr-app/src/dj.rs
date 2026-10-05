//! Two DJ decks in playr: `:dj` actions, loading library tracks onto the
//! decks, and editing their beat grids. The engine is `playr-dj`; its design
//! is in `docs/dev/dj-engine.md`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;

use playr_core::analysis::tempo;
use playr_core::analysis::TempoFix;
use playr_core::audio::output::DeviceEvent;
use playr_core::audio::resample::Resample;
use playr_core::audio::State;
use playr_core::db::Track;
use playr_core::event::JobId;
use playr_core::notice::Notice;
use playr_core::samples;
use playr_core::wave::Peaks;
pub use playr_dj::{Band, CueOut, Curve, Nudge, Range, Side, EQ_DB, HOT_CUES, LOOP_BEATS};
use playr_dj::{Engine, Handle, Returned, Setting};

use crate::dispatch::Frontend;
use crate::message::Message;

/// A change to a deck's beat grid. Each is stored with the track, so it
/// holds across loads and analyses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridEdit {
    /// Twice the tempo, as `:bpm x2` sets it.
    Double,
    Halve,
    /// The grid's beat one beat earlier, to set the downbeat.
    Earlier,
    Later,
    /// The grid moved this many ms later; negative is earlier.
    Offset(f32),
    /// A tap at the deck's position: two or more set the tempo, the last
    /// sets a beat.
    Tap,
    /// Back to the analysis's grid at the analysed tempo.
    Reset,
}

/// What `:dj` does. Values are checked where they are parsed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DjAction {
    /// Load the track under the cursor.
    Load(Side),
    /// Take over the track the player is playing: load it, then play it
    /// from where the player is, and pause the player.
    Take(Side),
    /// Play, pausing the player; during a held cue, keep playing.
    Play(Side),
    Pause(Side),
    /// CUE pressed and released at once.
    Cue(Side),
    /// CUE pressed (`true`) or released, as a held button sends it.
    CueHold(Side, bool),
    Sync(Side, bool),
    /// The rate fader, in percent.
    Rate(Side, f32),
    Range(Side, Range),
    Nudge(Side, Nudge),
    /// The trim, in dB.
    Gain(Side, f32),
    /// The channel fader, 0 to 1.
    Level(Side, f32),
    /// The crossfader, 0 for deck A to 1 for deck B.
    Xfade(f32),
    /// The crossfader glides to deck A's end, deck B's, or with `None` the
    /// centre.
    XfadeTo(Option<Side>),
    Quantize(bool),
    /// The deck heard on the cue side of the output, or none.
    CueBus(Option<Side>),
    Grid(Side, GridEdit),
    /// An EQ band's gain, in dB.
    Eq(Side, Band, f32),
    Kill(Side, Band, bool),
    /// The filter knob: -1 low-pass, 0 open, 1 high-pass.
    Filter(Side, f32),
    /// Hot cue `n`, from 1: set when empty, else jump there and play.
    HotCue(Side, u8),
    HotClear(Side, u8),
    /// Jump this many beats.
    Jump(Side, f32),
    /// Loop this many beats, or stop looping.
    Loop(Side, Option<f32>),
    CueOut(CueOut),
    Curve(Curve),
    /// Move the head to a time, or a percentage of the track.
    Seek(Side, crate::tape::Pos),
    /// Silence the deck in the main mix; the cue still hears it.
    Mute(Side, bool),
    /// Forget the track waiting to load onto a playing deck.
    Unqueue(Side),
    /// Jump to the next mark (`true`) or the previous one.
    Mark(Side, bool),
    /// On: a track picked for a playing deck waits until it stops. Off: it
    /// replaces the playing track, which fades out, and plays.
    Strict(bool),
}

/// What the decks report.
#[derive(Debug, Clone, PartialEq)]
pub enum DjMessage {
    Loading(Side),
    Loaded {
        side: Side,
        title: String,
        bpm: Option<f64>,
    },
    /// The track loaded has no current analysis; one runs.
    FindingGrid(Side),
    /// The track has no beat grid: analysis found no clear pulse.
    NoGrid(Side),
    /// The action took effect.
    Done(DjAction),
    /// An action on a deck with nothing loaded.
    Empty(Side),
    /// `load` with no track under the cursor.
    NoTrack,
    /// A load onto a deck that is playing.
    Playing(Side),
    /// Sync to a tempo no rate range reaches.
    OutOfReach(Side),
    /// A take while the player's speed, in semitones, is beyond any range.
    SpeedOutOfReach(i32),
    /// The deck took over from the player.
    Took(Side),
    /// A tap that started a new count; one more sets the tempo.
    Tapped(Side),
    /// The cue on channels 3 and 4 with a device of this many channels.
    NoCueChannels(u16),
    /// No mark that way from the head.
    NoMark(Side),
    /// A track picked for a playing deck; it loads once the deck stops.
    Queued {
        side: Side,
        title: String,
    },
    /// A grid edit took effect; the grid now.
    Grid {
        side: Side,
        bpm: f64,
        t0: f64,
    },
    /// The output device went away, and the tracks with it.
    DeviceLost(String),
    Failed(String),
}

impl From<DjMessage> for Message {
    fn from(m: DjMessage) -> Self {
        Message::Dj(m)
    }
}

/// Where the decks play.
enum Output {
    /// The device with this ID, or the default.
    Device(Option<String>),
    /// Nowhere: [`Decks::process`] runs the engine, for tests.
    Manual,
}

/// The engine playing, or ready to.
struct Live {
    handle: Handle,
    _stream: Option<cpal::Stream>,
    engine: Option<Engine>,
    rate: u32,
    /// The stream's channels; manual decks count 2.
    channels: u16,
}

/// A track read and ready to load.
struct Read {
    samples: Vec<f32>,
    channels: u16,
    peaks: Peaks,
}

/// A load in progress.
struct Loading {
    path: PathBuf,
    title: String,
    result: Receiver<Result<Read, String>>,
}

/// A queued track's load, kept until the engine takes it, so a deck that
/// starts again first leaves it queued.
type Sent = [Option<Track>; 2];

/// A track on a deck.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub path: PathBuf,
    pub title: String,
    pub frames: usize,
    pub peaks: Arc<Peaks>,
    pub grid: Option<tempo::Grid>,
    /// The hot cues, in frames, as last stored.
    pub hot: [Option<f64>; HOT_CUES],
    /// The track's marks, as the sampler set them, in frames, earliest first.
    pub marks: Vec<f64>,
}

/// The mixer's settings as last sent. Rate, range, sync and the transport
/// are read from [`playr_dj::Status`], since sync moves them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DjState {
    pub gain: [f32; 2],
    pub level: [f32; 2],
    pub xfade: f32,
    pub quantize: bool,
    pub cue_bus: Option<Side>,
    /// Each deck's EQ bands, low to high, in dB, and whether each is killed.
    pub eq: [[f32; 3]; 2],
    pub kill: [[bool; 3]; 2],
    pub filter: [f32; 2],
    pub cue_out: CueOut,
    pub curve: Curve,
    pub mute: [bool; 2],
    pub strict: bool,
}

impl Default for DjState {
    fn default() -> Self {
        DjState {
            gain: [0.0; 2],
            level: [1.0; 2],
            xfade: 0.5,
            quantize: false,
            cue_bus: None,
            eq: [[0.0; 3]; 2],
            kill: [[false; 3]; 2],
            filter: [0.0; 2],
            cue_out: CueOut::default(),
            curve: Curve::default(),
            mute: [false; 2],
            strict: false,
        }
    }
}

/// Taps longer apart than this, in track seconds, start a new count.
const TAP_GAP: f64 = 2.0;

/// The decks' state on the interface's side.
pub struct Decks {
    /// The master's soft clip knee.
    knee: f32,
    output: Output,
    live: Option<Live>,
    loaded: [Option<Loaded>; 2],
    loading: [Option<Loading>; 2],
    /// Tracks sent to the engine, until it takes or refuses them.
    arriving: [Option<Loaded>; 2],
    /// Analyses started for a loaded track with none, by job.
    analysing: Vec<(JobId, Side, PathBuf)>,
    /// Each deck's taps since the last gap, as track positions in seconds.
    taps: [Vec<f64>; 2],
    /// Tracks each deck's engine has taken, to match against its status.
    loads: [u64; 2],
    /// The track picked for each deck while it played, to load once it stops.
    next: [Option<Track>; 2],
    sent: Sent,
    /// Plays the next track once it loads: it replaced one playing.
    autoplay: [bool; 2],
    /// Takes over from the player once the track loads.
    handover: [bool; 2],
    /// The master volume last sent: the player's.
    volume: f32,
    state: DjState,
    errors: (Sender<DeviceEvent>, Receiver<DeviceEvent>),
}

fn i(side: Side) -> usize {
    match side {
        Side::A => 0,
        Side::B => 1,
    }
}

impl Decks {
    /// Decks playing to the output device `device`, or the default, with the
    /// master's soft clip at `knee`.
    pub fn new(device: Option<String>, knee: f32) -> Self {
        Decks::with(Output::Device(device), knee)
    }

    /// Decks with no device, which play only as [`Decks::process`] is called.
    pub fn manual() -> Self {
        Decks::with(Output::Manual, playr_dj::DEFAULT_KNEE)
    }

    fn with(output: Output, knee: f32) -> Self {
        Decks {
            knee,
            output,
            live: None,
            loaded: [None, None],
            loading: [None, None],
            arriving: [None, None],
            analysing: Vec::new(),
            taps: [Vec::new(), Vec::new()],
            loads: [0; 2],
            next: [None, None],
            sent: [None, None],
            autoplay: [false; 2],
            handover: [false; 2],
            volume: f32::NAN,
            state: DjState::default(),
            errors: channel(),
        }
    }

    /// The track on `side`.
    pub fn loaded(&self, side: Side) -> Option<&Loaded> {
        self.loaded[i(side)].as_ref()
    }

    pub fn loading(&self, side: Side) -> bool {
        self.loading[i(side)].is_some()
    }

    pub fn state(&self) -> &DjState {
        &self.state
    }

    /// The track waiting to load onto `side` once it stops.
    pub fn next(&self, side: Side) -> Option<&Track> {
        self.next[i(side)].as_ref()
    }

    /// What the engine publishes: each deck's transport, rate and phase.
    pub fn status(&self) -> Option<&playr_dj::Status> {
        self.live.as_ref().map(|l| &**l.handle.status())
    }

    /// The engine's rate, which every deck's frames count at.
    pub fn rate(&self) -> Option<u32> {
        self.live.as_ref().map(|l| l.rate)
    }

    /// Whether the crossfader is still moving to where it was last set.
    pub fn gliding(&self) -> bool {
        self.status()
            .is_some_and(|s| (s.xfade() - f64::from(self.state.xfade)).abs() > 1e-6)
    }

    pub fn playing(&self) -> bool {
        self.status()
            .is_some_and(|s| s.deck(Side::A).playing() || s.deck(Side::B).playing())
    }

    /// Runs manual decks' engine for `out`, interleaved stereo.
    pub fn process(&mut self, out: &mut [f32]) {
        if let Some(engine) = self.live.as_mut().and_then(|l| l.engine.as_mut()) {
            engine.process(out);
        }
    }

    /// The engine, opened at the first load: at `rate` where the device
    /// plays it, else at the device's.
    fn live(&mut self, rate: u32) -> Result<&mut Live, String> {
        if self.live.is_none() {
            let (rate, device) = match &self.output {
                Output::Device(want) => {
                    let device = playr_core::audio::output::device(want.as_deref())
                        .map_err(|e| e.to_string())?;
                    let to = playr_dj::device::rate(&device, rate).map_err(|e| e.to_string())?;
                    (to, Some(device))
                }
                Output::Manual => (rate, None),
            };
            let (engine, mut handle) = playr_dj::new(rate);
            handle
                .set(Setting::Knee(self.knee))
                .map_err(|e| e.to_string())?;
            let stream = match device {
                Some(device) => {
                    let errors = self.errors.0.clone();
                    let on_error = move |e: cpal::Error| _ = errors.send(e.into());
                    let (stream, channels) = playr_dj::device::open(&device, engine, on_error)
                        .map_err(|e| e.to_string())?;
                    (Some(stream), None, channels)
                }
                None => (None, Some(engine), 2),
            };
            let (stream, engine, channels) = stream;
            self.live = Some(Live {
                handle,
                _stream: stream,
                engine,
                rate,
                channels,
            });
        }
        Ok(self.live.as_mut().expect("opened above"))
    }

    /// Starts reading `track` for `side`.
    fn load(&mut self, side: Side, track: &Track) -> Result<(), String> {
        let path = PathBuf::from(&track.path);
        let from = match track.sample_rate {
            Some(rate) => rate,
            None => native_rate(&path)?,
        };
        let to = self.live(from)?.rate;
        let (tx, rx) = channel();
        let read_path = path.clone();
        std::thread::spawn(move || _ = tx.send(read(&read_path, from, to)));
        self.loading[i(side)] = Some(Loading {
            path,
            title: track.display_title(),
            result: rx,
        });
        Ok(())
    }

    /// Sends the player's volume as the master, when it changed. Once the
    /// engine is open; a new engine starts at NaN, so it gets it too.
    fn follow_volume(&mut self, volume: f32) {
        if volume == self.volume {
            return;
        }
        if let Some(live) = &mut self.live {
            if live.handle.set(Setting::Volume(f64::from(volume))).is_ok() {
                self.volume = volume;
            }
        }
    }

    /// Where the stream reports device events. Manual decks have no stream;
    /// whatever drives them reports here.
    pub fn device_events(&self) -> Sender<DeviceEvent> {
        self.errors.0.clone()
    }

    /// Starts afresh after the device went: the next load opens a new
    /// engine. Analyses under way still store their grids.
    fn lose_device(&mut self) {
        let output = std::mem::replace(&mut self.output, Output::Manual);
        let analysing = std::mem::take(&mut self.analysing);
        *self = Decks::with(output, self.knee);
        self.analysing = analysing;
    }

    fn set(&mut self, s: Setting) -> Result<(), String> {
        let live = self.live.as_mut().ok_or("no deck is loaded")?;
        live.handle.set(s).map_err(|e| e.to_string())
    }
}

/// The rate of the track at `path`, for one played from outside the
/// library, whose row does not record it. Decodes one chunk.
fn native_rate(path: &Path) -> Result<u32, String> {
    let fail = |e: playr_core::audio::decode::DecodeError| format!("{}: {e}", path.display());
    let mut stream = playr_core::audio::decode::AudioStream::open(path).map_err(fail)?;
    stream.next_chunk().map_err(fail)?;
    Ok(stream.spec().rate)
}

/// The whole track at `path`, at most 2 channels, resampled from `from` to
/// `to`, with its peaks.
fn read(path: &Path, from: u32, to: u32) -> Result<Read, String> {
    let (mut audio, channels) = samples::read_frames(path, from, 0, u64::MAX)?;
    if channels > 2 {
        audio = audio
            .chunks_exact(channels)
            .flat_map(|f| [f[0], f[1]])
            .collect();
    }
    let channels = channels.min(2) as u16;
    if to != from {
        let mut r = Resample::new(from, to, channels, 1.0)
            .ok_or_else(|| format!("cannot resample {from} Hz to {to} Hz"))?;
        let mut out = Vec::with_capacity(audio.len() * to as usize / from as usize + 64);
        r.push(&audio, &mut out);
        r.flush(&mut out);
        audio = out;
    }
    let peaks = Peaks::from_interleaved(&audio, channels.into(), to);
    Ok(Read {
        samples: audio,
        channels,
        peaks,
    })
}

fn dj_grid(g: tempo::Grid) -> Option<playr_dj::Grid> {
    playr_dj::Grid::new(g.bpm, g.t0).ok()
}

/// Takes in what finished since the last frame: reads, and the stream's
/// errors.
pub fn poll(f: &mut impl Frontend) {
    for side in [Side::A, Side::B] {
        let done = match f.dj().loading[i(side)]
            .as_ref()
            .map(|l| l.result.try_recv())
        {
            Some(Ok(result)) => Some(result),
            Some(Err(TryRecvError::Disconnected)) => Some(Err("the read stopped".into())),
            Some(Err(TryRecvError::Empty)) | None => None,
        };
        if let Some(result) = done {
            let loading = f.dj().loading[i(side)].take().expect("polled above");
            if let Err(e) = result.and_then(|r| start(f, side, loading, r)) {
                f.notify(DjMessage::Failed(e).into());
            }
        }
    }
    // Old tracks, and tracks a playing deck refused, are dropped here.
    while let Some(r) = f.dj().live.as_mut().and_then(|l| l.handle.poll()) {
        let message = match r {
            Returned::Replaced(side, _) => taken(f, side),
            Returned::Refused(side, _) => {
                let d = f.dj();
                d.arriving[i(side)] = None;
                match d.sent[i(side)].take() {
                    // Started again before the queued track arrived: it waits on.
                    Some(t) => {
                        d.next[i(side)] = Some(t);
                        None
                    }
                    None => Some(DjMessage::Playing(side)),
                }
            }
        };
        if let Some(m) = message {
            f.notify(m.into());
        }
    }
    while let Ok(e) = f.dj().errors.1.try_recv() {
        match e {
            DeviceEvent::Rerouted => {}
            DeviceEvent::Error(e) => f.notify(DjMessage::Failed(e).into()),
            // The tracks played in the stream's callback, and went with it.
            DeviceEvent::Lost(e) => {
                f.dj().lose_device();
                f.notify(DjMessage::DeviceLost(e).into());
            }
        }
    }
    let volume = f.session().player().volume();
    f.dj().follow_volume(volume);
    for side in [Side::A, Side::B] {
        if let Err(e) = keep_hot_cues(f, side) {
            f.notify(DjMessage::Failed(e).into());
        }
        load_next(f, side);
    }
}

/// Loads the track queued for `side` once the deck has stopped and nothing
/// else is on its way to it.
fn load_next(f: &mut impl Frontend, side: Side) {
    let d = f.dj();
    let k = i(side);
    let stopped = d.status().is_some_and(|s| {
        let s = s.deck(side);
        !s.playing() && s.loads() == d.loads[k]
    });
    let busy = d.loading[k].is_some() || d.arriving[k].is_some() || d.sent[k].is_some();
    if !stopped || busy {
        return;
    }
    let Some(track) = d.next[k].take() else {
        return;
    };
    match d.load(side, &track) {
        Ok(()) => {
            d.sent[k] = Some(track);
            f.notify(DjMessage::Loading(side).into());
        }
        Err(e) => f.notify(DjMessage::Failed(e).into()),
    }
}

/// Stores the hot cues the engine has set or cleared on `side` since the
/// last call, once its status is of the track the deck holds.
fn keep_hot_cues(f: &mut impl Frontend, side: Side) -> Result<(), String> {
    let d = f.dj();
    let (Some(status), Some(loaded), Some(rate)) = (d.status(), d.loaded(side), d.rate()) else {
        return Ok(());
    };
    let s = status.deck(side);
    if s.loads() != d.loads[i(side)] || s.hot_cues() == loaded.hot {
        return Ok(());
    }
    let (now, was, path) = (s.hot_cues(), loaded.hot, loaded.path.clone());
    for (slot, n) in now.iter().enumerate().filter(|(k, n)| **n != was[*k]) {
        let at = n.map(|frames| frames / f64::from(rate));
        f.session_mut().set_hot_cue(&path, slot as u8 + 1, at)?;
    }
    if let Some(l) = f.dj().loaded[i(side)].as_mut() {
        l.hot = now;
    }
    Ok(())
}

/// Sends a read track to the engine with its grid. The deck holds it once
/// the engine takes it, in [`taken`].
fn start(f: &mut impl Frontend, side: Side, loading: Loading, read: Read) -> Result<(), String> {
    let grid = f.session().grid(&loading.path);
    let rate = f64::from(f.dj().rate().ok_or("no engine")?);
    let mut hot = [None; HOT_CUES];
    for (slot, at) in f.session().hot_cues(&loading.path) {
        if let Some(h) = hot.get_mut(usize::from(slot).wrapping_sub(1)) {
            *h = Some(at * rate);
        }
    }
    let frames = read.samples.len() / usize::from(read.channels);
    let track = playr_dj::Track::new(read.samples, read.channels, grid.and_then(dj_grid))
        .map_err(|e| e.to_string())?
        .with_hot_cues(hot);
    let live = f.dj().live.as_mut().ok_or("no engine")?;
    live.handle.load(side, track).map_err(|e| e.to_string())?;
    f.dj().arriving[i(side)] = Some(Loaded {
        path: loading.path,
        title: loading.title,
        frames,
        peaks: Arc::new(read.peaks),
        grid,
        hot,
        marks: Vec::new(),
    });
    Ok(())
}

/// The engine has taken the track sent to `side`: the deck holds it, and a
/// track never analysed is analysed for its grid.
fn taken(f: &mut impl Frontend, side: Side) -> Option<DjMessage> {
    f.dj().sent[i(side)] = None;
    let mut loaded = f.dj().arriving[i(side)].take()?;
    let rate = f64::from(f.dj().rate()?);
    loaded.marks = f
        .session()
        .marks_of(&loaded.path)
        .iter()
        .map(|m| m.time().as_secs_f64() * rate)
        .collect();
    let d = f.dj();
    d.taps[i(side)].clear();
    d.loads[i(side)] += 1;
    d.loaded[i(side)] = Some(loaded.clone());
    if std::mem::take(&mut d.autoplay[i(side)]) {
        if let Err(e) = d.set(Setting::Play(side)) {
            return Some(DjMessage::Failed(e));
        }
    }
    let took = match std::mem::take(&mut f.dj().handover[i(side)]) {
        true => hand_over(f, side, &loaded.path),
        false => Ok(false),
    };
    match took {
        Ok(true) => return Some(DjMessage::Took(side)),
        Ok(false) => {}
        Err(m) => return Some(m),
    }
    let analysed = f.session().analysis_of(&loaded.path).is_some();
    if loaded.grid.is_none() && !analysed {
        if let Ok(job) = f.session_mut().analyze(Some(loaded.path.clone())) {
            f.dj().analysing.push((job, side, loaded.path));
            return Some(DjMessage::FindingGrid(side));
        }
    }
    Some(DjMessage::Loaded {
        side,
        title: loaded.title,
        bpm: loaded.grid.map(|g| g.bpm),
    })
}

/// Takes in an analysis [`start`] began, once it lands: true if it was one.
/// The deck gets the grid if it still holds the track.
pub fn analysed(f: &mut impl Frontend, job: JobId) -> bool {
    let Some(at) = f.dj().analysing.iter().position(|(j, ..)| *j == job) else {
        return false;
    };
    let (_, side, path) = f.dj().analysing.remove(at);
    if f.dj().loaded(side).map(|l| &l.path) != Some(&path) {
        return true;
    }
    let grid = f.session().grid(&path);
    let message = match grid {
        Some(g) => match send_grid(f, side, Some(g)) {
            Ok(()) => DjMessage::Loaded {
                side,
                title: f
                    .dj()
                    .loaded(side)
                    .map(|l| l.title.clone())
                    .unwrap_or_default(),
                bpm: Some(g.bpm),
            },
            Err(e) => DjMessage::Failed(e),
        },
        None => DjMessage::NoGrid(side),
    };
    f.notify(message.into());
    true
}

/// Gives `side` the grid `grid`, in the engine and on the interface's side.
fn send_grid(f: &mut impl Frontend, side: Side, grid: Option<tempo::Grid>) -> Result<(), String> {
    f.dj().set(Setting::Grid(side, grid.and_then(dj_grid)))?;
    if let Some(l) = f.dj().loaded[i(side)].as_mut() {
        l.grid = grid;
    }
    Ok(())
}

/// Does `action`.
pub fn act(f: &mut impl Frontend, action: DjAction) {
    let result = match action {
        DjAction::Load(side) => return load(f, side),
        DjAction::Take(side) => return take(f, side),
        DjAction::Grid(side, edit) => return grid(f, side, edit),
        _ => send(f, action),
    };
    match result {
        Ok(()) => f.notify(DjMessage::Done(action).into()),
        Err(m) => f.notify(m.into()),
    }
}

fn load(f: &mut impl Frontend, side: Side) {
    let Some(track) = crate::dispatch::cursor_track(f) else {
        return f.notify(DjMessage::NoTrack.into());
    };
    let d = f.dj();
    if d.status().is_some_and(|s| s.deck(side).playing()) {
        let title = track.display_title();
        d.next[i(side)] = Some(track);
        if d.state.strict {
            return f.notify(DjMessage::Queued { side, title }.into());
        }
        // Not strict: the deck fades out, takes the track, and plays it.
        d.autoplay[i(side)] = true;
        if let Err(e) = d.set(Setting::Pause(side)) {
            return f.notify(DjMessage::Failed(e).into());
        }
        return f.notify(DjMessage::Loading(side).into());
    }
    d.next[i(side)] = None;
    d.autoplay[i(side)] = false;
    d.handover[i(side)] = false;
    match f.dj().load(side, &track) {
        Ok(()) => f.notify(DjMessage::Loading(side).into()),
        Err(e) => f.notify(DjMessage::Failed(e).into()),
    }
}

/// The deck rate, in percent, that plays as the player's speed of `semitones`,
/// and the narrowest range that holds it; `None` past every range.
fn rate_for(semitones: i32) -> Option<(f64, Range)> {
    let pct = (2f64.powf(f64::from(semitones) / 12.0) - 1.0) * 100.0;
    [Range::Narrow, Range::Medium, Range::Wide]
        .into_iter()
        .find(|r| pct.abs() <= r.percent() + 1e-9)
        .map(|r| (pct, r))
}

/// Loads the player's track onto `side`, to take over once it is read. The
/// player plays on meanwhile.
fn take(f: &mut impl Frontend, side: Side) {
    if f.dj().status().is_some_and(|s| s.deck(side).playing()) {
        return f.notify(DjMessage::Playing(side).into());
    }
    let (path, rate) = match f.session().playing_track() {
        Ok(track) => track,
        Err(refusal) => return f.notify(refusal.into()),
    };
    let semitones = f.session().player().status().semitones;
    if rate_for(semitones).is_none() {
        return f.notify(DjMessage::SpeedOutOfReach(semitones).into());
    }
    // A track played from outside the library has no row.
    let track = (f.session().tracks_at(std::slice::from_ref(&path)).pop()).unwrap_or(Track {
        path: path.to_string_lossy().into(),
        sample_rate: Some(rate),
        ..Default::default()
    });
    let d = f.dj();
    d.next[i(side)] = None;
    d.autoplay[i(side)] = false;
    match d.load(side, &track) {
        Ok(()) => {
            d.handover[i(side)] = true;
            f.notify(DjMessage::Loading(side).into());
        }
        Err(e) => f.notify(DjMessage::Failed(e).into()),
    }
}

/// Moves the player's track, now loaded on `side`, from the player to the
/// deck: its rate as the player's speed, its trim as the ReplayGain the
/// player applies, at the player's position. With the other deck silent,
/// the crossfader moves to this one, which its centre would take 3 dB from.
/// A playing player pauses as the deck starts; a paused one leaves the deck
/// cued there. False when the player has moved on to another track.
fn hand_over(f: &mut impl Frontend, side: Side, path: &Path) -> Result<bool, DjMessage> {
    if f.session().playing_track().map(|(p, _)| p).as_deref() != Ok(path) {
        return Ok(false);
    }
    let status = f.session().player().status();
    let Some((pct, range)) = rate_for(status.semitones) else {
        return Err(DjMessage::SpeedOutOfReach(status.semitones));
    };
    let trim = status.gain_db.unwrap_or(0.0);
    let playing = status.state == State::Playing;
    let rate = f64::from(f.dj().rate().ok_or(DjMessage::Empty(side))?);
    let at = f.session().player().position().as_secs_f64() * rate;
    let start = match playing {
        true => Setting::PlayFrom(side, at),
        false => Setting::Seek(side, at),
    };
    let d = f.dj();
    let alone = !d.status().is_some_and(|s| s.deck(side.other()).playing());
    let x = if side == Side::A { 0.0 } else { 1.0 };
    let xfade = alone.then_some(Setting::Xfade(x));
    let settings = [
        Some(Setting::Range(side, range)),
        Some(Setting::Rate(side, pct)),
        Some(Setting::Gain(side, f64::from(trim))),
        xfade,
        Some(start),
    ];
    for s in settings.into_iter().flatten() {
        d.set(s).map_err(DjMessage::Failed)?;
    }
    d.state.gain[i(side)] = trim;
    if alone {
        d.state.xfade = x as f32;
    }
    if playing {
        f.session().send(playr_core::audio::Cmd::TogglePause);
    }
    Ok(true)
}

/// The deck an action acts on, if it acts on one.
fn deck_of(action: DjAction) -> Option<Side> {
    use DjAction as D;
    match action {
        D::Load(s)
        | D::Take(s)
        | D::Play(s)
        | D::Pause(s)
        | D::Cue(s)
        | D::CueHold(s, _)
        | D::Sync(s, _)
        | D::Rate(s, _)
        | D::Range(s, _)
        | D::Nudge(s, _)
        | D::Gain(s, _)
        | D::Level(s, _)
        | D::Grid(s, _)
        | D::Eq(s, ..)
        | D::Kill(s, ..)
        | D::Filter(s, _)
        | D::HotCue(s, _)
        | D::HotClear(s, _)
        | D::Jump(s, _)
        | D::Loop(s, _)
        | D::Seek(s, _)
        | D::Mark(s, _) => Some(s),
        D::Xfade(_)
        | D::XfadeTo(_)
        | D::Quantize(_)
        | D::CueBus(_)
        | D::CueOut(_)
        | D::Curve(_)
        | D::Mute(..)
        | D::Unqueue(_)
        | D::Strict(_) => None,
    }
}

/// Sends `action` to the engine, pausing the player when a deck starts.
fn send(f: &mut impl Frontend, action: DjAction) -> Result<(), DjMessage> {
    use DjAction as D;
    if let Some(side) = deck_of(action) {
        if f.dj().loaded(side).is_none() {
            return Err(DjMessage::Empty(side));
        }
    }
    match action {
        D::Sync(side, true) => sync_check(f, side)?,
        D::Jump(side, _) | D::Loop(side, Some(_))
            if f.dj().loaded(side).is_some_and(|l| l.grid.is_none()) =>
        {
            return Err(DjMessage::NoGrid(side));
        }
        D::CueOut(CueOut::Channels) => match f.dj().live.as_ref().map_or(2, |l| l.channels) {
            n if n < 4 => return Err(DjMessage::NoCueChannels(n)),
            _ => {}
        },
        _ => {}
    }
    let slot = |n: u8| usize::from(n) - 1;
    if let D::Mark(side, next) = action {
        let d = f.dj();
        let (Some(l), Some(s), Some(rate)) = (d.loaded(side), d.status(), d.rate()) else {
            return Err(DjMessage::Empty(side));
        };
        let pos = s.deck(side).pos();
        // Back past a mark just left, as a CDJ's previous does.
        let back = pos - 0.5 * f64::from(rate);
        let to = match next {
            true => l.marks.iter().find(|&&m| m > pos + 1.0),
            false => l.marks.iter().rev().find(|&&m| m < back),
        };
        let Some(&to) = to else {
            return Err(DjMessage::NoMark(side));
        };
        return d.set(Setting::Seek(side, to)).map_err(DjMessage::Failed);
    }
    let seek;
    let seek_to = match action {
        D::Seek(s, pos) => {
            let d = f.dj();
            let frames = d.loaded(s).map_or(0, |l| l.frames) as f64;
            let rate = f64::from(d.rate().unwrap_or(1));
            match pos {
                crate::tape::Pos::Percent(p) => frames * f64::from(p) / 100.0,
                crate::tape::Pos::Time(t) => t.as_secs_f64() * rate,
            }
        }
        _ => 0.0,
    };
    let settings: &[Setting] = match action {
        D::Play(s) => &[Setting::Play(s)],
        D::Pause(s) => &[Setting::Pause(s)],
        D::Cue(s) => &[Setting::Cue(s, true), Setting::Cue(s, false)],
        D::CueHold(s, down) => &[Setting::Cue(s, down)],
        D::Sync(s, on) => &[Setting::Sync(s, on)],
        D::Rate(s, pct) => &[Setting::Rate(s, f64::from(pct))],
        D::Range(s, r) => &[Setting::Range(s, r)],
        D::Nudge(s, n) => &[Setting::Nudge(s, n)],
        D::Gain(s, db) => &[Setting::Gain(s, f64::from(db))],
        D::Level(s, l) => &[Setting::Level(s, f64::from(l))],
        D::Xfade(x) => &[Setting::Xfade(f64::from(x))],
        D::XfadeTo(None) => &[Setting::XfadeGlide(0.5)],
        D::XfadeTo(Some(Side::A)) => &[Setting::XfadeGlide(0.0)],
        D::XfadeTo(Some(Side::B)) => &[Setting::XfadeGlide(1.0)],
        D::Quantize(on) => &[Setting::Quantize(on)],
        D::CueBus(c) => &[Setting::CueBus(c)],
        D::Eq(s, b, db) => &[Setting::Eq(s, b, f64::from(db))],
        D::Kill(s, b, on) => &[Setting::Kill(s, b, on)],
        D::Filter(s, k) => &[Setting::Filter(s, f64::from(k))],
        D::HotCue(s, n) => &[Setting::HotCue(s, slot(n))],
        D::HotClear(s, n) => &[Setting::HotClear(s, slot(n))],
        D::Jump(s, beats) => &[Setting::BeatJump(s, f64::from(beats))],
        D::Loop(s, beats) => &[Setting::Loop(s, beats.map(f64::from))],
        D::CueOut(c) => &[Setting::CueOut(c)],
        D::Curve(c) => &[Setting::Curve(c)],
        D::Seek(s, _) => {
            seek = [Setting::Seek(s, seek_to)];
            &seek
        }
        D::Mute(s, on) => &[Setting::Mute(s, on)],
        D::Unqueue(s) => {
            f.dj().next[i(s)] = None;
            f.dj().autoplay[i(s)] = false;
            &[]
        }
        D::Strict(on) => {
            f.dj().state.strict = on;
            &[]
        }
        D::Mark(..) => &[],
        D::Load(_) | D::Take(_) | D::Grid(..) => &[],
    };
    let d = f.dj();
    for s in settings {
        d.set(*s).map_err(DjMessage::Failed)?;
    }
    let st = &mut d.state;
    match action {
        D::Gain(s, db) => st.gain[i(s)] = db,
        D::Level(s, l) => st.level[i(s)] = l,
        D::Xfade(x) => st.xfade = x,
        D::XfadeTo(to) => {
            st.xfade = match to {
                None => 0.5,
                Some(Side::A) => 0.0,
                Some(Side::B) => 1.0,
            }
        }
        D::Quantize(on) => st.quantize = on,
        D::CueBus(c) => st.cue_bus = c,
        D::Eq(s, b, db) => st.eq[i(s)][b as usize] = db,
        D::Kill(s, b, on) => st.kill[i(s)][b as usize] = on,
        D::Filter(s, k) => st.filter[i(s)] = k,
        D::CueOut(c) => st.cue_out = c,
        D::Curve(c) => st.curve = c,
        D::Mute(s, on) => st.mute[i(s)] = on,
        _ => {}
    }
    // The decks play alone; the player stays paused until asked.
    let starts = matches!(
        action,
        D::Play(_) | D::Cue(_) | D::CueHold(_, true) | D::HotCue(..)
    );
    if starts && f.session().player().status().state == State::Playing {
        f.session().send(playr_core::audio::Cmd::TogglePause);
    }
    Ok(())
}

/// Refuses sync without both grids, or to a tempo no range reaches.
fn sync_check(f: &mut impl Frontend, side: Side) -> Result<(), DjMessage> {
    let d = f.dj();
    let grid = |s: Side| d.loaded(s).and_then(|l| l.grid);
    let Some(gf) = grid(side) else {
        return Err(DjMessage::NoGrid(side));
    };
    let Some(gl) = grid(side.other()) else {
        return Err(DjMessage::NoGrid(side.other()));
    };
    let pct = d.status().map_or(0.0, |s| s.deck(side.other()).pct());
    let lead = gl.bpm * (1.0 + pct / 100.0);
    match playr_dj::sync_pct(gf.bpm, lead).abs() <= Range::Wide.percent() {
        true => Ok(()),
        false => Err(DjMessage::OutOfReach(side)),
    }
}

/// Edits `side`'s grid and stores it with the track.
fn grid(f: &mut impl Frontend, side: Side, edit: GridEdit) {
    let Some(loaded) = f.dj().loaded(side).cloned() else {
        return f.notify(DjMessage::Empty(side).into());
    };
    let path = loaded.path;
    let stored = match edit {
        GridEdit::Double | GridEdit::Halve => {
            let fix = match edit {
                GridEdit::Double => TempoFix::Double,
                _ => TempoFix::Halve,
            };
            match f.session_mut().fix_tempo_of(&path, fix) {
                Notice::Done(_) => Ok(()),
                other => return f.notify(other.into()),
            }
        }
        GridEdit::Reset => f
            .session_mut()
            .set_grid(&path, None)
            .map(|()| _ = f.session_mut().fix_tempo_of(&path, TempoFix::Reset)),
        GridEdit::Tap => match tap(f, side, loaded.grid) {
            Some(g) => f.session_mut().set_grid(&path, Some(g)),
            None => return f.notify(DjMessage::Tapped(side).into()),
        },
        GridEdit::Earlier | GridEdit::Later | GridEdit::Offset(_) => {
            let Some(g) = loaded.grid else {
                return f.notify(DjMessage::NoGrid(side).into());
            };
            let by = match edit {
                GridEdit::Earlier => -g.bpm.recip() * 60.0,
                GridEdit::Later => g.bpm.recip() * 60.0,
                GridEdit::Offset(ms) => f64::from(ms) / 1000.0,
                _ => unreachable!("matched above"),
            };
            let moved = tempo::Grid { t0: g.t0 + by, ..g };
            f.session_mut().set_grid(&path, Some(moved))
        }
    };
    if let Err(e) = stored {
        return f.notify(DjMessage::Failed(e).into());
    }
    let grid = f.session().grid(&path);
    if let Err(e) = send_grid(f, side, grid) {
        return f.notify(DjMessage::Failed(e).into());
    }
    match grid {
        Some(g) => f.notify(
            DjMessage::Grid {
                side,
                bpm: g.bpm,
                t0: g.t0,
            }
            .into(),
        ),
        None => f.notify(DjMessage::NoGrid(side).into()),
    }
}

/// Counts a tap at `side`'s position. From the second tap of a count, the
/// grid: the mean interval's tempo, with a beat at the last tap. Taps on a
/// grid already set keep its tempo until the count has 4.
fn tap(f: &mut impl Frontend, side: Side, had: Option<tempo::Grid>) -> Option<tempo::Grid> {
    let d = f.dj();
    let rate = f64::from(d.rate()?);
    let at = d.status()?.deck(side).pos() / rate;
    let taps = &mut d.taps[i(side)];
    if taps.last().is_some_and(|&t| at <= t || at - t > TAP_GAP) {
        taps.clear();
    }
    taps.push(at);
    let n = taps.len();
    let bpm = match (n, had) {
        (1, _) => return None,
        (2..4, Some(g)) => g.bpm,
        _ => 60.0 * (n - 1) as f64 / (taps[n - 1] - taps[0]),
    };
    Some(tempo::Grid { bpm, t0: at })
}
