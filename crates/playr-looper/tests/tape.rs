//! The DSP, driven offline: the tests listed in `docs/dev/looper-engine.md`.

use playr_looper::{Filter, Loop, Setting, Tape, Window};

const SR: u32 = 48_000;

fn sine(frames: usize, hz: f32, amp: f32) -> Vec<f32> {
    (0..frames)
        .map(|i| amp * (std::f32::consts::TAU * hz * i as f32 / SR as f32).sin())
        .collect()
}

/// Deterministic white noise in -amp..amp.
fn noise(n: usize, amp: f32) -> Vec<f32> {
    let mut x: u32 = 0x9e37_79b9;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            amp * (x as f32 / u32::MAX as f32 * 2.0 - 1.0)
        })
        .collect()
}

fn tape(samples: Vec<f32>, channels: u16, settings: &[Setting]) -> Tape {
    let mut t = Tape::new(SR);
    t.load(Box::new(Loop::new(samples, channels).unwrap()));
    for &s in settings {
        t.set(s);
    }
    t.set(Setting::Play);
    t
}

/// `frames` stereo frames of output.
fn run(t: &mut Tape, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * 2];
    t.process(&mut out);
    out
}

fn left(out: &[f32]) -> Vec<f32> {
    out.iter().step_by(2).copied().collect()
}

fn max_step(x: &[f32]) -> f32 {
    x.windows(2).fold(0.0, |m, w| m.max((w[1] - w[0]).abs()))
}

fn buffer(t: &Tape) -> Vec<f32> {
    t.buffer().samples().to_vec()
}

#[test]
fn rate_one_plays_the_window_and_minus_one_reverses_it() {
    let src = noise(1000, 0.9);
    let w = Window::new(100, 600);
    let mut t = tape(
        src.clone(),
        1,
        &[Setting::Fade(0, 0.0), Setting::Window(0, w)],
    );
    let out = run(&mut t, 500);
    assert_eq!(left(&out), src[100..600]);
    // Centre pan is unity on both sides.
    assert_eq!(
        out[1..].iter().step_by(2).copied().collect::<Vec<_>>(),
        src[100..600]
    );

    let mut t = tape(
        src.clone(),
        1,
        &[
            Setting::Fade(0, 0.0),
            Setting::Rate(0, -1.0),
            Setting::Window(0, w),
        ],
    );
    let rev: Vec<f32> = src[100..600].iter().rev().copied().collect();
    assert_eq!(left(&run(&mut t, 500)), rev);
}

#[test]
fn stereo_loop_plays_its_channels() {
    let src = noise(2000, 0.5);
    let mut t = tape(src.clone(), 2, &[]);
    assert_eq!(run(&mut t, 1000), src);
}

#[test]
fn a_fade_smooths_the_wrap() {
    // The window ends near a peak and starts near a zero crossing.
    let src = sine(48_000, 440.0, 0.8);
    let w = Window::new(1000, 1000 + 4800 + 27);
    let material = max_step(&src);
    let wrap = |fade| {
        let mut t = tape(
            src.clone(),
            1,
            &[Setting::Window(0, w), Setting::Fade(0, fade)],
        );
        max_step(&left(&run(&mut t, 3 * w.len())))
    };
    let faded = wrap(10.0);
    let cut = wrap(0.0);
    assert!(faded <= material, "{faded} > {material}");
    assert!(cut > 4.0 * material, "{cut}");
}

#[test]
fn feedback_one_with_no_sends_is_bit_exact() {
    let mut src = noise(9600, 1.0);
    let peak = src.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    src.iter_mut().for_each(|s| *s *= 0.95 / peak);
    let mut t = tape(
        src.clone(),
        2,
        &[
            Setting::Write(true),
            Setting::Feedback(1.0),
            Setting::On(1, true),
            Setting::Rate(1, -0.37),
            Setting::On(2, true),
            Setting::Rate(2, 2.5),
        ],
    );
    run(&mut t, 4800 * 10);
    assert_eq!(buffer(&t), src);
}

