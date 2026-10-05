//! The DJ engine's own cpal output stream, opened as the looper's is.

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, ErrorKind, SampleFormat, StreamConfig, SupportedStreamConfigRange};

use crate::{Engine, Error};

/// Frames per `Engine::process_channels` call; a longer device buffer takes
/// several.
const CHUNK: usize = 4096;

/// `want` when `device` can play it, else the device's default rate. A track
/// at another rate must be resampled to the one returned before loading.
pub fn rate(device: &Device, want: u32) -> Result<u32, Error> {
    if usable(device)?.iter().any(|r| supports(r, want)) {
        return Ok(want);
    }
    device
        .default_output_config()
        .map(|c| c.sample_rate())
        .map_err(|_| Error::NoConfig)
}

/// Plays `engine` on `device` at its sample rate until the stream is dropped,
/// with 4 channels or more where the device offers them, for the cue on 3
/// and 4. Returns the stream and its channel count. `on_error` receives the
/// stream's errors, from the audio thread.
pub fn open(
    device: &Device,
    engine: Engine,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<(cpal::Stream, u16), Error> {
    let want = engine.mixer().sample_rate();
    let range = usable(device)?
        .into_iter()
        .filter(|r| supports(r, want))
        .min_by_key(|r| {
            let c = r.channels();
            (rank(r.sample_format()), c < 4, c.abs_diff(4))
        })
        .ok_or(Error::Rate(want))?;
    let config = StreamConfig {
        channels: range.channels(),
        sample_rate: want,
        buffer_size: cpal::BufferSize::Default,
    };
    let stream = match range.sample_format() {
        SampleFormat::F32 => build::<f32>(device, &config, engine, on_error, |v| v),
        SampleFormat::I32 => build::<i32>(device, &config, engine, on_error, |v| {
            (v.clamp(-1.0, 1.0) as f64 * i32::MAX as f64) as i32
        }),
        _ => build::<i16>(device, &config, engine, on_error, |v| {
            (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
        }),
    }?;
    stream.play().map_err(build_error)?;
    Ok((stream, config.channels))
}

fn usable(device: &Device) -> Result<Vec<SupportedStreamConfigRange>, Error> {
    Ok(device
        .supported_output_configs()
        .map_err(build_error)?
        .filter(|r| rank(r.sample_format()) < 3)
        .collect())
}

fn supports(r: &SupportedStreamConfigRange, rate: u32) -> bool {
    r.min_sample_rate() <= rate && rate <= r.max_sample_rate()
}

/// Formats the stream can write, best first; 3 for one it cannot.
fn rank(f: SampleFormat) -> u8 {
    match f {
        SampleFormat::F32 => 0,
        SampleFormat::I32 => 1,
        SampleFormat::I16 => 2,
        _ => 3,
    }
}

fn build_error(e: cpal::Error) -> Error {
    match e.kind() {
        ErrorKind::DeviceBusy => Error::Busy,
        _ => Error::Device(e.to_string()),
    }
}

/// The stream, playing `engine` through [`render`].
fn build<T: cpal::SizedSample + Send + 'static>(
    device: &Device,
    config: &StreamConfig,
    mut engine: Engine,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
    conv: fn(f32) -> T,
) -> Result<cpal::Stream, Error> {
    let ch = config.channels as usize;
    let mut four = vec![0.0f32; CHUNK * 4];
    device
        .build_output_stream::<T, _, _>(
            *config,
            move |out: &mut [T], _| render(&mut engine, out, ch, &mut four, conv),
            on_error,
            None,
        )
        .map_err(build_error)
}

/// Fills `out`, `channels` interleaved, from `engine`, as the stream does:
/// the first two channels' mean on a mono device, the first two on a stereo
/// one, silence past the fourth. `four` is the engine's scratch, 4 samples
/// a frame; a longer `out` takes several calls of the engine.
pub fn render<T: Copy>(
    engine: &mut Engine,
    out: &mut [T],
    channels: usize,
    four: &mut [f32],
    conv: fn(f32) -> T,
) {
    let ch = channels;
    let stride = ch.clamp(2, 4);
    for block in out.chunks_mut(four.len() / 4 * ch) {
        let frames = block.len() / ch;
        let mix = &mut four[..frames * stride];
        engine.process_channels(mix, stride);
        for (o, s) in block.chunks_exact_mut(ch).zip(mix.chunks_exact(stride)) {
            match ch {
                1 => o[0] = conv((s[0] + s[1]) / 2.0),
                _ => {
                    for (o, s) in o.iter_mut().zip(s) {
                        *o = conv(*s);
                    }
                    o[stride..].fill(conv(0.0));
                }
            }
        }
    }
}
