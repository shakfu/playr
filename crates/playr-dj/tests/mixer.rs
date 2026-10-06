//! The decks and the mix with no device: the test calls `Mixer::process`
//! in place of the engine.

use playr_dj::{Grid, Mixer, Nudge, Range, Setting, Side, Track};

const SR: u32 = 48_000;
const BLOCK: usize = 512;
/// Enough blocks for every ramp and declick to finish.
const SETTLE: usize = 4;

fn noise(n: usize, amp: f32, seed: u32) -> Vec<f32> {
    let mut x = seed;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            amp * (x as f32 / u32::MAX as f32 * 2.0 - 1.0)
        })
        .collect()
}

fn block(m: &mut Mixer) -> Vec<f32> {
    let mut out = vec![0.0; BLOCK * 2];
    m.process(&mut out);
    out
}

fn run(m: &mut Mixer, blocks: usize) {
    (0..blocks).for_each(|_| _ = block(m));
}

/// A silent mono track of `seconds` with a grid.
fn gridded(seconds: f64, bpm: f64, t0: f64) -> Box<Track> {
    gridded_at(SR, seconds, bpm, t0)
}

fn gridded_at(rate: u32, seconds: f64, bpm: f64, t0: f64) -> Box<Track> {
    let n = (seconds * rate as f64) as usize;
    let grid = Grid::new(bpm, t0).unwrap();
    Box::new(Track::new(vec![0.0; n], 1, Some(grid)).unwrap())
}

fn mixer(a: Box<Track>, b: Box<Track>) -> Mixer {
    mixer_at(SR, a, b)
}

fn mixer_at(rate: u32, a: Box<Track>, b: Box<Track>) -> Mixer {
    let mut m = Mixer::new(rate);
    m.load(Side::A, a).unwrap();
    m.load(Side::B, b).unwrap();
    m
}

