//! The scope view's state: what plays, read from the player once a frame, as
//! a loudness history, a spectrum, a stereo image and a waveform trace.
//! Drawing them is the frontend's.
//!
//! The history is kept in every view, so the scope shows the last minute when
//! opened. The rest reads the player's tap, which is on only while the scope
//! shows.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use playr_core::analysis::loudness::Histogram;
use playr_core::audio::meter::SILENCE_LUFS;
use playr_core::audio::Player;
use playr_core::settings::{DEFAULT_LOUDNESS_TARGET, LOUDNESS_TARGETS};
use playr_core::spectrum::{Analyser, BANDS, FFT};

/// Momentary loudness readings the history keeps: a minute, at ten a second.
pub const HISTORY: usize = 600;

/// How far the loudness history reaches below and above the target, in LU.
pub const BELOW_TARGET: f32 = 20.0;
pub const ABOVE_TARGET: f32 = 10.0;

/// How fast a band's level falls, in dB a second. It rises at once.
pub const FALL_DB_PER_SEC: f32 = 24.0;

/// The level the spectrum draws as empty, in dBFS.
pub const SPECTRUM_FLOOR_DB: f32 = -90.0;

/// The time constant the correlation is smoothed over, in seconds.
const CORRELATION_SECS: f32 = 0.3;

/// The trace's span, as a fraction of a second: 20 ms.
const TRACE_PER_SEC: u32 = 50;

/// See the module documentation.
#[derive(Default)]
pub struct Scope {
    /// The frames last read, interleaved, oldest first.
    samples: Vec<f32>,
    /// Their channels' mean.
    mono: Vec<f32>,
    rate: u32,
    channels: u16,
    /// Rebuilt when the rate changes.
    analyser: Option<(u32, Analyser)>,
    /// Each band's level shown, in dBFS; empty until audio is read.
    bands: Vec<f32>,
    correlation: Option<f32>,
    /// Momentary loudness in LUFS, oldest first; `None` for silence.
    history: VecDeque<Option<f32>>,
    /// Readings taken from the player so far.
    blocks: u64,
    /// The playing track's gating blocks, as far as it has played.
    integrated: Histogram,
    target: Target,
    track: Option<PathBuf>,
    read_at: Option<Instant>,
    /// What the player last handed over, kept for its allocation.
    read: Vec<f32>,
}

/// The loudness target, in LUFS.
#[derive(Clone, Copy)]
struct Target(f32);

impl Default for Target {
    fn default() -> Self {
        Target(DEFAULT_LOUDNESS_TARGET)
    }
}

impl Scope {
    /// A scope drawing its loudness history against `lufs`.
    pub fn with_target(lufs: f32) -> Scope {
        let mut scope = Scope::default();
        scope.set_target(lufs);
        scope
    }

    /// The level the loudness history is drawn against, in LUFS.
    pub fn target(&self) -> f32 {
        self.target.0
    }

    /// Sets the target, kept within [`LOUDNESS_TARGETS`].
    pub fn set_target(&mut self, lufs: f32) {
        self.target.0 = lufs.clamp(*LOUDNESS_TARGETS.start(), *LOUDNESS_TARGETS.end());
    }

    /// How far up the loudness history `lufs` sits, from 0 at
    /// [`BELOW_TARGET`] under the target to 1 at [`ABOVE_TARGET`] over it.
    pub fn height_of(&self, lufs: f32) -> f32 {
        ((lufs - self.target() + BELOW_TARGET) / (BELOW_TARGET + ABOVE_TARGET)).clamp(0.0, 1.0)
    }

