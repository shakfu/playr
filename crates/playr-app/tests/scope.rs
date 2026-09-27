//! The scope view's state, fed by hand, and the helpers it draws with.

use playr_app::scope::{
    correlation, stereo_point, trigger, Scope, ABOVE_TARGET, BELOW_TARGET, FALL_DB_PER_SEC, HISTORY,
};
use playr_core::spectrum::BANDS;

const RATE: u32 = 44_100;

/// `frames` of a sine at `hz`, stereo, the right channel scaled by `right`.
fn sine(hz: f32, frames: usize, right: f32) -> Vec<f32> {
    (0..frames)
        .flat_map(|i| {
            let v = (std::f32::consts::TAU * hz * i as f32 / RATE as f32).sin();
            [v, right * v]
        })
        .collect()
}

#[test]
fn correlation_runs_from_opposite_to_identical_and_is_none_in_silence() {
    let near = |a: Option<f32>, b: f32| (a.unwrap() - b).abs() < 1e-4;
    assert!(near(correlation(&sine(440.0, 2048, 1.0), 2), 1.0));
    assert!(near(correlation(&sine(440.0, 2048, -1.0), 2), -1.0));
    // A sine and its cosine are uncorrelated over whole cycles.
    let quadrature: Vec<f32> = (0..4410)
        .flat_map(|i| {
            let x = std::f32::consts::TAU * 100.0 * i as f32 / RATE as f32;
            [x.sin(), x.cos()]
        })
        .collect();
    assert!(near(correlation(&quadrature, 2), 0.0));
    assert_eq!(correlation(&[0.0; 64], 2), None);
    assert_eq!(correlation(&sine(440.0, 256, 0.0), 2), None);
    // One channel correlates with itself.
    assert!(near(correlation(&[0.5, -0.5, 0.25], 1), 1.0));
}

#[test]
fn the_stereo_image_puts_mono_upright_and_each_channel_on_its_diagonal() {
    assert_eq!(stereo_point(1.0, 1.0), (0.0, 1.0));
    assert_eq!(stereo_point(1.0, -1.0), (-1.0, 0.0));
    assert_eq!(stereo_point(1.0, 0.0), (-0.5, 0.5));
    assert_eq!(stereo_point(0.0, 1.0), (0.5, 0.5));
}

#[test]
fn the_trace_starts_at_the_latest_rising_crossing_with_room_after_it() {
    //        0     1    2    3     4    5    6     7
    let m = [-1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0];
    assert_eq!(trigger(&m, 3), 4);
    assert_eq!(trigger(&m, 4), 4);
    assert_eq!(trigger(&m, 5), 1);
    // No crossing: as late as it fits.
    assert_eq!(trigger(&[1.0; 8], 3), 5);
    assert_eq!(trigger(&[1.0; 2], 3), 0);
}

#[test]
fn a_tone_peaks_in_its_band_then_falls_at_the_set_rate_when_it_stops() {
    let mut scope = Scope::default();
    assert!(scope.bands().is_empty());
    scope.take_frames(RATE, 2, &sine(1000.0, 4096, 1.0), 0.0);
    let bands = scope.bands().to_vec();
    assert_eq!(bands.len(), BANDS);
    let loudest = (0..BANDS)
        .max_by(|&a, &b| bands[a].total_cmp(&bands[b]))
        .unwrap();
    assert_eq!(
        loudest,
        (scope.place(1000.0).unwrap() * BANDS as f32) as usize
    );
    assert!(scope.correlation().unwrap() > 0.999);
    // A 20 ms trace, from a rising crossing.
    let trace = scope.trace();
    assert_eq!(trace.len(), (RATE / 50) as usize);
    assert!(trace[0] >= 0.0 && trace[0] < 0.2, "{}", trace[0]);

    scope.take_frames(RATE, 2, &[0.0; 4096], 0.5);
    let fell = bands[loudest] - scope.bands()[loudest];
    assert!((fell - FALL_DB_PER_SEC / 2.0).abs() < 1e-3, "{fell}");
    assert_eq!(scope.correlation(), None);

    // Nothing read shows nothing.
    scope.take_frames(RATE, 0, &[], 0.1);
    assert!(scope.bands().is_empty() && scope.trace().is_empty());
}

#[test]
fn the_history_keeps_a_minute_and_integrates_what_passes_the_gates() {
    let mut scope = Scope::default();
    scope.take_loudness(&[-80.0, -20.0, -20.0]);
    assert_eq!(
        scope.history().iter().copied().collect::<Vec<_>>(),
        [None, Some(-20.0), Some(-20.0)]
    );
    assert!((scope.integrated().unwrap() + 20.0).abs() < 0.01);
    // A block more than 10 LU below the rest is left out.
    scope.take_loudness(&[-45.0]);
    assert!((scope.integrated().unwrap() + 20.0).abs() < 0.01);

    scope.take_loudness(&vec![-10.0; HISTORY]);
    assert_eq!(scope.history().len(), HISTORY);
    assert!(scope.history().iter().all(|l| *l == Some(-10.0)));
}

#[test]
fn the_history_scale_is_fixed_to_the_target_and_the_target_stays_in_range() {
    let mut scope = Scope::with_target(-16.0);
    assert_eq!(scope.target(), -16.0);
    let full = BELOW_TARGET + ABOVE_TARGET;
    assert_eq!(scope.height_of(-16.0), BELOW_TARGET / full);
    assert_eq!(scope.height_of(-16.0 - BELOW_TARGET), 0.0);
    assert_eq!(scope.height_of(-16.0 + ABOVE_TARGET), 1.0);
    assert_eq!(scope.height_of(-80.0), 0.0);
    assert_eq!(scope.height_of(5.0), 1.0);
    scope.set_target(-60.0);
    assert_eq!(scope.target(), -40.0);
    scope.set_target(3.0);
    assert_eq!(scope.target(), 0.0);
    assert_eq!(Scope::default().target(), -14.0);
}