#[test]
fn each_pass_scales_by_feedback_without_sends() {
    let src = noise(4800, 0.9);
    let mut t = tape(
        src.clone(),
        1,
        &[Setting::Write(true), Setting::Feedback(0.7)],
    );
    let mut want = src;
    for _ in 0..5 {
        run(&mut t, 4800);
        want.iter_mut().for_each(|s| *s *= 0.7);
        assert_eq!(buffer(&t), want);
    }
}

#[test]
fn a_reversed_voice_prints_its_material_into_the_loop() {
    let n = 9600;
    let src = sine(n, 1000.0, 0.4);
    let mut t = tape(
        src.clone(),
        1,
        &[
            Setting::Write(true),
            Setting::Feedback(0.0),
            Setting::Rate(0, -1.0),
            Setting::Send(0, 1.0),
        ],
    );
    run(&mut t, n);
    let got = &buffer(&t)[..n / 2];
    let want: Vec<f32> = src[n / 2..].iter().rev().copied().collect();
    // The 10 Hz DC blocker shifts a 1 kHz sine by under 1 degree.
    let err = got
        .iter()
        .zip(&want)
        .fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(err < 0.01, "{err}");
}

/// Mean squared first difference: energy weighted towards the top of the band.
fn brightness(x: &[f32]) -> f32 {
    x.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum::<f32>() / x.len() as f32
}

#[test]
fn wear_darkens_each_pass() {
    let src = noise(4800, 0.9);
    let mut t = tape(src.clone(), 1, &[Setting::Write(true), Setting::Wear(0.5)]);
    let mut last = brightness(&src);
    for pass in 0..5 {
        run(&mut t, 4800);
        let b = brightness(&buffer(&t));
        assert!(b < 0.9 * last, "pass {pass}: {b} vs {last}");
        last = b;
    }
}

#[test]
fn three_full_sends_stay_within_full_scale() {
    let src = noise(2400 * 2, 0.9);
    let mut t = tape(
        src,
        2,
        &[
            Setting::Write(true),
            Setting::Feedback(1.0),
            Setting::Send(0, 1.0),
            Setting::On(1, true),
            Setting::Rate(1, -0.5),
            Setting::Send(1, 1.0),
            Setting::On(2, true),
            Setting::Rate(2, 2.0),
            Setting::Send(2, 1.0),
        ],
    );
    for pass in 0..100 {
        run(&mut t, 2400);
        let peak = buffer(&t).iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak <= 1.0, "pass {pass}: {peak}");
    }
}

#[test]
fn an_offset_decays() {
    let mut t = tape(
        vec![0.2; 4800],
        1,
        &[
            Setting::Write(true),
            Setting::Feedback(0.5),
            Setting::Send(0, 0.5),
        ],
    );
    // The blocker rings around 0 in the loop, so the mean falls in envelope,
    // not on every pass.
    let mean = |t: &Tape| buffer(t).iter().sum::<f32>() / 4800.0;
    run(&mut t, 4800);
    assert!(mean(&t).abs() < 0.05, "{}", mean(&t));
    run(&mut t, 4800 * 9);
    assert!(mean(&t).abs() < 0.001, "{}", mean(&t));
}

#[test]
fn a_rate_change_moves_over_20_ms() {
    let src = sine(96_000, 220.0, 0.8);
    let mut t = tape(src, 1, &[]);
    run(&mut t, 1000);
    let before = t.voice(0);
    t.set(Setting::Rate(0, 2.0));
    let ramp = left(&run(&mut t, 960));
    // A linear ramp from 1 to 2 covers 1.5 frames a frame on average.
    let moved = t.voice(0) - before;
    assert!((moved - 1.5 * 960.0).abs() < 2.0, "{moved}");
    let steady = left(&run(&mut t, 2000));
    assert!(max_step(&ramp) <= max_step(&steady));
}

#[test]
fn frames_outside_the_write_window_are_untouched() {
    let src = noise(8000, 0.9);
    let w = Window::new(1000, 3000);
    for fb in [0.0, 0.5, 1.0] {
        let mut t = tape(
            src.clone(),
            1,
            &[
                Setting::Write(true),
                Setting::WriteWindow(w),
                Setting::Feedback(fb),
                Setting::Send(0, 1.0),
                Setting::On(1, true),
                Setting::Rate(1, -1.3),
                Setting::Send(1, 0.8),
            ],
        );
        run(&mut t, 2000 * 10);
        let b = buffer(&t);
        assert_eq!(b[..1000], src[..1000]);
        assert_eq!(b[3000..], src[3000..]);
    }
}

