//! The sampler view's geometry: which frames each column shows, and the dB scale.

use std::sync::Arc;
use std::time::Duration;

use playr_app::action::Nudge;
use playr_app::sampler::{
    db_height, fmt_frames, nudge, peaks_of, plan_text, snap, window, DetailRead, Layout, Sampler,
    Scale, Wave, DB_FLOOR, DETAIL_BELOW,
};
use playr_app::Display;
use playr_core::wave::{Detail, Peaks, BUCKET};

#[test]
fn the_window_fits_the_track_then_zooms_around_the_playhead() {
    // 100 columns of a 1,000,000-frame track: 10,000 frames a column, rounded
    // up to whole buckets of 32.
    assert_eq!(window(1_000_000, 100, 0, 500_000, 1), (0, 10_016, 1, 0));
    // One step in halves the frames a column, centred on the playhead, and
    // starts on a bucket.
    assert_eq!(
        window(1_000_000, 100, 1, 500_000, 1),
        (248_800, 5_024, 1, 1)
    );
    // Near either end the window stops at the track's edge.
    assert_eq!(window(1_000_000, 100, 1, 10_000, 1), (0, 5_024, 1, 1));
    assert_eq!(
        window(1_000_000, 100, 1, 990_000, 1),
        (497_600, 5_024, 1, 1)
    );
    let (start, per_column, _, _) = window(987_654, 77, 3, 432_109, 1);
    assert_eq!((start % BUCKET, per_column % BUCKET), (0, 0));
}

#[test]
fn past_the_peaks_zoom_reaches_a_frame_a_column_then_columns_a_frame() {
    // 10,000 >> 8 is 39: finer than the peaks, so not rounded to buckets.
    assert_eq!(window(1_000_000, 100, 8, 500_000, 1), (498_050, 39, 1, 8));
    const { assert!(39 < DETAIL_BELOW) };
    // 13 steps reach one frame a column; a terminal stops there.
    assert_eq!(window(1_000_000, 100, 40, 500_000, 1), (499_950, 1, 1, 13));
    // A window goes on, doubling the columns a frame takes, up to 16.
    assert_eq!(window(1_000_000, 100, 15, 500_000, 16), (499_988, 1, 4, 15));
    assert_eq!(
        window(1_000_000, 100, 40, 500_000, 16),
        (499_997, 1, 16, 17)
    );
    // A track shorter than the view is shown whole, and a window can still zoom.
    assert_eq!(window(50, 100, 3, 0, 1), (0, 1, 1, 0));
    assert_eq!(window(50, 100, 3, 0, 16), (0, 1, 8, 3));
}

#[test]
fn a_close_layout_places_frames_across_columns_and_reads_decoded_frames() {
    let peaks = Arc::new(steps());
    let at = |frame: u64| Duration::from_secs_f64(frame as f64 / 8_000.0);
    // 10 columns of 1,000 frames: six steps to a frame a column.
    let l = Layout::new(peaks.clone(), 10, 6, at(150), &[], 16);
    assert_eq!((l.start, l.per_column, l.per_frame), (145, 1, 1));
    assert_eq!((l.span_of(0), l.column_of(150)), ((145, 146), Some(5)));
    assert_eq!(l.scale(), "1 frame");
    // Two more: four columns a frame, three frames in view.
    let l = Layout::new(peaks.clone(), 10, 8, at(150), &[], 16);
    assert_eq!((l.start, l.per_frame, l.end()), (149, 4, 152));
    assert_eq!(
        [l.span_of(0), l.span_of(4), l.span_of(9)],
        [(149, 150), (150, 151), (151, 152)]
    );
    assert_eq!((l.column_of(150), l.column_of(152)), (Some(4), None));
    assert_eq!(l.time_at(4.0), at(150));
    assert_eq!(l.scale(), "1/4 frame");
    assert_eq!(
        l.columns(),
        Scale {
            start: 149,
            per_column: 1,
            per_frame: 4,
            columns: 10
        }
    );
    assert!(l.columns().needs_detail());
    assert_eq!(l.columns().shown(), (149, 152));
    assert_eq!(
        Layout::new(peaks.clone(), 10, 3, at(150), &[], 16).scale(),
        "12 frames"
    );

    // Frame 99 is the last positive one. Peaks widen it to its bucket, which
    // holds the crossing; decoded frames show it alone.
    let l = Layout::new(peaks.clone(), 10, 6, at(99), &[], 16);
    assert_eq!(l.extent(99, 100), (-1.0, 1.0));
    let samples: Vec<f32> = (90..110)
        .map(|f| if f < 100 { 0.5 } else { -0.5 })
        .collect();
    let detail = Arc::new(Detail::from_interleaved(samples, 1, 8_000, 90, 110));
    let l = l.with_detail(Some(detail));
    assert_eq!(
        (l.extent(99, 100), l.extent(100, 101)),
        ((1.0, 1.0), (-1.0, -1.0))
    );
    assert_eq!(
        l.extent(200, 201),
        (-1.0, 1.0),
        "outside the decoded frames: peaks"
    );
}

