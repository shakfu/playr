//! Playback engine.
//!
//! Three threads meet here. The caller (usually the UI) holds a [`Player`] and
//! sends [`Cmd`]s. An engine thread owns the decoder and fills a ring buffer.
//! The cpal callback drains that ring on the realtime thread and touches
//! nothing but atomics.

pub mod bus;
pub mod convert;
pub mod decode;
mod engine;
pub mod eq;
pub mod meter;
#[cfg(feature = "opus")]
pub mod opus;
pub mod order;
pub mod output;
pub mod record;
pub mod resample;

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use decode::AudioStream;
pub use decode::Spec;
use engine::Engine;

/// The file name alone, for messages that must fit on one line.
fn short(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
use crate::event::{Event, EventSink};
use crate::gain::{Gains, ReplayGain};
use convert::Converter;
pub use order::Mode;
use order::Order;
use output::{Backend, DeviceEvent, Output, Plan, Shared};

/// Furthest the playback speed may be shifted, in cents: 100 a semitone.
///
/// 1,200 is an octave, which is 0.5x and 2.0x. Past that a music track is no
/// longer recognisable, and the resampler is only built to span this range.
pub const MAX_CENTS: i32 = 1200;

/// Playback speed for a shift of `cents`.
///
/// Steps are geometric, so each is the same musical interval: 1,200 of them
/// double or halve the speed. Pitch moves with tempo, as it does on tape.
pub fn speed_at(cents: i32) -> f64 {
    2f64.powf(f64::from(cents.clamp(-MAX_CENTS, MAX_CENTS)) / 1200.0)
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

/// Marks the device has yet to reach, oldest first: the `frames_out` each
/// applies at, and the position offset from that frame on.
///
/// The callback advances `frames_out`; the engine advances `track_start` and
/// `position_offset` on its next pass, so between a loop's wrap and that pass
/// the three describe different instants and the position runs past the loop's
/// end. A reader applies the marks itself and gets the position being heard.
#[derive(Default)]
struct Pending(Mutex<Vec<(u64, u64)>>);

impl Pending {
    /// Replaces the marks with those still ahead of the device.
    fn set(&self, marks: impl Iterator<Item = (u64, u64)>) {
        if let Ok(mut held) = self.0.lock() {
            held.clear();
            held.extend(marks);
        }
    }

    /// `(track_start, position_offset)` at `frames_out`: the last mark the
    /// device has reached, or `current` while it has reached none.
    fn reached(&self, frames_out: u64, current: (u64, u64)) -> (u64, u64) {
        let Ok(held) = self.0.lock() else {
            return current;
        };
        held.iter()
            .take_while(|(at, _)| *at <= frames_out)
            .last()
            .copied()
            .unwrap_or(current)
    }

    /// The first mark after `frames_out`, where the position's terms change.
    fn next_after(&self, frames_out: u64) -> Option<u64> {
        let held = self.0.lock().ok()?;
        held.iter().map(|&(at, _)| at).find(|&at| at > frames_out)
    }
}

/// The position being heard: [`track_position`] with the marks the device has
/// reached applied, whether or not the engine has caught up with them.
fn position_now(shared: &Shared, pending: &Pending) -> Duration {
    let frames_out = shared.frames_out.load(Ordering::Relaxed);
    let (track_start, offset) = pending.reached(
        frames_out,
        (
            shared.track_start.load(Ordering::Relaxed),
            shared.position_offset.load(Ordering::Relaxed),
        ),
    );
    track_position(
        frames_out,
        track_start,
        offset,
        shared.position_rate.load(Ordering::Relaxed),
        shared.speed(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// What plays once the tracks queued by the listener have played.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AfterQueue {
    /// The listed track after the one the queue interrupted.
    #[default]
    Resume,
    Stop,
}

impl AfterQueue {
    /// Each choice's name in settings.
    pub const NAMES: [(&'static str, AfterQueue); 2] =
        [("resume", AfterQueue::Resume), ("stop", AfterQueue::Stop)];
}

#[derive(Debug, Clone)]
pub enum Cmd {
    /// Replace the queue and start at `index`.
    Play(Vec<PathBuf>, usize),
    /// As `Play`, marking which tracks were queued by the listener rather
    /// than listed; see [`Status::queued`].
    PlayWith(Vec<PathBuf>, Vec<bool>, usize),
    /// Append to the queue, starting playback if stopped.
    Enqueue(Vec<PathBuf>),
    /// Insert tracks queued by the listener at this index, starting playback
    /// there if stopped. A shuffle plays them next.
    Insert(usize, Vec<PathBuf>),
    /// Take the track at this index out of the queue. Taking out the track
    /// playing plays the one that would have followed it.
    Remove(usize),
    /// Move the queued track at the first index to the second.
    Move(usize, usize),
    /// Treat the track at this index as one of the list, not queued: it
    /// stays where it is, and what follows it follows as for the list.
    Unqueue(usize),
    /// Play the queued track at `index`, keeping the queue.
    Jump(usize),
    TogglePause,
    /// Play if paused, or start the current track if stopped; playing,
    /// nothing. A frontend that wants playback sends this, not a toggle
    /// chosen from a status the commands just before it may not show yet.
    Resume,
    Next,
    Prev,
    Stop,
    /// Seek to an absolute position in the current track.
    Seek(Duration),
    /// Seek forward (positive) or back (negative) by seconds.
    SeekBy(i64),
    SetVolume(f32),
    /// Cut or boost a band of the tone control, in dB. See [`eq`].
    SetEq(eq::Band, f32),
    /// Shift playback speed by this many cents, 100 a semitone; pitch moves
    /// with it.
    CentsBy(i32),
    /// Return to normal speed.
    SpeedReset,
    /// Set the speed shift to this many cents.
    SetCents(i32),
    /// Change how playback moves through the list. See [`Mode`].
    SetMode(Mode),
    /// What follows the last queued track. See [`AfterQueue`].
    SetAfterQueue(AfterQueue),
    /// Fade each track in over its first stretch this long and out over its
    /// last, or not at all for zero. Not on a loop or a one-shot.
    SetFade(Duration),
    /// Overlap each track's last stretch this long with the next one's first,
    /// or not at all for zero, where the track's length is known. A next
    /// track at another rate or channel count is converted to the stream's
    /// plan, to its end. It replaces `SetFade`'s fades at a join.
    SetCrossfade(Duration),
    /// Stop once the track playing ends, or no longer. Ignored while
    /// stopped, and cleared once playback stops. See [`Status::stop_after`].
    StopAfter(bool),
    /// Play source frames `start..end` of the current track over and over,
    /// or play on for `None`. A playhead outside them starts at `start`.
    Loop(Option<(u64, u64)>),
    /// Play source frames `start..end` of the current track once, then pause
    /// at `end`. The track is not torn down, so playing on continues past it.
    /// The last two are frames to fade in and out, as
    /// [`crate::samples::Edges::Fade`] writes a slice; 0 for none.
    PlayOnce(u64, u64, (u64, u64)),
    /// Choose which ReplayGain applies, from the audible position on.
    SetReplayGain(ReplayGain),
    /// Gains by path, from the library. A track not among them takes the
    /// gains in its tags. Tracks already started keep theirs.
    SetGains(Arc<HashMap<PathBuf, Gains>>),
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
    /// For each track in `queue`, whether the listener queued it, as against
    /// a track of the list played. Written with `queue`.
    pub queued: Arc<[bool]>,
    pub index: usize,
    pub duration: Option<Duration>,
    pub source: Option<Spec>,
    pub output_rate: u32,
    /// The device's sample format while a stream is open.
    pub output_format: Option<cpal::SampleFormat>,
    pub resampling: bool,
    /// A fade or a crossfade is set, so track edges are scaled.
    pub fading: bool,
    /// The most recent playback error, such as a file that could not be decoded.
    ///
    /// It is never cleared. The UI shows it when `error_seq` changes, which is
    /// what makes a skipped track visible: clearing the field instead would
    /// race with the UI's refresh and usually be missed.
    pub error: Option<String>,
    /// Increments on every error, so a repeat of the same message is still seen.
    pub error_seq: u64,
    /// Playback speed shift in cents, 100 a semitone; 0 is normal speed.
    pub cents: i32,
    /// How playback moves through the list. Like `queue`, only
    /// [`Player::send`] writes it, so it is current as soon as `send` returns.
    pub mode: Mode,
    /// The source frames looping in the current track, end exclusive.
    pub looping: Option<(u64, u64)>,
    /// The ReplayGain applied to the audible track, in dB; `None` while off.
    pub gain_db: Option<f32>,
    /// Whether playback stops once the track playing ends.
    pub stop_after: bool,
}

impl Status {
    pub fn current(&self) -> Option<&PathBuf> {
        self.queue.get(self.index)
    }
}

/// What the engine receives. A queue change carries the new queue itself, so
/// the engine plays exactly the list [`Status::queue`] shows.
enum Msg {
    Play(List, usize),
    Enqueue(List),
    /// The new queue, where the inserted tracks start in it, and how many.
    Insert(List, usize, usize),
    Remove(List, usize),
    Move(List, usize, usize),
    /// Which tracks are queued changed; the list did not.
    Relabel(List),
    Cmd(Cmd),
    /// A source to play beside the player, the id it is detached by, and
    /// the rate to open the stream at when none is open.
    Attach(u64, u32, Box<dyn bus::Source>),
    Detach(u64),
    /// The rate a source wanting this one must play at, answered on the sender.
    Rate(u32, Sender<u32>),
    /// At least this many output channels, where the device has them; the
    /// stream's channels after, answered on the sender.
    Channels(u16, Sender<u16>),
    /// Record the master to this file; answered with where the result comes.
    Record(PathBuf, Sender<Recording>),
    StopRecord,
}

/// The engine's answer to a recording asked for: where its result comes.
type Recording = Result<record::Done, String>;

/// A queue and which of its tracks the listener queued, as [`Status`] holds them.
type List = (Arc<[PathBuf]>, Arc<[bool]>);

/// Where the engine sends events, once a session gives it somewhere.
type Events = Arc<Mutex<Option<EventSink>>>;

/// Handle to the engine thread.
pub struct Player {
    tx: Sender<Msg>,
    status: Arc<Mutex<Status>>,
    shared: Arc<Shared>,
    pending: Arc<Pending>,
    events: Events,
    handle: Option<std::thread::JoinHandle<()>>,
    sources: Sources,
    /// The cpal device played to; `None` for any other backend.
    device: Option<output::DeviceInfo>,
}

/// Attaches sources to a player's output stream, from any thread. Cloned
/// freely; every clone reaches the same player.
#[derive(Clone)]
pub struct Sources {
    tx: Sender<Msg>,
    status: Arc<Mutex<Status>>,
    shared: Arc<Shared>,
    /// The id the last attached source took.
    ids: Arc<std::sync::atomic::AtomicU64>,
}

impl Sources {
    /// Plays `source` beside the player, summed into the same stream, until
    /// the attachment is dropped or [`Sources::detach`]. It must run at
    /// [`Sources::rate_for`]'s rate. A stream opens at `rate` if none is open.
    pub fn attach(&self, rate: u32, source: Box<dyn bus::Source>) -> Attachment {
        let id = self.ids.fetch_add(1, Ordering::Relaxed) + 1;
        let lost = self.lost();
        self.post(Msg::Attach(id, rate, source));
        Attachment {
            sources: self.clone(),
            id,
            lost,
        }
    }

    /// Stops playing the source attached as `id`, and drops it off the audio thread.
    pub fn detach(&self, id: u64) {
        self.post(Msg::Detach(id));
    }

    /// The rate a source that would play at `want` must be resampled to: the
    /// open stream's, which stays while sources play, or else the nearest
    /// the device takes. `want` if the engine does not answer.
    pub fn rate_for(&self, want: u32) -> u32 {
        let (tx, rx) = std::sync::mpsc::channel();
        self.post(Msg::Rate(want, tx));
        rx.recv_timeout(Duration::from_secs(2)).unwrap_or(want)
    }

    /// Reopens the stream with at least `n` channels, where the device has
    /// them, and returns its channels after; 0 with no stream open.
    pub fn widen(&self, n: u16) -> u16 {
        let (tx, rx) = std::sync::mpsc::channel();
        self.post(Msg::Channels(n, tx));
        rx.recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|_| self.channels())
    }

    /// Records the master to `path`, a 32-bit float stereo WAV at the
    /// stream's rate, opening a stream if none is open. One at a time.
    pub fn record(&self, path: &Path) -> Result<record::MasterRecording, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.post(Msg::Record(path.to_path_buf(), tx));
        let done = rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "the player did not answer".to_string())??;
        let sources = self.clone();
        Ok(record::MasterRecording {
            path: path.to_path_buf(),
            done,
            stop: Box::new(move || sources.post(Msg::StopRecord)),
        })
    }

    /// The largest magnitude of the master since the last call, which resets it.
    pub fn take_master_peak(&self) -> f32 {
        self.shared.take_master_peak()
    }

    /// The open stream's channels; 0 with none open.
    pub fn channels(&self) -> u16 {
        self.shared.out_channels.load(Ordering::Relaxed) as u16
    }

    /// How many times the device has been lost. Every source goes with it.
    pub fn lost(&self) -> u64 {
        self.shared.lost.load(Ordering::Relaxed)
    }

    /// Sends `msg`, counted as [`Player::send`] counts commands.
    fn post(&self, msg: Msg) {
        let Ok(_order) = self.status.lock() else {
            return;
        };
        self.shared.sent.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.send(msg);
    }
}

/// A source attached through [`Sources::attach`], detached when dropped.
pub struct Attachment {
    sources: Sources,
    id: u64,
    /// The device losses counted when it was attached.
    lost: u64,
}

impl Attachment {
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Whether the device has been lost since it was attached, which took
    /// the source with it.
    pub fn lost(&self) -> bool {
        self.sources.lost() > self.lost
    }
}

impl Drop for Attachment {
    fn drop(&mut self) {
        self.sources.detach(self.id);
    }
}

impl Player {
    /// Starts the engine thread on the output device `device` names, or the
    /// default for `None`. See [`output::device`].
    pub fn new(device: Option<&str>) -> Result<Self, output::OutputError> {
        if device == Some(output::NONE) {
            let mut player = Self::with_backend(output::Null)?;
            player.device = Some(output::DeviceInfo {
                id: output::NONE.into(),
                name: "no device; nothing is heard".into(),
                default: false,
            });
            return Ok(player);
        }
        let device = output::device(device)?;
        let info = output::info(&device);
        let mut player = Self::with_backend(output::Cpal(device))?;
        player.device = info;
        Ok(player)
    }

    /// Starts the engine thread, playing to `backend`.
    pub fn with_backend(backend: impl Backend) -> Result<Self, output::OutputError> {
        let (tx, rx) = std::sync::mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let shared = Arc::new(Shared::new());
        let pending: Arc<Pending> = Arc::default();
        let events: Events = Arc::default();

        let handle = {
            let (status, shared, pending, events) = (
                status.clone(),
                shared.clone(),
                pending.clone(),
                events.clone(),
            );
            std::thread::Builder::new()
                .name("playr-audio".into())
                .spawn(move || {
                    Engine::new(Box::new(backend), rx, status, shared, pending, events).run()
                })
                .map_err(|e| output::OutputError::Build(e.to_string()))?
        };

        let sources = Sources {
            tx: tx.clone(),
            status: status.clone(),
            shared: shared.clone(),
            ids: Arc::default(),
        };
        Ok(Player {
            tx,
            status,
            shared,
            pending,
            events,
            handle: Some(handle),
            sources,
            device: None,
        })
    }

    /// The largest magnitude of the master since the last call, which resets it.
    pub fn take_master_peak(&self) -> f32 {
        self.shared.take_master_peak()
    }

    /// The output device, as `playr devices` lists it.
    pub fn device(&self) -> Option<&output::DeviceInfo> {
        self.device.as_ref()
    }

    /// The handle that attaches sources to this player's stream.
    pub fn sources(&self) -> Sources {
        self.sources.clone()
    }

    /// Whether the engine has acted on every command sent and published the
    /// result, so [`Status::state`] is not one a command sent has yet to change.
    pub fn caught_up(&self) -> bool {
        self.shared.taken.load(Ordering::Acquire) >= self.shared.sent.load(Ordering::Relaxed)
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
        let list = |status: &Status| (status.queue.clone(), status.queued.clone());
        let msg = match cmd {
            Cmd::Play(paths, index) => {
                status.queued = vec![false; paths.len()].into();
                status.queue = paths.into();
                // Set here, so a command right after reads the track chosen.
                status.index = index;
                Msg::Play(list(&status), index)
            }
            Cmd::PlayWith(paths, queued, index) => {
                if queued.len() != paths.len() {
                    return;
                }
                status.queue = paths.into();
                status.queued = queued.into();
                status.index = index;
                Msg::Play(list(&status), index)
            }
            Cmd::Remove(i) => {
                if i >= status.queue.len() {
                    return;
                }
                let (mut queue, mut queued) = (status.queue.to_vec(), status.queued.to_vec());
                queue.remove(i);
                queued.remove(i);
                (status.queue, status.queued) = (queue.into(), queued.into());
                // Set here, so the playing row is right before the engine runs.
                if status.index > i {
                    status.index -= 1;
                }
                Msg::Remove(list(&status), i)
            }
            Cmd::Move(from, to) => {
                let len = status.queue.len();
                if from >= len || to >= len || from == to {
                    return;
                }
                let (mut queue, mut queued) = (status.queue.to_vec(), status.queued.to_vec());
                let (path, flag) = (queue.remove(from), queued.remove(from));
                queue.insert(to, path);
                queued.insert(to, flag);
                (status.queue, status.queued) = (queue.into(), queued.into());
                status.index = order::moved(status.index, from, to);
                Msg::Move(list(&status), from, to)
            }
            Cmd::Unqueue(i) => {
                if !status.queued.get(i).copied().unwrap_or(false) {
                    return;
                }
                let mut queued = status.queued.to_vec();
                queued[i] = false;
                status.queued = queued.into();
                Msg::Relabel(list(&status))
            }
            Cmd::Enqueue(paths) => {
                let n = paths.len();
                status.queue = status.queue.iter().cloned().chain(paths).collect();
                status.queued = status
                    .queued
                    .iter()
                    .copied()
                    .chain(vec![false; n])
                    .collect();
                Msg::Enqueue(list(&status))
            }
            Cmd::Insert(at, paths) => {
                let at = at.min(status.queue.len());
                let n = paths.len();
                let (mut queue, mut queued) = (status.queue.to_vec(), status.queued.to_vec());
                queue.splice(at..at, paths);
                queued.splice(at..at, vec![true; n]);
                (status.queue, status.queued) = (queue.into(), queued.into());
                Msg::Insert(list(&status), at, n)
            }
            // Set here, so the next `volume` call sees it even before the engine runs.
            Cmd::SetVolume(v) => return self.shared.set_volume(v),
            Cmd::SetEq(band, db) => return self.shared.eq.set(band, db),
            Cmd::SetMode(mode) => {
                status.mode = mode;
                Msg::Cmd(Cmd::SetMode(mode))
            }
            // Set here too, so a toggle sees it at once; the engine may shorten it.
            Cmd::Loop(bounds) => {
                status.looping = bounds;
                Msg::Cmd(Cmd::Loop(bounds))
            }
            cmd => Msg::Cmd(cmd),
        };
        // Under the lock, so the engine receives changes in the order they were made.
        self.shared.sent.fetch_add(1, Ordering::Relaxed);
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
        // Until the engine has reset them, the counters describe the audio
        // before a seek. Acquire pairs with the engine's release of each, so
        // a request seen has its target, and counters applied are the new ones.
        let requested = self.shared.flush_requested.load(Ordering::Acquire);
        if requested != self.shared.flush_applied.load(Ordering::Acquire) {
            return Duration::from_nanos(self.shared.seek_target.load(Ordering::Relaxed));
        }
        position_now(&self.shared, &self.pending)
    }

    /// Hands the output over to the source attached as `id` on the frame
    /// the device reaches `at` in the track playing: the player pauses on it
    /// and the source's [`Source::start`](bus::Source::start) runs before
    /// it. False, and nothing armed, while a seek is under way, or when `at`
    /// is behind the device or past a track change or loop already queued.
    /// A seek or a speed change cancels it. One at a time: this replaces any
    /// armed.
    pub fn hand_over(&self, id: u64, at: Duration) -> bool {
        let s = &self.shared;
        let requested = s.flush_requested.load(Ordering::Acquire);
        if requested != s.flush_applied.load(Ordering::Acquire) {
            return false;
        }
        let (rate, speed) = (s.position_rate.load(Ordering::Relaxed), s.speed());
        if rate == 0 || speed <= 0.0 {
            return false;
        }
        let frames_out = s.frames_out.load(Ordering::Relaxed);
        let (start, offset) = self.pending.reached(
            frames_out,
            (
                s.track_start.load(Ordering::Relaxed),
                s.position_offset.load(Ordering::Relaxed),
            ),
        );
        let into = (at.as_secs_f64() * f64::from(rate) - offset as f64) / speed;
        let frame = start + into.max(0.0).round() as u64;
        let changes = self.pending.next_after(frames_out);
        if into < 0.0 || frame <= frames_out || changes.is_some_and(|c| frame >= c) {
            return false;
        }
        s.handover_at.store(output::NO_HANDOVER, Ordering::Relaxed);
        s.handover_id.store(id, Ordering::Relaxed);
        s.handover
            .store(bus::Handover::Armed as u8, Ordering::Relaxed);
        s.handover_at.store(frame, Ordering::Release);
        true
    }

    /// Where the last [`Player::hand_over`] got to.
    pub fn handover(&self) -> bus::Handover {
        bus::Handover::from_u8(self.shared.handover.load(Ordering::Relaxed))
    }

    /// Drops an armed handover. One the callback has done stays done.
    pub fn cancel_handover(&self) {
        let s = &self.shared;
        if s.handover_at.swap(output::NO_HANDOVER, Ordering::Relaxed) != output::NO_HANDOVER {
            s.handover
                .store(bus::Handover::Cancelled as u8, Ordering::Relaxed);
        }
    }

    pub fn volume(&self) -> f32 {
        self.shared.volume()
    }

    /// The tone control's gains in dB, by [`eq::Band`].
    pub fn eq(&self) -> [f32; 3] {
        self.shared.eq.get()
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
