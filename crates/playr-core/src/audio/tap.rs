//! What the output callback plays, kept for a view to read: the latest
//! samples, and each momentary loudness block.
//!
//! The callback may not lock, and a reader may be slow or absent, so each is a
//! fixed ring of atomics the callback overwrites. A reader copies the latest
//! values; one that copies while the callback laps it reads a mix of old and
//! new samples, which a display shows for one frame. An `rtrb` ring was the
//! alternative, but it holds its producer by `&mut`, so it would not survive
//! the stream being rebuilt, and a full ring would stall rather than overwrite.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// A ring of `f32` with one writer. See the module documentation.
pub struct Ring {
    /// Values written since the ring was made.
    written: AtomicU64,
    slots: Box<[AtomicU32]>,
}

impl Ring {
    pub fn new(capacity: usize) -> Ring {
        Ring {
            written: AtomicU64::new(0),
            slots: (0..capacity.max(1)).map(|_| AtomicU32::new(0)).collect(),
        }
    }

    /// Appends `v`. Only one thread may write.
    pub fn push(&self, v: f32) {
        let w = self.written.load(Ordering::Relaxed);
        self.slots[(w % self.slots.len() as u64) as usize].store(v.to_bits(), Ordering::Relaxed);
        self.written.store(w + 1, Ordering::Release);
    }

    /// Skips to the next multiple of `n` values, so a reader rounding down to
    /// one finds whole frames. Only the writer may call it.
    fn align(&self, n: u64) {
        let w = self.written.load(Ordering::Relaxed);
        self.written
            .store(w.next_multiple_of(n.max(1)), Ordering::Release);
    }

    /// Values written so far.
    pub fn written(&self) -> u64 {
        self.written.load(Ordering::Acquire)
    }

    /// Replaces `out` with the values from `from` up to `to`, oldest first,
    /// dropping any the ring no longer holds.
    fn copy(&self, from: u64, to: u64, out: &mut Vec<f32>) {
        let cap = self.slots.len() as u64;
        out.clear();
        out.extend(
            (from.max(to.saturating_sub(cap))..to)
                .map(|i| f32::from_bits(self.slots[(i % cap) as usize].load(Ordering::Relaxed))),
        );
    }

    /// Replaces `out` with the values written since `since`, oldest first, and
    /// returns the count to pass next time.
    pub fn since(&self, since: u64, out: &mut Vec<f32>) -> u64 {
        let to = self.written();
        self.copy(since, to, out);
        to
    }
}

/// The samples the device last played, before the volume.
pub struct Tap {
    /// Off unless a view reads it, so the callback does no extra work.
    enabled: AtomicBool,
    channels: AtomicU32,
    ring: Ring,
}

/// Samples the tap holds: 16384 stereo frames, 370 ms at 44.1 kHz.
const TAP_SAMPLES: usize = 1 << 15;

impl Tap {
    pub fn new() -> Tap {
        Tap {
            enabled: AtomicBool::new(false),
            channels: AtomicU32::new(0),
            ring: Ring::new(TAP_SAMPLES),
        }
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    /// Called by the writer before a callback's samples: whether to push
    /// them. Keeps frames whole when the channel count changes.
    pub fn begin(&self, channels: u32) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }
        if self.channels.load(Ordering::Relaxed) != channels {
            self.ring.align(channels as u64);
            self.channels.store(channels, Ordering::Relaxed);
        }
        true
    }

    pub fn push(&self, v: f32) {
        self.ring.push(v);
    }

    /// Replaces `out` with up to the last `frames` whole frames, interleaved,
    /// and returns their channel count; 0 before anything is written.
    pub fn latest(&self, frames: usize, out: &mut Vec<f32>) -> u16 {
        let channels = self.channels.load(Ordering::Relaxed).max(1) as u64;
        let frames = (frames as u64).min(TAP_SAMPLES as u64 / channels);
        let to = self.ring.written();
        let to = to - to % channels;
        let from = to.saturating_sub(frames * channels);
        self.ring.copy(from, to, out);
        match to {
            0 => 0,
            _ => channels as u16,
        }
    }
}

impl Default for Tap {
    fn default() -> Self {
        Tap::new()
    }
}