    /// Takes in what `player` has played since the last call. The tap is
    /// read, and kept on, only while `showing`. `track` is the one playing,
    /// `None` when stopped.
    pub fn refresh(
        &mut self,
        player: &Player,
        showing: bool,
        track: Option<&PathBuf>,
        now: Instant,
    ) {
        if track != self.track.as_ref() {
            self.track = track.cloned();
            self.integrated = Histogram::default();
        }
        let mut read = std::mem::take(&mut self.read);
        self.blocks = player.loudness_since(self.blocks, &mut read);
        self.take_loudness(&read);

        player.set_tap(showing);
        let dt = self
            .read_at
            .map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f32());
        self.read_at = Some(now);
        if !showing || track.is_none() {
            self.clear();
        } else {
            let want = FFT.max(2 * trace_frames(self.rate));
            let (rate, channels) = player.latest(want, &mut read);
            self.take_frames(rate, channels, &read, dt);
        }
        self.read = read;
    }

    /// Takes momentary loudness readings in LUFS, oldest first, as
    /// [`Scope::refresh`] reads them from the player.
    pub fn take_loudness(&mut self, readings: &[f32]) {
        for &lufs in readings {
            let lufs = (lufs >= SILENCE_LUFS).then_some(lufs);
            if let Some(l) = lufs {
                self.integrated.add(l);
            }
            if self.history.len() == HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(lufs);
        }
    }

    /// Shows `samples`, interleaved, `channels` a frame at `rate`, taken `dt`
    /// seconds after the last, as [`Scope::refresh`] reads them from the player.
    pub fn take_frames(&mut self, rate: u32, channels: u16, samples: &[f32], dt: f32) {
        if channels == 0 || rate == 0 {
            return self.clear();
        }
        self.samples.clear();
        self.samples.extend_from_slice(samples);
        self.rate = rate;
        self.channels = channels;
        self.mono.clear();
        self.mono.extend(
            self.samples
                .chunks_exact(channels as usize)
                .map(|f| f.iter().sum::<f32>() / f.len() as f32),
        );

        if self.analyser.as_ref().map(|a| a.0) != Some(rate) {
            self.analyser = Some((rate, Analyser::new(rate)));
        }
        let (_, analyser) = self.analyser.as_mut().expect("just set");
        let levels = analyser.levels(&self.mono);
        if self.bands.len() != BANDS {
            self.bands = levels.to_vec();
        }
        for (shown, &now) in self.bands.iter_mut().zip(&levels) {
            *shown = now.max(*shown - FALL_DB_PER_SEC * dt);
        }

        let c = correlation(&self.samples, channels);
        self.correlation = match (self.correlation, c) {
            (Some(was), Some(c)) => Some(was + (c - was) * (dt / CORRELATION_SECS).min(1.0)),
            (_, c) => c,
        };
    }

    fn clear(&mut self) {
        self.samples.clear();
        self.mono.clear();
        self.bands.clear();
        self.correlation = None;
    }

    /// Momentary loudness over the last minute, in LUFS, oldest first, ten
    /// readings a second; `None` for silence.
    pub fn history(&self) -> &VecDeque<Option<f32>> {
        &self.history
    }

    /// The playing track's integrated loudness so far, in LUFS.
    pub fn integrated(&self) -> Option<f32> {
        self.integrated.integrated()
    }

    /// Each band's level in dBFS, lowest band first, [`BANDS`] of them; empty
    /// when nothing is read.
    pub fn bands(&self) -> &[f32] {
        &self.bands
    }

    /// Where `hz` sits across the bands, from 0 to 1; `None` outside them
    /// or before audio is read.
    pub fn place(&self, hz: f32) -> Option<f32> {
        self.analyser.as_ref()?.1.place(hz)
    }

    /// The correlation of the first two channels, from -1 for opposite
    /// signals to 1 for identical ones; `None` for silence.
    pub fn correlation(&self) -> Option<f32> {
        self.correlation
    }

    /// Each frame as a point on the stereo image. See [`stereo_point`].
    pub fn stereo(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        let ch = self.channels.max(1) as usize;
        self.samples
            .chunks_exact(ch)
            .map(|f| stereo_point(f[0], f.get(1).copied().unwrap_or(f[0])))
    }

    /// The last 20 ms or so of the channels' mean, from a rising zero
    /// crossing where there is one, so a steady tone stands still.
    pub fn trace(&self) -> &[f32] {
        let width = trace_frames(self.rate).min(self.mono.len());
        let start = trigger(&self.mono, width);
        &self.mono[start..start + width]
    }
}

/// Frames the trace spans at `rate`.
fn trace_frames(rate: u32) -> usize {
    (rate / TRACE_PER_SEC) as usize
}

/// A frame of `left` and `right` on the stereo image: `x` from -1, all left,
/// to 1, all right; `y` up for signals in phase. Mono lies on the vertical
/// axis, and a full-scale frame on the diamond `|x| + |y| = 1`.
pub fn stereo_point(left: f32, right: f32) -> (f32, f32) {
    ((right - left) / 2.0, (left + right) / 2.0)
}

/// The correlation of the first two channels of interleaved `samples`;
/// `None` when either is silent. A single channel correlates with itself.
pub fn correlation(samples: &[f32], channels: u16) -> Option<f32> {
    let ch = channels.max(1) as usize;
    let (mut lr, mut ll, mut rr) = (0.0f64, 0.0f64, 0.0f64);
    for f in samples.chunks_exact(ch) {
        let (l, r) = (f[0] as f64, f.get(1).copied().unwrap_or(f[0]) as f64);
        lr += l * r;
        ll += l * l;
        rr += r * r;
    }
    // About -100 dBFS over a few thousand frames.
    const SILENT: f64 = 1e-7;
    (ll > SILENT && rr > SILENT).then(|| (lr / (ll * rr).sqrt()) as f32)
}

/// Where a trace `width` long starts in `mono`: the latest rising zero
/// crossing that leaves room for it, else as late as it fits.
pub fn trigger(mono: &[f32], width: usize) -> usize {
    let last = mono.len().saturating_sub(width);
    (1..=last)
        .rev()
        .find(|&i| mono[i - 1] < 0.0 && mono[i] >= 0.0)
        .unwrap_or(last)
}
