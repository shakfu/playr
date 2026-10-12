//! The send effects' models: timing, feedback, decay, silence.

use playr_dsp::fx::{Delay, DelayModel, DelayParam, Reverb, ReverbModel, ReverbParam};

const RATE: u32 = 48000;

fn run(n: usize, mut f: impl FnMut(usize) -> [f32; 2]) -> Vec<[f32; 2]> {
    (0..n).map(&mut f).collect()
}

fn impulse(i: usize) -> [f32; 2] {
    if i == 0 {
        [1.0, 1.0]
    } else {
        [0.0, 0.0]
    }
}

/// The energy of channel `c` over `range`.
fn energy(y: &[[f32; 2]], c: usize, range: std::ops::Range<usize>) -> f32 {
    y[range].iter().map(|f| f[c] * f[c]).sum()
}

/// A delay of `model`, settled at `seconds`, `feedback` and `tone`.
fn delay(model: DelayModel, seconds: f32, feedback: f32, tone: f32) -> Delay {
    let mut d = Delay::new(RATE);
    d.set_model(model);
    d.set(DelayParam::Time, seconds);
    d.set(DelayParam::Feedback, feedback);
    d.set(DelayParam::Tone, tone);
    d.set(DelayParam::Wow, 0.0);
    // Settings glide; let them land before the impulse.
    for _ in 0..RATE {
        d.frame([0.0; 2]);
    }
    d
}

#[test]
fn every_delay_echoes_at_its_time_and_repeats_at_the_feedback() {
    let t = (0.01 * RATE as f32) as usize;
    for model in DelayModel::ALL {
        let mut d = delay(model, 0.01, 0.5, 20000.0);
        let y = run(4 * t, |i| d.frame(impulse(i)));
        assert!(
            energy(&y, 0, 0..t - 2) + energy(&y, 1, 0..t - 2) < 1e-9,
            "{model:?} sounds before its time"
        );
        // The sum of an echo's samples: the tone's low-pass passes DC at
        // unity, so each repeat's is the feedback times the last's. Both
        // channels, as ping-pong crosses them.
        let echo = |k: usize| -> f32 { y[k * t - 2..k * t + 10].iter().map(|f| f[0] + f[1]).sum() };
        let peak = (t - 2..t + 3)
            .max_by(|&a, &b| (y[a][0] + y[a][1]).total_cmp(&(y[b][0] + y[b][1])))
            .unwrap();
        assert!(
            peak.abs_diff(t) <= 1,
            "{model:?} first echo at {peak}, not {t}"
        );
        for k in [2, 3] {
            let ratio = echo(k) / echo(k - 1);
            assert!(
                (ratio - 0.5).abs() < 0.01,
                "{model:?} echo {k} is {ratio} of the one before"
            );
        }
    }
}

#[test]
fn ping_pong_alternates_sides() {
    let t = (0.01 * RATE as f32) as usize;
    let mut d = delay(DelayModel::PingPong, 0.01, 0.5, 20000.0);
    let y = run(3 * t + 10, |i| {
        d.frame(if i == 0 { [1.0, 0.0] } else { [0.0; 2] })
    });
    let side = |k: usize| {
        let w = k * t - 2..k * t + 3;
        (energy(&y, 0, w.clone()), energy(&y, 1, w))
    };
    let (l1, r1) = side(1);
    let (l2, r2) = side(2);
    assert!(r1 > 0.1 && l1 < 1e-9, "first echo left {l1} right {r1}");
    assert!(l2 > 0.01 && r2 < 1e-9, "second echo left {l2} right {r2}");
}

#[test]
fn tone_darkens_the_repeats_not_the_first_echo() {
    let t = (0.01 * RATE as f32) as usize;
    for model in DelayModel::ALL {
        let mut open = delay(model, 0.01, 0.5, 20000.0);
        let mut dark = delay(model, 0.01, 0.5, 500.0);
        let a = run(3 * t, |i| open.frame(impulse(i)));
        let b = run(3 * t, |i| dark.frame(impulse(i)));
        let peak = |y: &[[f32; 2]], k: usize| {
            y[k * t - 2..k * t + 3]
                .iter()
                .fold(0f32, |m, f| m.max(f[0].abs() + f[1].abs()))
        };
        assert_eq!(peak(&a, 1), peak(&b, 1), "{model:?} first echo");
        assert!(
            peak(&b, 2) < 0.5 * peak(&a, 2),
            "{model:?} repeat at 500 Hz peaks {} against {}",
            peak(&b, 2),
            peak(&a, 2)
        );
    }
}

#[test]
fn wow_bends_the_tape_echo() {
    let sine = |i: usize| {
        let x = (std::f32::consts::TAU * 440.0 * i as f32 / RATE as f32).sin() * 0.3;
        [x, x]
    };
    let mut still = delay(DelayModel::Tape, 0.1, 0.0, 20000.0);
    let mut wow = delay(DelayModel::Tape, 0.1, 0.0, 20000.0);
    wow.set(DelayParam::Wow, 1.0);
    let n = RATE as usize;
    let a = run(n, |i| still.frame(sine(i)));
    let b = run(n, |i| wow.frame(sine(i)));
    let diff: f32 = a.iter().zip(&b).map(|(x, y)| (x[0] - y[0]).powi(2)).sum();
    assert!(diff > 1.0, "wow moved the echo by {diff}");
}

