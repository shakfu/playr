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

/// A replace on a playing deck crossfades over 1 s with equal power: the old
/// track at 0.707 halfway, gone at the end, and handed back then. A second
/// replace during the fade is refused; one on a paused deck loads.
#[test]
fn a_replace_crossfades_from_the_playing_track() {
    let constant = |v: f32| Track::new(vec![v; SR as usize * 2 * 4], 2, None).unwrap();
    let (mut e, mut h) = playr_dj::new(SR);
    h.load(Side::A, constant(0.1)).unwrap();
    h.set(Setting::Play(Side::A)).unwrap();
    let mut out = vec![0.0; 2 * 480];
    for _ in 0..10 {
        e.process(&mut out);
    }
    let before = out[0];
    assert!(before > 0.0);
    while h.poll().is_some() {}

    h.replace(Side::A, constant(0.0)).unwrap();
    h.replace(Side::A, constant(0.3)).unwrap();
    let mut heard = Vec::new();
    for _ in 0..110 {
        e.process(&mut out);
        heard.extend(out.iter().step_by(2).copied());
    }
    assert!(matches!(h.poll(), Some(Returned::Fading(Side::A))));
    assert!(matches!(h.poll(), Some(Returned::Refused(Side::A, _))));
    // The new track is silent, so what is heard is the old one's fade.
    let half = heard[SR as usize / 2] / before;
    assert!(
        (half - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01,
        "{half}"
    );
    let step = heard
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max);
    assert!(step < before * 1e-3, "a step of {step}");
    assert!(heard[SR as usize + 10].abs() < 1e-6);
    assert!(h.status().deck(Side::A).playing());
    assert!(matches!(h.poll(), Some(Returned::Faded(Side::A, t)) if t.frames() == SR as usize * 4));

    h.set(Setting::Pause(Side::A)).unwrap();
    e.process(&mut out);
    h.replace(Side::A, constant(0.3)).unwrap();
    e.process(&mut out);
    assert!(matches!(h.poll(), Some(Returned::Replaced(Side::A, _))));
}

/// The interpolation switches what both decks read between frames: on noise
/// at +8% the two differ; on a whole frame at rate 1 they are the same.
#[test]
fn the_interpolation_changes_what_a_deck_reads() {
    use playr_dsp::Interp;
    let mut x: u32 = 0x9e37_79b9;
    let noise: Vec<f32> = (0..SR as usize * 2 * 2)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            0.3 * (x as f32 / u32::MAX as f32 * 2.0 - 1.0)
        })
        .collect();
    let heard = |interp, pct| {
        let (mut e, mut h) = playr_dj::new(SR);
        h.load(Side::A, Track::new(noise.clone(), 2, None).unwrap())
            .unwrap();
        h.set(Setting::Interp(interp)).unwrap();
        h.set(Setting::Rate(Side::A, pct)).unwrap();
        h.set(Setting::Play(Side::A)).unwrap();
        let mut out = vec![0.0; 2 * 4_800];
        e.process(&mut out);
        e.process(&mut out);
        out
    };
    let rms = |a: &[f32], b: &[f32]| {
        (a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f32>() / a.len() as f32).sqrt()
    };
    let (sinc, hermite) = (heard(Interp::Sinc, 8.0), heard(Interp::Hermite, 8.0));
    assert!(rms(&sinc, &hermite) > 0.005, "{}", rms(&sinc, &hermite));
    assert_eq!(heard(Interp::Sinc, 0.0), heard(Interp::Hermite, 0.0));
}
