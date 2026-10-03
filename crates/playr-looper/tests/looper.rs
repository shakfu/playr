//! The looper as the audio callback runs it, with no device: the test calls
//! `Looper::process` in place of cpal.

use playr_looper::{Handle, Loop, Looper, Returned, Setting, Window};

const SR: u32 = 48_000;
const BLOCK: usize = 512;

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

fn block(l: &mut Looper) -> Vec<f32> {
    let mut out = vec![0.0; BLOCK * 2];
    l.process(&mut out);
    out
}

/// A looper playing `samples`, writing with all three voices sending.
fn writing(samples: Vec<f32>, channels: u16) -> (Looper, Handle) {
    let (mut l, mut h) = playr_looper::new(SR);
    h.load(Loop::new(samples, channels).unwrap()).unwrap();
    for s in [
        Setting::Write(true),
        Setting::Feedback(0.6),
        Setting::Send(0, 0.5),
        Setting::On(1, true),
        Setting::Rate(1, -0.5),
        Setting::Send(1, 0.7),
        Setting::On(2, true),
        Setting::Rate(2, 1.5),
        Setting::Send(2, 0.4),
        Setting::Play,
    ] {
        h.set(s).unwrap();
    }
    block(&mut l);
    // The empty loop the load replaced.
    assert!(matches!(h.poll(), Some(Returned::Loop(_))));
    (l, h)
}

#[test]
fn load_returns_the_old_loop() {
    let (mut l, mut h) = playr_looper::new(SR);
    let a = noise(1000, 0.5);
    h.load(Loop::new(a.clone(), 1).unwrap()).unwrap();
    block(&mut l);
    assert!(matches!(h.poll(), Some(Returned::Loop(lp)) if lp.frames() == 0));
    h.load(Loop::new(noise(500, 0.5), 2).unwrap()).unwrap();
    block(&mut l);
    match h.poll() {
        Some(Returned::Loop(lp)) => assert_eq!(lp.samples(), a),
        other => panic!("{other:?}"),
    }
    assert!(h.poll().is_none());
}

#[test]
fn a_snapshot_is_the_loop_when_it_arrived() {
    let frames = 48_000;
    for w in [
        Window::new(frames / 2, frames),
        Window::new(frames / 4, frames * 3 / 4),
    ] {
        let (mut l, mut h) = writing(noise(frames * 2, 0.8), 2);
        h.set(Setting::WriteWindow(w)).unwrap();
        for _ in 0..20 {
            block(&mut l);
        }
        let want = l.tape().buffer().samples().to_vec();
        h.snapshot().unwrap();
        let got = loop {
            block(&mut l);
            match h.poll() {
                Some(Returned::Snapshot(s)) => break s,
                Some(other) => panic!("{other:?}"),
                None => {}
            }
        };
        assert_ne!(l.tape().buffer().samples(), want, "the loop kept changing");
        assert_eq!(*got, *want, "window {w:?}");
    }
}

#[test]
fn a_new_write_window_aborts_a_snapshot() {
    let (mut l, mut h) = writing(noise(48_000, 0.8), 1);
    h.snapshot().unwrap();
    block(&mut l);
    h.set(Setting::WriteWindow(Window::new(0, 1000))).unwrap();
    block(&mut l);
    assert!(matches!(h.poll(), Some(Returned::Aborted(_))));
}

#[test]
fn a_recording_is_the_blocks_processed_less_those_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mix.wav");
    // Long enough that no two blocks of output repeat.
    let (mut l, mut h) = writing(noise(BLOCK * 600, 0.8), 1);
    h.record(&path).unwrap();
    // A block is recorded, or dropped, when recording is on after it.
    let status = h.status().clone();
    let mut recorded = Vec::new();
    let mut step = |l: &mut Looper| {
        let out = block(l);
        if status.recording() {
            recorded.push(out);
        }
    };
    for _ in 0..500 {
        step(&mut l);
    }
    // The stop waits for the callback to acknowledge it.
    let stop = std::thread::spawn(move || (h.stop_recording(), h));
    while !stop.is_finished() {
        step(&mut l);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let (rec, h) = stop.join().unwrap();
    let rec = rec.unwrap();
    assert!(!h.status().recording());

    let wav: Vec<f32> = hound::WavReader::open(&path)
        .unwrap()
        .into_samples::<f32>()
        .map(Result::unwrap)
        .collect();
    assert_eq!(wav.len() as u64, rec.frames * 2);
    assert_eq!(
        rec.frames / BLOCK as u64 + rec.dropped,
        recorded.len() as u64
    );
    // What was written is the produced blocks, in order, less the dropped ones.
    let mut blocks = recorded.iter();
    for chunk in wav.chunks(BLOCK * 2) {
        assert!(blocks.any(|b| b == chunk));
    }
}

#[test]
fn the_status_follows_the_heads_and_the_waveform() {
    let (mut l, h) = writing(noise(4800, 0.8), 1);
    for _ in 0..3 {
        block(&mut l);
    }
    let s = h.status();
    assert!(s.playing());
    assert_eq!(s.voice(0), l.tape().voice(0));
    assert_eq!(s.write_head(), l.tape().write_head());
    assert!(s.take_peak() > 0.0);
    assert_eq!(s.columns(), *l.tape().buffer().peaks());
}

#[test]
fn reset_restores_the_loop_as_loaded() {
    let src = noise(4800, 0.8);
    let (mut l, mut h) = writing(src.clone(), 1);
    for _ in 0..20 {
        block(&mut l);
    }
    assert_ne!(l.tape().buffer().samples(), src);
    h.reset().unwrap();
    h.set(Setting::Stop).unwrap();
    block(&mut l);
    assert_eq!(l.tape().buffer().samples(), src);
    assert_eq!(l.tape().voice(0), 0.0);
}
