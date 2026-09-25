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
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("1.wav"), dir.path().join("2.wav"));
    // A tenth of a second, which the device can make up in a single catch-up
    // burst, so no poll need ever see `Playing`. Counted frames do not pass:
    // hearing the last one is what shows the track played.
    counting(&first, 800);
    silence(&second, 8_000, 1.0);
    let (player, control) = fake_player();
    player.send(Cmd::Play(vec![first], 0));
    until_heard(&player, &control, 0, |s| s.contains(&800));

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
    // Until the engine takes the seek, the position is the one before it; a
    // loaded runner may take longer than a fixed sleep allows.
    let deadline = Instant::now() + Duration::from_secs(30);
    while player.position() >= Duration::from_secs(1) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let position = player.position();
    let s = player.status();
    // A seek ignored, or taken in the second track, also ends under 1 s, but
    // in track 1.
    assert_eq!(s.index, 0);
    assert!(
        s.duration.is_some_and(|d| d < Duration::from_secs(3)),
        "the first track shows the second track's duration: {:?}",
        s.duration
    );
    assert!(position < Duration::from_secs(1), "{position:?}");

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
    let formats = playr_core::audio::output::device(None)
        .ok()
        .and_then(|d| d.supported_output_configs().ok())
        .map_or(0, |c| c.count());
    if formats == 0 {
        return skip("PLAYR_REQUIRE_DEVICE", "no output device with any format");
    }
    let player = match Player::new(None) {
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

/// The status once `done` holds, or the last status after `within`.
fn wait_within(player: &Player, within: Duration, done: impl Fn(&Status) -> bool) -> Status {
    let deadline = Instant::now() + within;
    loop {
        let s = player.status();
        if done(&s) || Instant::now() > deadline {
            return s;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_seek_past_the_end_of_a_track_moves_on_as_its_end_would() {
    let player = player();
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("1.wav"), dir.path().join("2.wav"));
    // Long enough that neither track plays out while the test waits.
    silence(&first, 8000, 20.0);
    silence(&second, 8000, 20.0);
    player.send(Cmd::Play(vec![first, second], 0));
    wait_for(&player, |s| s.state == State::Playing);
    let soon = Duration::from_secs(2);

    player.send(Cmd::Seek(Duration::from_secs(30)));
    let s = wait_within(&player, soon, |s| s.index == 1);
    assert_eq!(
        (s.index, s.state),
        (1, State::Playing),
        "the seek was ignored"
    );
    assert!(
        player.position() < Duration::from_secs(2),
        "{:?}",
        player.position()
    );

    // Relative, in the last track: playback ends.
    player.send(Cmd::SeekBy(30));
    let s = wait_within(&player, soon, |s| s.state == State::Stopped);
    assert_eq!(s.state, State::Stopped, "the seek was ignored");

    // Paused, the next track waits paused too.
    player.send(Cmd::Play(
        vec![dir.path().join("1.wav"), dir.path().join("2.wav")],
        0,
    ));
    wait_for(&player, |s| s.state == State::Playing);
    player.send(Cmd::TogglePause);
    wait_for(&player, |s| s.state == State::Paused);
    player.send(Cmd::Seek(Duration::from_secs(30)));
    let s = wait_within(&player, soon, |s| s.index == 1);
    assert_eq!((s.index, s.state), (1, State::Paused));
}

#[test]
fn the_position_is_the_seek_target_until_the_device_discards() {
    let (player, control) = fake_player();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    silence(&file, 8000, 20.0);
    player.send(Cmd::Play(vec![file], 0));
    wait_for(&player, |s| s.state == State::Playing);

    // A stalled device has not discarded the audio from before the seek.
    control.stall();
    player.send(Cmd::Seek(Duration::from_secs(10)));
    std::thread::sleep(Duration::from_millis(100));
    let pos = player.position();
    assert!(
        (Duration::from_secs(10)..Duration::from_millis(10_500)).contains(&pos),
        "position {pos:?}"
    );

    control
        .stall
        .store(false, std::sync::atomic::Ordering::Relaxed);
    let deadline = Instant::now() + Duration::from_secs(3);
    while player.position() < Duration::from_millis(10_200) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(player.position() >= Duration::from_millis(10_200));
}

#[test]
fn a_seek_the_position_has_reported_does_not_fall_back() {
    let (player, _control) = fake_player();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    silence(&file, 8000, 20.0);
    player.send(Cmd::Play(vec![file], 0));
    wait_for(&player, |s| s.state == State::Playing);

    // The device discards the pre-seek audio and says so a pass before the
    // engine resets the counters, and a read in between gave the old position.
    for round in 0..20 {
        player.send(Cmd::Seek(Duration::from_millis(100)));
        std::thread::sleep(Duration::from_millis(12));
        player.send(Cmd::Seek(Duration::from_secs(7)));

        // From when the engine has taken it: before that the old position is honest.
        let taken = Instant::now() + Duration::from_secs(5);
        while player.position() < Duration::from_secs(7) && Instant::now() < taken {
            std::thread::sleep(Duration::from_micros(200));
        }
        assert!(
            player.position() >= Duration::from_secs(7),
            "the seek was never taken"
        );
        let until = Instant::now() + Duration::from_millis(15);
        while Instant::now() < until {
            let at = player.position();
            assert!(
                at >= Duration::from_secs(7),
                "round {round}: position {at:?} after a seek to 7 s"
            );
        }
    }
}

#[test]
fn a_track_change_before_a_seek_is_discarded_keeps_the_next_track_whole() {
    let (player, control) = fake_player();
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("0.wav"), dir.path().join("1.wav"));
    levels(&first, 8000, &[(20.0, 0.25)]);
    levels(&second, 8000, &[(3.0, -0.25)]);
    player.send(Cmd::Play(vec![first, second], 0));
    wait_for(&player, |s| s.state == State::Playing);

    control.stall();
    player.send(Cmd::Seek(Duration::from_secs(5)));
    player.send(Cmd::Next);
    wait_for(&player, |s| s.index == 1);
    std::thread::sleep(Duration::from_millis(100));
    control
        .stall
        .store(false, std::sync::atomic::Ordering::Relaxed);

    let s = wait_for(&player, |s| s.state == State::Stopped);
    assert_eq!(s.state, State::Stopped);
    let played = control.played.lock().unwrap();
    let second_secs = played.iter().filter(|v| **v < -0.2).count() as f64 / 2.0 / 8000.0;
    assert!(
        (second_secs - 3.0).abs() < 0.1,
        "the second track played for {second_secs:.2} s"
    );
}

/// Writes 16-bit stereo at 8 kHz whose left channel counts frames from 1, so
/// a played sample names its frame and silence reads as 0.
fn counting(path: &Path, frames: i32) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 8_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for f in 0..frames {
        w.write_sample((f + 1) as i16).unwrap();
        w.write_sample(0i16).unwrap();
    }
    w.finalize().unwrap();
}

/// The frames the device has played since sample `from`, as the counting
/// track names them, without silence.
fn heard(control: &common::Control, from: usize) -> Vec<i64> {
    let played = control.played.lock().unwrap();
    played[from.min(played.len())..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|f| (f[0] * 32_768.0).round() as i64)
        .filter(|&v| v > 0)
        .collect()
}

/// Waits until `done` holds for what has been heard since `from`.
fn until_heard(
    player: &Player,
    control: &common::Control,
    from: usize,
    done: impl Fn(&[i64]) -> bool,
) -> Vec<i64> {
    // The same budget `wait_for` allows the engine: a loaded runner plays late.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let seen = heard(control, from);
        if done(&seen) {
            return seen;
        }
        if Instant::now() >= deadline {
            let s = player.status();
            panic!(
                "heard {} frames; {:?} at track {}; {}",
                seen.len(),
                s.state,
                s.index,
                control.report()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Every frame from the first `first` on follows the one before, or returns
/// from `last` to `first`.
fn assert_loops(seen: &[i64], first: i64, last: i64) {
    let from = seen
        .iter()
        .position(|&v| v == first)
        .expect("never reached the loop");
    for pair in seen[from..].windows(2) {
        assert!(
            pair[1] == pair[0] + 1 || (pair[0] == last && pair[1] == first),
            "{pair:?} in a loop of {first}..={last}"
        );
    }
}

#[test]
fn a_loop_repeats_its_frames_exactly_and_follows_new_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("count.wav");
    counting(&file, 24_000);
    let (player, control) = fake_player();
    player.send(Cmd::Play(vec![file], 0));
    wait_for(&player, |s| s.state == State::Playing);

    // Frames 8,000 to 8,399 carry 8,001 to 8,400; the playhead is before them.
    player.send(Cmd::Loop(Some((8_000, 8_400))));
    let returns = |seen: &[i64]| seen.windows(2).filter(|p| p == &[8_400, 8_001]).count();
    let seen = until_heard(&player, &control, 0, |s| returns(s) >= 5);
    assert_loops(&seen, 8_001, 8_400);
    assert_eq!(player.status().looping, Some((8_000, 8_400)));
    // Read over several passes: the position must stay inside the loop even
    // between the device reaching the wrap and the engine's next pass.
    for _ in 0..200 {
        let at = player.position().as_secs_f64();
        assert!((1.0..1.05).contains(&at), "position {at}");
        std::thread::sleep(Duration::from_millis(1));
    }

    // Narrower bounds apply once the audio decoded under the old ones is gone.
    player.send(Cmd::Loop(Some((8_100, 8_300))));
    std::thread::sleep(Duration::from_millis(300));
    let from = control.played.lock().unwrap().len();
    let seen = until_heard(&player, &control, from, |s| {
        s.windows(2).filter(|p| p == &[8_300, 8_101]).count() >= 5
    });
    assert_loops(&seen, 8_101, 8_300);

    // Off: playback runs on past the end.
    player.send(Cmd::Loop(None));
    until_heard(&player, &control, from, |s| s.iter().any(|&v| v > 9_000));
    assert_eq!(player.status().looping, None);
}

#[test]
fn a_loop_past_the_end_returns_from_the_end_and_a_new_track_ends_it() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.wav"), dir.path().join("b.wav"));
    counting(&a, 8_000);
    counting(&b, 8_000);
    let (player, control) = fake_player();
    player.send(Cmd::Play(vec![a, b], 0));
    wait_for(&player, |s| s.state == State::Playing);
    // Asked past the end, it ends there: 7,801 to 8,000, again and again.
    player.send(Cmd::Loop(Some((7_800, 99_000))));
    let seen = until_heard(&player, &control, 0, |s| {
        s.windows(2).filter(|p| p == &[8_000, 7_801]).count() >= 5
    });
    assert_loops(&seen, 7_801, 8_000);
    let status = wait_for(&player, |s| s.looping == Some((7_800, 8_000)));
    assert_eq!((status.looping, status.index), (Some((7_800, 8_000)), 0));

    player.send(Cmd::Next);
    let status = wait_for(&player, |s| s.index == 1);
    assert_eq!((status.index, status.looping), (1, None));
}

#[test]
fn a_one_shot_range_plays_through_once_and_pauses_at_its_end() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("count.wav");
    counting(&file, 24_000);
    let (player, control) = fake_player();
    player.send(Cmd::Play(vec![file], 0));
    wait_for(&player, |s| s.state == State::Playing);

    // The same frames a loop would take, played once.
    player.send(Cmd::PlayOnce(8_000, 8_400));
    wait_for(&player, |s| s.state == State::Paused);
    // Playback can already be past 8,400 when the command lands, and the engine
    // then seeks back, so the one-shot run is the last entry into the range.
    let seen = heard(&control, 0);
    let entered = seen
        .iter()
        .rposition(|&v| v == 8_001)
        .expect("never entered the range");
    let run = &seen[entered..];
    assert!(run.contains(&8_400), "never reached the end of the range");
    assert_eq!(
        run.windows(2).filter(|p| p == &[8_400, 8_001]).count(),
        0,
        "returned to the start, as a loop does"
    );
    assert!(
        !run.iter().any(|&v| v > 8_400),
        "played past the end of the range"
    );
    assert_eq!(player.status().looping, None, "left a loop behind");

    // Playing on continues after the range rather than restarting the track.
    let from = control.played.lock().unwrap().len();
    player.send(Cmd::TogglePause);
    let after = until_heard(&player, &control, from, |s| s.iter().any(|&v| v > 8_400));
    assert!(
        after.iter().all(|&v| v > 8_000),
        "went back into the track: {:?}",
        &after[..after.len().min(8)]
    );
}

#[test]
fn a_seek_while_stopped_cues_the_track_paused_and_silent() {
    let (player, control) = fake_player();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("loud.wav");
    levels(&file, 8000, &[(20.0, 0.5)]);
    player.send(Cmd::Play(vec![file], 0));
    wait_for(&player, |s| s.state == State::Playing);
    player.send(Cmd::Stop);
    wait_for(&player, |s| s.state == State::Stopped);

    control.played.lock().unwrap().clear();
    player.send(Cmd::Seek(Duration::from_secs(10)));
    let s = wait_for(&player, |s| s.state == State::Paused);
    assert_eq!(s.state, State::Paused, "{}", control.report());
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(player.position(), Duration::from_secs(10));
    assert!(
        control.played.lock().unwrap().iter().all(|&v| v == 0.0),
        "a cued track sounded"
    );

    // Playing on starts from the cued point.
    player.send(Cmd::TogglePause);
    wait_for(&player, |s| s.state == State::Playing);
    let deadline = Instant::now() + Duration::from_secs(3);
    while player.position() < Duration::from_millis(10_100) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let pos = player.position();
    assert!(
        (Duration::from_millis(10_100)..Duration::from_secs(12)).contains(&pos),
        "position {pos:?}"
    );
    assert!(control.played.lock().unwrap().iter().any(|&v| v != 0.0));
}

#[test]
fn a_one_shot_sent_again_inside_its_range_starts_it_again() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("count.wav");
    counting(&file, 24_000);
    let (player, control) = fake_player();
    player.send(Cmd::Play(vec![file], 0));
    wait_for(&player, |s| s.state == State::Playing);

    player.send(Cmd::PlayOnce(8_000, 16_000));
    until_heard(&player, &control, 0, |s| s.contains(&12_000));
    let from = control.played.lock().unwrap().len();
    player.send(Cmd::PlayOnce(8_000, 16_000));
    let again = until_heard(&player, &control, from, |s| s.contains(&16_000));
    assert!(
        again.contains(&8_001),
        "went on from the playhead: {:?}",
        &again[..again.len().min(8)]
    );
    wait_for(&player, |s| s.state == State::Paused);
    assert!(!heard(&control, from).iter().any(|&v| v > 16_000));
}
