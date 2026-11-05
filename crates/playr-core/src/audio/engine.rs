//! The engine thread: owns the decoder, fills the ring buffer the device
//! drains, and carries gapless joins, crossfades, loops and seeks.

use super::*;

mod blend;

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

/// Audio the ring holds before a device starts on it after a seek or on a new
/// stream. Decoding it takes milliseconds, so it delays the start by little,
/// and it covers the engine waking late for the next push.
const PRIME: Duration = Duration::from_millis(100);

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

/// A stretch of one track, in its source frames, played over and over or,
/// when `once`, played through and paused at its end.
#[derive(Clone, Copy)]
struct Loop {
    index: usize,
    start: u64,
    end: u64,
    once: bool,
    /// Frames a one-shot fades in and out over.
    fades: (u64, u64),
}

/// The next track, decoding early into the current one's last frames, which
/// it is mixed with. See [`Cmd::SetCrossfade`].
struct Incoming {
    stream: AudioStream,
    index: usize,
    duration: Option<Duration>,
    /// Decoded and not yet mixed: source samples, or with `conv`, output ones.
    buf: Vec<f32>,
    /// Its own conversion to the stream's plan, for a track whose rate or
    /// channels differ from the current one's. The two mix after conversion,
    /// and it carries on converting the track once it is the current one.
    conv: Option<Converter>,
    /// Source frames of it mixed or carried so far; with `conv`, converted.
    decoded: u64,
    /// The stream has ended.
    done: bool,
    /// The current track's frame where the overlap starts, and its length.
    start: u64,
    frames: u64,
    /// The mark at the overlap's start is placed.
    marked: bool,
}

/// A track queued behind the current one whose format needs a new stream.
struct Staged {
    stream: AudioStream,
    spec: Spec,
    plan: Plan,
    index: usize,
    first: Vec<f32>,
}

pub(super) struct Engine {
    backend: Box<dyn Backend>,
    rx: Receiver<Msg>,
    status: Arc<Mutex<Status>>,
    shared: Arc<Shared>,
    /// `marks` past the first, for readers. See [`Pending`].
    pending: Arc<Pending>,
    events: Events,
    /// The state and index last published, to send an event when either changes.
    published: (State, usize),
    /// Events the device reports from its own thread.
    device_events: (Sender<DeviceEvent>, Receiver<DeviceEvent>),

    /// Commands taken off the channel while searching for a playable track,
    /// handled before anything newer.
    deferred: VecDeque<Msg>,

    out: Option<Output>,
    /// Sources attached and waiting for a stream, or between two.
    waiting: Vec<bus::Attached>,
    /// The ids attached and not since detached.
    live: std::collections::HashSet<u64>,
    /// Commands for the stream's bus that found its ring full.
    bus_queue: VecDeque<bus::BusCmd>,
    /// Where a closed stream's bus sends its sources.
    home: (Sender<Vec<bus::Attached>>, Receiver<Vec<bus::Attached>>),
    /// Raised while the master's recording writer runs.
    recording: Arc<std::sync::atomic::AtomicBool>,
    /// Whether the stream was held for more than the player at the last turn.
    held: bool,
    stream: Option<AudioStream>,
    conv: Option<Converter>,

    queue: Arc<[PathBuf]>,
    /// Which tracks of `queue` the listener queued.
    queued: Arc<[bool]>,
    after_queue: AfterQueue,
    /// How long each track fades in and out; zero for none.
    fade: Duration,
    /// How long consecutive tracks overlap; zero for none.
    crossfade: Duration,
    incoming: Option<Incoming>,
    /// `stream` came in under a crossfade, so `fade` does not fade it in.
    crossfaded_in: bool,
    /// The queue index a crossfade was tried for, so it is tried once.
    crossfade_tried: Option<usize>,
    /// Stop once the audible track ends. See [`Cmd::StopAfter`].
    stop_after: bool,
    /// Which track follows which, for the current mode.
    order: Order,
    index: usize,
    state: State,
    /// Playback speed shift in cents; 0 is normal speed.
    cents: i32,

    /// Total frames written into the ring since the stream was opened.
    written: u64,
    /// `(output frame at which it starts, queue index, duration, position
    /// offset)` per stretch of audio: a track, or a loop's return to its start,
    /// whose offset is that start in output frames.
    marks: VecDeque<(u64, usize, Option<Duration>, u64)>,
    /// The source frame `stream` decodes next.
    decoded: u64,
    looping: Option<Loop>,
    /// The output frame to pause at, set when a one-shot range is decoded.
    pause_at: Option<u64>,
    /// Next track, held back because it needs a different output format.
    staged: Option<Staged>,
    /// A seek whose pre-seek audio the device has not yet discarded.
    flush: Option<Flush>,
    /// Samples converted but not yet accepted by the ring.
    carry: Vec<f32>,

