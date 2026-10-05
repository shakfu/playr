//! Tier 1 of the decks, on `Mixer::process` with no device: the EQ, the
//! filter, hot cues, beat jumps, loops, the phase lock, the cue on channels
//! 3 and 4, and the crossfader's curve.

use playr_dj::{Band, CueOut, Curve, Grid, Mixer, Nudge, Setting, Side, Track};

const SR: u32 = 48_000;
const BLOCK: usize = 512;
/// Enough blocks for every ramp and filter to settle.
const SETTLE: usize = 20;

fn sine(hz: f64, seconds: f64, amp: f32) -> Vec<f32> {
    let n = (seconds * SR as f64) as usize;
    (0..n)
        .map(|i| amp * (std::f64::consts::TAU * hz * i as f64 / SR as f64).sin() as f32)
        .collect()
}

fn noise(n: usize, amp: f32) -> Vec<f32> {
    let mut x: u32 = 0x1234_5678;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            amp * (x as f32 / u32::MAX as f32 * 2.0 - 1.0)
        })
        .collect()
}

fn run(m: &mut Mixer, blocks: usize) -> Vec<f32> {
    let mut all = Vec::with_capacity(blocks * BLOCK * 2);
    let mut out = vec![0.0; BLOCK * 2];
    for _ in 0..blocks {
        m.process(&mut out);
        all.extend_from_slice(&out);
    }
    all
}

fn db(x: &[f32]) -> f64 {
    let ms = x.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / x.len() as f64;
    10.0 * ms.log10()
}

/// Deck A alone on the main mix, playing mono `samples`.
fn playing(samples: Vec<f32>, grid: Option<Grid>) -> Mixer {
    let mut m = Mixer::new(SR);
    m.load(Side::A, Box::new(Track::new(samples, 1, grid).unwrap()))
        .unwrap();
    m.set(Setting::Xfade(0.0));
    m.set(Setting::Play(Side::A));
    m
}

/// The level of `samples` through deck A after `settings`, in dB.
fn level_after(samples: Vec<f32>, settings: &[Setting]) -> f64 {
    let mut m = playing(samples, None);
    for s in settings {
        m.set(*s);
    }
    run(&mut m, SETTLE);
    db(&run(&mut m, 40))
}

#[test]
fn a_kill_takes_its_band_down_by_40_db() {
    for (hz, band) in [
        (60.0, Band::Low),
        (785.0, Band::Mid),
        (10_000.0, Band::High),
    ] {
        let open = level_after(sine(hz, 3.0, 0.2), &[]);
        let killed = level_after(sine(hz, 3.0, 0.2), &[Setting::Kill(Side::A, band, true)]);
        assert!(
            open - killed >= 40.0,
            "{band:?} at {hz} Hz: {open} {killed}"
        );
        // The other bands pass it.
        for other in [Band::Low, Band::Mid, Band::High]
            .into_iter()
            .filter(|b| *b != band)
        {
            let kept = level_after(sine(hz, 3.0, 0.2), &[Setting::Kill(Side::A, other, true)]);
            assert!(
                (open - kept).abs() < 0.5,
                "{other:?} at {hz} Hz: {open} {kept}"
            );
        }
    }
}

#[test]
fn the_bands_sum_flat() {
    let open = level_after(noise(SR as usize * 3, 0.2), &[]);
    let all = [Band::Low, Band::Mid, Band::High].map(|b| Setting::Eq(Side::A, b, -6.0));
    let cut = level_after(noise(SR as usize * 3, 0.2), &all);
    assert!((open - cut - 6.0).abs() < 0.1, "{open} {cut}");
}

#[test]
fn a_band_s_gain_moves_its_level() {
    let open = level_after(sine(60.0, 3.0, 0.1), &[]);
    let boosted = level_after(
        sine(60.0, 3.0, 0.1),
        &[Setting::Eq(Side::A, Band::Low, 6.0)],
    );
    assert!((boosted - open - 6.0).abs() < 0.2, "{open} {boosted}");
}