#[test]
fn the_write_window_edges_do_not_step() {
    let src = sine(48_000, 330.0, 0.8);
    let material = max_step(&src);
    let w = Window::new(10_017, 30_011);
    let mut t = tape(
        src,
        1,
        &[
            Setting::Write(true),
            Setting::WriteWindow(w),
            Setting::Feedback(0.5),
            Setting::Rate(0, -1.0),
            Setting::Send(0, 0.3),
        ],
    );
    run(&mut t, w.len() * 10);
    let b = buffer(&t);
    for edge in [w.start, w.end] {
        let step = max_step(&b[edge - 600..edge + 600]);
        assert!(step <= material, "edge {edge}: {step} > {material}");
    }
}

#[test]
fn bad_settings_are_ignored() {
    let src = noise(1000, 0.5);
    let mut t = tape(
        src.clone(),
        1,
        &[
            Setting::Rate(0, f32::NAN),
            Setting::Level(0, f32::INFINITY),
            Setting::Rate(7, 2.0),
            Setting::Window(0, Window::new(500, 400)),
            Setting::Window(0, Window::new(0, 5000)),
        ],
    );
    assert_eq!(left(&run(&mut t, 1000)), src);
}

#[test]
fn stopped_is_silent_and_holds_the_heads() {
    let mut t = tape(noise(1000, 0.5), 1, &[]);
    run(&mut t, 100);
    t.set(Setting::Stop);
    assert!(run(&mut t, 100).iter().all(|&s| s == 0.0));
    assert_eq!(t.voice(0), 100.0);
}

#[test]
fn loops_are_validated() {
    assert!(Loop::new(vec![0.0; 6], 3).is_err());
    assert!(Loop::new(vec![0.0; 5], 2).is_err());
    assert!(Loop::new(vec![], 1).is_err());
    let lp = Loop::new(vec![f32::NAN, 0.5], 1).unwrap();
    assert_eq!(lp.samples(), [0.0, 0.5]);
}

#[test]
fn a_voice_s_wear_darkens_what_it_prints_not_what_is_heard() {
    let src = noise(4800, 0.4);
    let printed = |wear: f32| {
        let mut t = tape(
            src.clone(),
            1,
            &[
                Setting::Write(true),
                Setting::Feedback(0.0),
                Setting::Send(0, 1.0),
                Setting::VoiceWear(0, wear),
            ],
        );
        let heard = left(&run(&mut t, 4800));
        (heard, brightness(&buffer(&t)))
    };
    let (clean_heard, clean) = printed(0.0);
    let (worn_heard, worn) = printed(0.6);
    assert_eq!(worn_heard, clean_heard, "the level path is not filtered");
    assert_eq!(clean_heard, src);
    assert!(worn < 0.5 * clean, "{worn} vs {clean}");
}

#[test]
fn a_hard_pan_folds_a_stereo_loop_s_far_channel_into_the_near_one() {
    // Left holds one signal, right another; both stay audible panned hard.
    let frames = 1000;
    let l = noise(frames, 0.3);
    let r = sine(frames, 440.0, 0.3);
    let src: Vec<f32> = l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect();
    let mut t = tape(src.clone(), 2, &[Setting::Pan(0, -1.0)]);
    let out = run(&mut t, frames);
    for i in 0..frames {
        assert!((out[2 * i] - (l[i] + r[i])).abs() < 1e-6, "frame {i}");
        assert!(out[2 * i + 1].abs() < 1e-6, "frame {i}");
    }
    let mut t = tape(src.clone(), 2, &[Setting::Pan(0, 1.0)]);
    let out = run(&mut t, frames);
    for i in 0..frames {
        assert!(out[2 * i].abs() < 1e-6, "frame {i}");
        assert!((out[2 * i + 1] - (l[i] + r[i])).abs() < 1e-6, "frame {i}");
    }
}

