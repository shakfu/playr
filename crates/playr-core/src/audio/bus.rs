//! What the output callback plays beside the player: the tape and the DJ
//! decks, once they are attached, summed with the player into one stream.
//! `docs/dev/mixer.md`, under "Phase 3", has the design.
//!
//! A source is boxed on the thread that attaches it and moved to the callback
//! through a ring. A detached one comes back through another, to be dropped
//! off the audio thread. When a stream closes, its sources go back to the
//! engine, which hands them to the next one.

use std::sync::mpsc::Sender;

use rtrb::{Consumer, Producer, RingBuffer};

/// Where a source's headphone cue goes. The routing applies to the whole
/// master, the player's included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// No cue: the master in stereo.
    None,
    /// The master in mono on the left, the cue in mono on the right, or the
    /// other way round when `swap`.
    Split { swap: bool },
    /// The master on channels 1 and 2, the cue on 3 and 4. A stream of fewer
    /// than 4 channels splits instead.
    Channels { swap: bool },
}

/// Something the output callback plays. It runs on the audio thread, so it
/// must not lock, allocate or free.
pub trait Source: Send {
    /// Fills `main`, and `cue` when it has a cue, interleaved stereo at the
    /// stream's rate. Both arrive silent. Returns where the cue goes.
    fn process(&mut self, main: &mut [f32], cue: &mut [f32]) -> Cue;

    /// Starts playing, on the frame a handover reaches; see
    /// [`Player::hand_over`](super::Player::hand_over). Between two calls of
    /// `process`, so the first frame of the next is the handover's.
    fn start(&mut self) {}
}

/// Where the last handover from the player to a source got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Handover {
    /// None asked for since the player started.
    None,
    /// Waiting for the player to reach its frame.
    Armed,
    /// The player paused on its frame and the source started on it.
    Done,
    /// The frame was behind the device when the callback saw it, or its
    /// source was not attached. Nothing changed.
    Missed,
    /// A seek, a speed change or [`Player::cancel_handover`](super::Player::cancel_handover)
    /// dropped it.
    Cancelled,
}

impl Handover {
    pub(crate) fn from_u8(v: u8) -> Handover {
        match v {
            1 => Handover::Armed,
            2 => Handover::Done,
            3 => Handover::Missed,
            4 => Handover::Cancelled,
            _ => Handover::None,
        }
    }
}

/// A source as the bus keeps it: with the id it was attached under.
pub type Attached = (u64, Box<dyn Source>);

/// Most sources one stream plays at once. More are handed back unplayed.
pub const MAX_SOURCES: usize = 8;
/// Frames each source fills per pass; a longer callback takes several.
const CHUNK: usize = 1024;
const COMMANDS: usize = 32;

pub(crate) enum BusCmd {
    Attach(Attached),
    Detach(u64),
    /// Push the master into this ring, from the next callback.
    Record(Producer<f32>),
    /// Let go of the ring, which ends the recording once its writer drains it.
    StopRecord,
}

/// The sources, as the output callback runs them.
pub struct Bus {
    sources: Vec<Attached>,
    commands: Consumer<BusCmd>,
    gone: Producer<Box<dyn Source>>,
    /// Where the sources go when the stream closes.
    home: Option<Sender<Vec<Attached>>>,
    /// The sum of the sources' main and cue, and one source's, for a chunk.
    mix: Vec<f32>,
    cue: Vec<f32>,
    one: Vec<f32>,
    one_cue: Vec<f32>,
    /// Where the chunk's cue goes: the last source with one says.
    route: Cue,
    /// The master's recording ring, while recording.
    record: Option<Producer<f32>>,
    /// The source to start in this callback, and the frame it starts on.
    start: Option<(u64, usize)>,
}

/// The other end of a [`Bus`], which attaches and detaches its sources.
pub struct BusControl {
    commands: Producer<BusCmd>,
    gone: Consumer<Box<dyn Source>>,
}

impl BusControl {
    /// Attaches `source` as `id`, from the bus's next callback; with its
    /// ring full, hands it back.
    pub fn attach(&mut self, id: u64, source: Box<dyn Source>) -> Result<(), Box<dyn Source>> {
        self.send(BusCmd::Attach((id, source)))
            .map_err(|cmd| match cmd {
                BusCmd::Attach((_, s)) => s,
                _ => unreachable!("sent an attach"),
            })
    }

    /// Detaches the source attached as `id`; false with the ring full.
    pub fn detach(&mut self, id: u64) -> bool {
        self.send(BusCmd::Detach(id)).is_ok()
    }

    /// Pushes the master into `ring` from the bus's next callback, until
    /// [`BusControl::stop_recording`]; false with the command ring full.
    pub fn record(&mut self, ring: Producer<f32>) -> bool {
        self.send(BusCmd::Record(ring)).is_ok()
    }

    pub fn stop_recording(&mut self) -> bool {
        self.send(BusCmd::StopRecord).is_ok()
    }

    /// A source the bus let go: detached, or past [`MAX_SOURCES`].
    pub fn take_gone(&mut self) -> Option<Box<dyn Source>> {
        self.gone.pop().ok()
    }

    pub(crate) fn send(&mut self, cmd: BusCmd) -> Result<(), BusCmd> {
        self.commands
            .push(cmd)
            .map_err(|rtrb::PushError::Full(c)| c)
    }
}

