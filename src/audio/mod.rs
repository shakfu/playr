//! Playback engine.
//!
//! Three threads meet here. The caller (usually the UI) holds a [`Player`] and
//! sends [`Cmd`]s. An engine thread owns the decoder and fills a ring buffer.
//! The cpal callback drains that ring on the realtime thread and touches
//! nothing but atomics.

pub mod decode;
#[cfg(feature = "opus")]
pub mod opus;
pub mod output;
pub mod resample;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::Device;

use decode::AudioStream;
pub use decode::Spec;

/// The file name alone, for messages that must fit on one line.
fn short(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
use output::{Output, Plan, Shared};
use resample::Resample;

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
    Quit,
}

/// A snapshot of what the player is doing, for display.
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub state: State,
    /// Shared with the engine, so publishing a status does not copy the paths.
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
}

impl Status {
    pub fn current(&self) -> Option<&PathBuf> {
        self.queue.get(self.index)
    }
}

/// Handle to the engine thread.
pub struct Player {
    tx: Sender<Cmd>,
    status: Arc<Mutex<Status>>,
    shared: Arc<Shared>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Player {
    /// Starts the engine thread. Fails only if no output device exists.
    pub fn new() -> Result<Self, output::OutputError> {
        let device = output::default_device()?;
        let (tx, rx) = std::sync::mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let shared = Arc::new(Shared::new());

        let handle = {
            let status = status.clone();
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("playr-audio".into())
                .spawn(move || Engine::new(device, rx, status, shared).run())
                .map_err(|e| output::OutputError::Build(e.to_string()))?
        };

        Ok(Player {
            tx,
            status,
            shared,
            handle: Some(handle),
        })
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Audible position in the current track.
    pub fn position(&self) -> Duration {
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
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Quit);
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

/// A track queued behind the current one whose format needs a new stream.
struct Staged {
    stream: AudioStream,
    spec: Spec,
    plan: Plan,
    index: usize,
    first: Vec<f32>,
}

struct Engine {
    device: Device,
    rx: Receiver<Cmd>,
    status: Arc<Mutex<Status>>,
    shared: Arc<Shared>,
    /// Errors the device reports from its own thread, surfaced through `fail`.
    device_errors: (Sender<String>, Receiver<String>),

    out: Option<Output>,
    stream: Option<AudioStream>,
    resampler: Option<Resample>,
    src: Spec,

    queue: Arc<[PathBuf]>,
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
    /// Samples converted but not yet accepted by the ring.
    carry: Vec<f32>,
    scratch: Vec<f32>,
}

impl Engine {
    fn new(
        device: Device,
        rx: Receiver<Cmd>,
        status: Arc<Mutex<Status>>,
        shared: Arc<Shared>,
    ) -> Self {
        Engine {
            device,
            rx,
            status,
            shared,
            device_errors: std::sync::mpsc::channel(),
            out: None,
            stream: None,
            resampler: None,
            src: Spec {
                rate: 0,
                channels: 0,
            },
            queue: Arc::default(),
            index: 0,
            state: State::Stopped,
            semitones: 0,
            written: 0,
            marks: VecDeque::new(),
            staged: None,
            carry: Vec::new(),
            scratch: Vec::new(),
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
            match self.rx.recv_timeout(wait) {
                Ok(Cmd::Quit) => break,
                Ok(cmd) => {
                    self.handle(cmd);
                    while let Ok(next) = self.rx.try_recv() {
                        if matches!(next, Cmd::Quit) {
                            return;
                        }
                        self.handle(next);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }

            while let Ok(e) = self.device_errors.1.try_recv() {
                self.fail(format!("audio device: {e}"));
            }
            if self.state == State::Playing {
                self.pump();
            }
            self.advance_marks();
            self.publish();
        }
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Play(paths, index) => {
                self.queue = paths.into();
                self.index = index.min(self.queue.len().saturating_sub(1));
                self.teardown();
                if !self.queue.is_empty() {
                    self.start(self.index);
                }
            }
            Cmd::Enqueue(paths) => {
                let first_new = self.queue.len();
                self.queue = self.queue.iter().cloned().chain(paths).collect();
                if self.state == State::Stopped {
                    self.start(first_new);
                } else if self.stream.is_none() && self.staged.is_none() {
                    // The last track already decoded to its end, and `pump`
                    // stages nothing once the queue has run out.
                    self.stage_from(first_new);
                }
            }
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
                        self.start(self.index);
                    }
                }
            },
            Cmd::Next => {
                let next = self.index + 1;
                if next < self.queue.len() {
                    self.teardown();
                    self.index = next;
                    self.start(next);
                } else {
                    self.teardown();
                    self.state = State::Stopped;
                }
            }
            Cmd::Prev => {
                // Restart the track if we are past the start of it, which is
                // what a double-press of "previous" expects.
                let restart = self.elapsed() > Duration::from_secs(3) || self.index == 0;
                let target = if restart { self.index } else { self.index - 1 };
                self.teardown();
                self.index = target;
                self.start(target);
            }
            Cmd::Stop => {
                self.teardown();
                self.state = State::Stopped;
            }
            Cmd::Seek(pos) => self.seek(pos),
            Cmd::SeekBy(delta) => {
                let cur = self.elapsed().as_secs_f64();
                let target = (cur + delta as f64).max(0.0);
                self.seek(Duration::from_secs_f64(target));
            }
            Cmd::SetVolume(v) => self.shared.set_volume(v),
            Cmd::SpeedBy(delta) => self.set_semitones(self.semitones + delta),
            Cmd::SpeedReset => self.set_semitones(0),
            Cmd::Quit => {}
        }
    }

    /// Audible position within the current track.
    fn elapsed(&self) -> Duration {
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
        self.resampler = None;
        self.staged = None;
        self.carry.clear();
        self.marks.clear();
        self.written = 0;
        // Dropping the output drops the ring, discarding anything buffered.
        self.out = None;
        self.shared.frames_out.store(0, Ordering::Relaxed);
        self.shared.track_start.store(0, Ordering::Relaxed);
        // A new track starts from its own beginning, not a previous seek.
        self.shared.position_offset.store(0, Ordering::Relaxed);
    }

    /// Opens track `i`, or the first playable track after it, and (re)builds
    /// the output stream to match.
    ///
    /// Bad files are skipped in a loop, not by recursion: a run of thousands,
    /// such as an Opus library in a build without Opus, overflowed the stack.
    fn start(&mut self, mut i: usize) {
        let (stream, first, spec) = loop {
            let Some(path) = self.queue.get(i).cloned() else {
                self.state = State::Stopped;
                return;
            };
            match self.open_track(&path) {
                Some(opened) => break opened,
                // A bad file must not stall the queue.
                None => i += 1,
            }
        };

        let plan = match output::negotiate(&self.device, spec) {
            Ok(p) => p,
            Err(e) => {
                self.fail(e.to_string());
                self.state = State::Stopped;
                return;
            }
        };

        if let Err(e) = self.rebuild_output(plan) {
            self.fail(e.to_string());
            self.state = State::Stopped;
            return;
        }

        self.index = i;
        self.src = spec;
        self.resampler = self.make_resampler(spec, plan);
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
            &self.device,
            plan,
            self.shared.clone(),
            self.device_errors.0.clone(),
        )
    }

