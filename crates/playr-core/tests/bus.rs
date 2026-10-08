//! The bus: sources summed with the player in the output callback, without
//! allocating, and kept across the streams a player opens.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use playr_core::audio::bus::{Bus, BusControl, Cue, Source, MAX_SOURCES};
use playr_core::audio::eq::Eq;
use playr_core::audio::meter::Meter;
use playr_core::audio::output::{render, Shared};
use playr_core::audio::Cmd;

struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

fn count() {
    if ARMED.with(Cell::get) {
        COUNT.with(|c| c.set(c.get() + 1));
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        count();
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static A: Counting = Counting;

/// Allocations and frees on this thread while `f` runs.
fn counted(f: impl FnOnce()) -> usize {
    COUNT.with(|c| c.set(0));
    ARMED.with(|a| a.set(true));
    f();
    ARMED.with(|a| a.set(false));
    COUNT.with(Cell::get)
}

/// The same left and right sample, every frame.
struct Steady(f32, f32);

impl Source for Steady {
    fn process(&mut self, main: &mut [f32], _cue: &mut [f32]) -> Cue {
        for f in main.as_chunks_mut::<2>().0 {
            f.copy_from_slice(&[self.0, self.1]);
        }
        Cue::None
    }
}

/// A steady main of 0.25 and cue of 0.5, routed as `route` says.
struct Cued(Cue);

impl Source for Cued {
    fn process(&mut self, main: &mut [f32], cue: &mut [f32]) -> Cue {
        main.fill(0.25);
        cue.fill(0.5);
        self.0
    }
}

/// The frame's number, in both channels, counting on from the last call.
struct Counter(f32);

impl Source for Counter {
    fn process(&mut self, main: &mut [f32], _cue: &mut [f32]) -> Cue {
        for f in main.as_chunks_mut::<2>().0 {
            f.fill(self.0);
            self.0 += 1.0;
        }
        Cue::None
    }
}

/// Raises its flag when dropped.
struct Watched(Arc<AtomicBool>, f32);

impl Source for Watched {
    fn process(&mut self, main: &mut [f32], _cue: &mut [f32]) -> Cue {
        main.fill(self.1);
        Cue::None
    }
}

impl Drop for Watched {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Silent until started, then `level` in both channels.
struct Gated(bool, f32);

impl Source for Gated {
    fn process(&mut self, main: &mut [f32], _cue: &mut [f32]) -> Cue {
        if self.0 {
            main.fill(self.1);
        }
        Cue::None
    }

    fn start(&mut self) {
        self.0 = true;
    }
}

/// One callback of `out.len()` samples at `channels`, with a flat EQ.
fn callback(
    out: &mut [f32],
    consumer: &mut rtrb::Consumer<f32>,
    bus: &mut Bus,
    shared: &Shared,
    channels: u16,
) {
    let (mut eq, mut meter) = (Eq::new(8000, channels), Meter::new(8000, channels));
    render(
        out,
        consumer,
        bus,
        shared,
        &mut eq,
        &mut meter,
        u64::from(channels),
        |v| v,
    );
}

fn bus() -> (Bus, BusControl) {
    Bus::new(None)
}

#[test]
fn sources_add_to_the_player_sample_for_sample() {
    let (mut producer, mut consumer) = rtrb::RingBuffer::<f32>::new(64);
    for s in [0.1, -0.1, 0.2, -0.2] {
        producer.push(s).unwrap();
    }
    let (mut bus, mut control) = bus();
    control.attach(1, Box::new(Steady(0.25, 0.5))).ok().unwrap();
    control
        .attach(2, Box::new(Steady(0.125, 0.0)))
        .ok()
        .unwrap();
    let shared = Shared::new();
    let mut out = [9.0f32; 6];
    callback(&mut out, &mut consumer, &mut bus, &shared, 2);
    // The third frame is past what the player had: the sources alone.
    assert_eq!(out, [0.475, 0.4, 0.575, 0.3, 0.375, 0.5]);
    assert_eq!(shared.frames_out.load(Ordering::Relaxed), 2);
}

#[test]
fn a_paused_player_leaves_the_sources_playing() {
    let (mut producer, mut consumer) = rtrb::RingBuffer::<f32>::new(64);
    for s in [0.1, -0.1] {
        producer.push(s).unwrap();
    }
    let (mut bus, mut control) = bus();
    control.attach(1, Box::new(Steady(0.25, 0.5))).ok().unwrap();
    let shared = Shared::new();
    shared.paused.store(true, Ordering::Relaxed);
    let mut out = [9.0f32; 4];
    callback(&mut out, &mut consumer, &mut bus, &shared, 2);
    assert_eq!(out, [0.25, 0.5, 0.25, 0.5]);
    assert_eq!(consumer.slots(), 2, "the paused player's ring was read");
}

/// The player plays to the handover's frame and pauses there; the source
/// starts on it. What the player had after stays in its ring.
#[test]
fn a_handover_moves_the_output_on_one_frame() {
    use playr_core::audio::bus::Handover;
    let (mut producer, mut consumer) = rtrb::RingBuffer::<f32>::new(64);
    for _ in 0..16 {
        producer.push(0.5).unwrap();
    }
    let (mut bus, mut control) = bus();
    control
        .attach(7, Box::new(Gated(false, 0.25)))
        .ok()
        .unwrap();
    let shared = Shared::new();
    shared.frames_out.store(100, Ordering::Relaxed);
    shared.handover_id.store(7, Ordering::Relaxed);
    shared.handover_at.store(103, Ordering::Relaxed);
    let mut out = [9.0f32; 12];
    callback(&mut out, &mut consumer, &mut bus, &shared, 2);
    assert_eq!(
        out,
        [0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.25, 0.25, 0.25, 0.25, 0.25, 0.25]
    );
    assert_eq!(shared.frames_out.load(Ordering::Relaxed), 103);
    assert_eq!(consumer.slots(), 10);
    assert!(shared.paused.load(Ordering::Relaxed));
    assert!(shared.handed.load(Ordering::Relaxed));
    assert_eq!(
        shared.handover.load(Ordering::Relaxed),
        Handover::Done as u8
    );
}

/// A handover behind the device, or for a source not attached, is missed
/// and changes nothing; a flush cancels one armed.
#[test]
fn a_handover_missed_or_flushed_leaves_the_player_playing() {
    use playr_core::audio::bus::Handover;
    let (mut producer, mut consumer) = rtrb::RingBuffer::<f32>::new(64);
    for _ in 0..8 {
        producer.push(0.5).unwrap();
    }
    let (mut bus, mut control) = bus();
    control
        .attach(7, Box::new(Gated(false, 0.25)))
        .ok()
        .unwrap();
    let shared = Shared::new();
    let state = |s: &Shared| s.handover.load(Ordering::Relaxed);
    shared.frames_out.store(100, Ordering::Relaxed);
    shared.handover_id.store(7, Ordering::Relaxed);
    shared.handover_at.store(99, Ordering::Relaxed);
    let mut out = [9.0f32; 4];
    callback(&mut out, &mut consumer, &mut bus, &shared, 2);
    assert_eq!(out, [0.5; 4]);
    assert_eq!(state(&shared), Handover::Missed as u8);

    shared.handover_id.store(8, Ordering::Relaxed);
    shared.handover_at.store(103, Ordering::Relaxed);
    callback(&mut out, &mut consumer, &mut bus, &shared, 2);
    assert_eq!(out, [0.5; 4]);
    assert_eq!(state(&shared), Handover::Missed as u8);

    shared.handover_id.store(7, Ordering::Relaxed);
    shared.handover_at.store(110, Ordering::Relaxed);
    shared.flush_requested.store(1, Ordering::Relaxed);
    callback(&mut out, &mut consumer, &mut bus, &shared, 2);
    assert_eq!(out, [0.0; 4], "the flush emptied the ring");
    assert_eq!(state(&shared), Handover::Cancelled as u8);
    assert!(!shared.paused.load(Ordering::Relaxed));
}

#[test]
fn a_mono_device_hears_the_mean_and_channels_past_two_are_silent() {
    let (_producer, mut consumer) = rtrb::RingBuffer::<f32>::new(64);
    let shared = Shared::new();
    let (mut bus, mut control) = bus();
    control.attach(1, Box::new(Steady(0.25, 0.5))).ok().unwrap();
    let mut mono = [9.0f32; 3];
    callback(&mut mono, &mut consumer, &mut bus, &shared, 1);
    assert_eq!(mono, [0.375; 3]);
    let mut four = [9.0f32; 8];
    callback(&mut four, &mut consumer, &mut bus, &shared, 4);
    assert_eq!(four, [0.25, 0.5, 0.0, 0.0, 0.25, 0.5, 0.0, 0.0]);
}

/// The cue's routing takes in the whole master: the player is folded into
/// the master's side of a split, as the decks' main is.
#[test]
fn a_cue_routes_the_whole_master() {
    let shared = Shared::new();
    let routed = |route: Cue, channels: u16| {
        let (mut producer, mut consumer) = rtrb::RingBuffer::<f32>::new(64);
        // The player: 0.1 on the left, 0.3 on the right, on every channel.
        for _ in 0..2 {
            for c in 0..channels {
                producer.push(if c % 2 == 0 { 0.1 } else { 0.3 }).unwrap();
            }
        }
        let (mut bus, mut control) = bus();
        control.attach(1, Box::new(Cued(route))).ok().unwrap();
        let mut out = vec![9.0f32; 2 * channels as usize];
        callback(&mut out, &mut consumer, &mut bus, &shared, channels);
        out[..channels as usize].to_vec()
    };
    // Master: 0.35 left, 0.55 right; in mono 0.45. Cue: 0.5.
    assert_eq!(routed(Cue::None, 2), [0.35, 0.55]);
    assert_eq!(routed(Cue::Split { swap: false }, 2), [0.45, 0.5]);
    assert_eq!(routed(Cue::Split { swap: true }, 2), [0.5, 0.45]);
    assert_eq!(
        routed(Cue::Channels { swap: false }, 4),
        [0.35, 0.55, 0.5, 0.5]
    );
    assert_eq!(
        routed(Cue::Channels { swap: true }, 2),
        [0.5, 0.45],
        "too few channels: split"
    );
}

#[test]
fn a_long_callback_renders_its_sources_in_order() {
    let (_producer, mut consumer) = rtrb::RingBuffer::<f32>::new(64);
    let shared = Shared::new();
    let (mut bus, mut control) = bus();
    control.attach(1, Box::new(Counter(0.0))).ok().unwrap();
    // Longer than a chunk, and not a whole number of them.
    let mut out = vec![0.0f32; 2500 * 2];
    callback(&mut out, &mut consumer, &mut bus, &shared, 2);
    callback(&mut out[..100], &mut consumer, &mut bus, &shared, 2);
    let frames: Vec<f32> = out[..100].chunks(2).map(|f| f[0]).collect();
    let want: Vec<f32> = (2500..2550).map(|n| n as f32).collect();
    assert_eq!(frames, want);
}

#[test]
fn detached_sources_and_those_past_the_limit_come_back() {
    let (_producer, mut consumer) = rtrb::RingBuffer::<f32>::new(64);
    let shared = Shared::new();
    let (mut bus, mut control) = bus();
    for id in 0..=MAX_SOURCES as u64 {
        control
            .attach(id, Box::new(Steady(0.0625, 0.0)))
            .ok()
            .unwrap();
    }
    let mut out = [0.0f32; 2];
    callback(&mut out, &mut consumer, &mut bus, &shared, 2);
    assert_eq!(out[0], 0.0625 * MAX_SOURCES as f32);
    assert!(control.take_gone().is_some(), "the one past the limit");
    assert!(control.take_gone().is_none());

    assert!(control.detach(3));
    assert!(control.detach(99), "an id never attached is no error");
    callback(&mut out, &mut consumer, &mut bus, &shared, 2);
    assert_eq!(out[0], 0.0625 * (MAX_SOURCES - 1) as f32);
    assert!(control.take_gone().is_some());
    assert!(control.take_gone().is_none());
}

#[test]
fn a_dropped_bus_sends_its_sources_home_with_those_still_coming() {
    let (_producer, mut consumer) = rtrb::RingBuffer::<f32>::new(64);
    let shared = Shared::new();
    let (home, arrived) = mpsc::channel();
    let (mut bus, mut control) = Bus::new(Some(home));
    control.attach(1, Box::new(Steady(0.0, 0.0))).ok().unwrap();
    let mut out = [0.0f32; 2];
    callback(&mut out, &mut consumer, &mut bus, &shared, 2);
    control.attach(2, Box::new(Steady(0.0, 0.0))).ok().unwrap();
    drop(bus);
    let mut ids: Vec<u64> = arrived.recv().unwrap().into_iter().map(|a| a.0).collect();
    ids.sort();
    assert_eq!(ids, [1, 2]);
}

#[test]
fn the_callback_with_sources_neither_allocates_nor_frees() {
    assert!(counted(|| drop(vec![0u8; 8])) > 0, "the counter counts");
    let (mut producer, mut consumer) = rtrb::RingBuffer::<f32>::new(1 << 16);
    let shared = Shared::new();
    let (mut eq, mut meter) = (Eq::new(48_000, 2), Meter::new(48_000, 2));
    let (mut bus, mut control) = bus();
    for id in 0..=MAX_SOURCES as u64 {
        control.attach(id, Box::new(Counter(0.0))).ok().unwrap();
    }
    control.detach(2);
    let mut out = vec![0.0f32; 3000 * 2];
    for _ in 0..out.len() {
        producer.push(0.1).unwrap();
    }
    let n = counted(|| {
        for paused in [false, true] {
            shared.paused.store(paused, Ordering::Relaxed);
            render(
                &mut out,
                &mut consumer,
                &mut bus,
                &shared,
                &mut eq,
                &mut meter,
                2,
                |v| v,
            );
        }
    });
    assert_eq!(n, 0);
    // Dropped here, off the callback.
    while control.take_gone().is_some() {}
}

/// Waits until the fake device's last `n` samples all equal `v`.
fn until_last(control: &common::Control, n: usize, v: f32, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let played = control.played.lock().unwrap();
        if played.len() >= n && played[played.len() - n..].iter().all(|s| *s == v) {
            return;
        }
        drop(played);
        assert!(Instant::now() < deadline, "{what}: {}", control.report());
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Waits until `done` holds, or fails with `what` and the device's report.
fn until(control: &common::Control, what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "{what}: {}", control.report());
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A watched source of `level`, and the flag it raises when dropped.
fn watched(level: f32) -> (Box<Watched>, Arc<AtomicBool>) {
    let flag = Arc::new(AtomicBool::new(false));
    (Box::new(Watched(flag.clone(), level)), flag)
}

/// While a source plays, a track at another rate is resampled to the
/// stream's: the stream does not reopen, and the source plays on. A reopen
/// the device forces keeps the source too, and a detach drops it.
#[test]
fn a_source_pins_the_rate_and_outlives_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.wav"), dir.path().join("b.wav"));
    common::silence(&a, 8000, 30.0);
    common::silence(&b, 16000, 30.0);
    let (player, control) = common::fake_player();
    player.send(Cmd::Play(vec![a], 0));
    let (source, dropped) = watched(0.25);
    let sources = player.sources();
    let rate = sources.rate_for(16000);
    assert_eq!(rate, 8000, "the open stream's rate, not the one asked for");
    let attached = sources.attach(rate, source);
    until_last(&control, 160, 0.25, "the source beside the first track");

    let opened = control.opened.load(Ordering::Relaxed);
    player.send(Cmd::Play(vec![b], 0));
    until(&control, "the second track", || {
        player
            .status()
            .current()
            .is_some_and(|p| p.ends_with("b.wav"))
    });
    control.played.lock().unwrap().clear();
    until_last(&control, 160, 0.25, "the source with the second track");
    assert_eq!(
        control.opened.load(Ordering::Relaxed),
        opened,
        "the stream reopened"
    );
    assert_eq!(control.rate(), 8000);

    // A device that stops calling back is reopened after a seek times out.
    control.stall();
    player.send(Cmd::Seek(Duration::from_secs(5)));
    until(&control, "the reopen", || {
        control.opened.load(Ordering::Relaxed) > opened
    });
    control.stall.store(false, Ordering::Relaxed);
    control.played.lock().unwrap().clear();
    until_last(&control, 160, 0.25, "the source after the reopen");

    drop(attached);
    until_last(&control, 160, 0.0, "silence once detached");
    until(&control, "the detached source dropped", || {
        dropped.load(Ordering::Relaxed)
    });
}

/// With the player stopped, a source opens a stream at its rate, plays on
/// when a track stops, and its detach closes the stream.
#[test]
fn a_source_plays_without_a_track_and_through_a_stop() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.wav");
    common::silence(&a, 8000, 30.0);
    let (player, control) = common::fake_player();
    let sources = player.sources();
    assert_eq!(sources.channels(), 0, "no stream before a source");
    let (source, _) = watched(0.5);
    let attached = sources.attach(sources.rate_for(16000), source);
    until_last(&control, 320, 0.5, "the source alone");
    assert_eq!(control.rate(), 16000);
    assert_eq!(sources.channels(), 2);

    player.send(Cmd::Play(vec![a], 0));
    until(&control, "the track", || {
        player.status().state == playr_core::audio::State::Playing
    });
    player.send(Cmd::Stop);
    until(&control, "the stop", || {
        player.status().state == playr_core::audio::State::Stopped
    });
    control.played.lock().unwrap().clear();
    until_last(&control, 320, 0.5, "the source after the stop");

    sources.detach(attached.id());
    until(&control, "the stream closed", || sources.channels() == 0);
    let heard = control.played.lock().unwrap().len();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        control.played.lock().unwrap().len(),
        heard,
        "a closed stream played"
    );
}

/// A handover set by track position lands on its frame: the player pauses
/// there, reporting that position, and the source follows with no frame of
/// both or of neither.
#[test]
fn a_player_hands_over_to_a_source_at_a_position() {
    use playr_core::audio::bus::Handover;
    use playr_core::audio::State;
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.wav");
    common::levels(&a, 8000, &[(30.0, 0.5)]);
    let (player, control) = common::fake_player();
    player.send(Cmd::Play(vec![a], 0));
    let sources = player.sources();
    let attached = sources.attach(8000, Box::new(Gated(false, 0.25)));
    until(&control, "the track", || {
        player.status().state == State::Playing && player.position() > Duration::ZERO
    });
    let at = player.position() + Duration::from_millis(300);
    // The source may not be on the bus yet; the callback misses it then.
    until(&control, "the handover armed", || {
        player.handover() != Handover::Armed && player.hand_over(attached.id(), at)
    });
    until(&control, "the handover", || {
        player.handover() != Handover::Armed
    });
    assert_eq!(player.handover(), Handover::Done);
    until(&control, "the pause", || {
        player.status().state == State::Paused
    });
    assert_eq!(player.position(), at);
    let played = control.played.lock().unwrap().clone();
    let first = played.iter().position(|&s| s == 0.25).unwrap();
    assert!(played[first - 1] > 0.49, "{}", played[first - 1]);
    assert!(played[first..].iter().all(|&s| s == 0.25));
}

/// A lost device takes every source with it, and says so.
#[test]
fn a_lost_device_drops_the_sources() {
    let (player, control) = common::fake_player();
    let sources = player.sources();
    let (source, dropped) = watched(0.5);
    // Held, so only the loss can drop the source.
    let attached = sources.attach(8000, source);
    until_last(&control, 160, 0.5, "the source");
    assert!(!attached.lost());
    control.send(playr_core::audio::output::DeviceEvent::Lost(
        "unplugged".into(),
    ));
    until(&control, "the loss counted", || attached.lost());
    until(&control, "the source dropped", || {
        dropped.load(Ordering::Relaxed)
    });
}

/// Reads a recording's samples.
fn samples(path: &std::path::Path) -> Vec<f32> {
    hound::WavReader::open(path)
        .unwrap()
        .into_samples::<f32>()
        .map(Result::unwrap)
        .collect()
}

/// Waits for a recording to finish.
fn finished(
    control: &common::Control,
    rec: &playr_core::audio::record::MasterRecording,
) -> playr_core::audio::record::Recorded {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(done) = rec.finished() {
            return done.unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "never finished: {}",
            control.report()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The recording is the master as the device was sent it, before the cue's
/// routing: the cue is never in it, even where the device hears a split.
#[test]
fn the_master_is_recorded_without_the_cue() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("master.wav");
    let (player, control) = common::fake_player();
    let sources = player.sources();
    let _attached = sources.attach(8000, Box::new(Cued(Cue::Split { swap: false })));
    // The split: the master in mono on the left, the cue on the right.
    until(&control, "the split", || {
        let played = control.played.lock().unwrap();
        played.len() >= 2 && played[played.len() - 2..] == [0.25, 0.5]
    });
    let rec = sources.record(&path).unwrap();
    assert!(
        sources.record(&dir.path().join("two.wav")).is_err(),
        "one at a time"
    );
    std::thread::sleep(Duration::from_millis(200));
    rec.stop();
    let done = finished(&control, &rec);
    assert_eq!(done.rate, 8000);
    assert_eq!(done.dropped, 0);
    assert!(done.frames >= 800, "{} frames", done.frames);
    let wav = samples(&path);
    assert_eq!(wav.len() as u64, done.frames * 2);
    assert!(wav.iter().all(|s| *s == 0.25), "the cue was recorded");
    assert!((sources.take_master_peak() - 0.25).abs() < 1e-6);
}

/// A player alone is recorded too, and a closing stream ends the recording
/// with its file complete.
#[test]
fn a_player_alone_is_recorded_until_its_stream_closes() {
    let dir = tempfile::tempdir().unwrap();
    let (track, path) = (dir.path().join("t.wav"), dir.path().join("master.wav"));
    common::tone(&track, 8000, 30.0, -6.0);
    let (player, control) = common::fake_player();
    player.send(Cmd::Play(vec![track], 0));
    until(&control, "playing", || {
        player.status().state == playr_core::audio::State::Playing
    });
    let sources = player.sources();
    let rec = sources.record(&path).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    control.send(playr_core::audio::output::DeviceEvent::Lost(
        "unplugged".into(),
    ));
    let done = finished(&control, &rec);
    let wav = samples(&path);
    assert_eq!(wav.len() as u64, done.frames * 2);
    let peak = wav.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(
        (peak - 0.5).abs() < 0.05,
        "the tone at -6 dBFS peaks {peak}"
    );
}

#[test]
fn the_callback_recording_neither_allocates_nor_frees() {
    let (_producer, mut consumer) = rtrb::RingBuffer::<f32>::new(64);
    let shared = Shared::new();
    let (mut eq, mut meter) = (Eq::new(48_000, 2), Meter::new(48_000, 2));
    let (mut bus, mut control) = bus();
    control.attach(1, Box::new(Steady(0.25, 0.5))).ok().unwrap();
    let (ring, mut tap) = rtrb::RingBuffer::<f32>::new(1 << 16);
    assert!(control.record(ring));
    let mut out = vec![0.0f32; 3000 * 2];
    let n = counted(|| {
        render(
            &mut out,
            &mut consumer,
            &mut bus,
            &shared,
            &mut eq,
            &mut meter,
            2,
            |v| v,
        );
    });
    assert_eq!(n, 0);
    assert_eq!(tap.slots(), 3000 * 2);
    assert_eq!(tap.pop(), Ok(0.25));
    assert_eq!(tap.pop(), Ok(0.5));
    assert!(control.stop_recording());
    render(
        &mut out,
        &mut consumer,
        &mut bus,
        &shared,
        &mut eq,
        &mut meter,
        2,
        |v| v,
    );
    assert!(tap.is_abandoned(), "the bus let go of the ring");
}
