//! The queue: tracks the listener queued, spliced into the list playing.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use playr_core::audio::State;
use playr_core::db::{self, Track};
use playr_core::event;
use playr_core::notice::{Notice, Outcome};
use playr_core::session::Session;

/// A session, and `n` five-second tracks named t0, t1, ...
fn setup(n: usize) -> (Session, Vec<Track>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let tracks = (0..n)
        .map(|i| {
            let path = dir.path().join(format!("t{i}.wav"));
            common::silence(&path, 8000, 5.0);
            Track {
                path: path.to_string_lossy().into_owned(),
                ..Default::default()
            }
        })
        .collect();
    let conn = db::open_memory().unwrap();
    (
        Session::new(conn, common::fake_player().0, event::ignore()),
        tracks,
        dir,
    )
}

fn wait_until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The player's list, by track name.
fn list(s: &Session) -> Vec<String> {
    s.player()
        .queue()
        .iter()
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect()
}

/// The queue's rows, by track name.
fn rows(s: &Session) -> Vec<String> {
    let names = list(s);
    s.queue_rows()
        .into_iter()
        .map(|i| names[i].clone())
        .collect()
}

fn playing(s: &Session) -> String {
    let status = s.player().status();
    let path: &PathBuf = status.current().unwrap();
    path.file_stem().unwrap().to_string_lossy().into_owned()
}

#[test]
fn the_library_plays_on_its_own_and_fills_no_queue() {
    let (mut s, t, _dir) = setup(3);
    s.play(&t, 1);
    wait_until("playing", || s.player().status().state == State::Playing);
    assert_eq!(playing(&s), "t1");
    assert!(rows(&s).is_empty());
}

#[test]
fn the_first_track_queued_interrupts_and_the_rest_wait_in_order() {
    let (mut s, t, _dir) = setup(6);
    let library = &t[..3];
    s.play(library, 0);
    wait_until("playing", || s.player().status().state == State::Playing);

    s.enqueue(&t[3..4], false);
    wait_until("interrupted", || playing(&s) == "t3");
    assert_eq!(rows(&s), ["t3"]);
    s.enqueue(&t[4..5], false);
    assert_eq!(
        rows(&s),
        ["t3", "t4"],
        "a second track waits, and plays none"
    );
    assert_eq!(playing(&s), "t3");
    // Play next goes before the tracks waiting.
    s.enqueue(&t[5..6], true);
    assert_eq!(rows(&s), ["t3", "t5", "t4"]);
    // The library resumes after the track the queue interrupted.
    assert_eq!(list(&s), ["t0", "t3", "t5", "t4", "t1", "t2"]);

    // A queued track that has played leaves the list.
    s.send(playr_core::audio::Cmd::Next);
    wait_until("moved on", || playing(&s) == "t5");
    s.drop_played();
    assert_eq!(list(&s), ["t0", "t5", "t4", "t1", "t2"]);
    assert_eq!(rows(&s), ["t5", "t4"]);
}

#[test]
fn with_nothing_playing_a_queued_track_plays_at_once() {
    let (mut s, t, _dir) = setup(2);
    assert_eq!(
        s.enqueue(&t[1..], false),
        Outcome::Queued {
            tracks: 1,
            next: false
        }
    );
    wait_until("playing", || s.player().status().state == State::Playing);
    assert_eq!(playing(&s), "t1");
    assert_eq!(rows(&s), ["t1"]);
}

#[test]
fn playing_the_library_keeps_the_tracks_waiting() {
    let (mut s, t, _dir) = setup(5);
    s.play(&t[..3], 0);
    wait_until("playing", || s.player().status().state == State::Playing);
    s.enqueue(&t[3..4], false);
    s.enqueue(&t[4..5], false);
    wait_until("interrupted", || playing(&s) == "t3");
    // A new place in the library: t4 still waits, and plays after t2.
    s.play(&t[..3], 2);
    wait_until("replayed", || playing(&s) == "t2");
    assert_eq!(list(&s), ["t0", "t1", "t2", "t4"]);
    assert_eq!(rows(&s), ["t4"]);
}

#[test]
fn a_search_or_playlist_replaces_the_queue_over_the_library() {
    let (mut s, t, _dir) = setup(6);
    s.play(&t[..3], 0);
    wait_until("playing", || s.player().status().state == State::Playing);
    s.enqueue(&t[3..4], false);
    s.enqueue(&t[4..5], false);
    wait_until("interrupted", || playing(&s) == "t3");
    // Results t4, t5 from the second: t4, which waited, is replaced.
    assert_eq!(
        s.play_queued(&t[4..6], 1),
        Some(Outcome::QueueReplaced { tracks: 1 })
    );
    wait_until("replaced", || playing(&s) == "t5");
    assert_eq!(list(&s), ["t0", "t5", "t1", "t2"]);
    assert_eq!(rows(&s), ["t5"]);
    // Nothing waiting, so nothing is said.
    assert_eq!(s.play_queued(&t[4..5], 0), None);
}

#[test]
fn waiting_tracks_move_and_clear_but_the_one_playing_stays() {
    let (mut s, t, _dir) = setup(5);
    s.play(&t[..1], 0);
    wait_until("playing", || s.player().status().state == State::Playing);
    for i in 1..4 {
        s.enqueue(&t[i..i + 1], false);
    }
    wait_until("interrupted", || playing(&s) == "t1");
    assert_eq!(rows(&s), ["t1", "t2", "t3"]);
    assert_eq!(s.move_in_queue(0, 1), None, "the track playing moved");
    assert_eq!(s.move_in_queue(1, -1), None, "a track moved above it");
    assert_eq!(s.move_in_queue(2, -1), Some(1));
    assert_eq!(rows(&s), ["t1", "t3", "t2"]);
    assert_eq!(s.clear_queue(), Outcome::QueueCleared { tracks: 2 });
    assert_eq!(rows(&s), ["t1"]);
    assert_eq!(playing(&s), "t1");
}