#[test]
fn a_mono_loop_pans_with_equal_power() {
    let src = noise(1000, 0.5);
    let mut t = tape(src.clone(), 1, &[Setting::Pan(0, -1.0)]);
    let out = run(&mut t, 1000);
    for (i, s) in src.iter().enumerate() {
        assert!((out[2 * i] - s * 2f32.sqrt()).abs() < 1e-6);
        assert_eq!(out[2 * i + 1], 0.0);
    }
}

#[test]
fn the_crossfade_is_cut_to_half_the_window_s_crossing() {
    use playr_looper::fade_frames;
    assert_eq!(fade_frames(10.0, 48_000, 10_000, 1.0), 480);
    assert_eq!(fade_frames(100.0, 48_000, 2000, 1.0), 1000);
    assert_eq!(fade_frames(100.0, 48_000, 2000, -4.0), 250);
    assert_eq!(fade_frames(0.0, 48_000, 2000, 1.0), 0);
}

#[test]
fn a_wrap_fades_into_what_follows_the_window_and_keeps_its_length() {
    // Room after the window: the leaving head fades out past its end.
    let src = noise(10_000, 0.5);
    let w = Window::new(1000, 5000);
    let mut t = tape(src.clone(), 1, &[Setting::Window(0, w)]);
    let out = left(&run(&mut t, 4000 + 240));
    let fade = 480.0;
    for k in [0usize, 100, 239] {
        let phi = std::f32::consts::FRAC_PI_2 * k as f32 / fade;
        let want = src[1000 + k] * phi.sin() + src[5000 + k] * phi.cos();
        assert!((out[4000 + k] - want).abs() < 1e-5, "frame {k}");
    }
    run(&mut t, 4000 - 240);
    assert_eq!(t.voice(0), 1000.0, "one window a pass");
}

#[test]
fn a_window_at_the_loop_s_end_fades_in_from_before_its_start() {
    let src = noise(10_000, 0.5);
    let w = Window::new(1000, 10_000);
    let mut t = tape(src.clone(), 1, &[Setting::Window(0, w)]);
    // The wrap starts 480 frames early, with the new head 480 before the start.
    run(&mut t, 9000 - 480);
    assert_eq!(t.voice(0), 520.0);
    let out = left(&run(&mut t, 480));
    for k in [0usize, 200, 479] {
        let phi = std::f32::consts::FRAC_PI_2 * k as f32 / 480.0;
        let want = src[520 + k] * phi.sin() + src[9520 + k] * phi.cos();
        assert!((out[k] - want).abs() < 1e-5, "frame {k}");
    }
    assert_eq!(t.voice(0), 1000.0, "one window a pass");
    // In reverse, the same window has room past its start.
    let mut t = tape(src, 1, &[Setting::Rate(0, -1.0), Setting::Window(0, w)]);
    run(&mut t, 9000);
    assert_eq!(t.voice(0), 9999.0);
}

#[test]
fn with_no_room_either_side_a_wrap_cuts() {
    let src = noise(4000, 0.5);
    let mut t = tape(src.clone(), 1, &[]);
    let out = left(&run(&mut t, 4000 + 10));
    assert_eq!(out[4000..], src[..10]);
}

#[test]
fn the_crossfade_takes_the_side_with_room() {
    use playr_looper::{crossfade, Crossfade};
    let w = Window::new(1000, 5000);
    let x = |frames, rate| crossfade(w, w, frames, rate, 10.0, 48_000);
    let after = Crossfade {
        lead: 0.0,
        span: 480.0,
        frames: 480,
    };
    assert_eq!(x(10_000, 1.0), after);
    assert_eq!(
        x(5000, 1.0),
        Crossfade {
            lead: 480.0,
            ..after
        }
    );
    // 200 frames after, 1000 before; reverse reads the other way round.
    assert_eq!(x(5200, -1.0), after);
    assert_eq!(
        crossfade(
            Window::new(100, 5000),
            Window::new(100, 5000),
            5200,
            1.0,
            10.0,
            48_000
        ),
        Crossfade {
            lead: 0.0,
            span: 200.0,
            frames: 200
        }
    );
    // At twice the rate each head reads twice as far.
    assert_eq!(x(10_000, 2.0).span, 960.0);
}

