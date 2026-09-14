//! Waveform peaks: extremes and RMS over any range, read from a file.

use std::sync::atomic::AtomicBool;

use playr_core::wave::{Peaks, BUCKET};

/// Deterministic noise in -1..1.
fn noise(len: usize, channels: usize) -> Vec<f32> {
    let mut seed = 99u32;
    (0..len * channels)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0
        })
        .collect()
}

#[test]
fn a_range_of_peaks_covers_its_frames_and_little_more() {
    let channels = 2;
    let data = noise(100_003, channels);
    let peaks = Peaks::from_interleaved(&data, channels, 48_000);
    assert_eq!((peaks.frames, peaks.rate), (100_003, 48_000));

    let extent = |start: u64, end: u64| {
        data[start as usize * channels..end as usize * channels]
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &s| {
                (lo.min(s), hi.max(s))
            })
    };
    for (start, end) in [
        (0, 1),
        (5, 37),
        (1_000, 1_900),
        (12_345, 67_890),
        (0, 100_003),
        (99_990, 200_000),
    ] {
        let e = peaks.range(start, end).unwrap();
        let (lo, hi) = (e.min, e.max);
        let (true_lo, true_hi) = extent(start, end.min(100_003));
        assert!(
            lo <= true_lo && hi >= true_hi,
            "{start}..{end} misses frames"
        );
        // Widened to whole buckets, so by less than one at each end.
        let slack = BUCKET - 1;
        let (wide_lo, wide_hi) = extent(start.saturating_sub(slack), (end + slack).min(100_003));
        assert!(
            lo >= wide_lo && hi <= wide_hi,
            "{start}..{end} reads too far"
        );
    }
    // On bucket boundaries nothing is widened, so the RMS is exact.
    for (start, end) in [(0, 32), (64, 96_000), (32_000, 100_003), (0, 100_003)] {
        let e = peaks.range(start, end).unwrap();
        let samples = &data[start as usize * channels..end.min(100_003) as usize * channels];
        let rms = (samples
            .iter()
            .map(|&s| f64::from(s) * f64::from(s))
            .sum::<f64>()
            / samples.len() as f64)
            .sqrt() as f32;
        assert!(
            (e.rms - rms).abs() < 1e-5,
            "{start}..{end}: rms {} not {rms}",
            e.rms
        );
    }
    assert_eq!(peaks.range(50, 50), None);
    assert_eq!(peaks.range(200_000, 300_000), None);
    let loudest = data.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert_eq!(peaks.loudest(), loudest);
}

#[test]
fn peaks_read_from_a_file_match_its_samples_and_can_be_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("n.wav");
    let data = noise(20_000, 1);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    let ints: Vec<i16> = data.iter().map(|s| (s * 32_767.0) as i16).collect();
    for &s in &ints {
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();

    let decoded: Vec<f32> = ints.iter().map(|&s| s as f32 / 32_768.0).collect();
    let read = Peaks::read(&path, &AtomicBool::new(false))
        .unwrap()
        .unwrap();
    assert_eq!(read, Peaks::from_interleaved(&decoded, 1, 44_100));
    assert_eq!(Peaks::read(&path, &AtomicBool::new(true)), Ok(None));
    assert!(Peaks::read(&dir.path().join("gone.wav"), &AtomicBool::new(false)).is_err());
}