    fn fail(&mut self, msg: String) {
        if let Ok(mut s) = self.status.lock() {
            s.error = Some(msg);
            s.error_seq += 1;
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

    /// The resampler this track needs, if any.
    ///
    /// One is required whenever the device cannot take the source rate, and
    /// also at any speed other than normal, since varispeed is a change of
    /// resampling ratio.
    fn make_resampler(&self, spec: Spec, plan: Plan) -> Option<Resample> {
        if !plan.needs_resample(spec) && self.semitones == 0 {
            return None;
        }
        Resample::new(spec.rate, plan.rate, plan.channels, self.speed())
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
        if self.stream.is_some() {
            self.seek(at);
        }
    }

    fn seek(&mut self, pos: Duration) {
        let Some(stream) = self.stream.as_mut() else {
            return;
        };
        if stream.seek(pos).is_err() {
            return;
        }
        // Rebuilt rather than reused: it carries filter state from before the
        // seek, and its ratio may have changed with the playback speed.
        if let Some(plan) = self.out.as_ref().map(|o| o.plan) {
            self.resampler = self.make_resampler(self.src, plan);
        }
        self.carry.clear();
        self.staged = None;

        // Rebuild the ring so buffered pre-seek audio is not played.
        let plan = self.out.as_ref().map(|o| o.plan);
        if let Some(plan) = plan {
            self.out = None;
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

        let dur = self.stream.as_ref().and_then(|s| s.duration());
        let idx = self.index;
        self.written = 0;
        self.marks.clear();
        self.marks.push_back((0, idx, dur));
        // Playback resumes partway into the track while the device count
        // restarts at zero, so the difference is carried as an offset.
        let rate = self.shared.position_rate.load(Ordering::Relaxed) as f64;
        let offset = (pos.as_secs_f64() * rate).max(0.0) as u64;
        self.shared.frames_out.store(0, Ordering::Relaxed);
        self.shared.track_start.store(0, Ordering::Relaxed);
        self.shared.position_offset.store(offset, Ordering::Relaxed);
    }

    /// Converts decoded source samples to the output layout and appends to `carry`.
    fn convert_and_carry(&mut self, decoded: &[f32]) {
        let Some(out) = &self.out else { return };
        let dst_ch = out.plan.channels as usize;
        let src_ch = self.src.channels as usize;

        if let Some(r) = self.resampler.as_mut() {
            // Resample in the source channel count, then remap.
            self.scratch.clear();
            r.push(decoded, &mut self.scratch);
            let resampled = std::mem::take(&mut self.scratch);
            output::remap_channels(&resampled, src_ch, dst_ch, &mut self.carry);
            self.scratch = resampled;
        } else {
            output::remap_channels(decoded, src_ch, dst_ch, &mut self.carry);
        }
    }

    /// Decodes and pushes until the ring is comfortably full, or the time
    /// budget runs out and the run loop gets a turn.
    fn pump(&mut self) {
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
                    // Flush the resampler tail, then move to the next track.
                    if let Some(r) = self.resampler.as_mut() {
                        let mut tail = Vec::new();
                        r.flush(&mut tail);
                        if !tail.is_empty() {
                            let (src_ch, dst_ch) = (
                                self.src.channels as usize,
                                self.out
                                    .as_ref()
                                    .map(|o| o.plan.channels as usize)
                                    .unwrap_or(2),
                            );
                            output::remap_channels(&tail, src_ch, dst_ch, &mut self.carry);
                        }
                    }
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
    /// Bad files are skipped in a loop; see [`Engine::start`].
    fn stage_next(&mut self) {
        let next = self
            .marks
            .back()
            .map(|(_, i, _)| i + 1)
            .unwrap_or(self.index + 1);
        self.stage_from(next);
    }

    /// Stages the first playable track at or after `next`.
    fn stage_from(&mut self, mut next: usize) {
        let (stream, first, spec) = loop {
            let Some(path) = self.queue.get(next).cloned() else {
                return;
            };
            match self.open_track(&path) {
                Some(opened) => break opened,
                None => next += 1,
            }
        };
        let plan = match output::negotiate(&self.device, spec) {
            Ok(p) => p,
            Err(e) => {
                self.fail(e.to_string());
                return;
            }
        };
        let duration = stream.duration();

        if self.out.as_ref().map(|o| o.plan) == Some(plan) {
            // Same output format: continue seamlessly.
            self.src = spec;
            self.resampler = self.make_resampler(spec, plan);
            self.marks.push_back((self.written, next, duration));
            self.stream = Some(stream);
            self.convert_and_carry(&first);
        } else {
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
            self.state = State::Stopped;
            return;
        }
        self.src = staged.spec;
        self.resampler = self.make_resampler(staged.spec, staged.plan);
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
        let (written, _) = out.producer.push_partial_slice(&self.carry);
        let n = written.len();
        if n > 0 {
            self.carry.drain(..n);
            self.written += n as u64 / out.plan.channels as u64;
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

    fn publish(&self) {
        let Ok(mut s) = self.status.lock() else {
            return;
        };
        s.state = self.state;
        s.queue = self.queue.clone();
        s.index = self.index;
        s.duration = self.marks.front().and_then(|(_, _, d)| *d);
        s.source = if self.src.rate == 0 {
            None
        } else {
            Some(self.src)
        };
        s.output_rate = self.out.as_ref().map(|o| o.plan.rate).unwrap_or(0);
        s.resampling = self.resampler.is_some();
        s.semitones = self.semitones;
        // `error` and `error_seq` are owned by `fail`; publish must not touch them.
    }
}
