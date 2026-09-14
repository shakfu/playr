//! Sample rate conversion, used when the device cannot open the source rate and
//! for varispeed.
//!
//! A sinc resampler is used rather than linear interpolation: linear resampling
//! folds audible aliasing into the passband, which defeats the point of
//! decoding losslessly in the first place.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Async, FixedAsync, Indexing, Resampler, SincInterpolationParameters, WindowFunction};

/// Frames of input consumed per resampler call. 1024 keeps latency near 23ms at
/// 44.1kHz while keeping the per-call overhead small.
const CHUNK: usize = 1024;

/// Widest ratio change the resampler is built to accept, relative to its
/// starting ratio. Playback speed spans 0.5x to 2.0x, so a factor of two either
/// way is the most that can be asked of it.
const MAX_RATIO_SHIFT: f64 = 2.1;

pub struct Resample {
    inner: Async<f32>,
    channels: usize,
    /// Input frames not yet consumed, interleaved.
    pending: Vec<f32>,
    /// Reusable output scratch, interleaved.
    out_buf: Vec<f32>,
    out_max: usize,
    /// Output frames of resampler latency still to be discarded.
    ///
    /// The streaming API emits its filter delay as leading silence. Passing it
    /// through would push every track late by that much and desynchronise the
    /// reported position from what is audible.
    delay_left: usize,
    /// Input frames consumed and output frames emitted, for `flush` to end on.
    frames_in: u64,
    frames_out: u64,
}

impl Resample {
    /// Builds a resampler from `rate_in` to `rate_out`, playing at `speed`.
    ///
    /// `speed` above 1.0 consumes the source faster, which raises pitch along
    /// with tempo, as a tape machine does. It folds into the ratio rather than
    /// being a separate stage.
    pub fn new(rate_in: u32, rate_out: u32, channels: u16, speed: f64) -> Option<Self> {
        let channels = channels as usize;
        if rate_in == 0 || rate_out == 0 || channels == 0 || speed <= 0.0 {
            return None;
        }
        let ratio = rate_out as f64 / (rate_in as f64 * speed);
        let params = SincInterpolationParameters::new(256, WindowFunction::BlackmanHarris2);
        let inner = Async::<f32>::new_sinc(
            ratio,
            MAX_RATIO_SHIFT,
            &params,
            CHUNK,
            channels,
            FixedAsync::Input,
        )
        .ok()?;
        let out_max = inner.output_frames_max();
        let delay_left = inner.output_delay();
        Some(Resample {
            inner,
            channels,
            pending: Vec::with_capacity(CHUNK * channels * 2),
            out_buf: vec![0.0; out_max * channels],
            out_max,
            delay_left,
            frames_in: 0,
            frames_out: 0,
        })
    }

    /// Feeds interleaved input and appends all resampled output to `sink`.
    ///
    /// Input that does not fill a whole chunk is held until the next call, so
    /// the output is continuous across calls.
    pub fn push(&mut self, input: &[f32], sink: &mut Vec<f32>) {
        self.pending.extend_from_slice(input);
        loop {
            let need = self.inner.input_frames_next();
            if self.pending.len() < need * self.channels {
                return;
            }
            self.run(need, None, sink);
            self.frames_in += need as u64;
            self.pending.drain(..need * self.channels);
        }
    }

    /// Emits the rest of the output at end of stream, `input * ratio` frames in all.
    pub fn flush(&mut self, sink: &mut Vec<f32>) {
        let have = self.pending.len() / self.channels;
        self.frames_in += have as u64;
        let want = (self.frames_in as f64 * self.inner.resample_ratio()).round() as u64;
        let start = sink.len();
        let already = self.frames_out;

        // The filter still holds `output_delay` frames of input, so feed
        // silence until they are out, then cut the padding off.
        let mut partial = have;
        while self.frames_out < want {
            let need = self.inner.input_frames_next();
            self.pending.resize(need * self.channels, 0.0);
            if self.run(need, Some(partial), sink) == 0 {
                break;
            }
            partial = 0;
            self.pending.clear();
        }
        self.pending.clear();
        sink.truncate(start + (want.saturating_sub(already) as usize) * self.channels);
        self.frames_out = self.frames_out.min(want);
    }

    /// Resamples one chunk into `sink`, returning the frames emitted.
    fn run(&mut self, need: usize, partial: Option<usize>, sink: &mut Vec<f32>) -> usize {
        // Split the borrow: `process_into_buffer` needs `&mut inner` alongside
        // `&pending` and `&mut out_buf`.
        let Resample {
            inner,
            channels,
            pending,
            out_buf,
            out_max,
            delay_left,
            frames_out,
            ..
        } = self;
        let channels = *channels;

        let Ok(input) = InterleavedSlice::new(&pending[..need * channels], channels, need) else {
            return 0;
        };
        let indexing = Indexing {
            input_offset: 0,
            output_offset: 0,
            partial_len: partial,
            active_channels_mask: None,
        };
        let produced = {
            let Ok(mut output) = InterleavedSlice::new_mut(&mut out_buf[..], channels, *out_max)
            else {
                return 0;
            };
            inner
                .process_into_buffer(&input, &mut output, Some(&indexing))
                .map(|(_, out)| out)
        };
        let Ok(out_frames) = produced else {
            return 0;
        };
        let skip = (*delay_left).min(out_frames);
        *delay_left -= skip;
        sink.extend_from_slice(&out_buf[skip * channels..out_frames * channels]);
        *frames_out += (out_frames - skip) as u64;
        out_frames - skip
    }
}
