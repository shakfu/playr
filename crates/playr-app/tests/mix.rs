//! The mixer's fader laws and the gains it gives each source.

use playr_app::mix::{Law, Mix, Strip, RANGE_DB};

fn db(gain: f32) -> f32 {
    20.0 * gain.log10()
}

#[test]
fn both_laws_run_from_silence_to_unity_and_rise_throughout() {
    for law in [Law::Db, Law::Cubic] {
        assert_eq!(law.gain(0.0), 0.0, "{law:?}");
        assert_eq!(law.gain(1.0), 1.0, "{law:?}");
        let gains: Vec<f32> = (0..=1000).map(|i| law.gain(i as f32 / 1000.0)).collect();
        assert!(gains.windows(2).all(|w| w[0] < w[1]), "{law:?}");
        // Out of range is clamped.
        assert_eq!((law.gain(-1.0), law.gain(2.0)), (0.0, 1.0));
    }
    assert!((db(Law::Db.gain(0.5)) + 30.0).abs() < 1e-4);
    assert!((db(Law::Db.gain(0.01)) + 59.4).abs() < 1e-3);
    assert_eq!(Law::Cubic.gain(0.5), 0.125);
}

#[test]
fn db_is_even_in_decibels() {
    let step = RANGE_DB / 100.0;
    for i in 1..100 {
        let (a, b) = (i as f32 / 100.0, (i + 1) as f32 / 100.0);
        let d = db(Law::Db.gain(b)) - db(Law::Db.gain(a));
        assert!((d - step).abs() < 1e-3, "{a}: {d} dB");
    }
}

#[test]
fn each_law_s_position_inverts_its_gain() {
    for law in [Law::Db, Law::Cubic] {
        for i in 0..=100 {
            let p = i as f32 / 100.0;
            let back = law.position(law.gain(p));
            assert!((back - p).abs() < 1e-4, "{law:?} {p}: {back}");
        }
    }
    // Below the db law's range is the bottom of the fader.
    assert_eq!(Law::Db.position(1e-4), 0.0);
    assert_eq!(Law::Db.position(0.0), 0.0);
}

#[test]
fn a_source_plays_at_its_own_gain_times_the_master_s() {
    let mut mix = Mix::new(Law::Db, 0.5);
    mix.set_level(Strip::Tape, 0.75);
    let want = Law::Db.gain(0.5) * Law::Db.gain(0.75);
    assert_eq!(mix.gain(Strip::Tape), want);
    assert_eq!(mix.gain(Strip::Player), Law::Db.gain(0.5));
    // The headphones ignore the master.
    assert_eq!(mix.gain(Strip::Headphones), 1.0);

    mix.set_muted(Strip::Tape, true);
    assert_eq!(mix.gain(Strip::Tape), 0.0);
    assert_eq!(mix.level(Strip::Tape), 0.75, "a mute keeps the level");
    mix.set_muted(Strip::Tape, false);
    assert_eq!(mix.gain(Strip::Tape), want);

    mix.set_muted(Strip::Master, true);
    for s in [Strip::Player, Strip::Tape, Strip::Decks] {
        assert_eq!(mix.gain(s), 0.0, "{s:?}");
    }
    assert_eq!(mix.gain(Strip::Headphones), 1.0);

    // A level that is not a number leaves it; one out of range is clamped.
    mix.set_level(Strip::Decks, f32::NAN);
    assert_eq!(mix.level(Strip::Decks), 1.0);
    mix.set_level(Strip::Decks, -3.0);
    assert_eq!(mix.level(Strip::Decks), 0.0);
}
