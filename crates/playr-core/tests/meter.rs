//! Loudness metering against ITU-R BS.1770-4 and EBU Tech 3341.

use playr_core::audio::meter::{k_weighting, Meter, SILENCE_LUFS};

/// Feeds `secs` of a stereo 1 kHz sine at `dbfs` peak, and returns the last
/// momentary loudness. `channels` says which of the two carry it.
fn loudness_of_sine(rate: u32, dbfs: f64, secs: f64, channels: [bool; 2]) -> f32 {
    let amplitude = 10f64.powf(dbfs / 20.0);
    let mut meter = Meter::new(rate, 2);
    let mut last = f32::NEG_INFINITY;
    for i in 0..(rate as f64 * secs) as usize {
        let v =
            (amplitude * (std::f64::consts::TAU * 1000.0 * i as f64 / rate as f64).sin()) as f32;
        for on in channels {
            if let Some(l) = meter.sample(if on { v } else { 0.0 }) {
                last = l;
            }
        }
    }
    last
}

#[test]
fn k_weighting_at_48k_matches_the_standard() {
    // BS.1770-4, Annex 1, tables 1 and 2.
    let [shelf, high_pass] = k_weighting(48000);
    let close = |got: f64, want: f64| (got - want).abs() < 1e-8;
    for (got, want) in [
        (shelf.b0, 1.53512485958697),
        (shelf.b1, -2.69169618940638),
        (shelf.b2, 1.19839281085285),
        (shelf.a1, -1.69065929318241),
        (shelf.a2, 0.73248077421585),
        (high_pass.b0, 1.0),
        (high_pass.b1, -2.0),
        (high_pass.b2, 1.0),
        (high_pass.a1, -1.99004745483398),
        (high_pass.a2, 0.99007225036621),
    ] {
        assert!(close(got, want), "got {got}, want {want}");
    }
}

#[test]
fn a_stereo_sine_reads_its_level_in_lufs() {
    // EBU Tech 3341, test signals 1 and 2.
    for dbfs in [-23.0, -33.0] {
        let l = loudness_of_sine(48000, dbfs, 2.0, [true, true]);
        assert!((l as f64 - dbfs).abs() < 0.1, "{dbfs} dBFS read {l} LUFS");
    }
}

#[test]
fn a_full_scale_sine_in_one_channel_reads_minus_3_01() {
    // BS.1770-4: a 0 dB FS 1 kHz sine in the left, centre or right channel.
    let l = loudness_of_sine(48000, 0.0, 2.0, [true, false]);
    assert!((l + 3.01).abs() < 0.1, "read {l} LUFS");
}

#[test]
fn other_sample_rates_read_the_same() {
    for rate in [44100, 96000] {
        let l = loudness_of_sine(rate, -23.0, 2.0, [true, true]);
        assert!((l + 23.0).abs() < 0.1, "{rate} Hz read {l} LUFS");
    }
}

#[test]
fn loudness_falls_to_silence_within_the_window() {
    let mut meter = Meter::new(48000, 2);
    let mut last = 0.0;
    for i in 0..48000 {
        let v = (0.1 * (std::f64::consts::TAU * 1000.0 * i as f64 / 48000.0).sin()) as f32;
        for _ in 0..2 {
            if let Some(l) = meter.sample(v) {
                last = l;
            }
        }
    }
    assert!(last > -30.0, "tone read {last}");
    // 500 ms of silence is more than the 400 ms window.
    for _ in 0..24000 * 2 {
        if let Some(l) = meter.sample(0.0) {
            last = l;
        }
    }
    assert!(last < SILENCE_LUFS, "silence read {last}");
}

#[test]
fn the_peak_is_the_largest_magnitude_since_it_was_taken() {
    let mut meter = Meter::new(48000, 2);
    for v in [0.1, -0.7, 0.3, 0.2] {
        meter.sample(v);
    }
    assert_eq!(meter.take_peak(), 0.7);
    assert_eq!(meter.take_peak(), 0.0, "taking the peak did not reset it");
}
