//! The tape on the audio thread ([`Looper`]), and its controls on any other
//! ([`Handle`]): the command rings, the status atomics, the loop snapshot
//! and the mix recording.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rtrb::{Consumer, Producer, RingBuffer};

use crate::tape::{Loop, Setting, Tape};
use crate::{Error, COLUMNS, VOICES};

const COMMANDS: usize = 256;
/// Returns travel one or two per command, and the handle drains them.
const RETURNS: usize = 16;
/// How much of the mix the recording ring holds.
const RECORD_SECONDS: usize = 2;
/// How many loop frames a snapshot copies per output frame. Faster than the
/// write head, so each frame is copied before the head rewrites it.
const SNAPSHOT_SPEED: usize = 8;
/// How long the output fades out before a load while playing, and back in
/// after: the heads jump.
const LOAD_FADE_MS: f32 = 5.0;
/// How long a stopped recorder waits for the callback to acknowledge it.
const RECORD_STOP_WAIT: Duration = Duration::from_secs(1);

/// What the handle sends the callback.
#[derive(Debug)]
pub enum Cmd {
    Set(Setting),
    Record(bool),
    /// A buffer the length of the loop's samples, to copy the loop into.
    Snapshot(Box<[f32]>),
    Load(Box<Loop>),
}

impl Cmd {
    /// How many [`Returned`] values applying it can produce.
    fn returns(&self) -> usize {
        match self {
            Cmd::Load(_) => 2,
            Cmd::Snapshot(_) | Cmd::Set(Setting::WriteWindow(_)) => 1,
            _ => 0,
        }
    }
}

/// Memory the callback hands back, since it must not free it.
#[derive(Debug)]
pub enum Returned {
    /// The loop a `Load` replaced.
    Loop(Box<Loop>),
    /// A finished snapshot: the loop's samples as they were when it arrived.
    Snapshot(Box<[f32]>),
    /// A snapshot cut short by a `Load` or a new write window, or sent while
    /// another ran or with the wrong length.
    Aborted(Box<[f32]>),
}

/// What the callback publishes once per call.
#[derive(Debug)]
pub struct Status {
    playing: AtomicBool,
    voices: [AtomicU64; VOICES],
    write: AtomicU64,
    peak: AtomicU32,
    columns: [AtomicU32; COLUMNS],
    recording: AtomicBool,
    /// `Record` commands applied, so a stopping recorder knows the callback
    /// has stopped writing to the ring.
    record_acks: AtomicU64,
    dropped: AtomicU64,
}

impl Status {
    fn new() -> Self {
        Status {
            playing: AtomicBool::new(false),
            voices: std::array::from_fn(|_| AtomicU64::new(0)),
            write: AtomicU64::new(0),
            peak: AtomicU32::new(0),
            columns: std::array::from_fn(|_| AtomicU32::new(0)),
            recording: AtomicBool::new(false),
            record_acks: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    /// Voice `i`'s head, in frames.
    pub fn voice(&self, i: usize) -> f64 {
        f64::from_bits(self.voices[i].load(Ordering::Relaxed))
    }

    pub fn write_head(&self) -> usize {
        self.write.load(Ordering::Relaxed) as usize
    }

    /// The largest output magnitude since the last call, which resets it.
    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak.swap(0, Ordering::Relaxed))
    }

    /// The loop's waveform: the peak of each of [`COLUMNS`] parts, at most
    /// one pass old.
    pub fn columns(&self) -> [f32; COLUMNS] {
        std::array::from_fn(|i| f32::from_bits(self.columns[i].load(Ordering::Relaxed)))
    }

    pub fn recording(&self) -> bool {
        self.recording.load(Ordering::Relaxed)
    }

