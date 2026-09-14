use playr_core::audio::resample::Resample;

/// Interleaved stereo sine at `freq` Hz.
fn sine(rate: u32, freq: f32, frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|i| {
            let v = (std::f32::consts::TAU * freq * i as f32 / rate as f32).sin() * 0.5;
            [v, v]
        })
        .collect()
}

/// Amplitude of channel 0 at `freq`, by Goertzel.
///
/// Zero-crossing counting is not usable here: resampled output ripples around
/// zero, which multiplies the crossing count and reports a frequency far from
/// the real one even when the signal is clean.
fn amplitude_at(buf: &[f32], rate: u32, channels: usize, freq: f32) -> f32 {
    let ch0: Vec<f32> = buf.iter().step_by(channels).copied().collect();
    let w = std::f32::consts::TAU * freq / rate as f32;
    let c = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &x in &ch0 {
        let s0 = x + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let re = s1 - s2 * w.cos();
    let im = s2 * w.sin();
    (re * re + im * im).sqrt() / (ch0.len() as f32 / 2.0)
}

/// Asserts the tone sits at `freq` and nowhere else nearby.
fn assert_clean_tone(buf: &[f32], rate: u32, channels: usize, freq: f32) {
    let at = amplitude_at(buf, rate, channels, freq);
    assert!(
        at > 0.4 && at < 0.6,
        "amplitude at {freq} Hz is {at}, expected ~0.5"
    );
    for off in [-60.0, -30.0, 30.0, 60.0] {
        let side = amplitude_at(buf, rate, channels, freq + off);
        assert!(
            side < at / 5.0,
            "spurious energy at {} Hz: {side} vs {at}",
            freq + off
        );
    }
}

#[test]
fn output_length_follows_the_rate_ratio() {
    let mut r = Resample::new(44100, 48000, 2, 1.0).unwrap();
    let input = sine(44100, 440.0, 44100);
    let mut out = Vec::new();
    r.push(&input, &mut out);
    r.flush(&mut out);

    let frames = out.len() / 2;
    let expected = 48000.0;
    let err = (frames as f32 - expected).abs() / expected;
    assert!(err < 0.02, "got {frames} frames, expected ~{expected}");
}

#[test]
fn resampling_preserves_the_tone() {
    let mut r = Resample::new(44100, 48000, 2, 1.0).unwrap();
    let input = sine(44100, 440.0, 44100);
    let mut out = Vec::new();
    r.push(&input, &mut out);
    r.flush(&mut out);

    // Measure a steady window, clear of the startup transient and the
    // silence-padded flush at the end.
    assert_clean_tone(&out[10_000 * 2..40_000 * 2], 48000, 2, 440.0);
}

#[test]
fn downsampling_works_too() {
    let mut r = Resample::new(96000, 48000, 2, 1.0).unwrap();
    let input = sine(96000, 440.0, 96000);
    let mut out = Vec::new();
    r.push(&input, &mut out);
    r.flush(&mut out);

    let frames = out.len() / 2;
    let err = (frames as f32 - 48000.0).abs() / 48000.0;
    assert!(err < 0.02, "got {frames} frames, expected ~48000");

    assert_clean_tone(&out[10_000 * 2..40_000 * 2], 48000, 2, 440.0);
}

#[test]
fn output_is_continuous_across_small_pushes() {
    // Feeding the same signal in ragged pieces must give the same result as
    // feeding it whole; a resampler that resets per call would not.
    let input = sine(44100, 440.0, 44100);

    let mut whole = Vec::new();
    let mut r1 = Resample::new(44100, 48000, 2, 1.0).unwrap();
    r1.push(&input, &mut whole);
    r1.flush(&mut whole);

    let mut pieces = Vec::new();
    let mut r2 = Resample::new(44100, 48000, 2, 1.0).unwrap();
    for chunk in input.chunks(577 * 2) {
        r2.push(chunk, &mut pieces);
    }
    r2.flush(&mut pieces);

    assert_eq!(
        whole.len(),
        pieces.len(),
        "chunked feed changed the output length"
    );
    let max_diff = whole
        .iter()
        .zip(&pieces)
        .fold(0f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(max_diff < 1e-5, "chunked feed diverged by {max_diff}");
}

#[test]
fn mono_and_multichannel_shapes_are_respected() {
    for ch in [1u16, 2, 6] {
        let mut r = Resample::new(44100, 48000, ch, 1.0).unwrap();
        let frames = 44100;
        let input = vec![0.1f32; frames * ch as usize];
        let mut out = Vec::new();
        r.push(&input, &mut out);
        r.flush(&mut out);
        assert_eq!(
            out.len() % ch as usize,
            0,
            "{ch}ch output is not frame-aligned"
        );
        let got = out.len() / ch as usize;
        assert!(
            (got as f32 - 48000.0).abs() / 48000.0 < 0.02,
            "{ch}ch gave {got} frames"
        );
    }
}

// --- varispeed ---

/// Plays `input` at `speed` through a same-rate resampler and returns the output.
fn at_speed(input: &[f32], rate: u32, speed: f64) -> Vec<f32> {
    let mut r = Resample::new(rate, rate, 2, speed).unwrap();
    let mut out = Vec::new();
    r.push(input, &mut out);
    r.flush(&mut out);
    out
}

#[test]
fn playing_an_octave_up_doubles_the_frequency() {
    // True varispeed: pitch rises with tempo, as on tape. +12 semitones is 2x.
    let input = sine(44100, 440.0, 44100);
    let out = at_speed(&input, 44100, 2.0);
    assert_clean_tone(&out[2_000 * 2..18_000 * 2], 44100, 2, 880.0);
}

#[test]
fn playing_an_octave_down_halves_the_frequency() {
    let input = sine(44100, 440.0, 44100);
    let out = at_speed(&input, 44100, 0.5);
    assert_clean_tone(&out[10_000 * 2..70_000 * 2], 44100, 2, 220.0);
}

#[test]
fn one_semitone_up_is_a_semitone_of_pitch() {
    // 440Hz is A4; one semitone up is A#4 at 466.16Hz.
    let input = sine(44100, 440.0, 44100);
    let out = at_speed(&input, 44100, 2f64.powf(1.0 / 12.0));
    assert_clean_tone(&out[5_000 * 2..35_000 * 2], 44100, 2, 466.16);
}

#[test]
fn duration_scales_inversely_with_speed() {
    let input = sine(44100, 440.0, 44100);
    for (speed, want) in [(2.0f64, 22_050.0f32), (0.5, 88_200.0), (1.5, 29_400.0)] {
        let frames = at_speed(&input, 44100, speed).len() / 2;
        let err = (frames as f32 - want).abs() / want;
        assert!(
            err < 0.03,
            "at {speed}x got {frames} frames, expected ~{want}"
        );
    }
}

/// The resampler takes input in chunks of 1024 frames, so these lengths leave
/// a final partial chunk of 0, 1, 784 and 1023 frames, and one shorter than
/// the filter delay.
const CLIP_LENGTHS: [usize; 5] = [10_240, 10_241, 10_000, 10_239, 50];

#[test]
fn a_flushed_clip_is_exactly_as_long_as_the_ratio_and_ends_on_signal() {
    let cases = [
        (44100u32, 48000u32, 1.0f64),
        (48000, 44100, 1.0),
        (44100, 44100, 2f64.powf(1.0 / 12.0)),
        (44100, 44100, 2.0),
        (44100, 44100, 0.5),
    ];
    for (rate_in, rate_out, speed) in cases {
        for frames in CLIP_LENGTHS {
            let mut r = Resample::new(rate_in, rate_out, 2, speed).unwrap();
            let mut out = Vec::new();
            r.push(&vec![0.5; frames * 2], &mut out);
            r.flush(&mut out);

            let case = format!("{rate_in}->{rate_out} at {speed:.3}x, {frames} frames");
            let want = frames as f64 * rate_out as f64 / (rate_in as f64 * speed);
            let got = out.len() / 2;
            assert!(
                (got as f64 - want).abs() <= 1.0,
                "{case}: got {got} frames, want {want:.1}"
            );
            // Padding shows up as silence at the end, which a gapless boundary
            // carries as a dropout. The last frame may fall on the step to zero.
            let silent = out.iter().rev().take_while(|v| v.abs() < 0.1).count() / 2;
            assert!(silent <= 1, "{case}: ends in {silent} silent frames");
        }
    }
}

#[test]
fn normal_speed_is_unchanged() {
    let input = sine(44100, 440.0, 44100);
    let out = at_speed(&input, 44100, 1.0);
    let frames = out.len() / 2;
    assert!(
        (frames as f32 - 44100.0).abs() / 44100.0 < 0.03,
        "got {frames} frames"
    );
    assert_clean_tone(&out[5_000 * 2..35_000 * 2], 44100, 2, 440.0);
}
