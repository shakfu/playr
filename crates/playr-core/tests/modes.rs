//! Playback modes end to end, on the fake device.
//!
//! Each track holds a constant level of its own, so the order tracks played in
//! can be read back from what the device played.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{fake_player, levels, Control};
use playr_core::audio::{Cmd, Mode, Player, State, Status};

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

#[test]
fn an_inserted_track_plays_next_though_the_next_is_buffered() {
    let (player, control, dir) = setup(Mode::Normal);
    let levels_ = [0.2, -0.2, 0.4];
    let mut paths = tracks(dir.path(), 2.0, &levels_);
    let inserted = paths.pop().unwrap();
    player.send(Cmd::Play(paths.clone(), 0));
    // By 1.4 s the second track is decoding into the ring behind the first.
    assert!(wait_until(
        || player.position() >= Duration::from_millis(1400)
    ));
    player.send(Cmd::Insert(1, vec![inserted.clone()]));
    assert_eq!(
        player.queue().to_vec(),
        [paths[0].clone(), inserted, paths[1].clone()],
        "the queue is current as soon as the insert is sent"
    );
    assert_eq!(player.status().queued[..], [false, true, false]);
    assert!(wait_until(|| played_order(&control, &levels_).len() >= 3));
    assert_eq!(played_order(&control, &levels_), [0, 2, 1]);
}

#[test]
fn an_inserted_track_plays_next_in_shuffle() {
    let (player, control, dir) = setup(Mode::Shuffle);
    let levels_ = [0.1, 0.2, 0.3, 0.4, 0.5];
    let mut paths = tracks(dir.path(), 0.5, &levels_);
    let inserted = paths.pop().unwrap();
    player.send(Cmd::Play(paths, 0));
    assert!(wait_until(|| player.position() > Duration::ZERO));
    player.send(Cmd::Insert(1, vec![inserted]));
    assert!(wait_until(|| played_order(&control, &levels_).len() >= 2));
    assert_eq!(played_order(&control, &levels_)[..2], [0, 4]);
}

#[test]
fn inserting_while_stopped_plays_the_track() {
    let (player, control, dir) = setup(Mode::Normal);
    let levels_ = [0.2];
    player.send(Cmd::Insert(0, tracks(dir.path(), 0.3, &levels_)));
    assert!(wait_until(|| frames_of(&control, 0.2) > 0));
    assert_eq!(settled(&player).index, 0);
}

#[test]
fn tracks_inserted_one_after_another_play_in_the_order_sent() {
    for mode in [Mode::Normal, Mode::Shuffle] {
        let (player, control, dir) = setup(mode);
        let levels_ = [0.1, 0.2, 0.3, 0.4];
        let mut paths = tracks(dir.path(), 0.5, &levels_);
        let second = paths.pop().unwrap();
        let first = paths.pop().unwrap();
        player.send(Cmd::Play(paths, 0));
        assert!(wait_until(|| player.position() > Duration::ZERO));
        player.send(Cmd::Insert(1, vec![first]));
        player.send(Cmd::Insert(2, vec![second]));
        assert!(wait_until(|| played_order(&control, &levels_).len() >= 3));
        assert_eq!(played_order(&control, &levels_)[..3], [0, 2, 3], "{mode:?}");
    }
}

#[test]
fn a_removed_track_does_not_play_though_it_was_buffered() {
    let (player, control, dir) = setup(Mode::Normal);
    let levels_ = [0.2, -0.2, 0.4];
    player.send(Cmd::Play(tracks(dir.path(), 2.0, &levels_), 0));
    // By 1.4 s the second track is decoding into the ring behind the first.
    assert!(wait_until(
        || player.position() >= Duration::from_millis(1400)
    ));
    player.send(Cmd::Remove(1));
    assert_eq!(player.queue().len(), 2);
    assert!(wait_until(|| player.status().state == State::Stopped));
    assert_eq!(played_order(&control, &levels_), [0, 2]);
}

#[test]
fn removing_the_track_playing_plays_the_next() {
    let (player, control, dir) = setup(Mode::Normal);
    let levels_ = [0.2, -0.2, 0.4];
    player.send(Cmd::Play(tracks(dir.path(), 0.5, &levels_), 0));
    assert!(wait_until(|| frames_of(&control, 0.2) > RATE as usize / 10));
    player.send(Cmd::Remove(0));
    assert!(wait_until(|| player.status().state == State::Stopped));
    assert_eq!(played_order(&control, &levels_), [0, 1, 2]);
    assert!(
        frames_of(&control, 0.2) < (0.4 * RATE as f32) as usize,
        "the removed track played on"
    );
}

#[test]
fn a_moved_track_plays_in_its_new_place() {
    let (player, control, dir) = setup(Mode::Normal);
    let levels_ = [0.2, -0.2, 0.4];
    let paths = tracks(dir.path(), 0.5, &levels_);
    player.send(Cmd::Play(paths.clone(), 0));
    assert!(wait_until(|| player.position() > Duration::ZERO));
    player.send(Cmd::Move(2, 1));
    assert_eq!(
        player.queue().to_vec(),
        [paths[0].clone(), paths[2].clone(), paths[1].clone()]
    );
    assert!(wait_until(|| played_order(&control, &levels_).len() >= 3));
    assert_eq!(played_order(&control, &levels_), [0, 2, 1]);
}