/// How far `side`'s beats sit from the other deck's, in output frames,
/// counting its beats `k` to each beat of its grid.
fn phase_error(m: &Mixer, side: Side, k: f64) -> f64 {
    let (f, l) = (m.deck(side), m.deck(side.other()));
    let (gf, gl) = (f.grid().unwrap(), l.grid().unwrap());
    let sr = m.sample_rate() as f64;
    let at_f = gf.beats(f.pos() / sr) * k;
    let at_l = gl.beats(l.pos() / sr);
    let e = (at_l - at_f + 0.5).rem_euclid(1.0) - 0.5;
    e * gf.period() / k * sr / f.rate()
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

#[test]
fn at_rate_1_a_deck_plays_its_buffer_exactly() {
    let src = noise(SR as usize * 2, 0.4, 0x1234_5678);
    let mut m = Mixer::new(SR);
    m.load(Side::A, Box::new(Track::new(src.clone(), 2, None).unwrap()))
        .unwrap();
    m.set(Setting::Xfade(0.0));
    m.set(Setting::Play(Side::A));
    let out: Vec<f32> = (0..20).flat_map(|_| block(&mut m)).collect();
    // Past the 5 ms fade-in and the crossfader's 10 ms move.
    let from = 2 * BLOCK;
    assert_eq!(out[from..], src[from..out.len()]);
}

#[test]
fn the_master_passes_a_deck_at_unity_up_to_near_full_scale() {
    let play = |amp| {
        let src = noise(SR as usize * 2, amp, 0x1234_5678);
        let mut m = Mixer::new(SR);
        m.load(Side::A, Box::new(Track::new(src.clone(), 2, None).unwrap()))
            .unwrap();
        m.set(Setting::Xfade(0.0));
        m.set(Setting::Play(Side::A));
        let out: Vec<f32> = (0..20).flat_map(|_| block(&mut m)).collect();
        (src, out)
    };
    let from = 2 * BLOCK;
    let (src, out) = play(0.9);
    assert_eq!(out[from..], src[from..out.len()]);
    // Full-scale peaks lose under 0.25 dB and stay within it.
    let (_, out) = play(1.0);
    let peak = out[from..].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!((0.972..=1.0).contains(&peak), "{peak}");
}

#[test]
fn at_rate_r_a_deck_covers_r_frames_a_frame() {
    let mut m = mixer(gridded(10.0, 120.0, 0.0), gridded(10.0, 120.0, 0.0));
    m.set(Setting::Rate(Side::A, 6.0));
    m.set(Setting::Play(Side::A));
    run(&mut m, SETTLE);
    let before = m.deck(Side::A).pos();
    run(&mut m, 10);
    let covered = m.deck(Side::A).pos() - before;
    assert!(
        (covered - 1.06 * 10.0 * BLOCK as f64).abs() < 1e-6,
        "{covered}"
    );
}

#[test]
fn the_rate_is_clamped_to_its_range() {
    let mut m = mixer(gridded(1.0, 120.0, 0.0), gridded(1.0, 120.0, 0.0));
    m.set(Setting::Rate(Side::A, 12.0));
    assert_eq!(m.deck(Side::A).pct(), 8.0);
    m.set(Setting::Range(Side::A, Range::Medium));
    m.set(Setting::Rate(Side::A, 12.0));
    assert_eq!(m.deck(Side::A).pct(), 12.0);
    m.set(Setting::Range(Side::A, Range::Narrow));
    assert_eq!(m.deck(Side::A).pct(), 8.0);
}

#[test]
fn a_nudge_bends_the_rate_while_held() {
    let mut m = mixer(gridded(10.0, 120.0, 0.0), gridded(10.0, 120.0, 0.0));
    m.set(Setting::Play(Side::A));
    m.set(Setting::Nudge(Side::A, Nudge::Ahead));
    run(&mut m, SETTLE);
    assert!((m.deck(Side::A).rate() - 1.04).abs() < 1e-12);
    m.set(Setting::Nudge(Side::A, Nudge::Off));
    run(&mut m, SETTLE);
    assert_eq!(m.deck(Side::A).rate(), 1.0);
}

#[test]
fn tempo_sync_matches_the_effective_tempos() {
    let mut m = mixer(gridded(10.0, 128.0, 0.1), gridded(10.0, 124.3, 0.37));
    m.set(Setting::Rate(Side::A, 1.7));
    m.set(Setting::Sync(Side::B, true));
    run(&mut m, SETTLE);
    let (a, b) = (
        m.deck(Side::A).bpm().unwrap(),
        m.deck(Side::B).bpm().unwrap(),
    );
    assert!((a - b).abs() < 1e-9, "{a} {b}");
    assert!(m.deck(Side::B).synced());

    // A synced deck follows the leader's fader, but not its nudge.
    m.set(Setting::Rate(Side::A, -3.0));
    m.set(Setting::Nudge(Side::A, Nudge::Behind));
    run(&mut m, SETTLE);
    let b = m.deck(Side::B).bpm().unwrap();
    assert!((b - 128.0 * 0.97).abs() < 1e-9, "{b}");

    // Moving its own fader ends it.
    m.set(Setting::Rate(Side::B, 0.0));
    assert!(!m.deck(Side::B).synced());
}

#[test]
fn phase_sync_holds_for_5_minutes() {
    // At 8 kHz, a sixth of the frames 48 kHz takes to cover 5 minutes; a
    // frame is then 125 us.
    const SR: u32 = 8000;
    let mut m = mixer_at(
        SR,
        gridded_at(SR, 330.0, 128.0, 0.137),
        gridded_at(SR, 330.0, 125.0, 0.52),
    );
    m.set(Setting::Play(Side::A));
    m.set(Setting::Play(Side::B));
    run(&mut m, 7);
    assert!(phase_error(&m, Side::B, 1.0).abs() > 10.0);
    m.set(Setting::Sync(Side::B, true));
    run(&mut m, SETTLE);
    let e = phase_error(&m, Side::B, 1.0);
    assert!(e.abs() < 1.0, "{e}");
    run(&mut m, 5 * 60 * SR as usize / BLOCK);
    let e = phase_error(&m, Side::B, 1.0);
    assert!(e.abs() < 1.0, "after 5 minutes: {e}");
}

#[test]
fn phase_sync_holds_through_a_leader_rate_change() {
    let mut m = mixer(gridded(60.0, 128.0, 0.137), gridded(60.0, 125.0, 0.52));
    m.set(Setting::Play(Side::A));
    m.set(Setting::Play(Side::B));
    run(&mut m, 11);
    m.set(Setting::Sync(Side::B, true));
    run(&mut m, SETTLE);
    m.set(Setting::Rate(Side::A, 3.3));
    run(&mut m, 500);
    let e = phase_error(&m, Side::B, 1.0);
    assert!(e.abs() < 1.0, "{e}");
}

#[test]
fn half_tempo_syncs_to_double_at_rate_1() {
    let mut m = mixer(gridded(60.0, 174.0, 0.05), gridded(60.0, 87.0, 0.3));
    m.set(Setting::Play(Side::A));
    m.set(Setting::Play(Side::B));
    run(&mut m, 7);
    m.set(Setting::Sync(Side::B, true));
    run(&mut m, SETTLE);
    assert!(m.deck(Side::B).synced());
    assert!((m.deck(Side::B).rate() - 1.0).abs() < 1e-12);
    let e = phase_error(&m, Side::B, 2.0);
    assert!(e.abs() < 1.0, "{e}");
}

#[test]
fn sync_widens_the_range_or_refuses() {
    let mut m = mixer(gridded(10.0, 136.0, 0.0), gridded(10.0, 120.0, 0.0));
    m.set(Setting::Sync(Side::B, true));
    assert!(m.deck(Side::B).synced());
    assert_eq!(m.deck(Side::B).range(), Range::Medium);
    assert!((m.deck(Side::B).pct() - (136.0 / 120.0 - 1.0) * 100.0).abs() < 1e-9);

    // 70.5 follows 100 doubled, at -29%.
    let mut m = mixer(gridded(10.0, 100.0, 0.0), gridded(10.0, 70.5, 0.0));
    m.set(Setting::Sync(Side::B, true));
    assert_eq!(m.deck(Side::B).range(), Range::Wide);

    // 100 doubled is still 60% under 320.
    let mut m = mixer(gridded(10.0, 320.0, 0.0), gridded(10.0, 100.0, 0.0));
    m.set(Setting::Sync(Side::B, true));
    assert!(!m.deck(Side::B).synced());
    assert_eq!(m.deck(Side::B).pct(), 0.0);
}

#[test]
fn sync_needs_both_grids() {
    let mut m = mixer(gridded(10.0, 128.0, 0.0), gridded(10.0, 120.0, 0.0));
    m.set(Setting::Grid(Side::A, None));
    m.set(Setting::Sync(Side::B, true));
    assert!(!m.deck(Side::B).synced());
}

#[test]
fn syncing_one_deck_unsyncs_the_other() {
    let mut m = mixer(gridded(10.0, 128.0, 0.0), gridded(10.0, 120.0, 0.0));
    m.set(Setting::Sync(Side::B, true));
    m.set(Setting::Sync(Side::A, true));
    assert!(m.deck(Side::A).synced());
    assert!(!m.deck(Side::B).synced());
}

#[test]
fn quantized_play_starts_in_phase() {
    let mut m = mixer(gridded(30.0, 128.0, 0.137), gridded(30.0, 128.0, 0.52));
    m.set(Setting::Quantize(true));
    m.set(Setting::Play(Side::A));
    run(&mut m, 13);
    assert!(phase_error(&m, Side::B, 1.0).abs() > 10.0);
    m.set(Setting::Play(Side::B));
    let e = phase_error(&m, Side::B, 1.0);
    assert!(e.abs() < 1.0, "{e}");
    run(&mut m, 100);
    let e = phase_error(&m, Side::B, 1.0);
    assert!(e.abs() < 1.0, "{e}");
}

#[test]
fn unquantized_play_starts_where_it_is() {
    let mut m = mixer(gridded(30.0, 128.0, 0.137), gridded(30.0, 128.0, 0.52));
    m.set(Setting::Play(Side::A));
    run(&mut m, 13);
    m.set(Setting::Play(Side::B));
    assert_eq!(m.deck(Side::B).pos(), 0.0);
}

#[test]
fn the_cue_works_as_on_a_cdj() {
    let mut m = mixer(gridded(30.0, 120.0, 0.0), gridded(30.0, 120.0, 0.0));
    let a = |m: &Mixer| m.deck(Side::A).clone();

    // Playing: CUE returns to the cue point, 0, and pauses.
    m.set(Setting::Play(Side::A));
    run(&mut m, 20);
    m.set(Setting::Cue(Side::A, true));
    run(&mut m, 1);
    assert!(!a(&m).playing());
    assert_eq!(a(&m).pos(), 0.0);
    m.set(Setting::Cue(Side::A, false));

    // Paused elsewhere: CUE sets the cue point there and plays while held.
    m.set(Setting::Play(Side::A));
    run(&mut m, 20);
    m.set(Setting::Pause(Side::A));
    run(&mut m, 1);
    let here = a(&m).pos();
    m.set(Setting::Cue(Side::A, true));
    assert_eq!(a(&m).cue(), here);
    run(&mut m, 10);
    assert!(a(&m).playing() && a(&m).previewing());
    assert!(a(&m).pos() > here);

    // Released: back to the cue point, paused.
    m.set(Setting::Cue(Side::A, false));
    run(&mut m, 1);
    assert!(!a(&m).playing());
    assert_eq!(a(&m).pos(), here);

    // Held again from the cue point, then PLAY: keeps playing on release.
    m.set(Setting::Cue(Side::A, true));
    run(&mut m, 5);
    m.set(Setting::Play(Side::A));
    m.set(Setting::Cue(Side::A, false));
    run(&mut m, 5);
    assert!(a(&m).playing() && !a(&m).previewing());

    // Play resumes from where it paused.
    m.set(Setting::Pause(Side::A));
    run(&mut m, 1);
    let paused = a(&m).pos();
    run(&mut m, 5);
    m.set(Setting::Play(Side::A));
    assert_eq!(a(&m).pos(), paused);
}

#[test]
fn a_quantized_cue_point_falls_on_a_beat() {
    let mut m = mixer(gridded(30.0, 120.0, 0.1), gridded(30.0, 120.0, 0.0));
    m.set(Setting::Quantize(true));
    m.set(Setting::Play(Side::A));
    run(&mut m, 30);
    m.set(Setting::Pause(Side::A));
    run(&mut m, 1);
    m.set(Setting::Cue(Side::A, true));
    let g = m.deck(Side::A).grid().unwrap();
    let beats = g.beats(m.deck(Side::A).cue() / SR as f64);
    assert!((beats - beats.round()).abs() < 1e-9, "{beats}");
}

#[test]
fn a_deck_stops_at_the_end_of_its_track() {
    let mut m = mixer(gridded(0.1, 120.0, 0.0), gridded(0.1, 120.0, 0.0));
    m.set(Setting::Play(Side::A));
    run(&mut m, 20);
    assert!(!m.deck(Side::A).playing());
}

#[test]
fn a_playing_deck_refuses_a_load() {
    let mut m = mixer(gridded(10.0, 120.0, 0.0), gridded(10.0, 120.0, 0.0));
    m.set(Setting::Play(Side::A));
    let new = gridded(1.0, 90.0, 0.0);
    let back = m.load(Side::A, new).unwrap_err();
    assert_eq!(back.grid().unwrap().bpm, 90.0);
    assert_eq!(m.deck(Side::A).grid().unwrap().bpm, 120.0);
}

#[test]
fn the_crossfader_keeps_the_power_constant() {
    let n = SR as usize;
    let mut m = Mixer::new(SR);
    let a = Track::new(noise(n * 30, 0.2, 0x1234_5678), 1, None).unwrap();
    let b = Track::new(noise(n * 30, 0.2, 0x9e37_79b9), 1, None).unwrap();
    m.load(Side::A, Box::new(a)).unwrap();
    m.load(Side::B, Box::new(b)).unwrap();
    m.set(Setting::Play(Side::A));
    m.set(Setting::Play(Side::B));
    let mut levels = Vec::new();
    for i in 0..=10 {
        m.set(Setting::Xfade(i as f64 / 10.0));
        run(&mut m, SETTLE);
        let mut out = vec![0.0; n * 2];
        m.process(&mut out);
        levels.push(20.0 * rms(&out).log10());
    }
    let (lo, hi) = levels
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), &x| (l.min(x), h.max(x)));
    assert!(hi - lo < 0.1, "{levels:?}");
}