#[test]
fn windows_start_as_the_loop_s_range() {
    let lp = Loop::new(noise(1000, 0.5), 1)
        .unwrap()
        .with_range(Window::new(100, 900))
        .unwrap();
    assert!(Loop::new(vec![0.0; 10], 1)
        .unwrap()
        .with_range(Window::new(5, 11))
        .is_err());
    let mut t = Tape::new(SR);
    t.load(Box::new(lp));
    assert_eq!(t.voice(0), 100.0);
    assert_eq!(t.write_window(), Window::new(100, 900));
}

/// `tape`, with `range` chosen and the frames either side of it as rolls.
fn rolled(samples: Vec<f32>, range: Window, settings: &[Setting]) -> Tape {
    let mut t = Tape::new(SR);
    let lp = Loop::new(samples, 1).unwrap().with_range(range).unwrap();
    t.load(Box::new(lp));
    for &s in settings {
        t.set(s);
    }
    t.set(Setting::Play);
    t
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

#[test]
fn while_writing_the_range_the_rolls_decay_with_it() {
    let src = noise(6000, 0.9);
    let range = Window::new(1000, 5000);
    let mut t = rolled(
        src.clone(),
        range,
        &[Setting::Write(true), Setting::Feedback(0.7)],
    );
    let mut want = src;
    for _ in 0..5 {
        run(&mut t, range.len());
        want.iter_mut().for_each(|s| *s *= 0.7);
        assert_eq!(buffer(&t), want);
    }
}

#[test]
fn feedback_one_with_no_sends_leaves_the_rolls_exact() {
    let src = noise(6000, 0.9);
    let mut t = rolled(
        src.clone(),
        Window::new(1000, 5000),
        &[
            Setting::Write(true),
            Setting::Feedback(1.0),
            Setting::Rate(0, -1.0),
            Setting::Fade(0, 1000.0),
        ],
    );
    run(&mut t, 4000 * 10);
    assert_eq!(buffer(&t), src);
}

#[test]
fn a_wrap_into_the_rolls_is_no_louder_than_the_decayed_loop() {
    // Two voices in opposite directions fade past both edges into the rolls
    // while feedback and small sends shrink the loop.
    let sec = SR as usize;
    let range = Window::new(sec, 5 * sec);
    let mut t = rolled(
        noise(6 * sec, 0.3),
        range,
        &[
            Setting::Rate(0, -1.0),
            Setting::Send(0, 0.1),
            Setting::Fade(0, 1000.0),
            Setting::On(1, true),
            Setting::Rate(1, 1.0),
            Setting::Level(1, 0.4),
            Setting::Send(1, 0.2),
            Setting::Fade(1, 1000.0),
            Setting::Write(true),
            Setting::Feedback(0.5),
        ],
    );
    run(&mut t, range.len() * 12);
    let pass = left(&run(&mut t, range.len()));
    let wrap = rms(&pass[..sec / 20]);
    let body = rms(&pass[sec..3 * sec]);
    assert!(wrap <= 2.0 * body, "after the wrap {wrap}, mid-pass {body}");
}

#[test]
fn a_send_crosses_the_range_s_edges_without_a_step() {
    let src = sine(48_000, 330.0, 0.8);
    let material = max_step(&src);
    let range = Window::new(9000, 39_000);
    let mut t = rolled(
        src,
        range,
        &[
            Setting::Write(true),
            Setting::Feedback(0.5),
            Setting::Rate(0, -1.0),
            Setting::Send(0, 0.3),
        ],
    );
    // Half a pass on, so no head is near an edge.
    run(&mut t, range.len() * 10 + range.len() / 2);
    let b = buffer(&t);
    for edge in [range.start, range.end] {
        let step = max_step(&b[edge - 600..edge + 600]);
        assert!(step <= material, "edge {edge}: {step} > {material}");
    }
}

#[test]
fn ping_turns_at_the_window_s_edges_without_a_jump() {
    let src = noise(4000, 0.5);
    let w = Window::new(1000, 2000);
    let mut t = tape(
        src.clone(),
        1,
        &[Setting::Window(0, w), Setting::Ping(0, true)],
    );
    let out = left(&run(&mut t, 3000));
    for k in 0..1000 {
        assert_eq!(out[k], src[1000 + k], "forward {k}");
    }
    // Back from the last frame, without playing it twice, then forward again.
    for k in 0..999 {
        assert_eq!(out[1000 + k], src[1998 - k], "back {k}");
    }
    assert_eq!(out[1999], src[1001]);
    assert_eq!(out[2000], src[1002]);
    run(&mut t, 100_000);
    assert!(w.start as f64 <= t.voice(0) && t.voice(0) < w.end as f64);
}

#[test]
fn a_slew_sets_how_long_a_rate_change_takes() {
    let mut t = tape(sine(96_000, 220.0, 0.8), 1, &[Setting::Slew(0, 100.0)]);
    run(&mut t, 1000);
    let before = t.voice(0);
    t.set(Setting::Rate(0, 2.0));
    run(&mut t, 4800);
    // A linear ramp from 1 to 2 over 100 ms covers 1.5 frames a frame.
    let moved = t.voice(0) - before;
    assert!((moved - 1.5 * 4800.0).abs() < 2.0, "{moved}");
}

#[test]
fn drive_lifts_quiet_material_and_holds_loud_material_down() {
    let peak = |amp: f32| {
        let mut t = tape(sine(4800, 220.0, amp), 1, &[Setting::Drive(0, 1.0)]);
        left(&run(&mut t, 4800))
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()))
    };
    let quiet = peak(0.01);
    assert!((quiet / 0.01 - 4.0).abs() < 0.05, "{quiet}: 12 dB up");
    assert!(peak(0.9) <= 0.25);
}