/// Seconds for the late tail to fall 60 dB, by Schroeder's backward
/// integral, extrapolated from -25 to -55 dB: past the early reflections,
/// which in Zita fall faster than the tail.
fn t60(y: &[[f32; 2]], rate: u32) -> f32 {
    let mut tail: Vec<f64> = y
        .iter()
        .map(|f| f64::from(f[0] * f[0] + f[1] * f[1]))
        .collect();
    for i in (0..tail.len() - 1).rev() {
        tail[i] += tail[i + 1];
    }
    let db = |i: usize| 10.0 * (tail[i] / tail[0]).log10();
    let at = |level: f64| (0..tail.len()).find(|&i| db(i) <= level).unwrap() as f32;
    2.0 * (at(-55.0) - at(-25.0)) / rate as f32
}

fn tail(model: ReverbModel, rate: u32, decay: f32) -> f32 {
    let mut r = Reverb::new(rate);
    r.set_model(model);
    r.set(ReverbParam::Decay, decay);
    r.set(ReverbParam::Damp, 0.0);
    let y = run((rate as f32 * decay * 3.0) as usize, |i| {
        r.frame(impulse(i))
    });
    t60(&y, rate)
}

#[test]
fn the_t60_measure_reads_a_known_decay() {
    let mut seed = 1u32;
    let y = run(RATE as usize * 2, |i| {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let x = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
        let g = 10f32.powf(-3.0 * i as f32 / RATE as f32);
        [x * g, x * g]
    });
    assert!((t60(&y, RATE) - 1.0).abs() < 0.02, "{}", t60(&y, RATE));
}

#[test]
fn zita_and_plate_meet_their_decay_at_every_rate() {
    for rate in [44100, 48000, 96000] {
        for model in [ReverbModel::Zita, ReverbModel::Plate] {
            for decay in [1.0, 3.0] {
                let got = tail(model, rate, decay);
                assert!(
                    (got / decay - 1.0).abs() < 0.1,
                    "{model:?} at {rate} Hz: decay {decay} s measured {got} s"
                );
            }
        }
    }
}

/// JP's decay is approximate by its own documentation: measured 2 to 2.5
/// times the setting at 48 kHz, and less at 96 kHz (`docs/dev/fx-buses.md`).
#[test]
fn jp_decays_longer_as_its_decay_rises() {
    for rate in [44100, 48000, 96000] {
        let (short, long) = (
            tail(ReverbModel::Jp, rate, 1.0),
            tail(ReverbModel::Jp, rate, 3.0),
        );
        assert!(
            long > 2.0 * short,
            "at {rate} Hz: 1 s gave {short} s, 3 s gave {long} s"
        );
    }
}

#[test]
fn freeverb_rings_and_decays() {
    let mut r = Reverb::new(RATE);
    r.set_model(ReverbModel::Freeverb);
    let y = run(RATE as usize * 10, |i| r.frame(impulse(i)));
    let second = |s: usize| energy(&y, 0, s * RATE as usize..(s + 1) * RATE as usize);
    assert!(second(0) > 1e-4, "no reverb");
    assert!(second(1) < second(0) && second(9) < 1e-6 * second(0));
}

#[test]
fn silence_in_is_silence_out_after_a_switch() {
    let n = RATE as usize / 2;
    for model in DelayModel::ALL {
        let mut d = Delay::new(RATE);
        d.set(DelayParam::Feedback, 0.9);
        let _ = run(n, |i| d.frame(impulse(i)));
        d.set_model(DelayModel::Clean);
        d.set_model(DelayModel::Tape);
        d.set_model(model);
        let y = run(n, |_| d.frame([0.0; 2]));
        assert!(y.iter().all(|f| *f == [0.0; 2]), "{model:?} kept a tail");
    }
    for model in ReverbModel::ALL {
        let mut r = Reverb::new(RATE);
        let _ = run(n, |i| r.frame(impulse(i)));
        r.set_model(ReverbModel::Freeverb);
        r.set_model(ReverbModel::Zita);
        r.set_model(model);
        let y = run(n, |_| r.frame([0.0; 2]));
        // Zita adds 1e-20 in its loop against subnormals, so silence is
        // below -300 dB, not zero. The bus's bypass keeps the tape exact.
        let peak = y.iter().flatten().fold(0f32, |m, v| m.max(v.abs()));
        assert!(peak < 1e-15, "{model:?} kept a tail at {peak}");
    }
}

#[test]
fn values_not_finite_are_ignored_and_output_stays_finite() {
    let mut d = Delay::new(RATE);
    let mut r = Reverb::new(RATE);
    for model in DelayModel::ALL {
        d.set_model(model);
        d.set(DelayParam::Time, f32::NAN);
        d.set(DelayParam::Feedback, f32::INFINITY);
        let y = run(4800, |i| d.frame(impulse(i)));
        assert!(y.iter().flatten().all(|v| v.is_finite()), "{model:?}");
    }
    for model in ReverbModel::ALL {
        r.set_model(model);
        r.set(ReverbParam::Decay, f32::NAN);
        r.set(ReverbParam::Size, f32::NEG_INFINITY);
        let y = run(4800, |i| r.frame(impulse(i)));
        assert!(y.iter().flatten().all(|v| v.is_finite()), "{model:?}");
    }
}

#[test]
fn every_model_builds_at_192_khz() {
    let mut d = Delay::new(192000);
    let mut r = Reverb::new(192000);
    d.set(DelayParam::Time, 2.0);
    for (dm, rm) in DelayModel::ALL.into_iter().zip(ReverbModel::ALL) {
        d.set_model(dm);
        r.set_model(rm);
        assert!(d.frame([1.0; 2]).iter().all(|v| v.is_finite()));
        assert!(r.frame([1.0; 2]).iter().all(|v| v.is_finite()));
    }
}