#[test]
fn the_crossfader_ends_cut_the_other_deck() {
    let mut m = Mixer::new(SR);
    let a = Track::new(vec![0.25; SR as usize], 1, None).unwrap();
    m.load(Side::A, Box::new(a)).unwrap();
    m.set(Setting::Play(Side::A));
    m.set(Setting::Xfade(1.0));
    run(&mut m, SETTLE);
    assert!(block(&mut m).iter().all(|&s| s.abs() < 1e-7));
}

#[test]
fn trim_and_level_scale_the_deck() {
    let mut m = Mixer::new(SR);
    let a = Track::new(vec![0.1; SR as usize], 1, None).unwrap();
    m.load(Side::A, Box::new(a)).unwrap();
    m.set(Setting::Xfade(0.0));
    m.set(Setting::Gain(Side::A, 6.0));
    m.set(Setting::Level(Side::A, 0.5));
    m.set(Setting::Play(Side::A));
    run(&mut m, SETTLE);
    let want = 0.1 * 10f32.powf(6.0 / 20.0) * 0.5;
    assert!(block(&mut m).iter().all(|&s| (s - want).abs() < 1e-6));
}

#[test]
fn split_cue_puts_the_main_mix_and_the_cue_on_either_side() {
    let mut m = Mixer::new(SR);
    // Deck A: left 0.2, right 0.0. Deck B: left 0.0, right 0.3.
    let a: Vec<f32> = (0..SR).flat_map(|_| [0.2, 0.0]).collect();
    let b: Vec<f32> = (0..SR).flat_map(|_| [0.0, 0.3]).collect();
    m.load(Side::A, Box::new(Track::new(a, 2, None).unwrap()))
        .unwrap();
    m.load(Side::B, Box::new(Track::new(b, 2, None).unwrap()))
        .unwrap();
    m.set(Setting::Xfade(0.0));
    m.set(Setting::Play(Side::A));
    m.set(Setting::Play(Side::B));
    m.set(Setting::CueBus(Some(Side::B)));
    run(&mut m, SETTLE);
    let out = block(&mut m);
    for f in out.chunks(2) {
        assert!((f[0] - 0.1).abs() < 1e-6, "main: {}", f[0]);
        assert!((f[1] - 0.15).abs() < 1e-6, "cue: {}", f[1]);
    }
    m.set(Setting::CueSwap(true));
    let out = block(&mut m);
    for f in out.chunks(2) {
        assert!((f[0] - 0.15).abs() < 1e-6 && (f[1] - 0.1).abs() < 1e-6);
    }
}

