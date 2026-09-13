//! Engine behaviour when the output device fails, against a fake device.
//!
//! These need neither an audio device nor ffmpeg, so they always run.

mod common;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{fake_player as player, levels, silence, tone, Control};
use playr::audio::output::DeviceEvent;
use playr::audio::{Cmd, Player, State, Status};

/// Polls until `done` or five seconds pass, and returns the last status.
fn wait_for(player: &Player, done: impl Fn(&Status) -> bool) -> Status {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let s = player.status();
        if done(&s) || Instant::now() > deadline {
            return s;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A player on the fake device, playing 10 s of silence.
fn playing() -> (Player, Arc<Control>, tempfile::TempDir) {
    let (player, control) = player();
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("a.wav");
    silence(&track, 44100, 10.0);
    player.send(Cmd::Play(vec![track], 0));
    let s = wait_for(&player, |s| s.state == State::Playing);
    assert_eq!(s.state, State::Playing);
    (player, control, dir)
}

#[test]
fn the_fake_device_plays_in_real_time() {
    let (player, _control, _dir) = playing();
    std::thread::sleep(Duration::from_millis(500));
    let pos = player.position();
    assert!(
        pos > Duration::from_millis(300) && pos < Duration::from_millis(800),
        "position {pos:?} after 500 ms"
    );
}

#[test]
fn a_lost_device_stops_playback_and_says_why() {
    let (player, control, _dir) = playing();
    control.send(DeviceEvent::Lost("unplugged".into()));

    let s = wait_for(&player, |s| s.state == State::Stopped);
    assert_eq!(s.state, State::Stopped, "still playing to a lost device");
    let error = s.error.unwrap_or_default();
    assert!(error.contains("unplugged"), "error was {error:?}");
}

#[test]
fn a_rerouted_stream_keeps_playing_without_an_error() {
    let (player, control, _dir) = playing();
    let before = player.status().error_seq;
    control.send(DeviceEvent::Rerouted);

    std::thread::sleep(Duration::from_millis(300));
    let s = player.status();
    assert_eq!(s.state, State::Playing);
    assert_eq!(s.error_seq, before, "reported {:?}", s.error);
}

#[test]
fn other_device_errors_are_reported_without_stopping() {
    let (player, control, _dir) = playing();
    control.send(DeviceEvent::Error("buffer underrun".into()));

    let s = wait_for(&player, |s| s.error_seq > 0);
    assert!(s.error.unwrap_or_default().contains("buffer underrun"));
    assert_eq!(player.status().state, State::Playing);
}

#[test]
fn a_seek_discards_buffered_audio_without_reopening_the_device() {
    let (player, control) = player();
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("a.wav");
    levels(&track, 44100, &[(5.0, 0.5), (5.0, -0.5)]);
    player.send(Cmd::Play(vec![track], 0));
    wait_for(&player, |_| player.position() > Duration::from_millis(300));

    let mark = control.played.lock().unwrap().len();
    player.send(Cmd::Seek(Duration::from_secs(6)));
    std::thread::sleep(Duration::from_millis(300));

    let played = control.played.lock().unwrap()[mark..].to_vec();
    // The ring held a second or more of the first level when the seek arrived.
    let stale = played.iter().filter(|v| **v > 0.25).count() / 2;
    assert!(stale < 2205, "{stale} frames from before the seek played");
    assert!(
        played.iter().any(|v| *v < -0.25),
        "nothing from after the seek played"
    );
    assert_eq!(
        control.opened.load(Ordering::Relaxed),
        1,
        "the device was reopened"
    );
    let pos = player.position();
    assert!(
        pos > Duration::from_millis(6100) && pos < Duration::from_millis(6600),
        "position {pos:?}"
    );
}

#[test]
fn a_seek_while_paused_moves_the_position_and_stays_paused() {
    let (player, control, _dir) = playing();
    player.send(Cmd::TogglePause);
    player.send(Cmd::Seek(Duration::from_secs(3)));
    std::thread::sleep(Duration::from_millis(300));

    let s = player.status();
    assert_eq!(s.state, State::Paused);
    let pos = player.position();
    assert!(
        pos >= Duration::from_secs(3) && pos < Duration::from_millis(3050),
        "position {pos:?}"
    );
    assert_eq!(control.opened.load(Ordering::Relaxed), 1);

    player.send(Cmd::TogglePause);
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        player.position() > Duration::from_millis(3100),
        "did not resume"
    );
}

#[test]
fn a_seek_reopens_a_device_that_has_stopped_calling_back() {
    let (player, control, _dir) = playing();
    control.stall.store(true, Ordering::Relaxed);
    player.send(Cmd::Seek(Duration::from_secs(1)));

    let deadline = Instant::now() + Duration::from_secs(3);
    while control.opened.load(Ordering::Relaxed) < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(control.opened.load(Ordering::Relaxed), 2, "never reopened");
    control.stall.store(false, Ordering::Relaxed);
    let s = wait_for(&player, |_| player.position() > Duration::from_millis(1200));
    assert_eq!(s.state, State::Playing);
}

#[test]
fn a_seek_whose_stalled_device_cannot_be_reopened_stops_playback() {
    let (player, control, _dir) = playing();
    control.stall.store(true, Ordering::Relaxed);
    control.refuse_open.store(true, Ordering::Relaxed);
    player.send(Cmd::Seek(Duration::from_secs(1)));

    let s = wait_for(&player, |s| s.state == State::Stopped);
    assert_eq!(s.state, State::Stopped, "playing with no output");
    assert!(s.error.unwrap_or_default().contains("refused"));
}

#[test]
fn a_format_change_whose_output_will_not_open_stops_at_that_track() {
    let (player, control) = player();
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("0.wav"), dir.path().join("1.wav"));
    silence(&first, 44100, 0.3);
    silence(&second, 48000, 5.0);
    player.send(Cmd::Play(vec![first, second], 0));
    let s = wait_for(&player, |s| s.state == State::Playing);
    assert_eq!(s.state, State::Playing);
    // The second track needs a new output at 48 kHz once the first plays out.
    control.refuse_open.store(true, Ordering::Relaxed);

    let s = wait_for(&player, |s| s.state == State::Stopped);
    assert_eq!(s.state, State::Stopped);
    assert!(s.error.as_deref().unwrap_or_default().contains("refused"));
    assert_eq!(s.index, 1, "play would restart the previous track");
    assert_eq!((s.source, s.duration), (None, None), "stale state: {s:?}");
}

