//! The read heads' interpolators, on sines whose ideal output is known.

use playr_dsp::Interp;

const FS: f64 = 44_100.0;

/// `n` frames of a unit sine at `hz`, read at `rate` from frame 1,000.
fn read(interp: Interp, hz: f64, rate: f64, n: usize) -> Vec<f64> {
    let src: Vec<f32> = (0..(n as f64 * rate) as usize + 2_000)
        .map(|i| (std::f64::consts::TAU * hz * i as f64 / FS).sin() as f32)
        .collect();
    (0..n)
        .map(|k| {
            let pos = 1_000.0 + k as f64 * rate + 0.37;
            f64::from(interp.kernel(pos, rate).apply(|j| src[j as usize]))
        })
        .collect()
}

fn rms(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
}

/// What is left of `x` once the best sine at `hz` is taken out, by least
/// squares on a sine and a cosine: the error a perfect reader would not make.
fn residual(x: &[f64], hz: f64) -> f64 {
    let w = std::f64::consts::TAU * hz / FS;
    let (s, c): (Vec<f64>, Vec<f64>) = (0..x.len())
        .map(|k| ((w * k as f64).sin(), (w * k as f64).cos()))
        .unzip();
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(p, q)| p * q).sum::<f64>();
    let (ss, cc, sc) = (dot(&s, &s), dot(&c, &c), dot(&s, &c));
    let (xs, xc) = (dot(x, &s), dot(x, &c));
    let det = ss * cc - sc * sc;
    let (a, b) = ((xs * cc - xc * sc) / det, (xc * ss - xs * sc) / det);
    let r: Vec<f64> = (0..x.len()).map(|k| x[k] - a * s[k] - b * c[k]).collect();
    rms(&r) / std::f64::consts::FRAC_1_SQRT_2
}

fn db(v: f64) -> f64 {
    20.0 * v.log10()
}

#[test]
fn a_whole_frame_at_rate_1_is_the_stored_frame() {
    let src = [0.1f32, -0.4, 0.9, 0.25, -0.7];
    for interp in [Interp::Hermite, Interp::Sinc] {
        let k = interp.kernel(2.0, 1.0);
        assert_eq!(k.apply(|j| src[j as usize]), 0.9, "{interp:?}");
    }
}

#[test]
fn the_weights_sum_to_1_at_any_rate() {
    for interp in [Interp::Hermite, Interp::Sinc] {
        for rate in [0.5, 1.0, 1.16, 2.5, 4.0, 6.0, -1.5] {
            for t in [0.0, 0.25, 0.5, 0.999] {
                let sum: f32 = interp.kernel(100.0 + t, rate).weights().iter().sum();
                assert!((sum - 1.0).abs() < 1e-5, "{interp:?} x{rate} +{t}: {sum}");
            }
        }
    }
}

/// Between frames, a 10 kHz tone: the cubic's images are about -26 dB; the
/// sinc's, below -80.
#[test]
fn the_sinc_reads_between_frames_without_images() {
    let hz = 10_000.0;
    let hermite = residual(&read(Interp::Hermite, hz, 1.01, 8_192), hz * 1.01);
    let sinc = residual(&read(Interp::Sinc, hz, 1.01, 8_192), hz * 1.01);
    assert!(db(hermite) > -35.0, "hermite {:.1} dB", db(hermite));
    assert!(db(sinc) < -80.0, "sinc {:.1} dB", db(sinc));
}

/// Read fast, a tone past half the stored rate over the rate folds back
/// whole through the cubic; the stretched sinc removes it.
#[test]
fn a_fast_read_does_not_fold() {
    for (hz, rate) in [(15_000.0, 1.5), (8_000.0, 4.0)] {
        let hermite = rms(&read(Interp::Hermite, hz, rate, 8_192));
        let sinc = rms(&read(Interp::Sinc, hz, rate, 8_192));
        let unit = std::f64::consts::FRAC_1_SQRT_2;
        assert!(
            db(hermite / unit) > -6.0,
            "hermite x{rate}: {:.1} dB",
            db(hermite / unit)
        );
        assert!(
            db(sinc / unit) < -70.0,
            "sinc x{rate}: {:.1} dB",
            db(sinc / unit)
        );
    }
}

/// Up to 15 kHz the sinc passes a tone within 0.5 dB.
#[test]
fn the_sinc_s_passband_reaches_15_khz() {
    for hz in [1_000.0, 10_000.0, 15_000.0] {
        let level = rms(&read(Interp::Sinc, hz, 1.01, 8_192)) / std::f64::consts::FRAC_1_SQRT_2;
        assert!(db(level).abs() < 0.5, "{hz} Hz: {:.2} dB", db(level));
    }
}