#[test]
fn values_that_are_not_finite_are_ignored() {
    let mut m = mixer(gridded(1.0, 120.0, 0.0), gridded(1.0, 120.0, 0.0));
    m.set(Setting::Rate(Side::A, 3.0));
    m.set(Setting::Rate(Side::A, f64::NAN));
    assert_eq!(m.deck(Side::A).pct(), 3.0);
    m.set(Setting::Grid(Side::A, Some(Grid { bpm: 0.0, t0: 0.0 })));
    assert_eq!(m.deck(Side::A).grid().unwrap().bpm, 120.0);
}

/// A mono click track: 1 ms bursts of 0.4 on each beat of `grid`.
fn clicks(seconds: f64, grid: Grid) -> Box<Track> {
    let n = (seconds * SR as f64) as usize;
    let mut x = vec![0.0; n];
    let burst = SR as usize / 1000;
    let mut beat = 0.0;
    loop {
        let at = ((grid.t0 + beat * grid.period()) * SR as f64).round() as usize;
        if at + burst > n {
            break;
        }
        x[at..at + burst].fill(0.4);
        beat += 1.0;
    }
    Box::new(Track::new(x, 1, Some(grid)).unwrap())
}

/// Output frames where `ch` rises through 0.1.
fn onsets(out: &[f32], ch: usize) -> Vec<usize> {
    let x: Vec<f32> = out.chunks(2).map(|f| f[ch]).collect();
    (1..x.len())
        .filter(|&i| x[i - 1] < 0.1 && x[i] >= 0.1)
        .collect()
}

