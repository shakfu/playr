//! The realtime callback, against a ring filled by hand.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use playr_core::audio::meter::Meter;
use playr_core::audio::output::{render, Shared};

/// Renders `out.len()` samples of stereo from `consumer`.
fn callback(
    out: &mut [f32],
    consumer: &mut rtrb::Consumer<f32>,
    shared: &Shared,
    meter: &mut Meter,
) {
    render(out, consumer, shared, meter, 2, |v| v);
}

#[test]
fn an_underrun_plays_the_whole_frames_held_then_silence() {
    let (mut producer, mut consumer) = rtrb::RingBuffer::<f32>::new(64);
    let shared = Shared::new();
    let mut meter = Meter::new(8000, 2);
    for s in [0.1, -0.1, 0.2, -0.2, 0.3, -0.3] {
        producer.push(s).unwrap();
    }
    let mut out = [9.0f32; 10];
    callback(&mut out, &mut consumer, &shared, &mut meter);
    assert_eq!(out, [0.1, -0.1, 0.2, -0.2, 0.3, -0.3, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(shared.frames_out.load(Ordering::Relaxed), 3);
}

#[test]
fn frames_pushed_during_a_callback_keep_their_channels() {
    // A race, so several rounds: on code that splits frames, one round fails
    // most of the time.
    for _ in 0..5 {
        pushed_during_callbacks();
    }
}

/// Whole frames arrive while callbacks run on an often empty ring, as at the
/// start of a track or after a seek. Frame k is (k, -k), so a frame split
/// across the channels shows as a left sample that is not positive.
fn pushed_during_callbacks() {
    let (mut producer, mut consumer) = rtrb::RingBuffer::<f32>::new(4096);
    let done = Arc::new(AtomicBool::new(false));
    let pushing = {
        let done = done.clone();
        std::thread::spawn(move || {
            for k in 1..=20_000 {
                let v = k as f32 / 32_768.0;
                // Committed whole, as the engine commits whole frames.
                let mut chunk = loop {
                    match producer.write_chunk(2) {
                        Ok(chunk) => break chunk,
                        Err(_) => std::thread::yield_now(),
                    }
                };
                let (a, _) = chunk.as_mut_slices();
                a.copy_from_slice(&[v, -v]);
                chunk.commit_all();
                if k % 7 == 0 {
                    std::thread::yield_now();
                }
            }
            done.store(true, Ordering::Relaxed);
        })
    };
    let shared = Shared::new();
    let mut meter = Meter::new(8000, 2);
    let mut out = [0.0f32; 16];
    let mut frames = 0;
    while !done.load(Ordering::Relaxed) || !consumer.is_empty() {
        callback(&mut out, &mut consumer, &shared, &mut meter);
        for f in out.as_chunks::<2>().0.iter().filter(|f| **f != [0.0, 0.0]) {
            assert!(f[0] > 0.0 && f[1] == -f[0], "a split frame: {f:?}");
            frames += 1;
        }
    }
    pushing.join().unwrap();
    assert_eq!(frames, 20_000);
}
