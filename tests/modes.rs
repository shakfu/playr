//! Playback modes end to end, on the fake device.
//!
//! Each track holds a constant level of its own, so the order tracks played in
//! can be read back from what the device played.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{fake_player, levels, Control};
use playr::audio::{Cmd, Mode, Player, State, Status};

const RATE: u32 = 44100;

/// Writes one track per level, each `secs` long, and returns their paths.
fn tracks(dir: &std::path::Path, secs: f32, levels_: &[f32]) -> Vec<PathBuf> {
    levels_
        .iter()
        .enumerate()
        .map(|(i, &level)| {
            let path = dir.join(format!("{i}.wav"));
            levels(&path, RATE, &[(secs, level)]);
            path
        })
        .collect()
}

/// The tracks played, in order, by their levels, as indices into `levels_`.
/// Runs shorter than 50 ms are ignored, as are silent gaps.
fn played_order(control: &Control, levels_: &[f32]) -> Vec<usize> {
    let played = control.played.lock().unwrap();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for frame in played.chunks(2) {
        let Some(track) = levels_.iter().position(|l| (frame[0] - l).abs() < 0.02) else {
            continue;
        };
        match runs.last_mut() {
            Some((t, n)) if *t == track => *n += 1,
            _ => runs.push((track, 1)),
        }
    }
    let mut order: Vec<usize> = Vec::new();
    for (track, frames) in runs {
        if frames >= RATE as usize / 20 && order.last() != Some(&track) {
            order.push(track);
        }
    }
    order
}

fn frames_of(control: &Control, level: f32) -> usize {
    let played = control.played.lock().unwrap();
    played
        .chunks(2)
        .filter(|f| (f[0] - level).abs() < 0.02)
        .count()
}

fn wait_until(done: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

fn settled(player: &Player) -> Status {
    std::thread::sleep(Duration::from_millis(300));
    player.status()
}

fn setup(mode: Mode) -> (Player, Arc<Control>, tempfile::TempDir) {
    let (player, control) = fake_player();
    player.send(Cmd::SetMode(mode));
    (player, control, tempfile::tempdir().unwrap())
}

#[test]
fn the_mode_is_current_as_soon_as_it_is_sent() {
    let (player, _control) = fake_player();
    assert_eq!(player.mode(), Mode::Normal);
    player.send(Cmd::SetMode(Mode::Shuffle));
    assert_eq!(player.mode(), Mode::Shuffle);
    assert_eq!(player.status().mode, Mode::Shuffle);
}

#[test]
fn normal_plays_the_list_once() {
    let (player, control, dir) = setup(Mode::Normal);
    let levels_ = [0.2, -0.2];
    player.send(Cmd::Play(tracks(dir.path(), 0.3, &levels_), 0));
    assert!(wait_until(
        || player.status().state == State::Stopped && frames_of(&control, -0.2) > 0
    ));
    assert_eq!(played_order(&control, &levels_), [0, 1]);
}

#[test]
fn repeat_one_replays_the_track() {
    let (player, control, dir) = setup(Mode::RepeatOne);
    let levels_ = [0.2, -0.2];
    player.send(Cmd::Play(tracks(dir.path(), 0.3, &levels_), 0));
    let four_times = (0.3 * RATE as f32) as usize * 4;
    assert!(
        wait_until(|| frames_of(&control, 0.2) >= four_times),
        "the track did not play four times over"
    );
    assert_eq!(frames_of(&control, -0.2), 0, "the next track played");
    assert_eq!(player.status().state, State::Playing);
}

#[test]
fn repeat_starts_the_list_again() {
    let (player, control, dir) = setup(Mode::Repeat);
    let levels_ = [0.2, -0.2];
    player.send(Cmd::Play(tracks(dir.path(), 0.3, &levels_), 0));
    assert!(wait_until(|| played_order(&control, &levels_).len() >= 3));
    assert_eq!(played_order(&control, &levels_)[..3], [0, 1, 0]);
}

#[test]
fn shuffle_plays_each_track_once_per_pass_from_the_chosen_one() {
    let (player, control, dir) = setup(Mode::Shuffle);
    let levels_ = [0.1, 0.2, 0.3, 0.4, 0.5];
    player.send(Cmd::Play(tracks(dir.path(), 0.25, &levels_), 2));
    assert!(wait_until(|| played_order(&control, &levels_).len() >= 6));
    let order = played_order(&control, &levels_);
    assert_eq!(order[0], 2, "did not start at the chosen track: {order:?}");
    let mut pass = order[..5].to_vec();
    pass.sort();
    assert_eq!(
        pass,
        [0, 1, 2, 3, 4],
        "a pass repeated or missed a track: {order:?}"
    );
    assert_ne!(
        order[5], order[4],
        "the next pass began with the last track"
    );
}

#[test]
fn a_mode_change_applies_to_a_next_track_already_buffered() {
    let (player, control, dir) = setup(Mode::Normal);
    let levels_ = [0.2, -0.2];
    player.send(Cmd::Play(tracks(dir.path(), 2.0, &levels_), 0));
    // The decoder leads by just over a second: by 1.4 s the second track is
    // decoding into the ring behind the first.
    assert!(wait_until(
        || player.position() >= Duration::from_millis(1400)
    ));
    player.send(Cmd::SetMode(Mode::RepeatOne));

    let first = (2.0 * RATE as f32) as usize;
    assert!(wait_until(
        || frames_of(&control, 0.2) > first + first / 4 || frames_of(&control, -0.2) > 0
    ));
    assert_eq!(
        frames_of(&control, -0.2),
        0,
        "the buffered next track still played"
    );
}

#[test]
fn repeat_over_unplayable_files_stops_after_one_pass() {
    let (player, _control, dir) = setup(Mode::Repeat);
    let bad: Vec<PathBuf> = (0..3)
        .map(|i| {
            let p = dir.path().join(format!("{i}.mp3"));
            std::fs::write(&p, b"x").unwrap();
            p
        })
        .collect();
    player.send(Cmd::Play(bad, 0));
    assert!(wait_until(|| player.status().error_seq >= 3));
    let s = settled(&player);
    assert_eq!(s.state, State::Stopped);
    assert_eq!(s.error_seq, 3, "kept retrying the same bad files");
}

#[test]
fn previous_in_shuffle_returns_to_the_track_played_before() {
    let (player, _control, dir) = setup(Mode::Shuffle);
    let levels_ = [0.1, 0.2, 0.3, 0.4];
    player.send(Cmd::Play(tracks(dir.path(), 1.0, &levels_), 0));
    assert!(wait_until(|| player.status().index != 0));
    let second = player.status().index;

    player.send(Cmd::Prev);
    let s = settled(&player);
    assert_eq!(
        s.index, 0,
        "went from {second} to {} rather than back",
        s.index
    );
}

#[test]
fn next_under_repeat_one_moves_on() {
    let (player, _control, dir) = setup(Mode::RepeatOne);
    let levels_ = [0.2, -0.2];
    player.send(Cmd::Play(tracks(dir.path(), 5.0, &levels_), 0));
    assert!(wait_until(|| player.status().state == State::Playing));
    player.send(Cmd::Next);
    let s = settled(&player);
    assert_eq!((s.state, s.index), (State::Playing, 1));
}
