//! Two decks that play whole tracks at their own rates, a beat grid per
//! track for sync and quantize, and a mixer with a split headphone cue.
//!
//! - [`Mixer`]: the decks, sync and the mix; any thread, no device.
//! - [`Engine`]: the mixer in the audio callback, fed by a command ring.
//! - [`Handle`]: the commands and status, from any other thread.
//! - [`device`]: the engine's own cpal output stream.
//!
//! Audio is interleaved `f32` at the device's rate; the caller decodes and
//! resamples. The design is in `docs/dev/dj-engine.md`.

mod deck;
pub mod device;
mod engine;
mod fx;
mod mixer;
mod track;

pub use deck::{Deck, Nudge, Range, HOT_CUES};
pub use engine::{new, Cmd, DeckStatus, Engine, Handle, Returned, Status};
pub use fx::{Band, EQ_DB};
pub use mixer::{sync_pct, CueOut, Curve, Mixer, Setting, LOOP_BEATS};
pub use track::{Grid, Track};

/// One of the two decks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::A => Side::B,
            Side::B => Side::A,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug)]
pub enum Error {
    /// Tracks have 1 or 2 channels.
    Channels(u16),
    /// Not a whole number of frames.
    Length,
    /// A grid's tempo is not positive, or a value is not finite.
    Grid,
    /// The command ring is full.
    Full,
    /// The device cannot play at this rate; resample to [`device::rate`].
    Rate(u32),
    NoConfig,
    Busy,
    Device(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Channels(n) => write!(f, "a track has 1 or 2 channels, not {n}"),
            Error::Length => write!(f, "a track needs a whole number of frames"),
            Error::Grid => write!(f, "a grid needs a positive tempo and a finite first beat"),
            Error::Full => write!(f, "the DJ engine's command ring is full"),
            Error::Rate(r) => write!(f, "the output device cannot play {r} Hz"),
            Error::NoConfig => write!(f, "the output device offers no usable format"),
            Error::Busy => write!(f, "the output device is busy"),
            Error::Device(s) => write!(f, "could not open audio stream: {s}"),
        }
    }
}

impl std::error::Error for Error {}