    /// Mix blocks dropped from the current or last recording because the
    /// writer fell behind.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// A snapshot in progress.
#[derive(Debug)]
struct Snapshot {
    dest: Box<[f32]>,
    /// Where the write head was when it arrived.
    from: usize,
    /// Frames copied.
    done: usize,
}

/// The tape as the audio callback runs it. It never locks, allocates or
/// frees.
pub struct Looper {
    tape: Tape,
    commands: Consumer<Cmd>,
    returns: Producer<Returned>,
    status: Arc<Status>,
    record: Producer<f32>,
    recording: bool,
    snapshot: Option<Snapshot>,
    /// The output's gain, faded out round a load.
    gain: playr_dsp::Ramp<f32>,
    fade: u32,
}

/// A looper at `sample_rate` and the handle that controls it.
pub fn new(sample_rate: u32) -> (Looper, Handle) {
    let (cmd_tx, cmd_rx) = RingBuffer::new(COMMANDS);
    let (ret_tx, ret_rx) = RingBuffer::new(RETURNS);
    let (rec_tx, rec_rx) = RingBuffer::new(sample_rate as usize * RECORD_SECONDS * 2);
    let status = Arc::new(Status::new());
    let looper = Looper {
        tape: Tape::new(sample_rate),
        commands: cmd_rx,
        returns: ret_tx,
        status: status.clone(),
        record: rec_tx,
        recording: false,
        snapshot: None,
        gain: playr_dsp::Ramp::new(1.0),
        fade: (LOAD_FADE_MS * sample_rate as f32 / 1000.0) as u32,
    };
    let handle = Handle {
        commands: cmd_tx,
        returns: ret_rx,
        status,
        record: Some(rec_rx),
        recorder: None,
        record_sent: 0,
        original: None,
        sample_rate,
    };
    (looper, handle)
}

impl Looper {
    /// Applies pending commands, then fills `out`, interleaved stereo, and
    /// publishes the status.
    pub fn process(&mut self, out: &mut [f32]) {
        self.commands();
        self.copy(out.len() / 2 * SNAPSHOT_SPEED);
        self.tape.process(out);
        if !(self.gain.settled() && self.gain.value() == 1.0) {
            for frame in out.as_chunks_mut::<2>().0 {
                let g = self.gain.next();
                frame.iter_mut().for_each(|s| *s *= g);
            }
        }
        if self.recording && self.tape.playing() {
            match self.record.slots() >= out.len() {
                true => out.iter().for_each(|&s| _ = self.record.push(s)),
                false => _ = self.status.dropped.fetch_add(1, Ordering::Relaxed),
            }
        }
        self.publish(out);
    }

    pub fn tape(&self) -> &Tape {
        &self.tape
    }

    fn commands(&mut self) {
        while let Ok(cmd) = self.commands.peek() {
            // Left in the ring until the handle has drained room for what it returns.
            if self.returns.slots() < cmd.returns() {
                return;
            }
            // Left in the ring, holding back what follows, until faded out.
            if matches!(cmd, Cmd::Load(_)) && self.tape.playing() && self.gain.value() > 0.0 {
                if self.gain.target() != 0.0 {
                    self.gain.set(0.0, self.fade);
                }
                return;
            }
            let Ok(cmd) = self.commands.pop() else { return };
            match cmd {
                Cmd::Set(s) => {
                    if let Setting::WriteWindow(w) = s {
                        if w != self.tape.write_window() {
                            self.abort();
                        }
                    }
                    self.tape.set(s);
                }
                Cmd::Record(on) => {
                    if on && !self.recording {
                        self.status.dropped.store(0, Ordering::Relaxed);
                    }
                    self.recording = on;
                    self.status.recording.store(on, Ordering::Relaxed);
                    self.status.record_acks.fetch_add(1, Ordering::Release);
                }
                Cmd::Snapshot(dest) => {
                    if self.snapshot.is_some() || dest.len() != self.tape.buffer().samples().len() {
                        _ = self.returns.push(Returned::Aborted(dest));
                    } else {
                        self.snapshot = Some(Snapshot {
                            dest,
                            from: self.tape.write_head(),
                            done: 0,
                        });
                    }
                }
                Cmd::Load(lp) => {
                    self.abort();
                    let old = self.tape.load(lp);
                    let peaks = self.tape.buffer().peaks();
                    for (a, p) in self.status.columns.iter().zip(peaks) {
                        a.store(p.to_bits(), Ordering::Relaxed);
                    }
                    _ = self.returns.push(Returned::Loop(old));
                    self.gain.set(1.0, self.fade);
                }
            }
        }
    }

    fn abort(&mut self) {
        if let Some(s) = self.snapshot.take() {
            _ = self.returns.push(Returned::Aborted(s.dest));
        }
    }

