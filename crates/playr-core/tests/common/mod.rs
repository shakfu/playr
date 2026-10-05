//! Helpers shared by the integration tests.
//!
//! Each test file compiles its own copy, and none uses all of it.
#![allow(dead_code)]

use std::any::Any;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::SampleFormat;
use playr_core::audio::eq::Eq;
use playr_core::audio::meter::Meter;
use playr_core::audio::output::{render, Backend, DeviceEvent, OutputError, Plan, Shared};
use playr_core::audio::{Player, Spec};
use playr_core::event::Event;

/// Reports a test skipped for want of `what`, or fails it when `var` is set.
///
/// Skipping keeps `make test` usable without ffmpeg or an audio device. Setting
/// `PLAYR_REQUIRE_FFMPEG` or `PLAYR_REQUIRE_DEVICE` turns a skip into a failure,
/// so a CI run cannot pass by testing nothing.
pub fn skip(var: &str, what: &str) {
    if std::env::var_os(var).is_some() {
        panic!("{what}, and {var} is set");
    }
    eprintln!("skipping: {what}");
}

/// Whether to run a test that asks the machine for its audio devices. On
/// macOS each such test spends about 14 s listing them, so they run only
/// when `PLAYR_DEVICE_TESTS` is set; the calling test skips otherwise.
pub fn device_tests() -> bool {
    let on = std::env::var_os("PLAYR_DEVICE_TESTS").is_some();
    if !on {
        eprintln!("skipping: asks for the machine's audio devices; set PLAYR_DEVICE_TESTS to run");
    }
    on
}

/// Whether ffmpeg can be run; skips the calling test when it cannot.
pub fn have_ffmpeg() -> bool {
    let ok = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        skip("PLAYR_REQUIRE_FFMPEG", "ffmpeg not available");
    }
    ok
}

/// Writes `secs` of 16-bit stereo silence at `rate` as a WAV file.
pub fn silence(path: &Path, rate: u32, secs: f32) {
    levels(path, rate, &[(secs, 0.0)]);
}

/// Writes `secs` of a 16-bit stereo 1 kHz sine at `dbfs` peak, at `rate`.
pub fn tone(path: &Path, rate: u32, secs: f32, dbfs: f32) {
    let amplitude = 10f32.powf(dbfs / 20.0);
    let pcm: Vec<u8> = (0..(rate as f32 * secs) as usize)
        .flat_map(|i| {
            let v = amplitude * (std::f32::consts::TAU * 1000.0 * i as f32 / rate as f32).sin();
            let sample = ((v * i16::MAX as f32).round() as i16).to_le_bytes();
            [sample, sample].concat()
        })
        .collect();
    write_wav(path, rate, pcm);
}

/// Writes `secs` of 16-bit stereo clicks at `bpm`, at `rate`: a 10 ms burst
/// of a decaying 1 kHz sine on each beat, the first `t0` seconds in.
pub fn clicks(path: &Path, rate: u32, bpm: f32, t0: f32, secs: f32) {
    let period = rate as f64 * 60.0 / bpm as f64;
    let first = t0 as f64 * rate as f64;
    let burst = rate as usize / 100;
    let mut out = vec![0.0f32; (rate as f32 * secs) as usize];
    for beat in 0.. {
        let at = (first + beat as f64 * period).round() as usize;
        if at >= out.len() {
            break;
        }
        for (k, o) in out[at..].iter_mut().take(burst).enumerate() {
            let t = k as f32 / rate as f32;
            *o = 0.5 * (1.0 - k as f32 / burst as f32) * (std::f32::consts::TAU * 1000.0 * t).sin();
        }
    }
    let pcm: Vec<u8> = out
        .iter()
        .flat_map(|v| {
            let sample = ((v * i16::MAX as f32).round() as i16).to_le_bytes();
            [sample, sample].concat()
        })
        .collect();
    write_wav(path, rate, pcm);
}

/// Writes 16-bit stereo WAV at `rate`: each `(secs, level)` holds one constant level.
pub fn levels(path: &Path, rate: u32, parts: &[(f32, f32)]) {
    let pcm: Vec<u8> = parts
        .iter()
        .flat_map(|&(secs, level)| {
            let sample = ((level * i16::MAX as f32) as i16).to_le_bytes();
            std::iter::repeat_n([sample, sample].concat(), (rate as f32 * secs) as usize).flatten()
        })
        .collect();
    write_wav(path, rate, pcm);
}