#[test]
fn the_filter_cuts_lows_or_highs_and_passes_all_at_centre() {
    let high = || sine(10_000.0, 3.0, 0.2);
    let low = || sine(60.0, 3.0, 0.2);
    let open = level_after(high(), &[]);
    let lp = level_after(high(), &[Setting::Filter(Side::A, -1.0)]);
    assert!(open - lp >= 40.0, "{open} {lp}");
    let open = level_after(low(), &[]);
    let hp = level_after(low(), &[Setting::Filter(Side::A, 1.0)]);
    assert!(open - hp >= 40.0, "{open} {hp}");
    let half = level_after(low(), &[Setting::Filter(Side::A, -0.5)]);
    assert!(
        (open - half).abs() < 0.5,
        "a half-closed low-pass keeps 60 Hz"
    );

    // Within the deadband the deck plays its buffer exactly.
    let src = noise(SR as usize, 0.4);
    let mut m = playing(src.clone(), None);
    m.set(Setting::Filter(Side::A, 0.03));
    let out = run(&mut m, 20);
    let mono: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
    assert_eq!(mono[BLOCK..], src[BLOCK..mono.len()]);
}

fn gridded(seconds: f64, bpm: f64, t0: f64) -> Box<Track> {
    let n = (seconds * SR as f64) as usize;
    Box::new(Track::new(vec![0.0; n], 1, Some(Grid::new(bpm, t0).unwrap())).unwrap())
}

/// Where in its beat deck `side` is.
fn phase(m: &Mixer, side: Side) -> f64 {
    m.deck(side).phase().unwrap()
}

#[test]
fn a_hot_cue_is_set_on_a_beat_and_jumps_back_in_phase() {
    let beat = 60.0 / 120.0 * SR as f64;
    let mut m = Mixer::new(SR);
    m.load(Side::A, gridded(60.0, 120.0, 0.1)).unwrap();
    m.set(Setting::Quantize(true));
    m.set(Setting::Play(Side::A));
    run(&mut m, 40);
    m.set(Setting::HotCue(Side::A, 2));
    let at = m.deck(Side::A).hot()[2].unwrap();
    let beats = (at / SR as f64 - 0.1) / 0.5;
    assert!((beats - beats.round()).abs() < 1e-9, "{beats}");
    run(&mut m, 97);
    let before = phase(&m, Side::A);
    m.set(Setting::HotCue(Side::A, 2));
    assert!((phase(&m, Side::A) - before).abs() < 1e-9);
    assert!(m.deck(Side::A).pos() - at < beat);

    // Paused, it jumps to the cue and plays.
    m.set(Setting::Pause(Side::A));
    run(&mut m, 2);
    m.set(Setting::HotCue(Side::A, 2));
    assert_eq!(m.deck(Side::A).pos(), at);
    assert!(m.deck(Side::A).playing());

    m.set(Setting::HotClear(Side::A, 2));
    assert_eq!(m.deck(Side::A).hot()[2], None);
    m.set(Setting::HotCues(
        Side::A,
        [Some(1.0), None, None, Some(2.0)],
    ));
    assert_eq!(m.deck(Side::A).hot(), [Some(1.0), None, None, Some(2.0)]);
}

#[test]
fn a_beat_jump_moves_whole_beats() {
    let beat = 60.0 / 125.0 * SR as f64;
    let mut m = Mixer::new(SR);
    m.load(Side::A, gridded(60.0, 125.0, 0.0)).unwrap();
    run(&mut m, 1);
    m.set(Setting::BeatJump(Side::A, 4.0));
    assert!((m.deck(Side::A).pos() - 4.0 * beat).abs() < 1e-6);
    m.set(Setting::BeatJump(Side::A, -1.0));
    assert!((m.deck(Side::A).pos() - 3.0 * beat).abs() < 1e-6);
}

