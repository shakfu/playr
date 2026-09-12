//! Engine tests. They need the default output device and skip without one.
//!
//! Volume is set to zero before anything plays, so a run is silent.

use playr::audio::{Cmd, Player, State, Status};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Consecutive unplayable files. The recursive skip overflowed at 2,000 in a
/// debug build.
const BAD_RUN: usize = 5_000;

fn player() -> Option<Player> {
    match Player::new() {
        Ok(p) => {
            p.send(Cmd::SetVolume(0.0));
            Some(p)
        }
        Err(e) => {
            eprintln!("skipping: {e}");
            None
        }
    }
}

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

fn have_ffmpeg() -> bool {
    let ok = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("skipping: ffmpeg not available");
    }
    ok
}

/// Writes `secs` of stereo silence at `rate` as WAV.
fn silence(path: &Path, rate: u32, secs: f32) {
    let status = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
        .arg(format!("anullsrc=r={rate}:cl=stereo"))
        .args(["-t", &secs.to_string()])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success(), "ffmpeg failed to write {path:?}");
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
    let Some(player) = player() else { return };
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
    if !have_ffmpeg() {
        return;
    }
    let Some(player) = player() else { return };
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
fn varispeed_status(rates: &[u32], index: usize) -> Option<Status> {
    if !have_ffmpeg() {
        return None;
    }
    let player = player()?;
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
    Some(s)
}

#[test]
fn varispeed_applies_to_a_track_started_directly() {
    let Some(s) = varispeed_status(&[44100], 0) else {
        return;
    };
    assert!(s.resampling, "track plays at normal speed: {s:?}");
}

#[test]
fn varispeed_survives_a_gapless_track_change() {
    let Some(s) = varispeed_status(&[44100, 44100], 1) else {
        return;
    };
    assert!(s.resampling, "next track plays at normal speed: {s:?}");
}

#[test]
fn varispeed_survives_a_track_change_that_reopens_the_output() {
    let Some(s) = varispeed_status(&[44100, 48000], 1) else {
        return;
    };
    assert!(s.resampling, "next track plays at normal speed: {s:?}");
}

#[test]
fn publishing_a_status_does_not_copy_the_queue() {
    let Some(player) = player() else { return };
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
fn playing(secs: f32) -> Option<(Player, PathBuf, tempfile::TempDir)> {
    if !have_ffmpeg() {
        return None;
    }
    let player = player()?;
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("1.wav"), dir.path().join("2.wav"));
    silence(&first, 44100, secs);
    silence(&second, 44100, 1.0);
    player.send(Cmd::Play(vec![first], 0));
    let s = wait_for(&player, |s| s.state == State::Playing);
    assert_eq!(s.state, State::Playing);
    Some((player, second, dir))
}

#[test]
fn enqueueing_after_the_queue_ends_starts_playback() {
    let Some((player, second, _dir)) = playing(0.1) else {
        return;
    };
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
    let Some((player, second, _dir)) = playing(1.0) else {
        return;
    };
    std::thread::sleep(Duration::from_millis(200));
    player.send(Cmd::Enqueue(vec![second]));

    let s = wait_for(&player, |s| s.index == 1 || s.state == State::Stopped);
    assert_eq!((s.state, s.index), (State::Playing, 1), "playback stopped");
}
