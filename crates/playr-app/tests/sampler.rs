//! The sampler view's geometry: which frames each column shows, and the dB scale.

use std::sync::Arc;
use std::time::Duration;

use playr_app::sampler::{
    db_height, fmt_frames, peaks_of, plan_text, window, Layout, Sampler, Wave, DB_FLOOR,
    MIN_FRAMES_PER_COLUMN,
};
use playr_app::Display;
use playr_core::wave::Peaks;
use playr_core::wave::BUCKET;

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

/// 100 columns of 4 s at 8000 Hz: a quiet second, a loud one, then silence.
fn layout(zoom: u32, position: Duration, marks: &[Duration]) -> Layout {
    let data: Vec<f32> = (0..32_000)
        .map(|i| match i {
            0..8000 => 0.1,
            8000..16_000 => {
                if i % 2 == 0 {
                    0.8
                } else {
                    -0.8
                }
            }
            _ => 0.0,
        })
        .collect();
    let peaks = Arc::new(Peaks::from_interleaved(&data, 1, 8000));
    Layout::new(peaks, 100, zoom, position, marks)
}

#[test]
fn a_layout_places_the_playhead_marks_and_region_in_columns() {
    let secs = Duration::from_secs;
    let l = layout(0, Duration::from_millis(1500), &[secs(1), secs(2)]);
    // 320 frames a column, whole buckets, from the start.
    assert_eq!((l.start, l.per_column, l.zoom), (0, 320, 0));
    assert_eq!(l.playhead(), Some(37));
    assert_eq!(l.column_of(8000), Some(25));
    assert_eq!(l.column_of(40_000), None);
    assert_eq!(l.region, (8000, 16_000));
    assert!(l.in_region(30) && !l.in_region(10) && !l.in_region(60));
    assert_eq!(l.time_at(50.0), secs(2));
    assert_eq!(l.scale(), "40 ms");
    assert_eq!(l.shown(), "0:00.000-0:04.000");
    assert_eq!(
        l.region_text(),
        "region 0:01.000-0:02.000 (1.000 s)  marks 2"
    );
}

#[test]
fn a_layout_scales_levels_for_each_display() {
    let l = layout(0, Duration::ZERO, &[]);
    // The loud second is the loudest sample: full height on the linear scale.
    let (rms, peak) = l.heights(Display::Envelope, 30);
    assert!(
        (rms - 1.0).abs() < 1e-3 && (peak - 1.0).abs() < 1e-3,
        "{rms} {peak}"
    );
    let (_, quiet) = l.heights(Display::Envelope, 10);
    assert!((quiet - 0.125).abs() < 1e-3, "{quiet}");
    // On the dB scale 0.1 is -20 dBFS, 28 of 48 dB above the floor.
    let (_, quiet_db) = l.heights(Display::Decibels, 10);
    assert!((quiet_db - 28.0 / 48.0).abs() < 1e-2, "{quiet_db}");
    assert_eq!(l.heights(Display::Envelope, 70), (0.0, 0.0));
    assert_eq!(l.extent(8000, 8320), (-1.0, 1.0));
    // Past the end there are no samples: an empty extent.
    assert_eq!(l.extent(40_000, 40_320), (1.0, -1.0));
}

#[test]
fn the_waveform_waits_for_its_track_with_a_reason() {
    let playing = std::path::PathBuf::from("/m/a.wav");
    let mut sampler = Sampler::default();
    assert_eq!(
        peaks_of(&sampler, None).unwrap_err(),
        "Nothing is playing. Play a track to see its waveform."
    );
    sampler.wave = Wave::Reading {
        path: playing.clone(),
        job: 1,
    };
    assert_eq!(
        peaks_of(&sampler, Some(&playing)).unwrap_err(),
        "Reading the waveform..."
    );
    sampler.wave = Wave::Failed {
        path: playing.clone(),
        error: "bad header".into(),
    };
    assert_eq!(
        peaks_of(&sampler, Some(&playing)).unwrap_err(),
        "Cannot read the waveform: bad header"
    );
    sampler.wave = Wave::Ready {
        path: "/m/other.wav".into(),
        peaks: Arc::new(Peaks::from_interleaved(&[0.0], 1, 8000)),
    };
    assert!(
        peaks_of(&sampler, Some(&playing)).is_err(),
        "another track's peaks"
    );
    assert_eq!(plan_text(&sampler), "");
    sampler.planning = Some(1);
    assert_eq!(plan_text(&sampler), "planning slices");
}
