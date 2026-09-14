//! Helpers shared by the integration tests.
//!
//! Each test file compiles its own copy, and none uses all of it.
#![allow(dead_code)]

use std::any::Any;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::SampleFormat;
use playr_core::audio::meter::Meter;
use playr_core::audio::output::{render, Backend, DeviceEvent, OutputError, Plan, Shared};
use playr_core::audio::{Player, Spec};

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
    /// When set, streams stop calling back, as a hung device does.
    pub stall: AtomicBool,
    /// Streams opened so far.
    pub opened: AtomicUsize,
    /// Every sample played, interleaved.
    pub played: Mutex<Vec<f32>>,
    /// Event channel of the most recently opened stream.
    events: Mutex<Option<Sender<DeviceEvent>>>,
}

impl Control {
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
        let running = Arc::new(AtomicBool::new(true));
        let alive = running.clone();
        let control = self.0.clone();
        std::thread::spawn(move || {
            // 10 ms of audio every 10 ms, as a device pulls it.
            let mut buf = vec![0.0f32; (plan.rate / 100) as usize * plan.channels as usize];
            let mut meter = Meter::new(plan.rate, plan.channels);
            while alive.load(Ordering::Relaxed) {
                if !control.stall.load(Ordering::Relaxed) {
                    render(
                        &mut buf,
                        &mut consumer,
                        &shared,
                        &mut meter,
                        plan.channels as u64,
                        |v| v,
                    );
                    control.played.lock().unwrap().extend_from_slice(&buf);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        Ok(Box::new(Running(running)))
    }
}

/// A player on a fake device, and the handle that controls the device.
pub fn fake_player() -> (Player, Arc<Control>) {
    let control = Arc::new(Control::default());
    let player = Player::with_backend(Fake(control.clone())).unwrap();
    (player, control)
}