    replaygain: ReplayGain,
    gains: Arc<HashMap<PathBuf, Gains>>,
    /// The factor each queue index plays at, once worked out, kept until the
    /// queue or the ReplayGain setting changes. A mode change or new gains
    /// therefore never step a playing track's level.
    gain_of: HashMap<usize, f32>,
    /// Decoded samples with the gain applied, reused across chunks.
    scaled: Vec<f32>,
}

impl Engine {
    pub(super) fn new(
        backend: Box<dyn Backend>,
        rx: Receiver<Msg>,
        status: Arc<Mutex<Status>>,
        shared: Arc<Shared>,
        pending: Arc<Pending>,
        events: Events,
    ) -> Self {
        Engine {
            backend,
            rx,
            status,
            shared,
            pending,
            events,
            published: (State::Stopped, 0),
            device_events: std::sync::mpsc::channel(),
            deferred: VecDeque::new(),
            out: None,
            waiting: Vec::new(),
            live: Default::default(),
            bus_queue: VecDeque::new(),
            home: std::sync::mpsc::channel(),
            recording: Arc::default(),
            held: false,
            stream: None,
            conv: None,
            queue: Arc::default(),
            queued: Arc::default(),
            after_queue: AfterQueue::default(),
            fade: Duration::ZERO,
            crossfade: Duration::ZERO,
            incoming: None,
            crossfaded_in: false,
            crossfade_tried: None,
            stop_after: false,
            order: Order::new(0, Mode::Normal, 0, order::random_seed()),
            index: 0,
            state: State::Stopped,
            cents: 0,
            written: 0,
            marks: VecDeque::new(),
            decoded: 0,
            looping: None,
            pause_at: None,
            staged: None,
            flush: None,
            carry: Vec::new(),
            replaygain: ReplayGain::Off,
            gains: Arc::default(),
            gain_of: HashMap::new(),
            scaled: Vec::new(),
        }
    }