#[test]
fn a_track_queued_right_after_play_joins_what_starts_playing() {
    // Before the engine has started the list it reports stopped, which must
    // not be taken for nothing playing.
    let (mut s, t, _dir) = setup(4);
    s.play(&t[..2], 1);
    s.enqueue(&t[2..3], false);
    s.enqueue(&t[3..4], false);
    assert_eq!(list(&s), ["t0", "t1", "t2", "t3"]);
    wait_until("interrupted", || playing(&s) == "t2");
    assert_eq!(rows(&s), ["t2", "t3"]);
}

/// As [`setup`], with the tracks in the library, so a playlist can hold them.
fn setup_library(n: usize) -> (Session, Vec<Track>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open_memory().unwrap();
    for i in 0..n {
        let path = dir.path().join(format!("t{i}.wav"));
        common::silence(&path, 8000, 5.0);
        let track = Track {
            path: path.to_string_lossy().into_owned(),
            mtime: 1,
            size: 1,
            ..Default::default()
        };
        db::upsert(&conn, &track).unwrap();
    }
    let s = Session::new(conn, common::fake_player().0, event::ignore());
    let mut tracks = s.tracks().to_vec();
    tracks.sort_by(|a, b| a.path.cmp(&b.path));
    (s, tracks, dir)
}

fn names(tracks: &[Track]) -> Vec<String> {
    tracks.iter().map(Track::display_title).collect()
}

/// Plays t0, queues t1, t2 and t3, and moves on until t1 has played.
fn play_past_first_queued(s: &mut Session, t: &[Track]) {
    s.play(&t[..1], 0);
    wait_until("playing", || s.player().status().state == State::Playing);
    s.enqueue(&t[1..4], false);
    wait_until("interrupted", || playing(s) == "t1");
    s.send(playr_core::audio::Cmd::Next);
    wait_until("moved on", || playing(s) == "t2");
    s.drop_played();
    wait_until("dropped", || s.player().caught_up());
}

#[test]
fn queued_tracks_that_played_stay_until_taken_out_and_save_with_the_rest() {
    let (mut s, t, _dir) = setup_library(4);
    play_past_first_queued(&mut s, &t);
    assert_eq!(names(s.played()), ["t1"]);
    assert_eq!(rows(&s), ["t2", "t3"]);
    assert_eq!(names(&s.queue_tracks()), ["t1", "t2", "t3"]);

    // A played row moves only among the played rows.
    assert_eq!(s.move_in_queue(0, 1), None);
    // Rows after the played ones are numbered after them.
    assert_eq!(
        s.move_in_queue(2, -1),
        None,
        "a track moved above the playing one"
    );

    assert_eq!(
        s.save_queue("heard", false),
        Notice::Done(Outcome::Saved {
            name: "heard".into(),
            tracks: 3,
            left_out: 0
        })
    );
    let id = s.playlists().iter().find(|p| p.name == "heard").unwrap().id;
    assert_eq!(names(&s.playlist_tracks(id)), ["t1", "t2", "t3"]);

    assert_eq!(
        s.dequeue(0),
        Some(Outcome::RemovedTrack { title: "t1".into() })
    );
    assert!(s.played().is_empty());
    assert_eq!(rows(&s), ["t2", "t3"]);
}

#[test]
fn a_played_row_plays_again_and_clearing_empties_the_played_rows_too() {
    let (mut s, t, _dir) = setup_library(4);
    play_past_first_queued(&mut s, &t);
    s.play_queue_row(0);
    wait_until("replayed", || s.player().caught_up() && playing(&s) == "t1");
    assert!(s.played().is_empty(), "replayed and still listed as played");
    assert_eq!(rows(&s), ["t1", "t3"]);

    s.send(playr_core::audio::Cmd::Next);
    wait_until("moved on", || playing(&s) == "t3");
    s.drop_played();
    // t2, interrupted by the replay, has played as far as the queue goes.
    assert_eq!(names(s.played()), ["t2", "t1"]);
    assert_eq!(s.clear_queue(), Outcome::QueueCleared { tracks: 2 });
    assert!(s.played().is_empty());
}

#[test]
fn the_queue_is_remembered_with_the_track_and_taken_up_with_it() {
    let (mut s, t, dir) = setup_library(4);
    play_past_first_queued(&mut s, &t);
    let current = s.player().status().current().unwrap().clone();
    s.remember(&current, Duration::from_secs(1));

    let (path, at, queue) = s.resumable().unwrap();
    assert_eq!(path, current);
    let stems = |ps: &[PathBuf]| -> Vec<String> {
        ps.iter()
            .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
            .collect()
    };
    assert_eq!(stems(&queue.played), ["t1"]);
    assert!(queue.playing, "t2 was queued");
    assert_eq!(stems(&queue.waiting), ["t3"]);

    s.clear_queue();
    s.resume(path, at, queue);
    // The list changes as sent; the index once the engine has caught up.
    wait_until("taken up", || {
        s.player().caught_up() && list(&s) == ["t2", "t3"]
    });
    assert_eq!(names(s.played()), ["t1"]);
    assert_eq!(rows(&s), ["t2", "t3"]);

    // A file that has gone is left out of the queue offered.
    std::fs::remove_file(dir.path().join("t3.wav")).unwrap();
    assert!(s.resumable().unwrap().2.waiting.is_empty());
}
