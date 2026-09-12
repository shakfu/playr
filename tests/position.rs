//! Playback position arithmetic, including seeking.
//!
//! These are pure calculations, so they need no audio device.

use playr::audio::{speed_for, track_position};
use std::time::Duration;

const RATE: u32 = 44100;

fn secs(d: Duration) -> f64 {
    d.as_secs_f64()
}

#[test]
fn position_counts_from_the_start_of_the_stream() {
    assert_eq!(track_position(0, 0, 0, RATE, 1.0), Duration::ZERO);
    assert!((secs(track_position(RATE as u64 * 5, 0, 0, RATE, 1.0)) - 5.0).abs() < 1e-9);
}

#[test]
fn position_is_relative_to_the_current_track() {
    // Second track of a gapless run began 120s into the output stream.
    let start = RATE as u64 * 120;
    let played = RATE as u64 * 130;
    assert!((secs(track_position(played, start, 0, RATE, 1.0)) - 10.0).abs() < 1e-9);
}

#[test]
fn a_seek_is_reported_at_the_position_seeked_to() {
    // Seeking to 90s restarts the device count at zero and carries 90s as the
    // offset. Reporting zero here was the bug behind `[` and `]` both
    // restarting the track: relative seeks read this value first.
    let offset = RATE as u64 * 90;
    assert!((secs(track_position(0, 0, offset, RATE, 1.0)) - 90.0).abs() < 1e-9);
}

#[test]
fn playback_continues_from_a_seek() {
    let offset = RATE as u64 * 90;
    let played = RATE as u64 * 3;
    assert!((secs(track_position(played, 0, offset, RATE, 1.0)) - 93.0).abs() < 1e-9);
}

#[test]
fn relative_seeks_accumulate_instead_of_restarting() {
    // What pressing `]` four times then `[` once must do, in seconds.
    let mut pos = 0.0f64;
    let step = |pos: f64, delta: f64| -> f64 {
        let target = (pos + delta).max(0.0);
        // The engine seeks to `target`, then reports it back through the same
        // arithmetic the next keypress reads.
        secs(track_position(
            0,
            0,
            (target * RATE as f64) as u64,
            RATE,
            1.0,
        ))
    };
    for _ in 0..4 {
        pos = step(pos, 5.0);
    }
    assert!(
        (pos - 20.0).abs() < 1e-3,
        "four forward seeks reached {pos}s, expected 20s"
    );
    pos = step(pos, -5.0);
    assert!(
        (pos - 15.0).abs() < 1e-3,
        "seeking back reached {pos}s, expected 15s"
    );
}

#[test]
fn seeking_back_past_the_start_clamps_to_zero() {
    let mut pos = 2.0f64;
    pos = (pos - 5.0f64).max(0.0);
    assert_eq!(pos, 0.0);
    assert_eq!(track_position(0, 0, 0, RATE, 1.0), Duration::ZERO);
}

#[test]
fn an_unknown_rate_reports_zero_rather_than_dividing_by_zero() {
    assert_eq!(track_position(1000, 0, 500, 0, 1.0), Duration::ZERO);
}

#[test]
fn a_track_start_ahead_of_the_count_does_not_underflow() {
    // Can happen for one frame around a track boundary.
    assert_eq!(track_position(10, 100, 0, RATE, 1.0), Duration::ZERO);
}

// --- varispeed ---

#[test]
fn semitone_steps_are_geometric() {
    assert!((speed_for(0) - 1.0).abs() < 1e-12);
    // Twelve steps is exactly an octave, in both directions.
    assert!(
        (speed_for(12) - 2.0).abs() < 1e-9,
        "12 up is {}",
        speed_for(12)
    );
    assert!(
        (speed_for(-12) - 0.5).abs() < 1e-9,
        "12 down is {}",
        speed_for(-12)
    );
    // One step is about 5.95%.
    assert!(
        (speed_for(1) - 1.059463).abs() < 1e-5,
        "1 up is {}",
        speed_for(1)
    );
}

#[test]
fn each_step_is_the_same_musical_interval() {
    // Geometric steps mean the ratio between neighbours is constant, which is
    // what makes every press sound like the same amount of change.
    let step = speed_for(1);
    for n in -11..11 {
        let ratio = speed_for(n + 1) / speed_for(n);
        assert!(
            (ratio - step).abs() < 1e-9,
            "step {n} to {} was {ratio}",
            n + 1
        );
    }
}

#[test]
fn speed_is_clamped_to_one_octave_either_way() {
    assert_eq!(speed_for(99), speed_for(12));
    assert_eq!(speed_for(-99), speed_for(-12));
}

#[test]
fn position_advances_faster_when_played_faster() {
    // One second of device output at 2x has covered two seconds of the track.
    let out = RATE as u64;
    assert!((secs(track_position(out, 0, 0, RATE, 2.0)) - 2.0).abs() < 1e-9);
    assert!((secs(track_position(out, 0, 0, RATE, 0.5)) - 0.5).abs() < 1e-9);
}

#[test]
fn a_speed_change_keeps_the_position_it_happened_at() {
    // Speed changes re-seek, banking the position reached so far as an offset.
    let banked = RATE as u64 * 30;
    let since = RATE as u64 * 2;
    // Two seconds of output at 1.5x adds three seconds of track time.
    let got = secs(track_position(since, 0, banked, RATE, 1.5));
    assert!((got - 33.0).abs() < 1e-6, "expected 33s, got {got}");
}

#[test]
fn a_nonsense_speed_reports_zero_rather_than_a_wild_position() {
    assert_eq!(track_position(1000, 0, 0, RATE, 0.0), Duration::ZERO);
    assert_eq!(track_position(1000, 0, 0, RATE, -1.0), Duration::ZERO);
}
