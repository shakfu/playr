//! Engine tests, against the fake output device in `common`, so they need no
//! audio device. One smoke test plays to the real default device.

mod common;

use common::{fake_player, levels, silence, skip};
use playr_core::audio::{Cmd, Player, State, Status};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn player() -> Player {
    fake_player().0
}

/// Consecutive unplayable files. The recursive skip overflowed at 2,000 in a
/// debug build.
const BAD_RUN: usize = 5_000;

/// Writes `n` one-byte `.mp3` files, which fail to probe.
fn bad_files(dir: &Path, n: usize) -> Vec<PathBuf> {
    (0..n)
        .map(|i| {
            let p = dir.join(format!("{i:05}.mp3"));
            std::fs::write(&p, b"x").unwrap();
            p
        })
        .collect()
}

fn wait_for(player: &Player, done: impl Fn(&Status) -> bool) -> Status {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let s = player.status();
        if done(&s) || Instant::now() > deadline {
            return s;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_long_run_of_unplayable_files_is_skipped_at_start() {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    player.send(Cmd::Play(bad_files(dir.path(), BAD_RUN), 0));

    let s = wait_for(&player, |s| s.error_seq == BAD_RUN as u64);
    assert_eq!(
        s.error_seq, BAD_RUN as u64,
        "not every bad file was reported"
    );
    assert_eq!(s.state, State::Stopped);
}

#[test]
fn a_long_run_of_unplayable_files_is_skipped_after_a_track() {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.wav");
    silence(&good, 44100, 0.2);

    // The good track decodes to its end within the first pump, so the bad
    // run is reached through gapless staging rather than through `start`.
    let mut queue = vec![good];
    queue.extend(bad_files(dir.path(), BAD_RUN));
    player.send(Cmd::Play(queue, 0));

    let s = wait_for(&player, |s| s.error_seq == BAD_RUN as u64);
    assert_eq!(
        s.error_seq, BAD_RUN as u64,
        "not every bad file was reported"
    );
}

/// Plays `rates` in order at +3 semitones and returns the status once the
/// track at `index` is audible.
fn varispeed_status(rates: &[u32], index: usize) -> Status {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    // Every track but the last is short, so the next one is reached quickly.
    let queue: Vec<PathBuf> = rates
        .iter()
        .enumerate()
        .map(|(i, &rate)| {
            let p = dir.path().join(format!("{i}.wav"));
            let secs = if i + 1 == rates.len() { 3.0 } else { 0.2 };
            silence(&p, rate, secs);
            p
        })
        .collect();

    player.send(Cmd::SpeedBy(3));
    player.send(Cmd::Play(queue, 0));
    let s = wait_for(&player, |s| {
        s.state == State::Playing && s.index == index && s.source.is_some()
    });
    assert_eq!(s.index, index, "track {index} never started");
    assert_eq!(s.semitones, 3);
    s
}

#[test]
fn varispeed_applies_to_a_track_started_directly() {
    let s = varispeed_status(&[44100], 0);
    assert!(s.resampling, "track plays at normal speed: {s:?}");
}

#[test]
fn varispeed_survives_a_gapless_track_change() {
    let s = varispeed_status(&[44100, 44100], 1);
    assert!(s.resampling, "next track plays at normal speed: {s:?}");
}

#[test]
fn varispeed_survives_a_track_change_that_reopens_the_output() {
    let s = varispeed_status(&[44100, 48000], 1);
    assert!(s.resampling, "next track plays at normal speed: {s:?}");
}

#[test]
fn publishing_a_status_does_not_copy_the_queue() {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    player.send(Cmd::Play(bad_files(dir.path(), 3), 0));
    wait_for(&player, |s| s.error_seq == 3);

    // The engine republishes every few milliseconds; each status must share
    // the engine's queue rather than own a copy of every path.
    let (a, b) = (player.status(), player.status());
    assert_eq!(a.queue.len(), 3);
    assert!(std::sync::Arc::ptr_eq(&a.queue, &b.queue));
}

/// A player that has started `secs` of silence, and a second file to enqueue.
fn playing(secs: f32) -> (Player, PathBuf, tempfile::TempDir) {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("1.wav"), dir.path().join("2.wav"));
    silence(&first, 44100, secs);
    silence(&second, 44100, 1.0);
    player.send(Cmd::Play(vec![first], 0));
    let s = wait_for(&player, |s| s.state == State::Playing);
    assert_eq!(s.state, State::Playing);
    (player, second, dir)
}

#[test]
fn enqueueing_after_the_queue_ends_starts_playback() {
    let (player, second, _dir) = playing(0.1);
    let s = wait_for(&player, |s| s.state == State::Stopped);
    assert_eq!(s.state, State::Stopped, "first track never ended");

    player.send(Cmd::Enqueue(vec![second]));
    let s = wait_for(&player, |s| s.state == State::Playing);
    assert_eq!((s.state, s.index), (State::Playing, 1));
}

#[test]
fn enqueueing_while_the_last_track_plays_out_continues_into_it() {
    // The first track decodes to its end at once, then plays out of the ring
    // for about a second. The enqueue lands in that window.
    let (player, second, _dir) = playing(1.0);
    std::thread::sleep(Duration::from_millis(200));
    player.send(Cmd::Enqueue(vec![second]));

    let s = wait_for(&player, |s| s.index == 1 || s.state == State::Stopped);
    assert_eq!((s.state, s.index), (State::Playing, 1), "playback stopped");
}

/// Plays `secs` of silence per track, and returns once the first track's
/// position reaches 1.4 s. The decoder leads by just over a second, so a 2 s
/// first track has been read to its end by then.
fn near_the_end_of_a_track(secs: &[f32]) -> (Player, tempfile::TempDir) {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    let queue: Vec<PathBuf> = secs
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let p = dir.path().join(format!("{i}.wav"));
            silence(&p, 44100, s);
            p
        })
        .collect();
    player.send(Cmd::Play(queue, 0));
    let deadline = Instant::now() + Duration::from_secs(10);
    while player.position() < Duration::from_millis(1400) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(player.status().index, 0, "first track ended too soon");
    (player, dir)
}