#[test]
fn after_sync_the_clicks_coincide() {
    let a = clicks(30.0, Grid::new(128.0, 0.137).unwrap());
    let b = clicks(30.0, Grid::new(125.0, 0.52).unwrap());
    let mut m = mixer(a, b);
    // Main, deck A only, on the left; deck B on the right.
    m.set(Setting::Xfade(0.0));
    m.set(Setting::CueBus(Some(Side::B)));
    m.set(Setting::Play(Side::A));
    m.set(Setting::Play(Side::B));
    run(&mut m, 29);
    m.set(Setting::Sync(Side::B, true));
    run(&mut m, SETTLE);
    let out: Vec<f32> = (0..2000).flat_map(|_| block(&mut m)).collect();
    let (l, r) = (onsets(&out, 0), onsets(&out, 1));
    assert!(l.len() > 40);
    assert_eq!(l.len(), r.len());
    for (a, b) in l.iter().zip(&r) {
        assert!(a.abs_diff(*b) <= 1, "{a} {b}");
    }
}

#[test]
fn cue_right_after_play_returns_at_once() {
    let mut m = mixer(gridded(30.0, 120.0, 0.0), gridded(30.0, 120.0, 0.0));
    m.set(Setting::Play(Side::A));
    run(&mut m, 20);
    m.set(Setting::Pause(Side::A));
    run(&mut m, 1);
    m.set(Setting::Play(Side::A));
    m.set(Setting::Cue(Side::A, true));
    assert!(!m.deck(Side::A).playing());
    assert_eq!(m.deck(Side::A).pos(), 0.0);
}