    pub(super) fn run(mut self) {
        // Commands acted on, published once their result is.
        let mut taken = 0;
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
                    taken += 1;
                    while let Some(next) = self
                        .deferred
                        .pop_front()
                        .or_else(|| self.rx.try_recv().ok())
                    {
                        if matches!(next, Msg::Cmd(Cmd::Quit)) {
                            return;
                        }
                        self.handle(next);
                        taken += 1;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }

            while let Ok(event) = self.device_events.1.try_recv() {
                self.device_event(event);
            }
            self.feed_bus();
            // A recording that ended, as a detach does, can leave nothing on it.
            let held = self.holds_bus();
            if self.held && !held {
                self.close_idle_bus();
            }
            self.held = held;
            self.poll_flush();
            if self.state == State::Playing {
                self.pump();
                self.end_priming();
            }
            self.advance_marks();
            self.pause_when_reached();
            self.pause_when_handed();
            self.publish(taken);
            self.shared.taken.store(taken, Ordering::Release);
        }
    }

    fn handle(&mut self, msg: Msg) {
        let cmd = match msg {
            Msg::Play((queue, queued), index) => {
                (self.queue, self.queued) = (queue, queued);
                self.gain_of.clear();
                return self.jump(index);
            }
            Msg::Enqueue((queue, queued)) => {
                let first_new = self.queue.len();
                (self.queue, self.queued) = (queue, queued);
                return self.enqueued(first_new);
            }
            Msg::Insert((queue, queued), at, n) => {
                (self.queue, self.queued) = (queue, queued);
                return self.inserted(at, n);
            }
            Msg::Remove((queue, queued), i) => {
                (self.queue, self.queued) = (queue, queued);
                return self.removed(i);
            }
            // The next track chosen may have been chosen as the queue's end.
            Msg::Relabel((queue, queued)) => {
                (self.queue, self.queued) = (queue, queued);
                return self.discard_chosen();
            }
            Msg::Move((queue, queued), from, to) => {
                (self.queue, self.queued) = (queue, queued);
                self.renumber(|j| order::moved(j, from, to));
                self.order.relabel(|j| order::moved(j, from, to));
                return self.discard_chosen();
            }
            Msg::Cmd(cmd) => cmd,
            Msg::Attach(id, rate, source) => {
                self.live.insert(id);
                self.waiting.push((id, source));
                if self.out.is_none() {
                    self.open_bus(rate);
                }
                return self.feed_bus();
            }
            Msg::Detach(id) => {
                self.live.remove(&id);
                self.waiting.retain(|(s, _)| *s != id);
                self.bus_queue.push_back(bus::BusCmd::Detach(id));
                self.feed_bus();
                return self.close_idle_bus();
            }
            Msg::Rate(want, answer) => {
                let rate = match &self.out {
                    Some(out) => out.plan.rate,
                    None => self
                        .backend
                        .negotiate(Spec {
                            rate: want,
                            channels: 2,
                        })
                        .map_or(want, |p| p.rate),
                };
                let _ = answer.send(rate);
                return;
            }
            Msg::Record(path, answer) => {
                let _ = answer.send(self.record(&path));
                return;
            }
            Msg::StopRecord => {
                self.bus_queue.push_back(bus::BusCmd::StopRecord);
                return self.feed_bus();
            }
            Msg::Channels(n, answer) => {
                self.widen(n);
                let _ = answer.send(self.out.as_ref().map_or(0, |o| o.plan.channels));
                return;
            }
        };
        match cmd {
            // `Player::send` delivers these as their own `Msg`.
            Cmd::Play(..)
            | Cmd::PlayWith(..)
            | Cmd::Enqueue(..)
            | Cmd::Insert(..)
            | Cmd::Remove(..)
            | Cmd::Move(..)
            | Cmd::Unqueue(_) => {}
            Cmd::Jump(index) => self.jump(index),
            Cmd::TogglePause if self.state == State::Playing => {
                if let Some(o) = &self.out {
                    o.pause();
                }
                self.state = State::Paused;
            }
            Cmd::TogglePause | Cmd::Resume => match self.state {
                State::Playing => {}
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
            Cmd::Next => match self.following(self.index, true) {
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
            Cmd::SetVolume(_) | Cmd::SetEq(..) => {}
            Cmd::CentsBy(delta) => self.set_cents(self.cents.saturating_add(delta)),
            Cmd::SpeedReset => self.set_cents(0),
            Cmd::SetCents(cents) => self.set_cents(cents),
            Cmd::SetMode(mode) => self.set_mode(mode),
            Cmd::SetAfterQueue(after) => {
                self.after_queue = after;
                self.discard_chosen();
            }
            // From what is decoded next: the ring's 2 s play as they were.
            Cmd::SetFade(fade) => self.fade = fade,
            Cmd::SetCrossfade(crossfade) => {
                self.crossfade = crossfade;
                // A crossfade under way, or a join already decoded, changes.
                self.discard_chosen();
            }
            Cmd::StopAfter(on) => {
                self.stop_after = on && self.state != State::Stopped;
                // As a mode change: drops or restores the next track chosen.
                self.discard_chosen();
            }
            Cmd::Loop(bounds) => self.set_loop(bounds, false, (0, 0)),
            Cmd::PlayOnce(start, end, fades) => {
                self.set_loop(Some((start, end)), true, fades);
                // Auditioning from a pause plays: the point is to hear it.
                if self.looping.is_some() && self.state != State::Playing {
                    if let Some(o) = &self.out {
                        o.play();
                    }
                    self.state = State::Playing;
                }
            }
            Cmd::SetReplayGain(replaygain) => {
                if replaygain != self.replaygain {
                    self.replaygain = replaygain;
                    self.gain_of.clear();
                    // As a speed change: the ring holds audio at the old gain.
                    if !self.marks.is_empty() {
                        self.seek(self.elapsed());
                    }
                }
            }
            Cmd::SetGains(gains) => self.gains = gains,
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
                // The sources played on the lost device, and go with it.
                self.live.clear();
                self.waiting.clear();
                self.bus_queue.clear();
                self.shared.lost.fetch_add(1, Ordering::Relaxed);
                // Whatever held it, the stream is gone; a recording ends with it.
                self.out = None;
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

    /// The track to play after `i`, as the order has it, unless `i` is the
    /// last queued track and the queue is set to stop there, or playback is
    /// set to stop after it. A skip is never stopped.
    fn following(&mut self, i: usize, skipping: bool) -> Option<usize> {
        if self.stop_after && !skipping {
            return None;
        }
        let next = self.order.successor(i, skipping)?;
        let queued = |j: usize| self.queued.get(j).copied().unwrap_or(false);
        let ends_queue = queued(i) && !queued(next);
        (!(ends_queue && self.after_queue == AfterQueue::Stop)).then_some(next)
    }

    /// Takes in `n` tracks inserted at `at`, to play after the audible one.
    ///
    /// Every index from `at` on moves up by `n`. A next track already chosen
    /// is discarded by seeking to the current position, as a mode change does.
    fn inserted(&mut self, at: usize, n: usize) {
        self.renumber(|i| if i >= at { i + n } else { i });
        let current = self.marks.front().map_or(self.index, |m| m.1);
        self.order.insert(at, n, current);
        if self.state == State::Stopped {
            self.start(at, false);
            return;
        }
        self.discard_chosen();
    }

    /// Takes out the track at `i`; every index after it moves down one.
    /// Taking out the audible track plays the one that would have followed.
    fn removed(&mut self, i: usize) {
        let audible = self.marks.front().map_or(self.index, |m| m.1);
        let next = match self.state {
            State::Stopped => None,
            _ if audible != i => None,
            _ => Some(self.following(i, true)),
        };
        // Checked before renumbering, which gives the next track index `i`.
        let chosen_gone = self.marks.iter().skip(1).any(|m| m.1 == i)
            || self.staged.as_ref().is_some_and(|s| s.index == i);
        let shift = |j: usize| if j > i { j - 1 } else { j };
        self.gain_of.remove(&i);
        self.renumber(shift);
        self.order.remove(i);
        match next {
            Some(next) => {
                self.teardown();
                match next.filter(|&n| n != i).map(shift) {
                    Some(n) => {
                        self.index = n;
                        self.start(n, false);
                    }
                    None => self.state = State::Stopped,
                }
            }
            None if chosen_gone => self.discard_chosen(),
            None => {}
        }
        self.index = self.index.min(self.queue.len().saturating_sub(1));
    }

    /// Renames queue indices by `map` wherever the engine holds one, after
    /// the queue changed under them.
    fn renumber(&mut self, map: impl Fn(usize) -> usize) {
        self.gain_of = self.gain_of.drain().map(|(i, g)| (map(i), g)).collect();
        for mark in &mut self.marks {
            mark.1 = map(mark.1);
        }
        self.index = map(self.index);
        self.published.1 = map(self.published.1);
        if let Some(l) = &mut self.looping {
            l.index = map(l.index);
        }
        if let Some(s) = &mut self.staged {
            s.index = map(s.index);
        }
        if let Some(i) = &mut self.incoming {
            i.index = map(i.index);
        }
    }

    /// Drops a next track already chosen, whether decoding into the ring or
    /// staged, by seeking to the current position, as a mode change does.
    fn discard_chosen(&mut self) {
        let chosen = self.marks.len() > 1
            || self.staged.is_some()
            || self.incoming.is_some()
            || self.stream.is_none();
        if chosen && !self.marks.is_empty() {
            self.seek(self.elapsed());
        }
    }

    /// Audible position within the current track.
    fn elapsed(&self) -> Duration {
        // Until the device discards, the counters still describe the audio
        // before the seek; a second quick seek must start from the first.
        if let Some(flush) = self.flush {
            return flush.at;
        }
        position_now(&self.shared, &self.pending)
    }

    fn teardown(&mut self) {
        self.looping = None;
        self.pause_at = None;
        self.stream = None;
        self.drop_incoming();
        self.conv = None;
        self.staged = None;
        self.flush = None;
        self.carry.clear();
        self.marks.clear();
        self.publish_marks();
        self.written = 0;
        if self.holds_bus() && self.out.is_some() {
            // The sources play on: the device discards the player's audio, as
            // for a seek, and nothing is pushed until it has.
            self.shared.priming.store(true, Ordering::Relaxed);
            let generation = self.shared.flush_requested.fetch_add(1, Ordering::Release) + 1;
            self.flush = Some(Flush {
                generation,
                deadline: std::time::Instant::now() + FLUSH_TIMEOUT,
                at: Duration::ZERO,
                offset: 0,
            });
        } else {
            // Dropping the output drops the ring, discarding anything buffered.
            self.out = None;
            // A flush still pending would discard the next stream's first audio.
            self.settle_flush();
        }
        self.shared.frames_out.store(0, Ordering::Relaxed);
        self.shared.track_start.store(0, Ordering::Relaxed);
        // A new track starts from its own beginning, not a previous seek.
        self.shared.position_offset.store(0, Ordering::Relaxed);
        self.settle_position();
    }

    /// Opens track `i`, or the first playable track after it (before it, when
    /// `back` is set), and (re)builds the output stream to match.
    ///
    /// Returns false if no playable track was found.
    fn start(&mut self, i: usize, back: bool) -> bool {
        self.load(i, back, true)
    }

    /// As [`Self::start`], but left paused at its first frame when not `play`.
    fn load(&mut self, i: usize, back: bool, play: bool) -> bool {
        let Some((i, stream, first, spec)) = self.open_from(i, back) else {
            self.state = State::Stopped;
            return false;
        };

        let plan = match self.plan_for(spec) {
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
        self.crossfaded_in = false;
        self.decoded = (first.len() / spec.channels.max(1) as usize) as u64;
        self.reset_marks(i, duration);
        self.shared.track_start.store(0, Ordering::Relaxed);
        // A new track starts from its own beginning, not a previous seek.
        self.shared.position_offset.store(0, Ordering::Relaxed);

        // Paused before any audio reaches the ring, so a cued track is silent.
        if let Some(o) = &self.out {
            if !play {
                o.pause();
            }
        }
        self.carry_faded(&first, 0);
        if play {
            self.state = State::Playing;
            if let Some(o) = &self.out {
                o.play();
            }
        } else {
            self.state = State::Paused;
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
                self.following(i, true)?
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
        self.shared.priming.store(true, Ordering::Relaxed);
        self.shared
            .out_channels
            .store(u32::from(plan.channels), Ordering::Relaxed);
        Output::open(
            self.backend.as_ref(),
            plan,
            self.shared.clone(),
            self.device_events.0.clone(),
            self.home.0.clone(),
        )
    }

    /// Hands waiting sources to the open stream's bus, sources a closed
    /// stream sent home back to the waiting, and drops what the bus let go.
    /// A source detached while between streams is dropped as it arrives.
    fn feed_bus(&mut self) {
        while let Ok(sources) = self.home.1.try_recv() {
            let live = &self.live;
            self.waiting
                .extend(sources.into_iter().filter(|(id, _)| live.contains(id)));
        }
        let Some(out) = &mut self.out else {
            return;
        };
        while out.bus.take_gone().is_some() {}
        self.bus_queue
            .extend(self.waiting.drain(..).map(bus::BusCmd::Attach));
        while let Some(cmd) = self.bus_queue.pop_front() {
            if let Err(cmd) = out.bus.send(cmd) {
                self.bus_queue.push_front(cmd);
                break;
            }
        }
    }

    /// Opens a stream for the sources, at `rate` where the device takes it.
    fn open_bus(&mut self, rate: u32) {
        let spec = Spec { rate, channels: 2 };
        match self
            .backend
            .negotiate(spec)
            .and_then(|p| self.rebuild_output(p))
        {
            Ok(()) => {}
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// The output plan for a track of `spec`: the open stream's while the
    /// bus is held, else the device's choice.
    fn plan_for(&self, spec: Spec) -> Result<Plan, output::OutputError> {
        match self.out.as_ref().filter(|_| self.holds_bus()) {
            Some(o) => Ok(o.plan),
            None => self.backend.negotiate(spec),
        }
    }

    /// Whether the stream is held for something besides the player: a
    /// source, or the master's recording. Its rate stays while it is.
    fn holds_bus(&self) -> bool {
        !self.live.is_empty() || self.recording.load(Ordering::Relaxed)
    }

    /// Starts recording the master to `path`, opening a stream if none is open.
    fn record(&mut self, path: &Path) -> Recording {
        if self.recording.load(Ordering::Relaxed) {
            return Err("already recording the master".into());
        }
        if self.out.is_none() {
            self.open_bus(48_000);
        }
        let rate = self
            .out
            .as_ref()
            .map(|o| o.plan.rate)
            .ok_or("no output to record")?;
        let (ring, done) = record::start(
            path,
            rate,
            self.recording.clone(),
            self.shared.rec_dropped.clone(),
        )?;
        self.bus_queue.push_back(bus::BusCmd::Record(ring));
        self.feed_bus();
        Ok(done)
    }

    /// Closes the stream once nothing plays on it: no source, no track.
    fn close_idle_bus(&mut self) {
        if !self.holds_bus() && self.stream.is_none() && self.state == State::Stopped {
            self.out = None;
            self.flush = None;
            self.settle_flush();
            self.shared.out_channels.store(0, Ordering::Relaxed);
        }
    }

    /// Reopens the stream with at least `n` channels, where the device has
    /// them, and plays the track on from where it was.
    fn widen(&mut self, n: u16) {
        let Some(plan) = self.out.as_ref().map(|o| o.plan) else {
            return;
        };
        if plan.channels >= n {
            return;
        }
        let wider = match self.backend.negotiate(Spec {
            rate: plan.rate,
            channels: n,
        }) {
            Ok(p) if p.rate == plan.rate && p.channels >= n => p,
            _ => return,
        };
        let at = self.elapsed();
        let playing = self.stream.is_some();
        self.out = None;
        self.flush = None;
        self.settle_flush();
        match self.open_output(wider) {
            Ok(o) => {
                if self.state == State::Playing {
                    o.play();
                }
                self.out = Some(o);
                self.shared
                    .position_rate
                    .store(wider.rate, Ordering::Relaxed);
            }
            Err(e) => {
                self.fail(e.to_string());
                self.teardown();
                self.state = State::Stopped;
                return;
            }
        }
        if playing {
            self.seek(at);
        }
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
        speed_at(self.cents)
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
        let current = self.marks.front().map_or(self.index, |m| m.1);
        self.order.set_mode(mode, current);
        let chosen = self.marks.len() > 1
            || self.staged.is_some()
            || self.incoming.is_some()
            || self.stream.is_none();
        if chosen && !self.marks.is_empty() {
            self.seek(self.elapsed());
        }
    }

    /// Applies a speed change by re-seeking to the current position.
    ///
    /// The ring already holds up to two seconds resampled at the old ratio.
    /// Changing the ratio alone would leave that to play out first, so the key
    /// would appear dead for a second or two; re-seeking makes it immediate.
    fn set_cents(&mut self, cents: i32) {
        let want = cents.clamp(-MAX_CENTS, MAX_CENTS);
        if want == self.cents {
            return;
        }
        let at = self.elapsed();
        self.cents = want;
        self.shared.set_speed(self.speed());
        if !self.marks.is_empty() {
            self.seek(at);
        }
    }

    /// Seeks where the listener asked. The decoder refuses a target at or past
    /// the end, so that moves on as reaching the end would.
    fn seek_to(&mut self, pos: Duration) {
        // Stopped, the track is closed: a seek cues it, paused, so a point can
        // be placed and marked before playing.
        if self.state == State::Stopped && !self.queue.is_empty() {
            self.load(self.index, false, false);
        }
        match self.marks.front() {
            Some(&(_, audible, Some(duration), _)) if pos >= duration => self.played_out(audible),
            _ => self.seek(pos),
        }
    }

    /// Moves on from track `current` as its end does: to the next track in
    /// play order, still paused if it was, or stops when there is none.
    fn played_out(&mut self, current: usize) {
        let paused = self.state == State::Paused;
        self.teardown();
        match self.following(current, false) {
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
        let Some(&(_, audible, _, _)) = self.marks.front() else {
            return;
        };
        // The discard resets `written`, which the pause is counted in, and a
        // seek leaves the range behind in any case.
        self.pause_at = None;
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
        self.decoded = (pos.as_secs_f64() * src.rate as f64).round() as u64;
        // Rebuilt rather than reused: it carries filter state from before the
        // seek, and its ratio may have changed with the playback speed.
        if let Some(plan) = self.out.as_ref().map(|o| o.plan) {
            self.conv = Some(Converter::new(src, plan, self.speed()));
        }
        self.carry.clear();
        self.staged = None;
        self.drop_incoming();
        self.crossfaded_in = false;

        let dur = self.stream.as_ref().and_then(|s| s.duration());
        let idx = self.index;
        self.written = 0;
        self.reset_marks(idx, dur);

        // The device discards the pre-seek audio in its callback; nothing is
        // pushed until it has. Reopening the device instead cost a gap and
        // could click. Positions are reset once the discard is done.
        let rate = self.shared.position_rate.load(Ordering::Relaxed) as f64;
        // Before the request, so a reader that sees the request sees this target.
        self.shared.seek_target.store(
            pos.as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
        self.shared.priming.store(true, Ordering::Relaxed);
        let generation = self.shared.flush_requested.fetch_add(1, Ordering::Release) + 1;
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
        self.settle_position();
    }

    /// Marks every flush done, once no callback can act on the ring it named.
    fn settle_flush(&self) {
        let requested = self.shared.flush_requested.load(Ordering::Relaxed);
        self.shared.flush_done.store(requested, Ordering::Relaxed);
    }

    /// Marks the position counters current, so readers stop reporting the
    /// seek target. Called once they describe the audio now playing.
    fn settle_position(&self) {
        let requested = self.shared.flush_requested.load(Ordering::Relaxed);
        // Release: the counters stored before this are seen with it.
        self.shared
            .flush_applied
            .store(requested, Ordering::Release);
    }

    /// The factor queue index `index` plays at: its ReplayGain, from the
    /// library or else its tags, or 1 when off or unknown.
    fn gain_at(&mut self, index: usize) -> f32 {
        if let Some(&g) = self.gain_of.get(&index) {
            return g;
        }
        let Some(album) = self.replaygain.album(self.order.mode()) else {
            return 1.0;
        };
        let Some(path) = self.queue.get(index) else {
            return 1.0;
        };
        let gains = match self.gains.get(path) {
            Some(g) => *g,
            None => Gains::from_tags(path),
        };
        let g = gains.pick(album).map_or(1.0, |g| g.linear());
        self.gain_of.insert(index, g);
        g
    }

    /// Converts decoded source samples to the output layout and appends to
    /// `carry`, at the gain of the track being decoded.
    ///
    /// The gain is applied here, not in the callback: the ring spans track
    /// boundaries, so the callback would change it up to 2 s off the boundary.
    fn convert_and_carry(&mut self, decoded: &[f32]) {
        let decoding = self.marks.back().map_or(self.index, |m| m.1);
        let gain = self.gain_at(decoding);
        let Some(conv) = self.conv.as_mut() else {
            return;
        };
        // Exactly 1 leaves the samples untouched, bit for bit.
        if gain == 1.0 {
            conv.push(decoded, &mut self.carry);
        } else {
            self.scaled.clear();
            self.scaled.extend(decoded.iter().map(|x| x * gain));
            conv.push(&self.scaled, &mut self.carry);
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

            match stream.next_chunk().map(|c| c.map(<[f32]>::to_vec)) {
                Ok(Some(chunk)) => self.take_chunk(&chunk),
                // A loop running past the track's end returns from there,
                // unless it has just returned and found nothing to play.
                Ok(None) if self.loop_here().is_some_and(|l| self.decoded != l.start) => {
                    let l = self.loop_here().expect("checked");
                    self.wrap(l);
                }
                Ok(None) if self.incoming.is_some() => self.promote_incoming(),
                Ok(None) => {
                    // `stage_from` ends the conversion, unless the next track continues it.
                    self.stream = None;
                    self.stage_next();
                    if self.staged.is_some() {
                        return;
                    }
                }
                Err(e) => {
                    let playing = self.marks.back().map(|m| m.1).unwrap_or(self.index);
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
        let last = self.marks.back().map_or(self.index, |m| m.1);
        match self.following(last, false) {
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
        let plan = match self.plan_for(spec) {
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
            self.push_mark(self.written, next, duration, 0);
            self.stream = Some(stream);
            self.crossfaded_in = false;
            self.decoded = (first.len() / spec.channels.max(1) as usize) as u64;
            self.carry_faded(&first, 0);
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
        // A source attached since staging holds the stream: play on in it.
        let plan = match self.out.as_ref().filter(|_| self.holds_bus()) {
            Some(o) => o.plan,
            None => staged.plan,
        };
        let duration = staged.stream.duration();
        if self.out.as_ref().map(|o| o.plan) == Some(plan) {
            self.conv = Some(Converter::new(staged.spec, plan, self.speed()));
            self.push_mark(self.written, staged.index, duration, 0);
            self.stream = Some(staged.stream);
            self.crossfaded_in = false;
            self.decoded = (staged.first.len() / staged.spec.channels.max(1) as usize) as u64;
            self.carry_faded(&staged.first, 0);
            return;
        }
        if let Err(e) = self.rebuild_output(plan) {
            self.fail(e.to_string());
            // Otherwise the previous track's state stays, and play restarts it.
            self.teardown();
            self.index = staged.index;
            self.state = State::Stopped;
            return;
        }
        self.conv = Some(Converter::new(staged.spec, plan, self.speed()));
        self.index = staged.index;
        self.stream = Some(staged.stream);
        self.crossfaded_in = false;
        self.decoded = (staged.first.len() / staged.spec.channels.max(1) as usize) as u64;
        self.written = 0;
        self.reset_marks(staged.index, duration);
        self.shared.track_start.store(0, Ordering::Relaxed);
        // A new track starts from its own beginning, not a previous seek.
        self.shared.position_offset.store(0, Ordering::Relaxed);
        self.carry_faded(&staged.first, 0);
        if let Some(o) = &self.out {
            o.play();
        }
    }

    /// Converts a decoded chunk into `carry`. A chunk reaching the loop's end
    /// is cut there, and the decoder returns to the loop's start.
    fn take_chunk(&mut self, chunk: &[f32]) {
        let channels = self
            .conv
            .as_ref()
            .map_or(1, |c| c.src().channels.max(1) as usize);
        let frames = (chunk.len() / channels) as u64;
        // A one-shot with fades is heard as its slice would be written.
        let faded: Vec<f32>;
        let chunk = match self.loop_here() {
            Some(l) if l.once && l.fades != (0, 0) => {
                let len = l.end.saturating_sub(l.start);
                faded = chunk
                    .chunks_exact(channels)
                    .zip(self.decoded..)
                    .flat_map(|(frame, f)| {
                        let gain = match f.checked_sub(l.start).filter(|&i| i < len) {
                            Some(i) => crate::samples::fade_gain(i, len, l.fades),
                            None => 1.0,
                        };
                        frame.iter().map(move |&s| s * gain)
                    })
                    .collect();
                &faded[..]
            }
            _ => chunk,
        };
        match self.loop_here() {
            Some(l) if self.decoded < l.end && self.decoded + frames >= l.end => {
                let keep = (l.end - self.decoded) as usize * channels;
                self.convert_and_carry(&chunk[..keep]);
                if l.once {
                    self.halt(l);
                } else {
                    self.wrap(l);
                }
            }
            Some(_) => {
                let from = self.decoded;
                self.decoded += frames;
                self.carry_faded(chunk, from);
            }
            None => {
                let from = self.decoded;
                self.decoded += frames;
                if self.incoming.is_none() {
                    self.begin_crossfade(from, frames);
                }
                match self.incoming.is_some() {
                    true => self.carry_crossfaded(chunk, from),
                    false => self.carry_faded(chunk, from),
                }
            }
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
        // A one-shot range ends at `pause_at`, and the audio after it is
        // already decoded. Holding it back until the pause lands leaves the
        // device to underrun into silence: a late pause spills no audio.
        let mut room = self.carry.len();
        if let Some(at) = self.pause_at {
            room = room.min(at.saturating_sub(self.written) as usize * channels);
        }
        let whole = out.producer.slots().min(room) / channels * channels;
        let (written, _) = out.producer.push_partial_slice(&self.carry[..whole]);
        let n = written.len();
        if n > 0 {
            self.carry.drain(..n);
            self.written += (n / channels) as u64;
        }
    }

    /// Lets the device start once the ring holds [`PRIME`] of audio, or all
    /// there is: a one-shot pushed to its end, or a track decoded to its end.
    fn end_priming(&mut self) {
        let Some(out) = &self.out else { return };
        if self.flush.is_some() || !self.shared.priming.load(Ordering::Relaxed) {
            return;
        }
        let target = (f64::from(out.plan.rate) * PRIME.as_secs_f64()) as usize;
        let all_pushed = self.pause_at.is_some_and(|at| self.written >= at)
            || (self.stream.is_none() && self.carry.is_empty());
        if out.buffered_frames() >= target || all_pushed {
            self.shared.priming.store(false, Ordering::Relaxed);
        }
    }

    /// Pauses once the device has played to the end of a one-shot range.
    fn pause_when_reached(&mut self) {
        let Some(at) = self.pause_at else { return };
        if self.shared.frames_out.load(Ordering::Relaxed) < at {
            return;
        }
        self.pause_at = None;
        if self.state == State::Playing {
            if let Some(o) = &self.out {
                o.pause();
            }
            self.state = State::Paused;
        }
    }

    /// Pauses once the callback has paused for a handover.
    fn pause_when_handed(&mut self) {
        if self.shared.handed.swap(false, Ordering::Relaxed) && self.state == State::Playing {
            self.state = State::Paused;
        }
    }

    /// Publishes the marks the device has yet to reach, for [`Player::position`].
    ///
    /// The first is left out: it is already in `track_start` and
    /// `position_offset`, and after a seek its offset is zero until
    /// [`Engine::poll_flush`] stores the real one.
    fn publish_marks(&self) {
        self.pending
            .set(self.marks.iter().skip(1).map(|&(at, _, _, off)| (at, off)));
    }

    /// Adds a mark the device reaches at output frame `at`.
    fn push_mark(&mut self, at: u64, index: usize, duration: Option<Duration>, offset: u64) {
        self.marks.push_back((at, index, duration, offset));
        self.publish_marks();
    }

    /// Replaces the marks with `index` alone, playing from its own start.
    fn reset_marks(&mut self, index: usize, duration: Option<Duration>) {
        self.marks.clear();
        self.marks.push_back((0, index, duration, 0));
        self.publish_marks();
    }

    /// Moves the reported track forward once the device has reached its boundary.
    fn advance_marks(&mut self) {
        let played = self.shared.frames_out.load(Ordering::Relaxed);
        let mut moved = false;
        while self.marks.len() > 1 {
            let next_start = self.marks[1].0;
            if played < next_start {
                break;
            }
            self.marks.pop_front();
            moved = true;
            let (start, idx, _, offset) = self.marks[0];
            self.index = idx;
            self.shared.track_start.store(start, Ordering::Relaxed);
            // A new track starts from its own beginning; a loop, from its start.
            self.shared.position_offset.store(offset, Ordering::Relaxed);
            if self.looping.is_some_and(|l| l.index != idx) {
                self.looping = None;
            }
        }
        if moved {
            self.publish_marks();
        }
    }

    fn publish(&mut self, taken: u64) {
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
        if self.state == State::Stopped {
            self.stop_after = false;
        }
        let Ok(mut s) = self.status.lock() else {
            return;
        };
        s.state = self.state;
        // `queue` is written by `Player::send`; the engine's may be older. So
        // is `index` while commands wait, as `Cmd::Remove` shifts it there.
        if taken >= self.shared.sent.load(Ordering::Relaxed) {
            s.index = self.index;
        }
        s.duration = self.marks.front().and_then(|m| m.2);
        // A one-shot range is not a loop: `Cmd::Loop` toggles on this, and a
        // range playing through once must not read as one to switch off.
        s.looping = self
            .looping
            .filter(|l| !l.once && l.index == self.index)
            .map(|l| (l.start, l.end));
        s.source = self.conv.as_ref().map(Converter::src);
        s.output_rate = self.out.as_ref().map(|o| o.plan.rate).unwrap_or(0);
        s.output_format = self.out.as_ref().map(|o| o.plan.format);
        s.resampling = self.conv.as_ref().is_some_and(Converter::resampling);
        s.fading = !self.fade.is_zero() || !self.crossfade.is_zero();
        s.cents = self.cents;
        s.stop_after = self.stop_after;
        s.gain_db = match self.replaygain {
            ReplayGain::Off => None,
            _ => Some(
                20.0 * self
                    .gain_of
                    .get(&self.index)
                    .copied()
                    .unwrap_or(1.0)
                    .log10(),
            ),
        };
        // `error` and `error_seq` are owned by `fail`; publish must not touch them.
    }
}
