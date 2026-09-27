//! The tone control: its bands, the level it keeps, and a flat pass-through.

use playr_core::audio::eq::{Band, Eq, Gains, RANGE_DB};

const RATE: u32 = 44_100;

/// The gain, in dB, `eq` gives a stereo sine at `hz` once settled.
fn gain_db(eq: &mut Eq, hz: f32) -> f32 {
    let amplitude = 0.5;
    let settled = (0..RATE)
        .map(|i| {
            let x = amplitude * (std::f32::consts::TAU * hz * i as f32 / RATE as f32).sin();
            let y = eq.process(x);
            eq.process(x);
            y
        })
        .skip(RATE as usize / 2)
        .fold(0.0f32, |m, y| m.max(y.abs()));
    20.0 * (settled / amplitude).log10()
}

fn eq(gains: [f32; 3]) -> Eq {
    let mut eq = Eq::new(RATE, 2);
    eq.follow(gains);
    eq
}

#[test]
fn flat_passes_samples_unchanged() {
    let mut eq = eq([0.0; 3]);
    assert!(eq.is_flat());
    for x in [0.0, 1.0, -0.3, 1e-9, f32::MIN_POSITIVE] {
        assert_eq!(eq.process(x).to_bits(), x.to_bits());
    }
}

#[test]
fn each_band_moves_its_own_frequencies() {
    let bass = |hz| gain_db(&mut eq([-6.0, 0.0, 0.0]), hz);
    assert!((bass(30.0) + 6.0).abs() < 0.5, "{}", bass(30.0));
    assert!(bass(5000.0).abs() < 0.2, "{}", bass(5000.0));
    let mid = |hz| gain_db(&mut eq([0.0, -6.0, 0.0]), hz);
    assert!((mid(1000.0) + 6.0).abs() < 0.1, "{}", mid(1000.0));
    assert!(mid(40.0).abs() < 0.5 && mid(16_000.0).abs() < 1.0);
    let treble = |hz| gain_db(&mut eq([0.0, 0.0, -6.0]), hz);
    assert!((treble(18_000.0) + 6.0).abs() < 0.8, "{}", treble(18_000.0));
    assert!(treble(200.0).abs() < 0.2, "{}", treble(200.0));
}

#[test]
fn a_boost_raises_its_band_and_leaves_the_rest() {
    let at = |gains, hz| gain_db(&mut eq(gains), hz);
    assert!((at([6.0, 0.0, 0.0], 30.0) - 6.0).abs() < 0.5);
    assert!(at([6.0, 0.0, 0.0], 5000.0).abs() < 0.2);
    assert!((at([0.0, 6.0, 0.0], 1000.0) - 6.0).abs() < 0.1);
    assert!((at([0.0, 0.0, 6.0], 18_000.0) - 6.0).abs() < 0.8);
    assert!(at([0.0, 0.0, 6.0], 200.0).abs() < 0.2);
}

#[test]
fn gains_stay_in_range() {
    let gains = Gains::default();
    assert_eq!(gains.get(), [0.0; 3]);
    gains.set(Band::Bass, 20.0);
    gains.set(Band::Mid, f32::NAN);
    gains.set(Band::Treble, -3.5);
    assert_eq!(gains.get(), [RANGE_DB, 0.0, -3.5]);
    assert_eq!(Band::Treble.name(), "treble");
}
