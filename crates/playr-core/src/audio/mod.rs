//! Playback engine.
//!
//! Three threads meet here. The caller (usually the UI) holds a [`Player`] and
//! sends [`Cmd`]s. An engine thread owns the decoder and fills a ring buffer.
//! The cpal callback drains that ring on the realtime thread and touches
//! nothing but atomics.

pub mod convert;
pub mod decode;
pub mod meter;
#[cfg(feature = "opus")]
pub mod opus;
pub mod order;
pub mod output;
pub mod resample;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use decode::AudioStream;
pub use decode::Spec;

/// The file name alone, for messages that must fit on one line.
fn short(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
use crate::event::{Event, EventSink};
use convert::Converter;
pub use order::Mode;
use order::Order;
use output::{Backend, DeviceEvent, Output, Plan, Shared};

/// Furthest the playback speed may be shifted, in semitones.
///
/// Twelve is an octave, which is 0.5x and 2.0x. Past that a music track is no
/// longer recognisable, and the resampler is only built to span this range.
pub const MAX_SEMITONES: i32 = 12;

/// Playback speed for a shift of `semitones`.
///
/// Steps are geometric, so each is the same musical interval: twelve of them
/// double or halve the speed. Pitch moves with tempo, as it does on tape.
pub fn speed_for(semitones: i32) -> f64 {
    2f64.powf(semitones.clamp(-MAX_SEMITONES, MAX_SEMITONES) as f64 / 12.0)
}

/// Position within the current track.
///
/// `frames_out` counts what the device has played since the stream was opened,
/// `track_start` marks where this track began in that count, and `offset` is
/// how far into the track playback resumed after a seek or a speed change.
///
/// `speed` converts device time into track time: at 2x, one second of output
/// has covered two seconds of the recording.
pub fn track_position(
    frames_out: u64,
    track_start: u64,
    offset: u64,
    rate: u32,
    speed: f64,
) -> Duration {
    if rate == 0 || speed <= 0.0 {
        return Duration::ZERO;
    }
    let played = frames_out.saturating_sub(track_start) as f64 * speed;
    Duration::from_secs_f64((played + offset as f64) / rate as f64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone)]
pub enum Cmd {
    /// Replace the queue and start at `index`.
    Play(Vec<PathBuf>, usize),
    /// Append to the queue, starting playback if stopped.
    Enqueue(Vec<PathBuf>),
    /// Play the queued track at `index`, keeping the queue.
    Jump(usize),
    TogglePause,
    Next,
    Prev,
    Stop,
    /// Seek to an absolute position in the current track.
    Seek(Duration),
    /// Seek forward (positive) or back (negative) by seconds.
    SeekBy(i64),
    SetVolume(f32),
    /// Shift playback speed by whole semitones; pitch moves with it.
    SpeedBy(i32),
    /// Return to normal speed.
    SpeedReset,
    /// Set the speed shift to this many semitones.
    SetSpeed(i32),
    /// Change how playback moves through the list. See [`Mode`].
    SetMode(Mode),
    Quit,
}

/// A snapshot of what the player is doing, for display.
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub state: State,
    /// The one queue, which the engine plays from.
    ///
    /// Only [`Player::send`] changes it, before the engine sees the command, so
    /// it is current as soon as `send` returns. `index` and `state` follow once
    /// the engine has acted.
    pub queue: Arc<[PathBuf]>,
    pub index: usize,
    pub duration: Option<Duration>,
    pub source: Option<Spec>,
    pub output_rate: u32,
    pub resampling: bool,
    /// The most recent playback error, such as a file that could not be decoded.
    ///
    /// It is never cleared. The UI shows it when `error_seq` changes, which is
    /// what makes a skipped track visible: clearing the field instead would
    /// race with the UI's refresh and usually be missed.
    pub error: Option<String>,
    /// Increments on every error, so a repeat of the same message is still seen.
    pub error_seq: u64,
    /// Playback speed shift in semitones; 0 is normal speed.
    pub semitones: i32,
    /// How playback moves through the list. Like `queue`, only
    /// [`Player::send`] writes it, so it is current as soon as `send` returns.
    pub mode: Mode,
}

impl Status {
    pub fn current(&self) -> Option<&PathBuf> {
        self.queue.get(self.index)
    }
}

/// What the engine receives. A queue change carries the new queue itself, so
/// the engine plays exactly the list [`Status::queue`] shows.
enum Msg {
    Play(Arc<[PathBuf]>, usize),
    Enqueue(Arc<[PathBuf]>),
    Cmd(Cmd),
}

