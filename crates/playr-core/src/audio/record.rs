//! Recording the master: the output callback pushes what it plays, before
//! the cue is routed, into a ring, and a writer thread writes it to a 32-bit
//! float stereo WAV. Overs are kept. `docs/dev/mixer.md`, under "Phase 3".

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use rtrb::{Consumer, Producer, RingBuffer};

/// How much of the master the ring holds.
const SECONDS: u32 = 2;

/// Where a recording's result arrives once its file is written.
pub(crate) type Done = Receiver<Result<Recorded, String>>;

/// A finished recording.
#[derive(Debug, Clone, PartialEq)]
pub struct Recorded {
    pub path: PathBuf,
    /// Stereo frames written.
    pub frames: u64,
    pub rate: u32,
    /// Callbacks whose audio was lost because the writer fell behind.
    pub dropped: u64,
}

/// A recording under way, from the thread that started it.
pub struct MasterRecording {
    pub(crate) path: PathBuf,
    pub(crate) done: Done,
    pub(crate) stop: Box<dyn Fn() + Send>,
}

impl MasterRecording {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Asks the callback to stop; [`MasterRecording::finished`] says when
    /// the file is complete.
    pub fn stop(&self) {
        (self.stop)();
    }

    /// The result, once the file is complete: after a stop, or when the
    /// stream closed under it.
    pub fn finished(&self) -> Option<Result<Recorded, String>> {
        self.done.try_recv().ok()
    }
}

/// Opens `path` and starts the writer. Returns the ring's producer, for the
/// callback, and the receiver of the result. `alive` is lowered when the
/// writer finishes; `dropped` counts what the callback could not push.
pub(crate) fn start(
    path: &Path,
    rate: u32,
    alive: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
) -> Result<(Producer<f32>, Done), String> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let wav =
        hound::WavWriter::create(path, spec).map_err(|e| format!("{}: {e}", path.display()))?;
    let (producer, consumer) = RingBuffer::new((rate * SECONDS * 2) as usize);
    let (tx, rx) = std::sync::mpsc::channel();
    let path = path.to_path_buf();
    dropped.store(0, Ordering::Relaxed);
    alive.store(true, Ordering::Relaxed);
    std::thread::Builder::new()
        .name("playr-record".into())
        .spawn(move || {
            let result = write(consumer, wav, &path, rate, &dropped);
            alive.store(false, Ordering::Relaxed);
            let _: Result<(), _> = Sender::send(&tx, result);
        })
        .map_err(|e| e.to_string())?;
    Ok((producer, rx))
}

/// Writes what arrives until the callback lets go of the ring and it is empty.
fn write(
    mut ring: Consumer<f32>,
    mut wav: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    path: &Path,
    rate: u32,
    dropped: &AtomicU64,
) -> Result<Recorded, String> {
    let fail = |e: hound::Error| format!("{}: {e}", path.display());
    let mut samples = 0u64;
    loop {
        let n = ring.slots();
        if n > 0 {
            let chunk = ring.read_chunk(n).expect("as many as are there");
            let (a, b) = chunk.as_slices();
            for &s in a.iter().chain(b) {
                wav.write_sample(s).map_err(fail)?;
            }
            chunk.commit_all();
            samples += n as u64;
        } else if ring.is_abandoned() {
            break;
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    wav.finalize().map_err(fail)?;
    Ok(Recorded {
        path: path.to_path_buf(),
        frames: samples / 2,
        rate,
        dropped: dropped.load(Ordering::Relaxed),
    })
}