#[test]
fn cpal_errors_map_to_device_events() {
    use cpal::{Error, ErrorKind};
    let event = |kind| DeviceEvent::from(Error::with_message(kind, "detail"));
    assert_eq!(event(ErrorKind::DeviceChanged), DeviceEvent::Rerouted);
    for kind in [
        ErrorKind::DeviceNotAvailable,
        ErrorKind::HostUnavailable,
        ErrorKind::StreamInvalidated,
    ] {
        assert_eq!(event(kind), DeviceEvent::Lost("detail".into()), "{kind:?}");
    }
    assert_eq!(
        event(ErrorKind::DeviceBusy),
        DeviceEvent::Error("detail".into())
    );
}

#[test]
fn quick_seeks_add_up() {
    // Sent together, all three reach the engine before the device has
    // discarded for the first, so each must start from the one before.
    let (player, _control, _dir) = playing();
    for _ in 0..3 {
        player.send(Cmd::SeekBy(2));
    }
    std::thread::sleep(Duration::from_millis(300));
    let pos = player.position();
    assert!(
        pos > Duration::from_millis(6000) && pos < Duration::from_millis(7000),
        "position {pos:?} after three 2 s seeks"
    );
}

#[test]
fn the_meter_reads_the_recording_not_the_volume() {
    let (player, _control) = player();
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("tone.wav");
    tone(&track, 44100, 5.0, -23.0);
    // A quarter volume is 12 dB down; the meter must not see it.
    player.send(Cmd::SetVolume(0.25));
    player.send(Cmd::Play(vec![track], 0));
    wait_for(&player, |_| player.position() > Duration::from_millis(800));

    let lufs = player.loudness().expect("no loudness while playing");
    assert!((lufs + 23.0).abs() < 0.2, "read {lufs} LUFS");
    let peak = 20.0 * player.take_peak().log10();
    assert!((peak + 23.0).abs() < 0.2, "peak {peak} dBFS");
    assert_eq!(player.take_peak(), 0.0, "taking the peak did not reset it");
}

#[test]
fn silence_has_no_loudness() {
    let (player, _control, _dir) = playing();
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(player.loudness(), None);
    assert_eq!(player.take_peak(), 0.0);
}