/// Where the engine sends events, once a session gives it somewhere.
type Events = Arc<Mutex<Option<EventSink>>>;

/// Handle to the engine thread.
pub struct Player {
    tx: Sender<Msg>,
    status: Arc<Mutex<Status>>,
    shared: Arc<Shared>,
    events: Events,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Player {
    /// Starts the engine thread on the default output device. Fails only if
    /// no output device exists.
    pub fn new() -> Result<Self, output::OutputError> {
        Self::with_backend(output::Cpal(output::default_device()?))
    }

    /// Starts the engine thread, playing to `backend`.
    pub fn with_backend(backend: impl Backend) -> Result<Self, output::OutputError> {
        let (tx, rx) = std::sync::mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let shared = Arc::new(Shared::new());
        let events: Events = Arc::default();

        let handle = {
            let (status, shared, events) = (status.clone(), shared.clone(), events.clone());
            std::thread::Builder::new()
                .name("playr-audio".into())
                .spawn(move || Engine::new(Box::new(backend), rx, status, shared, events).run())
                .map_err(|e| output::OutputError::Build(e.to_string()))?
        };

        Ok(Player {
            tx,
            status,
            shared,
            events,
            handle: Some(handle),
        })
    }

    /// Sends the engine's events, track and state changes and playback
    /// errors, to `sink` from now on.
    pub fn set_events(&self, sink: EventSink) {
        if let Ok(mut events) = self.events.lock() {
            *events = Some(sink);
        }
    }

    pub fn send(&self, cmd: Cmd) {
        let Ok(mut status) = self.status.lock() else {
            return;
        };
        let msg = match cmd {
            Cmd::Play(paths, index) => {
                status.queue = paths.into();
                Msg::Play(status.queue.clone(), index)
            }
            Cmd::Enqueue(paths) => {
                status.queue = status.queue.iter().cloned().chain(paths).collect();
                Msg::Enqueue(status.queue.clone())
            }
            // Set here, so the next `volume` call sees it even before the engine runs.
            Cmd::SetVolume(v) => return self.shared.set_volume(v),
            Cmd::SetMode(mode) => {
                status.mode = mode;
                Msg::Cmd(Cmd::SetMode(mode))
            }
            cmd => Msg::Cmd(cmd),
        };
        // Under the lock, so the engine receives changes in the order they were made.
        let _ = self.tx.send(msg);
    }

    /// The playback mode. See [`Status::mode`].
    pub fn mode(&self) -> Mode {
        self.status.lock().map(|s| s.mode).unwrap_or_default()
    }

    /// The queue. See [`Status::queue`].
    pub fn queue(&self) -> Arc<[PathBuf]> {
        self.status
            .lock()
            .map(|s| s.queue.clone())
            .unwrap_or_default()
    }

    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Audible position in the current track.
    pub fn position(&self) -> Duration {
        // Until the device discards, the counters describe the audio before a seek.
        let requested = self.shared.flush_requested.load(Ordering::Relaxed);
        if requested != self.shared.flush_done.load(Ordering::Relaxed) {
            return Duration::from_nanos(self.shared.seek_target.load(Ordering::Relaxed));
        }
        track_position(
            self.shared.frames_out.load(Ordering::Relaxed),
            self.shared.track_start.load(Ordering::Relaxed),
            self.shared.position_offset.load(Ordering::Relaxed),
            self.shared.position_rate.load(Ordering::Relaxed),
            self.shared.speed(),
        )
    }

    pub fn volume(&self) -> f32 {
        self.shared.volume()
    }

    /// Momentary loudness of what is playing, in LUFS; `None` for silence.
    pub fn loudness(&self) -> Option<f32> {
        self.shared.loudness()
    }

    /// The largest sample magnitude played since the last call, which resets it.
    pub fn take_peak(&self) -> f32 {
        self.shared.take_peak()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Cmd(Cmd::Quit));
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Frames of headroom below which the engine tops the ring back up.
const REFILL_BELOW: f64 = 0.5;

/// Longest a single `pump` may spend decoding before returning to the run loop.
///
/// Without a bound, filling a two second ring happens in one call, and for that
/// whole time no command is handled and no status is published: at track start
/// the player looks frozen. How long that takes depends on the codec, so the
/// limit is a deadline rather than a packet count. Opus decodes several times
/// slower than FLAC and is what made this visible.
const PUMP_BUDGET: Duration = Duration::from_millis(8);

/// Longest a seek waits for the device to discard buffered audio before
/// reopening the device instead. A device that has stopped calling back never
/// discards.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(1);

/// A seek waiting for the device to discard what was buffered before it.
#[derive(Clone, Copy)]
struct Flush {
    /// The `flush_requested` value the device must reach.
    generation: u64,
    deadline: std::time::Instant,
    /// Where in the track playback resumes.
    at: Duration,
    /// `at` in output frames.
    offset: u64,
}

/// A track queued behind the current one whose format needs a new stream.
struct Staged {
    stream: AudioStream,
    spec: Spec,
    plan: Plan,
    index: usize,
    first: Vec<f32>,
}

struct Engine {
    backend: Box<dyn Backend>,
    rx: Receiver<Msg>,
    status: Arc<Mutex<Status>>,
    shared: Arc<Shared>,
    events: Events,
    /// The state and index last published, to send an event when either changes.
    published: (State, usize),
    /// Events the device reports from its own thread.
    device_events: (Sender<DeviceEvent>, Receiver<DeviceEvent>),

    /// Commands taken off the channel while searching for a playable track,
    /// handled before anything newer.
    deferred: VecDeque<Msg>,

    out: Option<Output>,
    stream: Option<AudioStream>,
    conv: Option<Converter>,

    queue: Arc<[PathBuf]>,
    /// Which track follows which, for the current mode.
    order: Order,
    index: usize,
    state: State,
    /// Playback speed shift in semitones; 0 is normal speed.
    semitones: i32,

    /// Total frames written into the ring since the stream was opened.
    written: u64,
    /// `(output frame at which it starts, queue index, duration)` per track.
    marks: VecDeque<(u64, usize, Option<Duration>)>,
    /// Next track, held back because it needs a different output format.
    staged: Option<Staged>,
    /// A seek whose pre-seek audio the device has not yet discarded.
    flush: Option<Flush>,
    /// Samples converted but not yet accepted by the ring.
    carry: Vec<f32>,
}

impl Engine {
    fn new(
        backend: Box<dyn Backend>,
        rx: Receiver<Msg>,
        status: Arc<Mutex<Status>>,
        shared: Arc<Shared>,
        events: Events,
    ) -> Self {
        Engine {
            backend,
            rx,
            status,
            shared,
            events,
            published: (State::Stopped, 0),
            device_events: std::sync::mpsc::channel(),
            deferred: VecDeque::new(),
            out: None,
            stream: None,
            conv: None,
            queue: Arc::default(),
            order: Order::new(0, Mode::Normal, 0, order::random_seed()),
            index: 0,
            state: State::Stopped,
            semitones: 0,
            written: 0,
            marks: VecDeque::new(),
            staged: None,
            flush: None,
            carry: Vec::new(),
        }
    }

    fn run(mut self) {
        loop {
            // Poll often while playing so the ring never runs dry; block when
            // idle so a stopped player costs nothing.
            let wait = if self.state == State::Playing {
                Duration::from_millis(5)
            } else {
                Duration::from_millis(100)
            };
            let first = match self.deferred.pop_front() {
                Some(cmd) => Ok(cmd),
                None => self.rx.recv_timeout(wait),
            };
            match first {
                Ok(Msg::Cmd(Cmd::Quit)) => break,
                Ok(msg) => {
                    self.handle(msg);
                    while let Some(next) = self
                        .deferred
                        .pop_front()
                        .or_else(|| self.rx.try_recv().ok())
                    {
                        if matches!(next, Msg::Cmd(Cmd::Quit)) {
                            return;
                        }
                        self.handle(next);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }

            while let Ok(event) = self.device_events.1.try_recv() {
                self.device_event(event);
            }
            self.poll_flush();
            if self.state == State::Playing {
                self.pump();
            }
            self.advance_marks();
            self.publish();
        }
    }

    fn handle(&mut self, msg: Msg) {
        let cmd = match msg {
            Msg::Play(queue, index) => {
                self.queue = queue;
                return self.jump(index);
            }
            Msg::Enqueue(queue) => {
                let first_new = self.queue.len();
                self.queue = queue;
                return self.enqueued(first_new);
            }
            Msg::Cmd(cmd) => cmd,
        };
        match cmd {
            // `Player::send` delivers these as `Msg::Play` and `Msg::Enqueue`.
            Cmd::Play(..) | Cmd::Enqueue(..) => {}
            Cmd::Jump(index) => self.jump(index),
            Cmd::TogglePause => match self.state {
                State::Playing => {
                    if let Some(o) = &self.out {
                        o.pause();
                    }
                    self.state = State::Paused;
                }
                State::Paused => {
                    if let Some(o) = &self.out {
                        o.play();
                    }
                    self.state = State::Playing;
                }
                State::Stopped => {
                    if !self.queue.is_empty() {
                        self.start(self.index, false);
                    }
                }
            },
            Cmd::Next => match self.order.successor(self.index, true) {
                Some(next) => {
                    self.teardown();
                    self.index = next;
                    self.start(next, false);
                }
                None => {
                    self.teardown();
                    self.state = State::Stopped;
                }
            },
            Cmd::Prev => {
                // Restart the track if we are past the start of it, which is
                // what a double-press of "previous" expects.
                let current = self.index;
                let before = if self.elapsed() > Duration::from_secs(3) {
                    None
                } else {
                    self.order.predecessor(current)
                };
                self.teardown();
                // Step back over unplayable tracks; with none before, restart.
                if !before.is_some_and(|b| self.start(b, true)) {
                    self.start(current, false);
                }
            }
            Cmd::Stop => {
                self.teardown();
                self.state = State::Stopped;
            }
            Cmd::Seek(pos) => self.seek_to(pos),
            Cmd::SeekBy(delta) => {
                let cur = self.elapsed().as_secs_f64();
                let target = (cur + delta as f64).max(0.0);
                self.seek_to(Duration::from_secs_f64(target));
            }
            // Applied by `Player::send`, which never forwards it.
            Cmd::SetVolume(_) => {}
            Cmd::SpeedBy(delta) => self.set_semitones(self.semitones + delta),
            Cmd::SpeedReset => self.set_semitones(0),
            Cmd::SetSpeed(semitones) => self.set_semitones(semitones),
            Cmd::SetMode(mode) => self.set_mode(mode),
            Cmd::Quit => {}
        }
    }

    fn device_event(&mut self, event: DeviceEvent) {
        match event {
            // Playback continues on the new device; nothing went wrong.
            DeviceEvent::Rerouted => {}
            // The ring would never drain, freezing the position while `Playing`.
            DeviceEvent::Lost(e) => {
                self.fail(format!("audio device lost: {e}"));
                self.teardown();
                self.state = State::Stopped;
            }
            DeviceEvent::Error(e) => self.fail(format!("audio device: {e}")),
        }
    }

    /// Plays the queued track at `index`, or stops if the queue is empty.
    fn jump(&mut self, index: usize) {
        self.index = index.min(self.queue.len().saturating_sub(1));
        self.order = Order::new(
            self.queue.len(),
            self.order.mode(),
            self.index,
            order::random_seed(),
        );
        self.teardown();
        if self.queue.is_empty() {
            self.state = State::Stopped;
        } else {
            self.start(self.index, false);
        }
    }

    /// Continues into tracks appended from `first_new`.
    fn enqueued(&mut self, first_new: usize) {
        self.order.extend(self.queue.len());
        if self.state == State::Stopped {
            self.start(first_new, false);
        } else if self.stream.is_none() && self.staged.is_none() {
            // The last track already decoded to its end, and `pump`
            // stages nothing once the queue has run out.
            self.stage_from(first_new);
        }
    }

    /// Audible position within the current track.
    fn elapsed(&self) -> Duration {
        // Until the device discards, the counters still describe the audio
        // before the seek; a second quick seek must start from the first.
        if let Some(flush) = self.flush {
            return flush.at;
        }
        track_position(
            self.shared.frames_out.load(Ordering::Relaxed),
            self.shared.track_start.load(Ordering::Relaxed),
            self.shared.position_offset.load(Ordering::Relaxed),
            self.shared.position_rate.load(Ordering::Relaxed),
            self.shared.speed(),
        )
    }

    fn teardown(&mut self) {
        self.stream = None;
        self.conv = None;
        self.staged = None;
        self.flush = None;
        self.carry.clear();
        self.marks.clear();
        self.written = 0;
        // Dropping the output drops the ring, discarding anything buffered.
        self.out = None;
        // A flush still pending would discard the next stream's first audio.
        self.settle_flush();
        self.shared.frames_out.store(0, Ordering::Relaxed);
        self.shared.track_start.store(0, Ordering::Relaxed);
        // A new track starts from its own beginning, not a previous seek.
        self.shared.position_offset.store(0, Ordering::Relaxed);
    }

    /// Opens track `i`, or the first playable track after it (before it, when
    /// `back` is set), and (re)builds the output stream to match.
    ///
    /// Returns false if no playable track was found.
    fn start(&mut self, i: usize, back: bool) -> bool {
        let Some((i, stream, first, spec)) = self.open_from(i, back) else {
            self.state = State::Stopped;
            return false;
        };

        let plan = match self.backend.negotiate(spec) {
            Ok(p) => p,
            Err(e) => {
                self.fail(e.to_string());
                self.state = State::Stopped;
                return true;
            }
        };

        if let Err(e) = self.rebuild_output(plan) {
            self.fail(e.to_string());
            self.state = State::Stopped;
            return true;
        }

        self.index = i;
        self.conv = Some(Converter::new(spec, plan, self.speed()));
        let duration = stream.duration();
        self.stream = Some(stream);
        self.marks.clear();
        self.marks.push_back((0, i, duration));
        self.shared.track_start.store(0, Ordering::Relaxed);
        // A new track starts from its own beginning, not a previous seek.
        self.shared.position_offset.store(0, Ordering::Relaxed);

        self.convert_and_carry(&first);
        self.state = State::Playing;
        if let Some(o) = &self.out {
            o.play();
        }
        self.pump();
        true
    }

    /// Opens the first playable track from `i`, stepping on in play order, or
    /// back when `back` is set. `None` when the order runs out, a command
    /// interrupts, or a whole list's worth of tracks has failed.
    ///
    /// Bad files are skipped in a loop, not by recursion: a run of thousands,
    /// such as an Opus library in a build without Opus, overflowed the stack.
    /// The limit matters under repeat, whose order never runs out.
    fn open_from(
        &mut self,
        mut i: usize,
        back: bool,
    ) -> Option<(usize, AudioStream, Vec<f32>, Spec)> {
        for _ in 0..self.queue.len() {
            let path = self.queue.get(i)?.clone();
            if let Some((stream, first, spec)) = self.open_track(&path) {
                return Some((i, stream, first, spec));
            }
            if self.interrupted() {
                return None;
            }
            i = if back {
                self.order.predecessor(i)?
            } else {
                self.order.successor(i, true)?
            };
        }
        None
    }

    /// Moves waiting commands to `deferred`, and reports whether one of them
    /// replaces the search for a playable track.
    ///
    /// Without this, a long run of bad files held every command, quit included.
    fn interrupted(&mut self) -> bool {
        let mut stop = false;
        loop {
            match self.rx.try_recv() {
                Ok(msg) => {
                    stop |= matches!(
                        msg,
                        Msg::Play(..)
                            | Msg::Cmd(
                                Cmd::Quit | Cmd::Stop | Cmd::Jump(_) | Cmd::Next | Cmd::Prev
                            )
                    );
                    self.deferred.push_back(msg);
                }
                Err(TryRecvError::Empty) => return stop,
                Err(TryRecvError::Disconnected) => return true,
            }
        }
    }

    /// Opens `path` and decodes its first chunk, reporting any failure.
    ///
    /// The sample rate is not always in the container header, so one chunk is
    /// decoded before the device format can be negotiated.
    fn open_track(&mut self, path: &std::path::Path) -> Option<(AudioStream, Vec<f32>, Spec)> {
        let mut stream = match AudioStream::open(path) {
            Ok(s) => s,
            Err(e) => {
                self.fail(format!("{}: {e}", short(path)));
                return None;
            }
        };
        let first = match stream.next_chunk() {
            Ok(Some(c)) => c.to_vec(),
            Ok(None) => Vec::new(),
            Err(e) => {
                self.fail(format!("{}: {e}", short(path)));
                return None;
            }
        };
        let spec = stream.spec();
        if spec.rate == 0 || spec.channels == 0 {
            self.fail(format!("{}: unknown stream format", short(path)));
            return None;
        }
        Some((stream, first, spec))
    }

    fn open_output(&self, plan: Plan) -> Result<Output, output::OutputError> {
        Output::open(
            self.backend.as_ref(),
            plan,
            self.shared.clone(),
            self.device_events.0.clone(),
        )
    }

    fn fail(&mut self, msg: String) {
        if let Ok(mut s) = self.status.lock() {
            s.error = Some(msg.clone());
            s.error_seq += 1;
        }
        self.emit(Event::PlaybackError(msg));
    }

    fn emit(&self, event: Event) {
        if let Some(sink) = self.events.lock().ok().and_then(|e| e.clone()) {
            sink(event);
        }
    }

    fn rebuild_output(&mut self, plan: Plan) -> Result<(), output::OutputError> {
        if self.out.as_ref().map(|o| o.plan) == Some(plan) {
            return Ok(());
        }
        self.out = None;
        let out = self.open_output(plan)?;
        self.written = 0;
        self.shared.frames_out.store(0, Ordering::Relaxed);
        self.shared
            .position_rate
            .store(plan.rate, Ordering::Relaxed);
        self.out = Some(out);
        Ok(())
    }

    fn speed(&self) -> f64 {
        speed_for(self.semitones)
    }

    /// Changes the playback mode.
    ///
    /// A next track already chosen, whether decoding into the ring or staged,
    /// was chosen under the old mode. Seeking to the current position discards
    /// it, so the new mode applies from the next track rather than the one after.
    fn set_mode(&mut self, mode: Mode) {
        if mode == self.order.mode() {
            return;
        }
        let current = self.marks.front().map_or(self.index, |(_, i, _)| *i);
        self.order.set_mode(mode, current);
        let chosen = self.marks.len() > 1 || self.staged.is_some() || self.stream.is_none();
        if chosen && !self.marks.is_empty() {
            self.seek(self.elapsed());
        }
    }

    /// Applies a speed change by re-seeking to the current position.
    ///
    /// The ring already holds up to two seconds resampled at the old ratio.
    /// Changing the ratio alone would leave that to play out first, so the key
    /// would appear dead for a second or two; re-seeking makes it immediate.
    fn set_semitones(&mut self, semitones: i32) {
        let want = semitones.clamp(-MAX_SEMITONES, MAX_SEMITONES);
        if want == self.semitones {
            return;
        }
        let at = self.elapsed();
        self.semitones = want;
        self.shared.set_speed(self.speed());
        if !self.marks.is_empty() {
            self.seek(at);
        }
    }

    /// Seeks where the listener asked. The decoder refuses a target at or past
    /// the end, so that moves on as reaching the end would.
    fn seek_to(&mut self, pos: Duration) {
        match self.marks.front() {
            Some(&(_, audible, Some(duration))) if pos >= duration => self.played_out(audible),
            _ => self.seek(pos),
        }
    }

    /// Moves on from track `current` as its end does: to the next track in
    /// play order, still paused if it was, or stops when there is none.
    fn played_out(&mut self, current: usize) {
        let paused = self.state == State::Paused;
        self.teardown();
        match self.order.successor(current, false) {
            Some(next) => {
                if self.start(next, false) && paused && self.state == State::Playing {
                    if let Some(o) = &self.out {
                        o.pause();
                    }
                    self.state = State::Paused;
                }
            }
            None => self.state = State::Stopped,
        }
    }

    /// Seeks within the audible track, which is `marks[0]`.
    ///
    /// The decoder reads 1-2 s ahead, so near a track's end it has already
    /// finished it: `stream` is the next track, or `None`. Seeking `stream`
    /// then played the next track from this one's position, under this one's
    /// title, and played it again from the start afterwards.
    fn seek(&mut self, pos: Duration) {
        let Some(&(_, audible, _)) = self.marks.front() else {
            return;
        };
        let src = if self.marks.len() > 1 || self.stream.is_none() {
            let Some(path) = self.queue.get(audible).cloned() else {
                return;
            };
            let Some((mut stream, _, spec)) = self.open_track(&path) else {
                return;
            };
            // Seek before replacing anything, so a failed seek changes nothing.
            if stream.seek(pos).is_err() {
                return;
            }
            self.stream = Some(stream);
            self.index = audible;
            spec
        } else {
            let (Some(stream), Some(conv)) = (self.stream.as_mut(), self.conv.as_ref()) else {
                return;
            };
            if stream.seek(pos).is_err() {
                return;
            }
            conv.src()
        };
        // Rebuilt rather than reused: it carries filter state from before the
        // seek, and its ratio may have changed with the playback speed.
        if let Some(plan) = self.out.as_ref().map(|o| o.plan) {
            self.conv = Some(Converter::new(src, plan, self.speed()));
        }
        self.carry.clear();
        self.staged = None;

        let dur = self.stream.as_ref().and_then(|s| s.duration());
        let idx = self.index;
        self.written = 0;
        self.marks.clear();
        self.marks.push_back((0, idx, dur));

        // The device discards the pre-seek audio in its callback; nothing is
        // pushed until it has. Reopening the device instead cost a gap and
        // could click. Positions are reset once the discard is done.
        let rate = self.shared.position_rate.load(Ordering::Relaxed) as f64;
        // Before the request, so a reader that sees the request sees this target.
        self.shared.seek_target.store(
            pos.as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
        let generation = self.shared.flush_requested.fetch_add(1, Ordering::Relaxed) + 1;
        self.flush = Some(Flush {
            generation,
            deadline: std::time::Instant::now() + FLUSH_TIMEOUT,
            at: pos,
            offset: (pos.as_secs_f64() * rate).max(0.0) as u64,
        });
        self.poll_flush();
    }

    /// Finishes a pending seek once the device has discarded its buffer, or
    /// reopens the device if it has not within `FLUSH_TIMEOUT`.
    fn poll_flush(&mut self) {
        let Some(flush) = self.flush else { return };
        if self.shared.flush_done.load(Ordering::Relaxed) != flush.generation {
            if std::time::Instant::now() < flush.deadline {
                return;
            }
            let Some(plan) = self.out.as_ref().map(|o| o.plan) else {
                self.flush = None;
                return;
            };
            self.out = None;
            // The stalled device never will; the new ring starts empty.
            self.settle_flush();
            match self.open_output(plan) {
                Ok(o) => {
                    if self.state == State::Playing {
                        o.play();
                    }
                    self.out = Some(o);
                }
                // Without an output nothing can play; staying `Playing`
                // would freeze the position with no message.
                Err(e) => {
                    self.fail(e.to_string());
                    self.teardown();
                    self.state = State::Stopped;
                    return;
                }
            }
        }
        self.flush = None;
        // Playback resumes partway into the track while the device count
        // restarts at zero, so the difference is carried as an offset.
        self.shared.frames_out.store(0, Ordering::Relaxed);
        self.shared.track_start.store(0, Ordering::Relaxed);
        self.shared
            .position_offset
            .store(flush.offset, Ordering::Relaxed);
    }

    /// Marks every flush done, once no callback can act on the ring it named.
    fn settle_flush(&self) {
        let requested = self.shared.flush_requested.load(Ordering::Relaxed);
        self.shared.flush_done.store(requested, Ordering::Relaxed);
    }

    /// Converts decoded source samples to the output layout and appends to `carry`.
    fn convert_and_carry(&mut self, decoded: &[f32]) {
        if let Some(conv) = self.conv.as_mut() {
            conv.push(decoded, &mut self.carry);
        }
    }

    /// Appends the rest of the current track's conversion to `carry`.
    fn finish_track(&mut self) {
        if let Some(conv) = self.conv.as_mut() {
            conv.finish(&mut self.carry);
        }
    }

    /// Decodes and pushes until the ring is comfortably full, or the time
    /// budget runs out and the run loop gets a turn.
    fn pump(&mut self) {
        // Pushing before the device has discarded would lose the new audio too.
        if self.flush.is_some() {
            return;
        }
        let deadline = std::time::Instant::now() + PUMP_BUDGET;
        // A format change waits for the ring to empty, then opens a new stream.
        if self.staged.is_some() {
            let drained = self.out.as_ref().map(|o| o.is_drained()).unwrap_or(true);
            if drained {
                self.promote_staged();
            } else {
                self.push_carry();
                return;
            }
        }

        loop {
            self.push_carry();
            // Hand the run loop a turn even if the ring is not full yet, so
            // commands stay responsive and the status keeps updating.
            if std::time::Instant::now() >= deadline {
                return;
            }
            let Some(out) = &self.out else { return };
            let free = out.producer.slots();
            if (free as f64) < out.capacity as f64 * REFILL_BELOW {
                return;
            }
            if !self.carry.is_empty() {
                // Ring is full for now; carry stays until the next pass.
                return;
            }

            let Some(stream) = self.stream.as_mut() else {
                // Nothing more to decode: stop once the device has caught up.
                if self.out.as_ref().map(|o| o.is_drained()).unwrap_or(true) {
                    self.state = State::Stopped;
                    self.teardown();
                }
                return;
            };

            match stream.next_chunk() {
                Ok(Some(chunk)) => {
                    let chunk = chunk.to_vec();
                    self.convert_and_carry(&chunk);
                }
                Ok(None) => {
                    // `stage_from` ends the conversion, unless the next track continues it.
                    self.stream = None;
                    self.stage_next();
                    if self.staged.is_some() {
                        return;
                    }
                }
                Err(e) => {
                    let playing = self.marks.back().map(|(_, i, _)| *i).unwrap_or(self.index);
                    if let Some(path) = self.queue.get(playing).cloned() {
                        self.fail(format!("{}: {e}", short(&path)));
                    }
                    self.stream = None;
                    self.stage_next();
                    if self.staged.is_some() {
                        return;
                    }
                }
            }
        }
    }

    /// Opens the next track. If its format matches the open stream, decoding
    /// continues into the same ring, which is what makes playback gapless.
    ///
    /// Bad files are skipped in a loop; see [`Engine::open_from`].
    fn stage_next(&mut self) {
        let last = self.marks.back().map_or(self.index, |(_, i, _)| *i);
        match self.order.successor(last, false) {
            Some(next) => self.stage_from(next),
            None => self.finish_track(),
        }
    }

    /// Stages the first playable track at or after `next`.
    ///
    /// The current track's conversion is finished first, except when the next
    /// track shares its source format and output: then it runs on through the
    /// same resampler, so a resampled join matches one continuous stream.
    fn stage_from(&mut self, next: usize) {
        let Some((next, stream, first, spec)) = self.open_from(next, false) else {
            self.finish_track();
            return;
        };
        let plan = match self.backend.negotiate(spec) {
            Ok(p) => p,
            Err(e) => {
                self.fail(e.to_string());
                self.finish_track();
                return;
            }
        };
        let duration = stream.duration();

        if self.out.as_ref().map(|o| o.plan) == Some(plan) {
            // Same output format: continue seamlessly.
            let speed = self.speed();
            if !self
                .conv
                .as_ref()
                .is_some_and(|c| c.continues(spec, plan, speed))
            {
                self.finish_track();
                self.conv = Some(Converter::new(spec, plan, speed));
            }
            self.marks.push_back((self.written, next, duration));
            self.stream = Some(stream);
            self.convert_and_carry(&first);
        } else {
            self.finish_track();
            self.staged = Some(Staged {
                stream,
                spec,
                plan,
                index: next,
                first,
            });
        }
    }

    fn promote_staged(&mut self) {
        let Some(staged) = self.staged.take() else {
            return;
        };
        if let Err(e) = self.rebuild_output(staged.plan) {
            self.fail(e.to_string());
            // Otherwise the previous track's state stays, and play restarts it.
            self.teardown();
            self.index = staged.index;
            self.state = State::Stopped;
            return;
        }
        self.conv = Some(Converter::new(staged.spec, staged.plan, self.speed()));
        let duration = staged.stream.duration();
        self.index = staged.index;
        self.stream = Some(staged.stream);
        self.written = 0;
        self.marks.clear();
        self.marks.push_back((0, staged.index, duration));
        self.shared.track_start.store(0, Ordering::Relaxed);
        // A new track starts from its own beginning, not a previous seek.
        self.shared.position_offset.store(0, Ordering::Relaxed);
        self.convert_and_carry(&staged.first);
        if let Some(o) = &self.out {
            o.play();
        }
    }

    /// Pushes as much of `carry` into the ring as fits.
    fn push_carry(&mut self) {
        let Some(out) = self.out.as_mut() else { return };
        if self.carry.is_empty() {
            return;
        }
        // Whole frames only: `slots` read during a callback can be odd, and a
        // split frame would undercount `written` and could swap channels.
        let channels = out.plan.channels as usize;
        let whole = out.producer.slots().min(self.carry.len()) / channels * channels;
        let (written, _) = out.producer.push_partial_slice(&self.carry[..whole]);
        let n = written.len();
        if n > 0 {
            self.carry.drain(..n);
            self.written += (n / channels) as u64;
        }
    }

    /// Moves the reported track forward once the device has reached its boundary.
    fn advance_marks(&mut self) {
        let played = self.shared.frames_out.load(Ordering::Relaxed);
        while self.marks.len() > 1 {
            let next_start = self.marks[1].0;
            if played < next_start {
                break;
            }
            self.marks.pop_front();
            let (start, idx, _) = self.marks[0];
            self.index = idx;
            self.shared.track_start.store(start, Ordering::Relaxed);
            // A new track starts from its own beginning, not a previous seek.
            self.shared.position_offset.store(0, Ordering::Relaxed);
        }
    }

    fn publish(&mut self) {
        let (state, index) = self.published;
        if self.index != index {
            let path = self.queue.get(self.index).cloned();
            self.emit(Event::TrackChanged {
                index: self.index,
                path,
            });
        }
        if self.state != state {
            self.emit(Event::StateChanged(self.state));
        }
        self.published = (self.state, self.index);
        let Ok(mut s) = self.status.lock() else {
            return;
        };
        s.state = self.state;
        // `queue` is written by `Player::send`; the engine's may be older.
        s.index = self.index;
        s.duration = self.marks.front().and_then(|(_, _, d)| *d);
        s.source = self.conv.as_ref().map(Converter::src);
        s.output_rate = self.out.as_ref().map(|o| o.plan.rate).unwrap_or(0);
        s.resampling = self.conv.as_ref().is_some_and(Converter::resampling);
        s.semitones = self.semitones;
        // `error` and `error_seq` are owned by `fail`; publish must not touch them.
    }
}