/// Writes interleaved 16-bit stereo `pcm` at `rate` as a WAV file.
fn write_wav(path: &Path, rate: u32, pcm: Vec<u8>) {
    let data = pcm.len() as u32;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&2u16.to_le_bytes()); // channels
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 4).to_le_bytes()); // bytes per second
    wav.extend_from_slice(&4u16.to_le_bytes()); // bytes per frame
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    wav.extend_from_slice(&pcm);
    std::fs::write(path, wav).unwrap();
}

/// What a test can do to the fake device while the engine plays to it.
#[derive(Default)]
pub struct Control {
    /// When set, opening a stream fails.
    pub refuse_open: AtomicBool,
    /// When set, streams stop calling back, as a hung device does. Set it with
    /// [`Control::stall`], which waits until the device has stopped.
    pub stall: AtomicBool,
    /// Passes the device's thread has made while stalled.
    stalled_passes: AtomicUsize,
    /// When set, the device plays only what [`Control::advance`] allows, so
    /// the playhead stays where a seek put it. Set it with [`Control::hold`].
    held: AtomicBool,
    /// Passes the device's thread has made while held.
    held_passes: AtomicUsize,
    /// Frames a held device may still play.
    budget: AtomicU64,
    /// The rate of the stream opened last.
    rate: AtomicU32,
    /// Streams opened so far.
    pub opened: AtomicUsize,
    /// Every sample played, interleaved.
    pub played: Mutex<Vec<f32>>,
    /// Event channel of the most recently opened stream.
    events: Mutex<Option<Sender<DeviceEvent>>>,
    /// What the engine has reported, for [`Control::report`].
    reported: Mutex<Vec<String>>,
}