#[test]
fn the_filter_passes_its_band_and_cuts_the_rest() {
    // Cutoff 0.5 is 632 Hz; 100 Hz is 2.7 octaves below, 5 kHz 3 above.
    let gain = |kind: Filter, hz: f32| {
        let src = sine(48_000, hz, 0.5);
        let mut t = tape(
            src.clone(),
            1,
            &[Setting::Cutoff(0, 0.5), Setting::Filter(0, kind)],
        );
        let out = left(&run(&mut t, 48_000));
        rms(&out[4800..]) / rms(&src[4800..])
    };
    assert!(gain(Filter::Low, 100.0) > 0.95);
    assert!(gain(Filter::Low, 5000.0) < 0.03);
    assert!(gain(Filter::High, 100.0) < 0.03);
    assert!(gain(Filter::High, 5000.0) > 0.95);
    assert!(gain(Filter::Band, 632.0) > 0.95);
    // Butterworth damping makes the band wide: about 0.22 and 0.18 here.
    assert!(gain(Filter::Band, 100.0) < 0.25);
    assert!(gain(Filter::Band, 5000.0) < 0.25);
}

#[test]
fn solo_changes_what_is_heard_not_what_is_sent() {
    let src = noise(4800, 0.5);
    let settings = [
        Setting::Write(true),
        Setting::Feedback(0.5),
        Setting::Send(0, 0.5),
        Setting::On(1, true),
        Setting::Rate(1, -1.0),
        Setting::Send(1, 0.3),
    ];
    let mut soloed = tape(src.clone(), 1, &settings);
    soloed.set(Setting::Solo(1, true));
    let mut muted = tape(src, 1, &settings);
    muted.set(Setting::Level(0, 0.0));
    assert_eq!(run(&mut soloed, 4800 * 3), run(&mut muted, 4800 * 3));
    assert_eq!(buffer(&soloed), buffer(&muted));
}

#[test]
fn thin_takes_the_low_end_out_each_pass() {
    // Brightness over power rises as the low end goes.
    let tilt = |x: &[f32]| brightness(x) / rms(x).powi(2);
    let src = noise(4800, 0.9);
    let mut t = tape(src.clone(), 1, &[Setting::Write(true), Setting::Thin(1.0)]);
    let mut last = tilt(&src);
    for pass in 0..5 {
        run(&mut t, 4800);
        let b = tilt(&buffer(&t));
        assert!(b > 1.02 * last, "pass {pass}: {b} vs {last}");
        last = b;
    }
}

