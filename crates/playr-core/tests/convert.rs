//! Conversion across a track change, without an output device.

use cpal::SampleFormat;
use playr_core::audio::convert::Converter;
use playr_core::audio::output::Plan;
use playr_core::audio::{speed_for, Spec};

const STEREO_44K: Plan = Plan {
    rate: 44100,
    channels: 2,
    format: SampleFormat::F32,
};

fn spec(rate: u32, channels: u16) -> Spec {
    Spec { rate, channels }
}

/// Interleaved noise, the same in every channel, from a fixed seed.
fn noise(frames: usize, channels: usize) -> Vec<f32> {
    let mut seed = 0x1234_5678u32;
    (0..frames)
        .flat_map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let v = (seed as f32 / u32::MAX as f32 - 0.5) * 0.6;
            std::iter::repeat_n(v, channels)
        })
        .collect()
}

fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

#[test]
fn a_continued_track_matches_resampling_one_stream() {
    let (src, speed) = (spec(44100, 2), speed_for(1));
    let input = noise(40_000, 2);
    // Not on a chunk boundary, and not on a whole output frame.
    let (a, b) = input.split_at(17_333 * 2);

    let mut whole = Vec::new();
    let mut one = Converter::new(src, STEREO_44K, speed);
    one.push(&input, &mut whole);
    one.finish(&mut whole);

    let mut joined = Vec::new();
    let mut conv = Converter::new(src, STEREO_44K, speed);
    conv.push(a, &mut joined);
    assert!(conv.continues(src, STEREO_44K, speed));
    conv.push(b, &mut joined);
    conv.finish(&mut joined);

    assert_eq!(joined.len(), whole.len());
    let diff = max_diff(&joined, &whole);
    assert!(diff < 1e-5, "the join differs by {diff}");
}

#[test]
fn a_track_continues_only_with_the_same_format_and_speed() {
    let (src, speed) = (spec(44100, 2), speed_for(1));
    let conv = Converter::new(src, STEREO_44K, speed);
    assert!(conv.continues(src, STEREO_44K, speed));
    assert!(!conv.continues(spec(48000, 2), STEREO_44K, speed));
    assert!(!conv.continues(spec(44100, 1), STEREO_44K, speed));
    assert!(!conv.continues(src, STEREO_44K, speed_for(2)));
    let other = Plan {
        rate: 48000,
        ..STEREO_44K
    };
    assert!(!conv.continues(src, other, speed));

    let mut finished = Converter::new(src, STEREO_44K, speed);
    finished.finish(&mut Vec::new());
    assert!(
        !finished.continues(src, STEREO_44K, speed),
        "a finished converter continued"
    );
}

#[test]
fn finishing_twice_adds_nothing() {
    let mut conv = Converter::new(spec(44100, 2), STEREO_44K, speed_for(1));
    let mut out = Vec::new();
    conv.push(&noise(5_000, 2), &mut out);
    conv.finish(&mut out);
    let len = out.len();
    conv.finish(&mut out);
    assert_eq!(out.len(), len);
}

#[test]
fn a_mono_source_is_resampled_as_mono() {
    // Mono fanned out to stereo must match a stereo source whose channels are
    // equal. Resampling mono as if it were stereo paired up adjacent samples
    // and distorted everything above a few kHz.
    let speed = speed_for(1);
    let mut from_mono = Vec::new();
    let mut conv = Converter::new(spec(44100, 1), STEREO_44K, speed);
    conv.push(&noise(20_000, 1), &mut from_mono);
    conv.finish(&mut from_mono);

    let mut from_stereo = Vec::new();
    let mut conv = Converter::new(spec(44100, 2), STEREO_44K, speed);
    conv.push(&noise(20_000, 2), &mut from_stereo);
    conv.finish(&mut from_stereo);

    assert_eq!(from_mono.len(), from_stereo.len());
    let diff = max_diff(&from_mono, &from_stereo);
    assert!(diff < 1e-5, "mono differs from dual mono by {diff}");
}

#[test]
fn native_rate_at_normal_speed_passes_through() {
    let input = noise(3_000, 2);
    let mut out = Vec::new();
    let mut conv = Converter::new(spec(44100, 2), STEREO_44K, 1.0);
    assert!(!conv.resampling());
    conv.push(&input, &mut out);
    conv.finish(&mut out);
    assert_eq!(out, input);
}