#[test]
fn an_octave_fix_on_the_leader_keeps_the_follower_rate() {
    let mut m = mixer(gridded(30.0, 87.0, 0.0), gridded(30.0, 172.0, 0.0));
    m.set(Setting::Sync(Side::B, true));
    let pct = m.deck(Side::B).pct();
    m.set(Setting::Grid(Side::A, Some(Grid::new(174.0, 0.0).unwrap())));
    run(&mut m, 1);
    assert!(m.deck(Side::B).synced());
    assert!((m.deck(Side::B).pct() - pct).abs() < 1e-9);
}

#[test]
fn loading_the_leader_ends_sync() {
    let mut m = mixer(gridded(30.0, 128.0, 0.0), gridded(30.0, 125.0, 0.0));
    m.set(Setting::Sync(Side::B, true));
    m.load(Side::A, gridded(30.0, 90.0, 0.0)).unwrap();
    run(&mut m, 1);
    assert!(!m.deck(Side::B).synced());
    assert!((m.deck(Side::B).pct() - (128.0 / 125.0 - 1.0) * 100.0).abs() < 1e-9);
}

#[test]
fn the_knee_sets_where_the_master_starts_to_clip() {
    let peak = |knee: Option<f32>| {
        let src = noise(SR as usize, 0.8, 0x1234_5678);
        let mut m = Mixer::new(SR);
        m.load(Side::A, Box::new(Track::new(src, 2, None).unwrap()))
            .unwrap();
        m.set(Setting::Xfade(0.0));
        knee.into_iter().for_each(|k| m.set(Setting::Knee(k)));
        m.set(Setting::Play(Side::A));
        let out: Vec<f32> = (0..20).flat_map(|_| block(&mut m)).collect();
        out.iter().fold(0.0f32, |m, x| m.max(x.abs()))
    };
    assert!((peak(None) - 0.8).abs() < 1e-3, "{}", peak(None));
    // 0.5 + 0.5 tanh(0.6), and a knee not finite is ignored.
    assert!((peak(Some(0.5)) - 0.768).abs() < 1e-3);
    assert_eq!(peak(Some(f32::NAN)), peak(None));
}