/// A 40 Hz sine of 0.4, whose own largest step is 0.0021, in a loop of whole
/// cycles.
fn low_sine() -> Vec<f32> {
    sine(48_000, 40.0, 0.4)
}

/// The largest step round the loop, wrap included.
fn max_step_round(x: &[f32]) -> f32 {
    max_step(x).max((x[0] - x[x.len() - 1]).abs())
}

#[test]
fn thin_leaving_and_reaching_0_writes_no_step() {
    let mut t = tape(low_sine(), 1, &[Setting::Write(true)]);
    run(&mut t, 4800);
    // Its lowest setting is a 20 Hz high-pass; switched in at once, it wrote
    // a step of 0.15.
    t.set(Setting::Thin(0.001));
    run(&mut t, 48_000);
    t.set(Setting::Thin(0.0));
    run(&mut t, 4800);
    let step = max_step_round(&buffer(&t));
    assert!(step < 0.003, "{step}");
}

#[test]
fn the_voice_filter_leaving_its_ends_or_changing_kind_does_not_step() {
    for (from, to) in [
        // High at 0 and low at 1 pass the input unchanged.
        (
            [Setting::Filter(0, Filter::High), Setting::Cutoff(0, 0.0)],
            [Setting::Cutoff(0, 0.001), Setting::Cutoff(0, 0.001)],
        ),
        (
            [Setting::Filter(0, Filter::Low), Setting::Cutoff(0, 1.0)],
            [Setting::Cutoff(0, 0.999), Setting::Cutoff(0, 0.999)],
        ),
        (
            [Setting::Filter(0, Filter::Low), Setting::Cutoff(0, 0.3)],
            [
                Setting::Filter(0, Filter::High),
                Setting::Filter(0, Filter::Band),
            ],
        ),
    ] {
        let mut t = tape(low_sine(), 1, &from);
        let mut out = left(&run(&mut t, 4800));
        t.set(to[0]);
        out.extend(left(&run(&mut t, 4800)));
        t.set(to[1]);
        out.extend(left(&run(&mut t, 4800)));
        let step = max_step(&out[2400..]);
        assert!(step < 0.003, "{from:?} to {to:?}: {step}");
    }
}

#[test]
fn drive_reaching_0_does_not_step() {
    // Above the clip's knee, drive 0.01 gave 0.881 of 1.0 and drive 0 all of it.
    let mut t = tape(vec![1.0; 4800], 1, &[Setting::Drive(0, 0.01)]);
    let mut out = left(&run(&mut t, 4800));
    t.set(Setting::Drive(0, 0.0));
    out.extend(left(&run(&mut t, 4800)));
    assert_eq!(out.last(), Some(&1.0));
    let step = max_step(&out[2400..]);
    assert!(step < 0.001, "{step}");
}

#[test]
fn a_fading_loop_reaches_0_without_subnormals() {
    let mut t = tape(
        noise(4800, 0.9),
        1,
        &[Setting::Write(true), Setting::Feedback(0.5)],
    );
    // Unflushed, they appeared at pass 106 and filled the loop by 140.
    for pass in 0..200 {
        let out = run(&mut t, 4800);
        let b = buffer(&t);
        assert!(
            !b.iter().chain(&out).any(|s| s.is_subnormal()),
            "pass {pass}"
        );
    }
    assert!(buffer(&t).iter().all(|s| *s == 0.0));
}

#[test]
fn the_knee_sets_where_the_write_head_starts_to_clip() {
    let peak = |knee: Option<f32>| {
        let mut settings = vec![
            Setting::Write(true),
            Setting::Feedback(0.0),
            Setting::Send(0, 1.0),
        ];
        settings.extend(knee.map(Setting::Knee));
        let mut t = tape(sine(4800, 1000.0, 0.8), 1, &settings);
        run(&mut t, 4800);
        buffer(&t).iter().fold(0.0f32, |m, s| m.max(s.abs()))
    };
    let (default, high) = (peak(None), peak(Some(0.9)));
    // 0.5 + 0.5 tanh(0.6) at the default knee, 0.5.
    assert!((default - 0.768).abs() < 0.01, "{default}");
    assert!((high - 0.8).abs() < 0.01, "{high}");
}