#[test]
fn a_seek_near_the_end_of_a_track_stays_in_that_track() {
    // Each track holds its own level, so what played can be counted from the
    // device's output rather than timed: the fake plays slower under load.
    let (player, control) = fake_player();
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("0.wav"), dir.path().join("1.wav"));
    levels(&first, 44100, &[(2.0, 0.25)]);
    levels(&second, 44100, &[(6.0, -0.25)]);
    player.send(Cmd::Play(vec![first, second], 0));
    // The decoder leads by just over a second, so by 1.4 s the first track
    // has been read to its end and the second is staged.
    let deadline = Instant::now() + Duration::from_secs(10);
    while player.position() < Duration::from_millis(1400) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(player.status().index, 0, "first track ended too soon");

    player.send(Cmd::Seek(Duration::from_millis(500)));
    std::thread::sleep(Duration::from_millis(100));
    let s = player.status();
    assert_eq!(s.index, 0);
    assert!(
        s.duration.is_some_and(|d| d < Duration::from_secs(3)),
        "the first track shows the second track's duration: {:?}",
        s.duration
    );
    assert!(player.position() < Duration::from_secs(1));

    let s = wait_for(&player, |s| s.state == State::Stopped);
    assert_eq!(s.state, State::Stopped);
    // Seeking the second track instead played 5.5 s of it, then all 6 s again.
    let played = control.played.lock().unwrap();
    let second_secs = played.iter().filter(|v| **v < -0.2).count() as f64 / 2.0 / 44100.0;
    assert!(
        (second_secs - 6.0).abs() < 0.1,
        "the second track played for {second_secs:.2} s"
    );
}

#[test]
fn a_seek_in_the_last_track_after_it_is_decoded_is_not_ignored() {
    let (player, _dir) = near_the_end_of_a_track(&[2.0]);
    player.send(Cmd::Seek(Duration::from_millis(500)));
    // Ignored, the track would have ended 0.6 s from now.
    std::thread::sleep(Duration::from_secs(1));

    let s = player.status();
    assert_eq!(s.state, State::Playing, "the seek was ignored");
    let pos = player.position();
    assert!(pos < Duration::from_millis(1800), "position {pos:?}");
}