#[test]
fn a_detail_read_answers_for_the_frames_it_covers() {
    let path = std::path::PathBuf::from("/m/t.wav");
    let other = std::path::PathBuf::from("/m/u.wav");
    let reading = DetailRead::Reading {
        path: path.clone(),
        job: 1,
        start: 100,
        end: 200,
    };
    assert!(reading.answers(&path, 120, 200));
    assert!(!reading.answers(&path, 90, 180));
    assert!(!reading.answers(&other, 120, 180));
    let ready = DetailRead::Ready {
        path: path.clone(),
        detail: Arc::new(Detail::from_interleaved(vec![0.0; 50], 1, 8_000, 100, 200)),
    };
    assert!(
        ready.answers(&path, 150, 200),
        "asked to 200, though fewer were read"
    );
    assert!(!DetailRead::None.answers(&path, 0, 1));
    let mut sampler = Sampler {
        detail: ready,
        ..Sampler::default()
    };
    assert!(sampler.detail(Some(&path)).is_some());
    assert!(sampler.detail(Some(&other)).is_none());
    sampler.detail = reading;
    assert!(sampler.detail(Some(&path)).is_none(), "not read yet");
}

/// 1,000 frames at 8 kHz that change sign every 100: crossings at 100, 200 ... 900.
fn steps() -> Peaks {
    let data: Vec<f32> = (0..1_000)
        .map(|f| if f / 100 % 2 == 0 { 0.5 } else { -0.5 })
        .collect();
    Peaks::from_interleaved(&data, 1, 8_000)
}

#[test]
fn a_nudge_counts_columns_or_a_share_of_the_view() {
    let scale = Scale {
        start: 0,
        per_column: 64,
        per_frame: 1,
        columns: 50,
    };
    assert_eq!(scale.frames(Nudge::Columns(1)), 64);
    assert_eq!(scale.frames(Nudge::Columns(-3)), -192);
    assert_eq!(scale.frames(Nudge::Percent(10)), 5 * 64);
    assert_eq!(scale.frames(Nudge::Percent(-10)), -5 * 64);
    // Less than a column rounds up to one; more than the view stops at it.
    assert_eq!(scale.frames(Nudge::Percent(1)), 64);
    assert_eq!(scale.frames(Nudge::Percent(-500)), -50 * 64);
    // Columns a frame: a column is less than a frame, and a nudge moves one.
    let close = Scale {
        start: 0,
        per_column: 1,
        per_frame: 16,
        columns: 1_000,
    };
    assert_eq!(close.frames(Nudge::Columns(1)), 1);
    assert_eq!(close.frames(Nudge::Columns(-3)), -1);
    assert_eq!(
        close.frames(Nudge::Percent(10)),
        6,
        "100 columns, 6 whole frames"
    );
}

#[test]
fn a_snapped_nudge_lands_on_a_crossing_past_where_it_started() {
    let peaks = steps();
    // 10 ms at 8 kHz is 80 frames either side.
    assert_eq!(nudge(&peaks, 0, 64, false), 64);
    assert_eq!(nudge(&peaks, 0, 64, true), 100);
    assert_eq!(nudge(&peaks, 100, 64, true), 200, "not back to 100");
    assert_eq!(nudge(&peaks, 200, -64, true), 100);
    // No crossing before 100 within reach: the unsnapped point.
    assert_eq!(nudge(&peaks, 100, -64, true), 36);
    // Moves stop inside the track.
    assert_eq!(nudge(&peaks, 990, 64, false), 999);
    assert_eq!(nudge(&peaks, 10, -64, true), 0);
}

#[test]
fn a_snap_takes_the_nearest_crossing_within_10_ms() {
    let peaks = steps();
    assert_eq!(snap(&peaks, 130), 100);
    assert_eq!(snap(&peaks, 240), 200);
    assert_eq!(snap(&peaks, 20), 100, "80 frames away is within reach");
    assert_eq!(snap(&peaks, 19), 19, "81 is not");
    assert_eq!(
        snap(&Peaks::from_interleaved(&[0.0; 1_000], 1, 8_000), 240),
        240
    );
}

#[test]
fn a_range_end_set_across_the_other_drops_it() {
    let path = std::path::PathBuf::from("/m/t.wav");
    let other = std::path::PathBuf::from("/m/u.wav");
    let mut sampler = Sampler::default();
    sampler.set_range_start(&path, 500);
    assert_eq!(sampler.range_ends(Some(&path)), (Some(500), None));
    assert_eq!(sampler.range(Some(&path)), None, "one end is not a range");
    sampler.set_range_end(&path, 200);
    assert_eq!(sampler.range_ends(Some(&path)), (None, Some(200)));
    sampler.set_range_start(&path, 100);
    assert_eq!(sampler.range(Some(&path)), Some((100, 200)));
    assert_eq!(sampler.range(Some(&other)), None, "another track has none");
    sampler.set_range_start(&other, 50);
    assert_eq!(
        sampler.range_ends(Some(&other)),
        (Some(50), None),
        "ends kept per track"
    );
}

#[test]
fn a_range_replaces_the_region_in_the_layout() {
    let ms = Duration::from_millis;
    let peaks = Arc::new(steps());
    let layout = Layout::new(peaks.clone(), 10, 0, ms(10), &[ms(50)], 1);
    assert_eq!(layout.region, (0, 400));
    let ranged =
        Layout::new(peaks.clone(), 10, 0, ms(10), &[ms(50)], 1).with_range((Some(200), Some(700)));
    assert_eq!(ranged.region, (200, 700));
    assert!(
        ranged.region_text().starts_with("range 0:00.025-0:00.087"),
        "{}",
        ranged.region_text()
    );
    let half = Layout::new(peaks, 10, 0, ms(10), &[ms(50)], 1).with_range((Some(200), None));
    assert_eq!((half.region, half.range), ((0, 400), (Some(200), None)));
    // 100 frames a column, rounded up to whole buckets.
    assert_eq!(
        half.columns(),
        Scale {
            start: 0,
            per_column: 128,
            per_frame: 1,
            columns: 10
        }
    );
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
    Layout::new(peaks, 100, zoom, position, marks, 1)
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