impl Control {
    /// Stalls the device, returning once its thread has passed through a turn
    /// stalled, so no callback already under way can act on what comes next.
    pub fn stall(&self) {
        let seen = self.stalled_passes.load(Ordering::Relaxed);
        self.stall.store(true, Ordering::Relaxed);
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.stalled_passes.load(Ordering::Relaxed) <= seen && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Holds the device's clock: it goes on taking seeks, pauses and flushes,
    /// but plays nothing until [`Control::advance`]. Returns once its thread
    /// has passed through a turn held, so no chunk under way plays after.
    pub fn hold(&self) {
        let seen = self.held_passes.load(Ordering::Relaxed);
        self.held.store(true, Ordering::Relaxed);
        // A stream opened later starts held.
        if self.opened.load(Ordering::Relaxed) == 0 {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.held_passes.load(Ordering::Relaxed) <= seen && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Plays exactly `by` of audio on a held device, and returns once it has.
    /// Panics if it cannot within five seconds, as when paused.
    pub fn advance(&self, by: Duration) {
        assert!(self.held.load(Ordering::Relaxed), "the device is not held");
        let rate = self.rate.load(Ordering::Relaxed);
        assert!(rate > 0, "no stream open");
        let frames = (by.as_secs_f64() * f64::from(rate)).round() as u64;
        self.budget.fetch_add(frames, Ordering::Relaxed);
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.budget.load(Ordering::Relaxed) > 0 {
            assert!(
                Instant::now() < deadline,
                "could not play {by:?}: {}",
                self.report()
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// What the device and the engine have done, for a failure message.
    ///
    /// A test that waits to hear something hears nothing whether the file
    /// would not open, the device would not open or the engine stopped, and
    /// the engine's own report of it goes nowhere unless a sink is attached.
    pub fn report(&self) -> String {
        let opened = self.opened.load(Ordering::Relaxed);
        let played = self.played.lock().map_or(0, |p| p.len());
        let reported = match self.reported.lock() {
            Ok(r) if r.is_empty() => "nothing".to_string(),
            Ok(r) => r.join("; "),
            Err(_) => "lost to a panic".to_string(),
        };
        format!("{opened} streams opened, {played} samples played, engine reported: {reported}")
    }

    pub fn send(&self, event: DeviceEvent) {
        let events = self.events.lock().unwrap();
        events
            .as_ref()
            .expect("no stream open")
            .send(event)
            .unwrap();
    }
}

/// An output device that accepts every source rate and plays in real time.
struct Fake(Arc<Control>);

/// Stops a fake stream's thread when dropped.
struct Running(Arc<AtomicBool>);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

impl Backend for Fake {
    fn negotiate(&self, src: Spec) -> Result<Plan, OutputError> {
        Ok(Plan {
            rate: src.rate,
            channels: 2,
            format: SampleFormat::F32,
        })
    }

    fn start(
        &self,
        plan: Plan,
        mut consumer: rtrb::Consumer<f32>,
        shared: Arc<Shared>,
        events: Sender<DeviceEvent>,
    ) -> Result<Box<dyn Any>, OutputError> {
        if self.0.refuse_open.load(Ordering::Relaxed) {
            return Err(OutputError::Build("refused by the fake device".into()));
        }
        *self.0.events.lock().unwrap() = Some(events);
        self.0.opened.fetch_add(1, Ordering::Relaxed);
        self.0.rate.store(plan.rate, Ordering::Relaxed);
        let running = Arc::new(AtomicBool::new(true));
        let alive = running.clone();
        let control = self.0.clone();
        std::thread::spawn(move || {
            // 10 ms of audio every 10 ms, as a device pulls it. The chunks are
            // due by the clock, not by the sleeps: a busy machine wakes the
            // thread late, and counting sleeps would play slower than real time.
            const CHUNK: Duration = Duration::from_millis(10);
            // Most chunks made up at one wake; a device that falls further
            // behind skips the rest, as a real one underruns.
            const CATCH_UP: u32 = 10;
            let mut buf = vec![0.0f32; (plan.rate / 100) as usize * plan.channels as usize];
            let mut meter = Meter::new(plan.rate, plan.channels);
            let mut eq = Eq::new(plan.rate, plan.channels);
            let mut due = Instant::now();
            while alive.load(Ordering::Relaxed) {
                let now = Instant::now();
                if control.stall.load(Ordering::Relaxed) {
                    due = now;
                    control.stalled_passes.fetch_add(1, Ordering::Relaxed);
                } else if control.held.load(Ordering::Relaxed) {
                    // A call every chunk, so a seek's flush still lands, of
                    // only as many frames as the test has let it play.
                    let channels = plan.channels as usize;
                    let budget = control.budget.load(Ordering::Relaxed) as usize;
                    let frames = budget.min(buf.len() / channels);
                    let before = shared.frames_out.load(Ordering::Relaxed);
                    let out = &mut buf[..frames * channels];
                    render(
                        out,
                        &mut consumer,
                        &shared,
                        &mut eq,
                        &mut meter,
                        plan.channels as u64,
                        |v| v,
                    );
                    // Saturating: the engine zeroes the count when it opens a stream.
                    let played = (shared.frames_out.load(Ordering::Relaxed)).saturating_sub(before);
                    control
                        .played
                        .lock()
                        .unwrap()
                        .extend_from_slice(&out[..played as usize * channels]);
                    control.budget.fetch_sub(played, Ordering::Relaxed);
                    control.held_passes.fetch_add(1, Ordering::Relaxed);
                    due = now + CHUNK;
                } else {
                    let mut chunks = 0;
                    // The stall is checked before each chunk: a device told to
                    // stall must not go on to honour a seek it was sent after.
                    while due <= now && chunks < CATCH_UP && !control.stall.load(Ordering::Relaxed)
                    {
                        render(
                            &mut buf,
                            &mut consumer,
                            &shared,
                            &mut eq,
                            &mut meter,
                            plan.channels as u64,
                            |v| v,
                        );
                        control.played.lock().unwrap().extend_from_slice(&buf);
                        due += CHUNK;
                        chunks += 1;
                    }
                    if due <= now {
                        due = now + CHUNK;
                    }
                }
                std::thread::sleep(
                    due.saturating_duration_since(Instant::now())
                        .max(Duration::from_millis(1)),
                );
            }
        });
        Ok(Box::new(Running(running)))
    }
}

/// A player on a fake device, and the handle that controls the device.
pub fn fake_player() -> (Player, Arc<Control>) {
    let control = Arc::new(Control::default());
    let player = Player::with_backend(Fake(control.clone())).unwrap();
    let recorder = control.clone();
    player.set_events(Arc::new(move |event| {
        let line = match event {
            Event::PlaybackError(e) => format!("playback error: {e}"),
            Event::StateChanged(state) => format!("{state:?}"),
            Event::TrackChanged { index, .. } => format!("track {index}"),
            other => format!("{other:?}"),
        };
        if let Ok(mut reported) = recorder.reported.lock() {
            reported.push(line);
        }
    }));
    (player, control)
}