/// Repeats of one unplayable file: several seconds of skipping in a debug build.
const LONG_RUN: usize = 300_000;

fn long_run_of_unplayable_files(dir: &Path) -> Vec<PathBuf> {
    vec![bad_files(dir, 1).remove(0); LONG_RUN]
}

#[test]
fn stop_interrupts_a_long_run_of_unplayable_files() {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    player.send(Cmd::Play(long_run_of_unplayable_files(dir.path()), 0));
    player.send(Cmd::Stop);

    std::thread::sleep(Duration::from_millis(200));
    let before = player.status().error_seq;
    std::thread::sleep(Duration::from_millis(200));
    let after = player.status().error_seq;
    assert_eq!(before, after, "still skipping after stop");
    assert!(after < LONG_RUN as u64);
}

#[test]
fn quitting_does_not_wait_for_a_long_run_of_unplayable_files() {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    player.send(Cmd::Play(long_run_of_unplayable_files(dir.path()), 0));
    std::thread::sleep(Duration::from_millis(50));

    let quit = Instant::now();
    drop(player);
    let took = quit.elapsed();
    assert!(took < Duration::from_secs(1), "quit took {took:?}");
}

/// Plays `queue` from `index` and waits until that track is playing.
fn playing_at(player: &Player, queue: Vec<PathBuf>, index: usize) {
    player.send(Cmd::Play(queue, index));
    let s = wait_for(player, |s| s.state == State::Playing && s.index == index);
    assert_eq!((s.state, s.index), (State::Playing, index));
}

/// The status once an error past `errors` is reported and the command that
/// raised it has finished.
///
/// `fail` records an error at once, but state and index are published only
/// after the command, so reading them as soon as the error shows is a race.
fn settled_after_an_error(player: &Player, errors: u64) -> Status {
    wait_for(player, |s| s.error_seq > errors);
    std::thread::sleep(Duration::from_millis(300));
    player.status()
}

#[test]
fn previous_steps_back_over_an_unplayable_track() {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    let (first, last) = (dir.path().join("0.wav"), dir.path().join("2.wav"));
    silence(&first, 44100, 5.0);
    silence(&last, 44100, 5.0);
    let bad = bad_files(dir.path(), 1).remove(0);
    playing_at(&player, vec![first, bad, last], 2);
    let errors = player.status().error_seq;

    // Within three seconds of the start, so this steps back, not restarts.
    player.send(Cmd::Prev);
    let s = settled_after_an_error(&player, errors);
    assert!(s.error_seq > errors, "the unplayable track was never tried");
    assert_eq!(
        (s.state, s.index),
        (State::Playing, 0),
        "landed on {} after {:?}",
        s.index,
        s.error
    );
}

#[test]
fn previous_with_only_unplayable_tracks_before_restarts_the_track() {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("1.wav");
    silence(&good, 44100, 5.0);
    let bad = bad_files(dir.path(), 1).remove(0);
    playing_at(&player, vec![bad, good], 1);
    let errors = player.status().error_seq;

    player.send(Cmd::Prev);
    let s = settled_after_an_error(&player, errors);
    assert!(s.error_seq > errors, "the earlier track was never tried");
    assert_eq!((s.state, s.index), (State::Playing, 1));
}