    /// Copies up to `budget` steps of the snapshot. Each step of the first
    /// `n` copies a frame of the write window, from the write head round,
    /// ahead of the head. While the window is the range, the head also
    /// writes the frames `n` before and after, so the step copies those too.
    /// The rest are never written, and follow one a step.
    fn copy(&mut self, budget: usize) {
        let Some(snap) = &mut self.snapshot else {
            return;
        };
        let lp = self.tape.buffer();
        let (frames, c) = (lp.frames(), lp.channels() as usize);
        let w = self.tape.write_window();
        let n = w.len();
        let rolls = w == lp.range();
        // The frames the head writes are `lo..hi`.
        let (lo, hi) = match rolls {
            true => (w.start.saturating_sub(n), frames.min(w.end + n)),
            false => (w.start, w.end),
        };
        let steps = n + lo + (frames - hi);
        let mut take = |i: usize| {
            snap.dest[i * c..i * c + c].copy_from_slice(&lp.samples()[i * c..i * c + c]);
        };
        let end = (snap.done + budget).min(steps);
        for j in snap.done..end {
            if j >= n {
                let k = j - n;
                take(if k < lo { k } else { hi + (k - lo) });
                continue;
            }
            let i = w.start + (snap.from - w.start + j) % n;
            take(i);
            if rolls && i + n < frames {
                take(i + n);
            }
            if rolls && i >= n {
                take(i - n);
            }
        }
        snap.done = end;
        if snap.done == steps && self.returns.slots() > 0 {
            let snap = self.snapshot.take().expect("checked above");
            _ = self.returns.push(Returned::Snapshot(snap.dest));
        }
    }

    fn publish(&self, out: &[f32]) {
        let s = &self.status;
        s.playing.store(self.tape.playing(), Ordering::Relaxed);
        for (i, a) in s.voices.iter().enumerate() {
            a.store(self.tape.voice(i).to_bits(), Ordering::Relaxed);
        }
        s.write
            .store(self.tape.write_head() as u64, Ordering::Relaxed);
        // The reader resets it, so a plain store could undo a higher peak.
        // A magnitude is never negative, and such floats order as their bits.
        let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        s.peak.fetch_max(peak.to_bits(), Ordering::Relaxed);
        if self.tape.playing() {
            for (a, p) in s.columns.iter().zip(self.tape.buffer().peaks()) {
                a.store(p.to_bits(), Ordering::Relaxed);
            }
        }
    }
}

/// A recording of the mix that has finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    pub path: PathBuf,
    /// Stereo frames written.
    pub frames: u64,
    /// Blocks lost because the writer fell behind.
    pub dropped: u64,
}

/// The writer thread, which returns the ring's consumer when it finishes.
struct Recorder {
    path: PathBuf,
    /// The `record_acks` count the callback reaches once it has stopped
    /// recording; `u64::MAX` until a stop is asked for.
    stop_at: Arc<AtomicU64>,
    thread: JoinHandle<(Consumer<f32>, Result<u64, hound::Error>)>,
}

/// Controls a [`Looper`] from outside the audio thread.
pub struct Handle {
    commands: Producer<Cmd>,
    returns: Consumer<Returned>,
    status: Arc<Status>,
    record: Option<Consumer<f32>>,
    recorder: Option<Recorder>,
    record_sent: u64,
    /// The loop as last loaded, for [`Handle::reset`].
    original: Option<Loop>,
    sample_rate: u32,
}

impl Handle {
    /// Sends `cmd`, or hands it back when the ring is full.
    pub fn send(&mut self, cmd: Cmd) -> Result<(), Cmd> {
        self.commands
            .push(cmd)
            .map_err(|rtrb::PushError::Full(c)| c)
    }

    pub fn set(&mut self, s: Setting) -> Result<(), Error> {
        self.send(Cmd::Set(s)).map_err(|_| Error::Full)
    }

    /// Loads `lp`, keeping a copy for [`Handle::reset`].
    pub fn load(&mut self, lp: Loop) -> Result<(), Error> {
        let copy = lp.clone();
        self.send(Cmd::Load(Box::new(lp)))
            .map_err(|_| Error::Full)?;
        self.original = Some(copy);
        Ok(())
    }

    /// Restores the loop as last loaded, and every head to its window's start.
    pub fn reset(&mut self) -> Result<(), Error> {
        let lp = self.original.clone().ok_or(Error::Empty)?;
        self.send(Cmd::Load(Box::new(lp))).map_err(|_| Error::Full)
    }

    /// Starts copying the loop. The copy arrives through [`Handle::poll`] as
    /// [`Returned::Snapshot`].
    pub fn snapshot(&mut self) -> Result<(), Error> {
        let n = self.original.as_ref().ok_or(Error::Empty)?.samples().len();
        let dest = vec![0.0; n].into_boxed_slice();
        self.send(Cmd::Snapshot(dest)).map_err(|_| Error::Full)
    }

