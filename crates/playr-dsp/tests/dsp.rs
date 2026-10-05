//! The shared DSP on its own; `playr-looper` and `playr-dj` test it in use.

use playr_dsp::{clip, flush, hermite, soft_clip, Ramp, Svf};

const SR: u32 = 48_000;

#[test]
fn a_ramp_reaches_its_target_exactly_in_its_frames() {
    let mut r = Ramp::new(0.0f32);
    r.set(1.0, 3);
    let got: Vec<f32> = (0..4).map(|_| r.next()).collect();
    for (g, want) in got.iter().zip([1.0 / 3.0, 2.0 / 3.0]) {
        assert!((g - want).abs() < 1e-6, "{got:?}");
    }
    assert_eq!(got[2..], [1.0, 1.0]);
    assert!(r.settled());

    // Zero frames jumps; a retarget starts from where it is.
    r.set(5.0, 0);
    assert_eq!(r.value(), 5.0);
    r.set(1.0, 4);
    r.next();
    r.set(0.0, 2);
    assert_eq!([r.next(), r.next()], [2.0, 0.0]);
}

#[test]
fn a_ramp_s_lag_is_what_its_remaining_values_fall_short() {
    let mut r = Ramp::new(1.0f64);
    r.set(3.0, 100);
    (0..30).for_each(|_| _ = r.next());
    let lag = r.lag();
    let short: f64 = (0..70).map(|_| 3.0 - r.next()).sum();
    assert!((lag - short).abs() < 1e-9, "{lag} {short}");
}

#[test]
fn a_long_f32_ramp_neither_drifts_nor_stalls() {
    let n = 480_000;
    // Its step, 1e-3 / n, is below half an ulp of 1: accumulated, it never moved.
    let mut r = Ramp::new(1.0f32);
    r.set(1.001, n);
    (0..n / 2).for_each(|_| _ = r.next());
    assert!((r.value() - 1.0005).abs() < 1e-6, "{}", r.value());

    // Accumulated, it ended 0.0245 short and jumped.
    let mut r = Ramp::new(1.0f32);
    r.set(4.0, n);
    let mut last = 1.0;
    let mut worst = 0.0f32;
    for _ in 0..n {
        let v = r.next();
        worst = worst.max((v - last).abs());
        last = v;
    }
    assert_eq!(last, 4.0);
    assert!(worst < 2.0 * 3.0 / n as f32, "{worst}");
}

fn sine(hz: f32, n: usize) -> impl Iterator<Item = f32> {
    (0..n).map(move |i| (std::f32::consts::TAU * hz * i as f32 / SR as f32).sin())
}

/// The peak of `f` over the last half of `n` frames of a unit sine at `hz`.
fn peak(hz: f32, n: usize, mut f: impl FnMut(f32) -> f32) -> f32 {
    sine(hz, n)
        .map(&mut f)
        .skip(n / 2)
        .fold(0.0, |m, y| m.max(y.abs()))
}

#[test]
fn the_svf_band_is_unity_at_the_cutoff_and_the_all_pass_at_every_frequency() {
    let mut s = Svf::new(4.0);
    s.tune(1000.0, SR);
    let band = peak(1000.0, 48_000, |x| s.tick(0, x).band);
    assert!((band - 1.0).abs() < 0.01, "{band}");
    for hz in [50.0, 1000.0, 10_000.0] {
        let mut s = Svf::butterworth();
        s.tune(1000.0, SR);
        let all = peak(hz, 48_000, |x| s.tick(0, x).all());
        assert!((all - 1.0).abs() < 0.01, "{hz} Hz: {all}");
    }
}

#[test]
fn the_svf_stays_bounded_while_its_cutoff_sweeps() {
    let mut s = Svf::new(10.0);
    for (i, x) in sine(440.0, SR as usize).enumerate() {
        let t = i as f32 / SR as f32;
        s.tune(20.0 * 1000f32.powf((t * 37.0).sin().abs()), SR);
        let y = s.tick(0, x);
        assert!(y.low.abs() < 20.0 && y.high.abs() < 20.0, "{i}: {y:?}");
    }
}

#[test]
fn a_decaying_svf_reaches_zero_without_subnormals() {
    let mut s = Svf::butterworth();
    s.tune(100.0, SR);
    s.tick(0, 1.0);
    let mut out = 1.0f32;
    for _ in 0..10 * SR {
        out = s.tick(0, 0.0).low;
        assert!(!out.is_subnormal());
    }
    assert_eq!(out, 0.0);
    assert_eq!(flush(1e-31), 0.0);
    assert_eq!(flush(-1e-29), -1e-29);
}

#[test]
fn the_soft_clip_passes_the_knee_and_bends_towards_full_scale() {
    for knee in [0.5, 0.9] {
        for x in [0.0, 0.1, -0.3, knee, -knee] {
            assert_eq!(soft_clip(x, knee), x);
        }
        // Slope 1 at the knee, so no corner.
        let d = 1e-3;
        let slope = (soft_clip(knee + d, knee) - knee) / d;
        assert!((slope - 1.0).abs() < 1e-2, "{slope}");
        let mut last = knee;
        for i in 1..1000 {
            let y = soft_clip(knee + i as f32 * 0.01, knee);
            assert!(y >= last && y <= 1.0, "{y}");
            assert_eq!(soft_clip(-(knee + i as f32 * 0.01), knee), -y);
            last = y;
        }
    }
    assert_eq!(clip(0.7), soft_clip(0.7, 0.5));
}

#[test]
fn hermite_meets_its_endpoints_and_is_exact_on_quadratics() {
    assert_eq!(hermite(0.3, -0.2, 0.9, 0.1, 0.0), -0.2);
    assert!((hermite(0.3, -0.2, 0.9, 0.1, 1.0) - 0.9).abs() < 1e-6);
    let q = |x: f32| 0.5 * x * x - 2.0 * x + 1.0;
    for t in [0.1, 0.25, 0.5, 0.8] {
        let got = hermite(q(-1.0), q(0.0), q(1.0), q(2.0), t);
        assert!((got - q(t)).abs() < 1e-6, "{t}: {got} {}", q(t));
    }
}

#[test]
fn the_svf_survives_cutoffs_out_of_range_and_inputs_not_finite() {
    for hz in [30_000.0, 0.0, -5.0, f32::NAN, f32::INFINITY] {
        let mut s = Svf::butterworth();
        s.tune(1000.0, SR);
        s.tune(hz, SR);
        let out = peak(440.0, SR as usize, |x| s.tick(0, x).low);
        assert!(out.is_finite() && out < 2.0, "{hz} Hz: {out}");
        // Held at 1 Hz, its low-pass settles on DC; at 0 its state froze.
        if hz <= 0.0 {
            let dc = (0..5 * SR).fold(0.0, |_, _| s.tick(0, 1.0).low);
            assert!((dc - 1.0).abs() < 1e-3, "{hz} Hz: {dc}");
        }
    }
    let mut s = Svf::butterworth();
    s.tune(1000.0, SR);
    s.tick(0, f32::NAN);
    let out = peak(440.0, SR as usize, |x| s.tick(0, x).low);
    assert!(out.is_finite() && out > 0.9, "{out}");
    s.reset();
    assert_eq!(s.tick(0, 0.0).low, 0.0);
}