#[test]
fn an_auto_loop_repeats_whole_beats_from_a_beat() {
    let beat = 60.0 / 128.0 * SR as f64;
    let mut m = Mixer::new(SR);
    m.load(Side::A, gridded(60.0, 128.0, 0.05)).unwrap();
    m.set(Setting::Quantize(true));
    m.set(Setting::Play(Side::A));
    run(&mut m, 33);
    m.set(Setting::Loop(Side::A, Some(2.0)));
    let (start, len) = m.deck(Side::A).looping().unwrap();
    assert!((len - 2.0 * beat).abs() < 1e-6);
    let beats = (start / SR as f64 - 0.05) * 128.0 / 60.0;
    assert!((beats - beats.round()).abs() < 1e-9, "{beats}");
    for _ in 0..200 {
        run(&mut m, 1);
        let pos = m.deck(Side::A).pos();
        assert!(
            pos >= start && pos < start + len,
            "{pos} outside {start}..{}",
            start + len
        );
    }
    // A beat jump moves the loop with the head.
    m.set(Setting::BeatJump(Side::A, 8.0));
    assert_eq!(m.deck(Side::A).looping().unwrap().0, start + 8.0 * beat);
    m.set(Setting::Loop(Side::A, None));
    run(&mut m, 200);
    assert!(m.deck(Side::A).pos() > start + 8.0 * beat + len);
}

/// Deck B synced to deck A, both playing.
fn synced() -> Mixer {
    let mut m = Mixer::new(SR);
    m.load(Side::A, gridded(120.0, 128.0, 0.137)).unwrap();
    m.load(Side::B, gridded(120.0, 125.0, 0.52)).unwrap();
    m.set(Setting::Play(Side::A));
    m.set(Setting::Play(Side::B));
    run(&mut m, 7);
    m.set(Setting::Sync(Side::B, true));
    run(&mut m, 4);
    m
}

/// How far deck B's beats are from deck A's, in beats.
fn error(m: &Mixer) -> f64 {
    (phase(m, Side::A) - phase(m, Side::B) + 0.5).rem_euclid(1.0) - 0.5
}

#[test]
fn the_phase_lock_pulls_a_synced_deck_back_into_phase() {
    let mut m = synced();
    assert!(error(&m).abs() < 1e-3);
    // A grid 40 ms off puts deck B a twelfth of a beat out.
    m.set(Setting::Grid(
        Side::B,
        Some(Grid::new(125.0, 0.56).unwrap()),
    ));
    assert!(error(&m).abs() > 0.05, "{}", error(&m));
    run(&mut m, 5 * SR as usize / BLOCK);
    assert!(error(&m).abs() <= 0.011, "{}", error(&m));
    assert!(m.deck(Side::B).synced());
}

#[test]
fn a_nudge_on_a_synced_deck_moves_the_phase_the_lock_holds() {
    let mut m = synced();
    // 4% faster for 1.07 s: about 0.09 of a beat.
    m.set(Setting::Nudge(Side::B, Nudge::Ahead));
    run(&mut m, 100);
    m.set(Setting::Nudge(Side::B, Nudge::Off));
    run(&mut m, 4);
    let left = error(&m);
    assert!(left.abs() > 0.05, "{left}");
    run(&mut m, 5 * SR as usize / BLOCK);
    assert!((error(&m) - left).abs() < 0.011, "{left} {}", error(&m));
}

#[test]
fn the_cue_goes_to_channels_3_and_4_in_stereo() {
    let mut m = Mixer::new(SR);
    let a: Vec<f32> = (0..SR).flat_map(|_| [0.2, 0.0]).collect();
    let b: Vec<f32> = (0..SR).flat_map(|_| [0.0, 0.3]).collect();
    m.load(Side::A, Box::new(Track::new(a, 2, None).unwrap()))
        .unwrap();
    m.load(Side::B, Box::new(Track::new(b, 2, None).unwrap()))
        .unwrap();
    for s in [
        Setting::Xfade(0.0),
        Setting::Play(Side::A),
        Setting::Play(Side::B),
        Setting::CueBus(Some(Side::B)),
        Setting::CueOut(CueOut::Channels),
    ] {
        m.set(s);
    }
    let mut out = vec![0.0; BLOCK * 4];
    for _ in 0..4 {
        m.process_channels(&mut out, 4);
    }
    for f in out.chunks(4) {
        assert!(
            (f[0] - 0.2).abs() < 1e-6 && f[1].abs() < 1e-6,
            "main: {f:?}"
        );
        assert!(f[2].abs() < 1e-6 && (f[3] - 0.3).abs() < 1e-6, "cue: {f:?}");
    }
    // A stereo device gets the split cue.
    let mut out = vec![0.0; BLOCK * 2];
    m.process_channels(&mut out, 2);
    for f in out.chunks(2) {
        assert!(
            (f[0] - 0.1).abs() < 1e-6 && (f[1] - 0.15).abs() < 1e-6,
            "{f:?}"
        );
    }
}

