//! The sampler view's geometry: which frames each column shows, and the dB scale.

use playr_app::sampler::{db_height, fmt_frames, window, DB_FLOOR, MIN_FRAMES_PER_COLUMN};
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
