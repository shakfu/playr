//! A tape looper: 3 voices read one loop at their own rates and directions,
//! and a write head records their sends back into it with feedback.
//!
//! - [`Tape`]: the loop and all of the DSP; any thread, no device.
//! - [`Looper`]: the tape in the audio callback, fed by a command ring.
//! - [`Handle`]: the commands, status, snapshot and recording, from any
//!   other thread.
//!
//! Audio is interleaved `f32` at the stream's rate; the caller decodes,
//! resamples, and plays [`Looper`] on its stream. The design is in `docs/dev/looper-engine.md`.

mod looper;
mod tape;

use std::path::Path;

pub use looper::{new, Cmd, Handle, Looper, Recording, Returned, Status};
pub use tape::{
    crossfade, fade_frames, Crossfade, Filter, Loop, Setting, Tape, Window, DEFAULT_FADE_MS,
    DEFAULT_KNEE, DEFAULT_SLEW_MS, MAX_SLEW_MS,
};

/// Voices reading the loop.
pub const VOICES: usize = 3;
/// Columns in the loop's peak grid, which draws its waveform.
pub const COLUMNS: usize = 512;

#[derive(Debug)]
pub enum Error {
    /// Loops have 1 or 2 channels.
    Channels(u16),
    /// No samples, or not a whole number of frames.
    Length,
    /// Nothing has been loaded.
    Empty,
    /// The command ring is full.
    Full,
    Recording,
    NotRecording,
    /// The recording's writer thread panicked.
    Writer,
    Wav(hound::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Channels(n) => write!(f, "a loop has 1 or 2 channels, not {n}"),
            Error::Length => write!(f, "a loop needs at least one whole frame"),
            Error::Empty => write!(f, "no loop is loaded"),
            Error::Full => write!(f, "the looper's command ring is full"),
            Error::Recording => write!(f, "already recording"),
            Error::NotRecording => write!(f, "not recording"),
            Error::Writer => write!(f, "the recording's writer thread failed"),
            Error::Wav(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

/// Writes interleaved `samples` as a 32-bit float WAV.
pub fn write_wav(path: &Path, samples: &[f32], channels: u16, rate: u32) -> Result<(), Error> {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(Error::Wav)?;
    samples
        .iter()
        .try_for_each(|&s| w.write_sample(s))
        .and_then(|()| w.finalize())
        .map_err(Error::Wav)
}