#[test]
fn the_default_output_device_plays() {
    use cpal::traits::DeviceTrait;
    // ALSA reports a default device on a machine with no sound card, such as
    // a CI runner, but it offers no formats. One that offers formats playr
    // cannot use is a failure, not a skip.
    let formats = playr_core::audio::output::default_device()
        .ok()
        .and_then(|d| d.supported_output_configs().ok())
        .map_or(0, |c| c.count());
    if formats == 0 {
        return skip("PLAYR_REQUIRE_DEVICE", "no output device with any format");
    }
    let player = match Player::new() {
        Ok(p) => p,
        Err(e) => return skip("PLAYR_REQUIRE_DEVICE", &e.to_string()),
    };
    player.send(Cmd::SetVolume(0.0));
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("a.wav");
    silence(&track, 44100, 5.0);
    player.send(Cmd::Play(vec![track], 0));

    let deadline = Instant::now() + Duration::from_secs(5);
    while player.position() < Duration::from_millis(300) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let s = player.status();
    assert_eq!(s.state, State::Playing, "error: {:?}", s.error);
    assert!(
        player.position() >= Duration::from_millis(300),
        "the device did not play"
    );

    // The device's callback discards the buffer, well inside the timeout that
    // would reopen the device instead.
    player.send(Cmd::Seek(Duration::from_secs(3)));
    std::thread::sleep(Duration::from_millis(300));
    let pos = player.position();
    assert!(
        pos >= Duration::from_secs(3) && pos < Duration::from_millis(3400),
        "position {pos:?} 300 ms after seeking to 3 s"
    );
}

#[test]
fn playing_an_empty_queue_stops() {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("a.wav");
    silence(&track, 44100, 5.0);
    playing_at(&player, vec![track], 0);

    player.send(Cmd::Play(Vec::new(), 0));
    let s = wait_for(&player, |s| s.state == State::Stopped);
    assert_eq!(s.state, State::Stopped, "playing with nothing queued");
}

#[test]
fn a_track_that_opens_but_fails_its_first_packet_is_skipped() {
    if !common::have_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // Faststart puts the index first, so a truncated file still opens.
    let broken = dir.path().join("broken.m4a");
    let ok = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "anoisesrc=d=2:r=44100:seed=3",
        ])
        .args(["-ac", "2", "-c:a", "aac", "-movflags", "+faststart"])
        .arg(&broken)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        return skip("PLAYR_REQUIRE_FFMPEG", "this ffmpeg cannot encode AAC");
    }
    let bytes = std::fs::read(&broken).unwrap();
    std::fs::write(&broken, &bytes[..bytes.len() * 6 / 10]).unwrap();
    let mut probe = playr_core::audio::decode::AudioStream::open(&broken).expect("no longer opens");
    assert!(
        probe.next_chunk().is_err(),
        "the first packet no longer fails"
    );

    let good = dir.path().join("good.wav");
    silence(&good, 44100, 3.0);
    let player = player();
    player.send(Cmd::Play(vec![broken, good], 0));

    let s = settled_after_an_error(&player, 0);
    assert!(
        s.error
            .as_deref()
            .unwrap_or_default()
            .contains("broken.m4a"),
        "not reported: {:?}",
        s.error
    );
    assert_eq!((s.state, s.index), (State::Playing, 1));
}

#[test]
fn the_queue_is_current_as_soon_as_send_returns() {
    // The engine publishes its status every few milliseconds. It must never
    // put back a queue older than the one `send` installed, so each change is
    // watched for a while, not checked once.
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("a.wav");
    // Resampling 192 kHz keeps the engine decoding between messages, which
    // is when a stale publish would happen.
    silence(&track, 192_000, 30.0);
    player.send(Cmd::SpeedBy(1));
    for n in 1..=60 {
        player.send(Cmd::Enqueue(vec![track.clone()]));
        assert_eq!(player.queue().len(), n);
        let until = Instant::now() + Duration::from_millis(20);
        while Instant::now() < until {
            let len = player.status().queue.len();
            assert_eq!(len, n, "a stale queue of {len} was published");
        }
    }
}

#[test]
fn jumping_plays_a_queued_track_and_keeps_the_queue() {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.wav"), dir.path().join("b.wav"));
    silence(&a, 44100, 5.0);
    silence(&b, 44100, 5.0);
    playing_at(&player, vec![a, b], 0);
    let queue = player.queue();

    player.send(Cmd::Jump(1));
    let s = wait_for(&player, |s| s.index == 1);
    assert_eq!((s.state, s.index), (State::Playing, 1));
    assert!(
        std::sync::Arc::ptr_eq(&queue, &player.queue()),
        "the queue was replaced"
    );
}
