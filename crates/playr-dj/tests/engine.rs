//! The engine as the audio callback runs it, with no device.

use playr_dj::{Grid, Returned, Setting, Side, Track};

const SR: u32 = 48_000;

fn track(bpm: f64) -> Track {
    Track::new(
        vec![0.1; SR as usize * 4],
        2,
        Some(Grid::new(bpm, 0.0).unwrap()),
    )
    .unwrap()
}

#[test]
fn commands_reach_the_mixer_and_tracks_come_back() {
    let (mut e, mut h) = playr_dj::new(SR);
    h.load(Side::A, track(128.0)).unwrap();
    h.load(Side::B, track(120.0)).unwrap();
    h.set(Setting::Play(Side::A)).unwrap();
    h.set(Setting::Sync(Side::B, true)).unwrap();
    let mut out = vec![0.0; 1024];
    e.process(&mut out);
    // The empty tracks the loads replaced.
    for side in [Side::A, Side::B] {
        assert!(matches!(h.poll(), Some(Returned::Replaced(s, t)) if s == side && t.frames() == 0));
    }
    let s = h.status().deck(Side::B);
    assert!(s.synced());
    assert!((s.pct() - (128.0 / 120.0 - 1.0) * 100.0).abs() < 1e-9);
    assert!(h.status().deck(Side::A).playing());
    assert!(h.status().deck(Side::A).pos() > 0.0);
    assert_eq!(h.status().deck(Side::A).bpm(), Some(128.0));
    assert!(h.status().take_peak() > 0.0);

    // Deck A is playing, so it refuses a new track.
    h.load(Side::A, track(90.0)).unwrap();
    e.process(&mut out);
    assert!(
        matches!(h.poll(), Some(Returned::Refused(Side::A, t)) if t.grid().unwrap().bpm == 90.0)
    );
}

#[test]
fn a_deck_without_a_grid_reports_no_tempo() {
    let (mut e, mut h) = playr_dj::new(SR);
    h.load(Side::A, Track::new(vec![0.0; 100], 1, None).unwrap())
        .unwrap();
    e.process(&mut [0.0; 64]);
    assert_eq!(h.status().deck(Side::A).bpm(), None);
    assert_eq!(h.status().deck(Side::A).phase(), None);
}

#[test]
fn tracks_are_checked() {
    assert!(Track::new(vec![0.0; 3], 2, None).is_err());
    assert!(Track::new(vec![0.0; 3], 3, None).is_err());
    assert!(Track::new(vec![], 1, Some(Grid { bpm: -1.0, t0: 0.0 })).is_err());
    assert!(Grid::new(120.0, f64::INFINITY).is_err());
}