#[test]
fn after_the_queue_the_list_resumes_or_playback_stops() {
    use playr_core::audio::AfterQueue;
    for (after, expected) in [
        (AfterQueue::Resume, &[0, 2, 1][..]),
        (AfterQueue::Stop, &[0, 2][..]),
    ] {
        let (player, control, dir) = setup(Mode::Normal);
        player.send(Cmd::SetAfterQueue(after));
        let levels_ = [0.2, -0.2, 0.4];
        let mut paths = tracks(dir.path(), 0.5, &levels_);
        let queued = paths.pop().unwrap();
        player.send(Cmd::Play(paths, 0));
        assert!(wait_until(|| player.position() > Duration::ZERO));
        player.send(Cmd::Insert(1, vec![queued]));
        assert!(wait_until(|| player.status().state == State::Stopped));
        assert_eq!(played_order(&control, &levels_), expected, "{after:?}");
    }
}

#[test]
fn which_tracks_were_queued_follows_every_change_to_the_list() {
    let (player, _control, dir) = setup(Mode::Normal);
    let p = tracks(dir.path(), 5.0, &[0.1, 0.2, 0.3, 0.4]);
    player.send(Cmd::PlayWith(p[..2].to_vec(), vec![false, true], 0));
    assert_eq!(player.status().queued[..], [false, true]);
    player.send(Cmd::Insert(1, vec![p[2].clone()]));
    player.send(Cmd::Enqueue(vec![p[3].clone()]));
    assert_eq!(player.status().queued[..], [false, true, true, false]);
    player.send(Cmd::Move(3, 1));
    assert_eq!(player.status().queued[..], [false, false, true, true]);
    player.send(Cmd::Remove(2));
    assert_eq!(player.status().queued[..], [false, false, true]);
    // A list whose marks do not match it is refused.
    player.send(Cmd::PlayWith(p.clone(), vec![true], 0));
    assert_eq!(player.queue().len(), 3);
    player.send(Cmd::Play(p, 0));
    assert!(player.status().queued.iter().all(|q| !q));
}

#[test]
fn stop_after_ends_playback_with_the_track_playing_then_clears() {
    let (player, control, dir) = setup(Mode::Normal);
    let levels_ = [0.2, -0.2, 0.4];
    let paths = tracks(dir.path(), 0.5, &levels_);
    player.send(Cmd::Play(paths.clone(), 0));
    assert!(wait_until(|| player.position() > Duration::ZERO));
    player.send(Cmd::StopAfter(true));
    assert!(wait_until(|| player.status().stop_after));
    // Long enough to count as played.
    assert!(wait_until(|| player.position() > Duration::from_millis(150)));
    // A skip is the listener's own choice, so it is not stopped.
    player.send(Cmd::Next);
    assert!(wait_until(|| player.status().state == State::Stopped));
    assert_eq!(played_order(&control, &levels_), [0, 1]);
    assert!(!player.status().stop_after, "still set once stopped");

    // Set while stopped, it is ignored rather than left for a later play.
    player.send(Cmd::StopAfter(true));
    player.send(Cmd::Play(paths, 0));
    assert!(wait_until(|| {
        player.caught_up() && player.status().state == State::Stopped
    }));
    assert_eq!(played_order(&control, &levels_), [0, 1, 0, 1, 2]);
}

#[test]
fn stop_after_turned_off_plays_on() {
    let (player, control, dir) = setup(Mode::Normal);
    let levels_ = [0.2, -0.2];
    let paths = tracks(dir.path(), 0.5, &levels_);
    player.send(Cmd::Play(paths, 0));
    assert!(wait_until(|| player.position() > Duration::ZERO));
    player.send(Cmd::StopAfter(true));
    player.send(Cmd::StopAfter(false));
    assert!(wait_until(|| player.status().state == State::Stopped));
    assert_eq!(played_order(&control, &levels_), [0, 1]);
}

#[test]
fn an_unqueued_track_plays_on_into_the_list() {
    use playr_core::audio::AfterQueue;
    let (player, control, dir) = setup(Mode::Normal);
    player.send(Cmd::SetAfterQueue(AfterQueue::Stop));
    let levels_ = [0.2, -0.2, 0.4];
    let paths = tracks(dir.path(), 0.5, &levels_);
    // The queued track would end the queue, and stop playback there.
    player.send(Cmd::PlayWith(paths, vec![false, true, false], 1));
    assert!(wait_until(|| player.position() > Duration::ZERO));
    player.send(Cmd::Unqueue(1));
    assert_eq!(player.status().queued[..], [false, false, false]);
    assert!(wait_until(|| player.status().state == State::Stopped));
    assert_eq!(played_order(&control, &levels_), [1, 2]);
}