#[test]
fn the_sharp_curve_holds_both_decks_until_the_ends() {
    let gains = |x: f64| {
        let mut m = Mixer::new(SR);
        m.load(
            Side::A,
            Box::new(Track::new(vec![0.25; SR as usize], 1, None).unwrap()),
        )
        .unwrap();
        m.set(Setting::Curve(Curve::Sharp));
        m.set(Setting::Xfade(x));
        m.set(Setting::Play(Side::A));
        let out = run(&mut m, 10);
        out[out.len() - 2] / 0.25
    };
    assert!((gains(0.5) - 1.0).abs() < 1e-6);
    assert!((gains(0.9) - 1.0).abs() < 1e-6);
    assert!((gains(0.98) - 0.4).abs() < 1e-5, "{}", gains(0.98));
    assert!(gains(1.0).abs() < 1e-6);
}

#[test]
fn a_seek_moves_the_head_keeping_phase_with_quantize() {
    let beat = 60.0 / 120.0 * SR as f64;
    let mut m = Mixer::new(SR);
    m.load(Side::A, gridded(60.0, 120.0, 0.1)).unwrap();
    m.set(Setting::Seek(Side::A, 100_000.0));
    assert_eq!(m.deck(Side::A).pos(), 100_000.0);

    m.set(Setting::Quantize(true));
    m.set(Setting::Play(Side::A));
    run(&mut m, 21);
    let before = phase(&m, Side::A);
    m.set(Setting::Seek(Side::A, 1_000_000.0));
    assert!((phase(&m, Side::A) - before).abs() < 1e-9);
    assert!((m.deck(Side::A).pos() - 1_000_000.0).abs() < beat);

    // Inside a loop it stays; outside, the loop ends.
    m.set(Setting::Loop(Side::A, Some(4.0)));
    let (start, _) = m.deck(Side::A).looping().unwrap();
    m.set(Setting::Seek(Side::A, start + beat));
    assert!(m.deck(Side::A).looping().is_some());
    m.set(Setting::Seek(Side::A, 100_000.0));
    assert_eq!(m.deck(Side::A).looping(), None);
}

#[test]
fn a_mute_silences_the_main_mix_but_not_the_cue() {
    let mut m = playing(vec![0.2; SR as usize], None);
    m.set(Setting::Mute(Side::A, true));
    run(&mut m, SETTLE);
    assert!(run(&mut m, 1).iter().all(|s| s.abs() < 1e-7));
    m.set(Setting::CueBus(Some(Side::A)));
    let out = run(&mut m, 1);
    for f in out.chunks(2) {
        assert!(f[0].abs() < 1e-7 && (f[1] - 0.2).abs() < 1e-6, "{f:?}");
    }
    m.set(Setting::Mute(Side::A, false));
    m.set(Setting::CueBus(None));
    run(&mut m, SETTLE);
    assert!(run(&mut m, 1).iter().all(|s| (s - 0.2).abs() < 1e-6));
}

#[test]
fn the_crossfader_glides_to_a_point_over_400_ms() {
    let mut m = Mixer::new(SR);
    m.set(Setting::XfadeGlide(0.0));
    // 18 blocks, 192 ms in: 0.48 of the way from the centre.
    run(&mut m, 18);
    let done = 18.0 * BLOCK as f64 / (0.4 * SR as f64);
    assert!(
        (m.xfade() - 0.5 * (1.0 - done)).abs() < 1e-9,
        "{}",
        m.xfade()
    );
    run(&mut m, (0.21 * SR as f64) as usize / BLOCK + 1);
    assert_eq!(m.xfade(), 0.0);
    // A move by hand takes over from wherever the glide is.
    m.set(Setting::XfadeGlide(1.0));
    run(&mut m, 10);
    m.set(Setting::Xfade(0.3));
    run(&mut m, 2);
    assert_eq!(m.xfade(), 0.3);
}