#[test]
fn a_leader_started_after_its_follower_is_locked_in_phase() {
    let mut m = mixer(gridded(60.0, 128.0, 0.137), gridded(60.0, 125.0, 0.52));
    m.set(Setting::Play(Side::B));
    m.set(Setting::Sync(Side::B, true));
    run(&mut m, 37);
    // Quantize is off, so the leader starts out of phase.
    m.set(Setting::Play(Side::A));
    run(&mut m, 2);
    let start = phase_error(&m, Side::B, 1.0);
    run(&mut m, 20 * SR as usize / BLOCK);
    let e = phase_error(&m, Side::B, 1.0);
    // Within the lock's dead band, 0.01 beat: 225 frames at 128 BPM.
    assert!(e.abs() < 230.0, "from {start} to {e}");
}

/// The master volume scales the main mix only, and the headphones the cue
/// only. The peak is the main mix's, before the master.
#[test]
fn the_master_and_the_headphones_scale_their_own_side() {
    let mut m = Mixer::new(SR);
    // Deck A: 0.2 in both channels. Deck B, cued: 0.3.
    let a: Vec<f32> = vec![0.2; SR as usize * 2];
    let b: Vec<f32> = vec![0.3; SR as usize * 2];
    m.load(Side::A, Box::new(Track::new(a, 2, None).unwrap()))
        .unwrap();
    m.load(Side::B, Box::new(Track::new(b, 2, None).unwrap()))
        .unwrap();
    m.set(Setting::Xfade(0.0));
    m.set(Setting::Play(Side::A));
    m.set(Setting::Play(Side::B));
    m.set(Setting::CueBus(Some(Side::B)));
    m.set(Setting::Volume(0.0));
    run(&mut m, SETTLE);
    m.take_peak();
    let out = block(&mut m);
    for f in out.chunks(2) {
        assert_eq!(f[0], 0.0, "main");
        assert!((f[1] - 0.3).abs() < 1e-6, "cue: {}", f[1]);
    }
    assert!(
        (m.take_peak() - 0.2).abs() < 1e-6,
        "the peak followed the master"
    );

    m.set(Setting::Volume(1.0));
    m.set(Setting::Headphones(0.5));
    run(&mut m, SETTLE);
    let out = block(&mut m);
    for f in out.chunks(2) {
        assert!((f[0] - 0.2).abs() < 1e-6, "main: {}", f[0]);
        assert!((f[1] - 0.15).abs() < 1e-6, "cue: {}", f[1]);
    }
}
