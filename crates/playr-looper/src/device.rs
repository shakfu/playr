//! The looper's own cpal output stream.

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, ErrorKind, SampleFormat, StreamConfig, SupportedStreamConfigRange};

use crate::{Error, Looper};

/// Frames per `Looper::process` call; a longer device buffer takes several.
const CHUNK: usize = 4096;

/// `want` when `device` can play it, else the device's default rate. A loop
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

/// Plays `looper` on `device` at its sample rate until the stream is dropped.
/// `on_error` receives the stream's errors, from the audio thread.
pub fn open(
    device: &Device,
    looper: Looper,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, Error> {
    let want = looper.tape().sample_rate();
    let range = usable(device)?
        .into_iter()
        .filter(|r| supports(r, want))
        .min_by_key(|r| (rank(r.sample_format()), r.channels().abs_diff(2)))
        .ok_or(Error::Rate(want))?;
    let config = StreamConfig {
        channels: range.channels(),
        sample_rate: want,
        buffer_size: cpal::BufferSize::Default,
    };
    let stream = match range.sample_format() {
        SampleFormat::F32 => build::<f32>(device, &config, looper, on_error, |v| v),
        SampleFormat::I32 => build::<i32>(device, &config, looper, on_error, |v| {
            (v.clamp(-1.0, 1.0) as f64 * i32::MAX as f64) as i32
        }),
        _ => build::<i16>(device, &config, looper, on_error, |v| {
            (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
        }),
    }?;
    stream.play().map_err(build_error)?;
    Ok(stream)
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

/// The stream, mapping the looper's stereo to the device's channels: their
/// mean on a mono device, silence on channels past the second.
fn build<T: cpal::SizedSample + Send + 'static>(
    device: &Device,
    config: &StreamConfig,
    mut looper: Looper,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
    conv: fn(f32) -> T,
) -> Result<cpal::Stream, Error> {
    let ch = config.channels as usize;
    let mut stereo = vec![0.0f32; CHUNK * 2];
    device
        .build_output_stream::<T, _, _>(
            *config,
            move |out: &mut [T], _| {
                for block in out.chunks_mut(CHUNK * ch) {
                    let frames = block.len() / ch;
                    let stereo = &mut stereo[..frames * 2];
                    looper.process(stereo);
                    for (o, s) in block.chunks_exact_mut(ch).zip(stereo.as_chunks::<2>().0) {
                        match ch {
                            1 => o[0] = conv((s[0] + s[1]) / 2.0),
                            _ => {
                                o[0] = conv(s[0]);
                                o[1] = conv(s[1]);
                                o[2..].fill(conv(0.0));
                            }
                        }
                    }
                }
            },
            on_error,
            None,
        )
        .map_err(build_error)
}
