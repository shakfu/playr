//! Recording the master: the output callback pushes what it plays, before
//! the cue is routed, into a ring, and a writer thread writes it to a 32-bit
//! float stereo WAV. Overs are kept. `docs/dev/mixer.md`, under "Phase 3".
//!
//! A WAV's sizes are u32, so a recording past [`PART_BYTES`] goes on in
//! `<stem>-part2.wav` and so on. Parts over RF64, which Symphonia 0.6 cannot
//! read, so playr could not play or add its own recording.

use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use rtrb::{Consumer, Producer, RingBuffer};

/// How much of the master the ring holds.
const SECONDS: u32 = 2;

/// Sample bytes in one file: 4 GiB less 1 MiB, room for the header, and a
/// whole number of stereo frames.
pub const PART_BYTES: u64 = 0xFFF0_0000;

/// Where a recording's result arrives once its file is written.
pub(crate) type Done = Receiver<Result<Recorded, String>>;

/// A finished recording.
#[derive(Debug, Clone, PartialEq)]
pub struct Recorded {
    /// The files written, in order; more than one past [`PART_BYTES`].
    pub paths: Vec<PathBuf>,
    /// Stereo frames written, over all the files.
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
    /// the file is complete. Also needed after an error, to free the output.
    pub fn stop(&self) {
        (self.stop)();
    }

    /// The result, once the file is complete: after a stop, when the stream
    /// closed under it, or at once on an error.
    pub fn finished(&self) -> Option<Result<Recorded, String>> {
        self.done.try_recv().ok()
    }
}

/// Opens `path` and starts the writer. Returns the ring's producer, for the
/// callback, and the receiver of the result. `alive` is lowered when the
/// writer lets go of the ring; `dropped` counts what the callback could not push.
pub(crate) fn start(
    path: &Path,
    rate: u32,
    alive: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
) -> Result<(Producer<f32>, Done), String> {
    let (producer, consumer) = RingBuffer::new((rate * SECONDS * 2) as usize);
    let done = spawn(consumer, path, rate, PART_BYTES / 4, alive, dropped)?;
    Ok((producer, done))
}

/// Starts the writer on `ring`, with `part` samples to a file.
fn spawn(
    mut ring: Consumer<f32>,
    path: &Path,
    rate: u32,
    part: u64,
    alive: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
) -> Result<Done, String> {
    let wav = create(path, rate)?;
    let (tx, rx) = std::sync::mpsc::channel();
    let path = path.to_path_buf();
    dropped.store(0, Ordering::Relaxed);
    alive.store(true, Ordering::Relaxed);
    let lower = alive.clone();
    std::thread::Builder::new()
        .name("playr-record".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                write(&mut ring, wav, &path, rate, part, &dropped)
            }))
            .unwrap_or_else(|_| Err(format!("{}: the writer panicked", path.display())));
            let _: Result<(), _> = Sender::send(&tx, result);
            // After an error the callback still holds the ring. Wait for it to
            // let go, so the buffer is freed here and not on the audio thread.
            while !ring.is_abandoned() {
                std::thread::sleep(Duration::from_millis(10));
            }
            alive.store(false, Ordering::Relaxed);
        })
        .map_err(|e| {
            lower.store(false, Ordering::Relaxed);
            e.to_string()
        })?;
    Ok(rx)
}

type Wav = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

fn create(path: &Path, rate: u32) -> Result<Wav, String> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    hound::WavWriter::create(path, spec).map_err(|e| failed(path, e))
}

fn failed(path: &Path, e: hound::Error) -> String {
    format!("{}: {e}", path.display())
}

/// The file part `n` of a recording to `path` is written to; part 1 is `path`.
fn part_path(path: &Path, n: usize) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!("{stem}-part{n}.wav"))
}

/// Writes what arrives until the callback lets go of the ring and it is empty,
/// starting a new file every `part` samples.
fn write(
    ring: &mut Consumer<f32>,
    mut wav: Wav,
    path: &Path,
    rate: u32,
    part: u64,
    dropped: &AtomicU64,
) -> Result<Recorded, String> {
    let mut paths = vec![path.to_path_buf()];
    let (mut samples, mut in_part) = (0u64, 0u64);
    loop {
        // Abandonment first: a push made before the callback let go is then
        // counted in `slots`.
        let gone = ring.is_abandoned();
        let n = ring.slots();
        if n == 0 {
            if gone {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        let chunk = ring.read_chunk(n).expect("as many as are there");
        let (a, b) = chunk.as_slices();
        for &s in a.iter().chain(b) {
            if in_part == part {
                let last = &paths[paths.len() - 1];
                wav.finalize().map_err(|e| failed(last, e))?;
                let next = part_path(path, paths.len() + 1);
                wav = create(&next, rate)?;
                paths.push(next);
                in_part = 0;
            }
            wav.write_sample(s)
                .map_err(|e| failed(&paths[paths.len() - 1], e))?;
            in_part += 1;
        }
        chunk.commit_all();
        samples += n as u64;
    }
    wav.finalize()
        .map_err(|e| failed(&paths[paths.len() - 1], e))?;
    Ok(Recorded {
        paths,
        frames: samples / 2,
        rate,
        dropped: dropped.load(Ordering::Relaxed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> Vec<f32> {
        hound::WavReader::open(path)
            .unwrap()
            .into_samples::<f32>()
            .map(Result::unwrap)
            .collect()
    }

    fn lowered(alive: &AtomicBool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while alive.load(Ordering::Relaxed) {
            assert!(std::time::Instant::now() < deadline, "alive never lowered");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn wait(done: &Done) -> Result<Recorded, String> {
        done.recv_timeout(Duration::from_secs(10)).unwrap()
    }

    #[test]
    fn a_recording_past_a_part_goes_on_in_the_next_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("master.wav");
        let (mut producer, consumer) = RingBuffer::new(64);
        let alive = Arc::new(AtomicBool::new(false));
        let done = spawn(consumer, &path, 8000, 6, alive.clone(), Arc::default()).unwrap();
        for i in 0..14 {
            while producer.push(i as f32).is_err() {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        drop(producer);
        let r = wait(&done).unwrap();
        assert_eq!(r.frames, 7);
        assert_eq!(
            r.paths,
            ["master.wav", "master-part2.wav", "master-part3.wav"].map(|n| dir.path().join(n))
        );
        let all: Vec<f32> = r.paths.iter().flat_map(|p| read(p)).collect();
        assert_eq!(all, (0..14).map(|i| i as f32).collect::<Vec<_>>());
        assert_eq!(read(&r.paths[2]), [12.0, 13.0], "parts end on frames");
        lowered(&alive);
    }

    #[test]
    fn an_error_is_sent_at_once_and_the_writer_holds_the_ring_until_let_go() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("master.wav");
        // Part 2 cannot be created: a directory takes its name.
        std::fs::create_dir(dir.path().join("master-part2.wav")).unwrap();
        let (mut producer, consumer) = RingBuffer::new(64);
        let alive = Arc::new(AtomicBool::new(false));
        let done = spawn(consumer, &path, 8000, 2, alive.clone(), Arc::default()).unwrap();
        for i in 0..4 {
            producer.push(i as f32).unwrap();
        }
        let e = wait(&done).unwrap_err();
        assert!(e.contains("master-part2.wav"), "{e}");
        std::thread::sleep(Duration::from_millis(50));
        assert!(alive.load(Ordering::Relaxed), "the writer let go first");
        assert!(!producer.is_abandoned());
        drop(producer);
        lowered(&alive);
    }
}
