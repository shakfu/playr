//! A whole track in memory and its beat grid.

use crate::Error;

/// A constant-tempo beat grid: beat `n` falls at `t0 + n * 60 / bpm`
/// seconds of the track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    pub bpm: f64,
    /// A beat, ideally a downbeat, in seconds.
    pub t0: f64,
}

impl Grid {
    pub fn new(bpm: f64, t0: f64) -> Result<Self, Error> {
        match bpm.is_finite() && bpm > 0.0 && t0.is_finite() {
            true => Ok(Grid { bpm, t0 }),
            false => Err(Error::Grid),
        }
    }

    /// Seconds per beat.
    pub fn period(&self) -> f64 {
        60.0 / self.bpm
    }

    /// Beats from `t0` to `t` seconds; whole numbers fall on beats.
    pub fn beats(&self, t: f64) -> f64 {
        (t - self.t0) / self.period()
    }

    fn valid(&self) -> bool {
        Grid::new(self.bpm, self.t0).is_ok()
    }
}

/// Interleaved `f32` at the device's rate, with its grid if it has one.
#[derive(Debug, Clone)]
pub struct Track {
    samples: Vec<f32>,
    channels: usize,
    grid: Option<Grid>,
}

impl Track {
    pub fn new(samples: Vec<f32>, channels: u16, grid: Option<Grid>) -> Result<Self, Error> {
        if !(1..=2).contains(&channels) {
            return Err(Error::Channels(channels));
        }
        if !samples.len().is_multiple_of(channels as usize) {
            return Err(Error::Length);
        }
        if grid.is_some_and(|g| !g.valid()) {
            return Err(Error::Grid);
        }
        Ok(Track {
            samples,
            channels: channels as usize,
            grid,
        })
    }

    pub(crate) fn empty() -> Self {
        Track {
            samples: Vec::new(),
            channels: 1,
            grid: None,
        }
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn channels(&self) -> u16 {
        self.channels as u16
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }

    pub fn grid(&self) -> Option<Grid> {
        self.grid
    }

    /// Ignores a grid that [`Grid::new`] would refuse.
    pub(crate) fn set_grid(&mut self, grid: Option<Grid>) {
        if grid.is_none_or(|g| g.valid()) {
            self.grid = grid;
        }
    }

    /// Frame `pos`, cubic Hermite interpolated, as stereo. Frames outside
    /// the track are silence; a mono track feeds both channels.
    pub(crate) fn read(&self, pos: f64) -> [f32; 2] {
        let frames = self.frames() as i64;
        let i = pos.floor();
        let t = (pos - i) as f32;
        let i = i as i64;
        let mut out = [0.0; 2];
        // Every tap is outside the track.
        if i < -2 || i > frames {
            return out;
        }
        let c = self.channels;
        let at = |k: i64, ch: usize| match (0..frames).contains(&k) {
            true => self.samples[k as usize * c + ch],
            false => 0.0,
        };
        for (ch, o) in out.iter_mut().enumerate().take(c) {
            let x0 = at(i, ch);
            if t == 0.0 {
                *o = x0;
                continue;
            }
            *o = playr_dsp::hermite(at(i - 1, ch), x0, at(i + 1, ch), at(i + 2, ch), t);
        }
        if c == 1 {
            out[1] = out[0];
        }
        out
    }
}
