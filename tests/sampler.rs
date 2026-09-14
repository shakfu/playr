//! Waveform peaks and the sampler view's drawing arithmetic.

use std::sync::atomic::AtomicBool;

use playr::ui::sampler::{
    braille_rows, db_height, envelope_rows, fmt_frames, window, DB_FLOOR, MIN_FRAMES_PER_COLUMN,
};
use playr::wave::{Peaks, BUCKET};

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

#[test]
fn the_window_fits_the_track_then_zooms_around_the_playhead() {
    // 100 columns of a 1,000,000-frame track: 10,000 frames a column, rounded
    // up to whole buckets of 32.
    assert_eq!(window(1_000_000, 100, 0, 500_000), (0, 10_016, 0));
    // One step in halves the frames a column, centred on the playhead, and
    // starts on a bucket.
    assert_eq!(window(1_000_000, 100, 1, 500_000), (248_800, 5_024, 1));
    // Near either end the window stops at the track's edge.
    assert_eq!(window(1_000_000, 100, 1, 10_000), (0, 5_024, 1));
    assert_eq!(window(1_000_000, 100, 1, 990_000), (497_600, 5_024, 1));
    let (start, per_column, _) = window(987_654, 77, 3, 432_109);
    assert_eq!((start % BUCKET, per_column % BUCKET), (0, 0));
    // Zooming stops once a column shows the fewest frames it may.
    let (_, per_column, zoom) = window(1_000_000, 100, 40, 0);
    assert_eq!(per_column, MIN_FRAMES_PER_COLUMN);
    assert_eq!(zoom, 8, "10,000 >> 8 is 39, the first step at or below 64");
    // A track shorter than the view is shown whole.
    assert_eq!(window(50, 100, 3, 0), (0, 1, 0));
}

#[test]
fn the_envelope_draws_rms_inside_peak_in_eighths() {
    use playr::ui::sampler::{Cell, Fill};
    // Two rows, 16 eighths. Columns as (rms, peak).
    let rows = envelope_rows(
        &[
            (0.0, 0.0),
            (0.0, 1.0 / 16.0),
            (0.25, 1.0),
            (0.5, 0.75),
            (1.0, 1.0),
            (0.8, 0.2),
        ],
        2,
    );
    let text = |row: &Vec<Cell>| row.iter().map(|c| c.glyph).collect::<String>();
    assert_eq!(text(&rows[0]), "  \u{2588}\u{2584}\u{2588}\u{2585}");
    assert_eq!(text(&rows[1]), " \u{2581}\u{2584}\u{2588}\u{2588}\u{2588}");
    let cell = |glyph, fg, behind| Cell { glyph, fg, behind };
    // A peak alone.
    assert_eq!(rows[1][1], cell('\u{2581}', Fill::Peak, Fill::Empty));
    // RMS a quarter up, peak to the top: the RMS glyph over the peak's colour,
    // then the peak filling the row above.
    assert_eq!(rows[1][2], cell('\u{2584}', Fill::Rms, Fill::Peak));
    assert_eq!(rows[0][2], cell('\u{2588}', Fill::Peak, Fill::Empty));
    // RMS to half, peak ending in the next cell: that cell draws the peak alone.
    assert_eq!(rows[1][3], cell('\u{2588}', Fill::Rms, Fill::Empty));
    assert_eq!(rows[0][3], cell('\u{2584}', Fill::Peak, Fill::Empty));
    // A peak below the RMS is drawn as the RMS: no peak shows.
    assert_eq!(rows[0][5], cell('\u{2585}', Fill::Rms, Fill::Empty));
}

#[test]
fn braille_draws_each_dot_column_between_its_extremes() {
    // One row: four dot rows, top +1 and bottom -1. A column from -1 to +1
    // fills all four dots; one at the top fills only the top dot.
    let rows = braille_rows(&[(-1.0, 1.0), (1.0, 1.0)], 1);
    // Left column all dots (1,2,3,7), right column top dot (4).
    assert_eq!(rows, ["\u{284f}"]);

    // Two rows, eight dot rows, with no row exactly at 0. A quiet column from
    // -0.1 to 0.1 spans the two middle dot rows: the bottom dot of the top cell
    // (0x40) and the top dot of the bottom cell (0x01). Silence rounds to the
    // lower of the two (0x08, right column), and -1 to the bottom dot (0x40).
    let rows = braille_rows(&[(-0.1, 0.1), (0.0, 0.0), (-1.0, -1.0)], 2);
    assert_eq!(rows[0], "\u{2840}\u{2800}");
    assert_eq!(rows[1], "\u{2809}\u{2840}");
    // An empty column, as past the end of the track, draws nothing.
    assert_eq!(braille_rows(&[(1.0, -1.0)], 1), ["\u{2800}"]);
}

#[test]
fn frames_are_shown_to_the_millisecond() {
    assert_eq!(fmt_frames(0, 44_100), "0:00.000");
    assert_eq!(fmt_frames(154_881, 44_100), "0:03.512");
    assert_eq!(fmt_frames(44_100 * 125, 44_100), "2:05.000");
}

#[test]
fn the_db_scale_runs_from_the_floor_to_full_scale() {
    assert_eq!(DB_FLOOR, -48.0);
    assert_eq!(db_height(1.0), 1.0);
    assert_eq!(db_height(0.0), 0.0);
    let at = |db: f32| db_height(10f32.powf(db / 20.0));
    assert!((at(-12.0) - 0.75).abs() < 1e-5);
    assert!((at(-24.0) - 0.5).abs() < 1e-5);
    assert_eq!(at(-60.0), 0.0, "below the floor");
    assert_eq!(db_height(2.0), 1.0, "above full scale");
}