    /// The next piece of memory the callback has handed back.
    pub fn poll(&mut self) -> Option<Returned> {
        self.returns.pop().ok()
    }

    /// Shared, so a drawing thread can read it without the handle.
    pub fn status(&self) -> &Arc<Status> {
        &self.status
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The channel count of the loop last loaded.
    pub fn channels(&self) -> Option<u16> {
        self.original.as_ref().map(Loop::channels)
    }

    /// Starts recording the mix to a 32-bit float WAV at `path`.
    pub fn record(&mut self, path: &Path) -> Result<(), Error> {
        let Some(mut ring) = self.record.take() else {
            return Err(Error::Recording);
        };
        // Blocks left from a recording whose stop the callback never saw.
        if let Ok(chunk) = ring.read_chunk(ring.slots()) {
            chunk.commit_all();
        }
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: self.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let wav = match hound::WavWriter::create(path, spec) {
            Ok(w) => w,
            Err(e) => {
                self.record = Some(ring);
                return Err(Error::Wav(e));
            }
        };
        if self.send(Cmd::Record(true)).is_err() {
            self.record = Some(ring);
            return Err(Error::Full);
        }
        self.record_sent += 1;
        let stop_at = Arc::new(AtomicU64::new(u64::MAX));
        let thread = {
            let (stop_at, status) = (stop_at.clone(), self.status.clone());
            std::thread::spawn(move || write(ring, wav, &stop_at, &status))
        };
        self.recorder = Some(Recorder {
            path: path.to_owned(),
            stop_at,
            thread,
        });
        Ok(())
    }

    /// Stops recording and finishes the WAV.
    /// Whether a recording runs: started, and not yet stopped by
    /// [`Handle::stop_recording`].
    pub fn recording(&self) -> bool {
        self.recorder.is_some()
    }

    pub fn stop_recording(&mut self) -> Result<Recording, Error> {
        if self.recorder.is_none() {
            return Err(Error::NotRecording);
        }
        self.send(Cmd::Record(false)).map_err(|_| Error::Full)?;
        self.record_sent += 1;
        let r = self.recorder.take().expect("checked above");
        self.finish(r)
    }

    fn finish(&mut self, r: Recorder) -> Result<Recording, Error> {
        r.stop_at.store(self.record_sent, Ordering::Release);
        let (ring, frames) = r.thread.join().map_err(|_| Error::Writer)?;
        self.record = Some(ring);
        Ok(Recording {
            path: r.path,
            frames: frames.map_err(Error::Wav)?,
            dropped: self.status.dropped(),
        })
    }
}

impl Drop for Handle {
    /// Finishes a recording still running, so its WAV is valid.
    fn drop(&mut self) {
        if let Some(r) = self.recorder.take() {
            _ = self.send(Cmd::Record(false));
            self.record_sent += 1;
            _ = self.finish(r);
        }
    }
}

/// The writer thread: drains `ring` into `wav` until stopped. A write error
/// keeps it draining, so the callback does not count drops, and is returned
/// at the end.
fn write(
    mut ring: Consumer<f32>,
    mut wav: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    stop_at: &AtomicU64,
    status: &Status,
) -> (Consumer<f32>, Result<u64, hound::Error>) {
    let mut samples = 0u64;
    let mut err = None;
    let mut stopping: Option<Instant> = None;
    loop {
        // Read before draining, so nothing pushed before the acknowledgement is missed.
        let target = stop_at.load(Ordering::Acquire);
        let acked = target != u64::MAX
            && (status.record_acks.load(Ordering::Acquire) >= target
                || stopping.is_some_and(|t| t.elapsed() > RECORD_STOP_WAIT));
        if target != u64::MAX && stopping.is_none() {
            stopping = Some(Instant::now());
        }
        let n = ring.slots();
        if let Ok(chunk) = ring.read_chunk(n) {
            for s in chunk {
                if err.is_none() {
                    match wav.write_sample(s) {
                        Ok(()) => samples += 1,
                        Err(e) => err = Some(e),
                    }
                }
            }
        }
        if acked {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = match err {
        Some(e) => Err(e),
        None => wav.finalize().map(|()| samples / 2),
    };
    (ring, result)
}