impl Bus {
    /// A bus with no sources, and the end that attaches them. Its sources are
    /// sent to `home` when it is dropped.
    pub fn new(home: Option<Sender<Vec<Attached>>>) -> (Bus, BusControl) {
        let (cmd_tx, cmd_rx) = RingBuffer::new(COMMANDS);
        let (gone_tx, gone_rx) = RingBuffer::new(COMMANDS);
        let bus = Bus {
            sources: Vec::with_capacity(MAX_SOURCES),
            commands: cmd_rx,
            gone: gone_tx,
            home,
            mix: vec![0.0; CHUNK * 2],
            cue: vec![0.0; CHUNK * 2],
            one: vec![0.0; CHUNK * 2],
            one_cue: vec![0.0; CHUNK * 2],
            route: Cue::None,
            record: None,
            start: None,
        };
        let control = BusControl {
            commands: cmd_tx,
            gone: gone_rx,
        };
        (bus, control)
    }

    /// A bus that never has sources, for a callback driven by hand.
    pub fn idle() -> Bus {
        Bus::new(None).0
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Whether the source attached as `id` plays.
    pub(crate) fn has(&self, id: u64) -> bool {
        self.sources.iter().any(|(s, _)| *s == id)
    }

    /// Starts the source attached as `id` on frame `frame` of this callback,
    /// before its frames are asked for.
    pub(crate) fn start_at(&mut self, id: u64, frame: usize) {
        self.start = Some((id, frame));
    }

    /// Whether the master is being recorded.
    pub(crate) fn recording(&self) -> bool {
        self.record.is_some()
    }

    /// Whether the recording ring has room for `frames` stereo frames.
    pub(crate) fn record_room(&self, frames: usize) -> bool {
        self.record
            .as_ref()
            .is_some_and(|r| r.slots() >= frames * 2)
    }

    /// Pushes a frame of the master; call only after [`Bus::record_room`].
    pub(crate) fn record(&mut self, frame: [f32; 2]) {
        if let Some(r) = &mut self.record {
            _ = r.push(frame[0]);
            _ = r.push(frame[1]);
        }
    }

    /// Applies attaches and detaches. A source past [`MAX_SOURCES`], or one
    /// detached, goes back to be dropped off the audio thread; with that ring
    /// full, it is kept and dropped later.
    pub(crate) fn commands(&mut self) {
        while let Ok(cmd) = self.commands.peek() {
            if self.gone.is_full() {
                return;
            }
            match cmd {
                BusCmd::Attach(_) if self.sources.len() == MAX_SOURCES => {
                    let Ok(BusCmd::Attach((_, s))) = self.commands.pop() else {
                        return;
                    };
                    _ = self.gone.push(s);
                }
                BusCmd::Attach(_) => {
                    if let Ok(BusCmd::Attach(a)) = self.commands.pop() {
                        self.sources.push(a);
                    }
                }
                BusCmd::Detach(id) => {
                    let id = *id;
                    _ = self.commands.pop();
                    if let Some(i) = self.sources.iter().position(|(s, _)| *s == id) {
                        let (_, s) = self.sources.swap_remove(i);
                        _ = self.gone.push(s);
                    }
                }
                // The writer still holds the ring's other end, so letting go
                // of this one frees nothing here.
                BusCmd::Record(_) => {
                    if let Ok(BusCmd::Record(r)) = self.commands.pop() {
                        self.record = Some(r);
                    }
                }
                BusCmd::StopRecord => {
                    _ = self.commands.pop();
                    self.record = None;
                }
            }
        }
    }

    /// The sources' main and cue for frame `frame` of a callback of `frames`
    /// frames, and where the cue goes. A chunk is rendered as `frame` reaches
    /// it, so frames must be asked for in order.
    pub(crate) fn frame(&mut self, frame: usize, frames: usize) -> ([f32; 2], [f32; 2], Cue) {
        let f = frame % CHUNK;
        if f == 0 {
            self.render(frame, (frames - frame).min(CHUNK));
        }
        let main = [self.mix[f * 2], self.mix[f * 2 + 1]];
        let cue = [self.cue[f * 2], self.cue[f * 2 + 1]];
        (main, cue, self.route)
    }

    /// Renders `frames` frames from frame `at` of the callback.
    fn render(&mut self, at: usize, frames: usize) {
        let n = frames * 2;
        self.mix[..n].fill(0.0);
        self.cue[..n].fill(0.0);
        self.route = Cue::None;
        // Where in this chunk a source starts, and which.
        let start = self
            .start
            .filter(|&(_, k)| (at..at + frames).contains(&k))
            .map(|(id, k)| (id, (k - at) * 2));
        for (id, source) in &mut self.sources {
            let (one, one_cue) = (&mut self.one[..n], &mut self.one_cue[..n]);
            one.fill(0.0);
            one_cue.fill(0.0);
            let route = match start {
                Some((s, k)) if s == *id => {
                    source.process(&mut one[..k], &mut one_cue[..k]);
                    source.start();
                    self.start = None;
                    source.process(&mut one[k..], &mut one_cue[k..])
                }
                _ => source.process(one, one_cue),
            };
            self.mix[..n]
                .iter_mut()
                .zip(one.iter())
                .for_each(|(m, s)| *m += s);
            if route != Cue::None {
                self.route = route;
                self.cue[..n]
                    .iter_mut()
                    .zip(one_cue.iter())
                    .for_each(|(m, s)| *m += s);
            }
        }
    }
}

impl Drop for Bus {
    /// Sends the sources home, with those still on their way in.
    fn drop(&mut self) {
        let Some(home) = &self.home else {
            return;
        };
        let mut sources = std::mem::take(&mut self.sources);
        while let Ok(cmd) = self.commands.pop() {
            if let BusCmd::Attach(a) = cmd {
                sources.push(a);
            }
        }
        if !sources.is_empty() {
            _ = home.send(sources);
        }
    }
}
