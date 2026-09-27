//! The queue: tracks the listener queued, spliced into the list playing.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use playr_core::audio::State;
use playr_core::db::{self, Track};
use playr_core::event;
use playr_core::notice::Outcome;
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
