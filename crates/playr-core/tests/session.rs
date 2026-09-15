//! The session API, driven without any frontend: every operation takes what it
//! acts on as an argument and reports a notice.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use playr_core::audio::{Mode, State};
use playr_core::db::{self, query, Track};
use playr_core::event;
use playr_core::notice::{Notice, Outcome, Refusal};
use playr_core::samples::Cut;
use playr_core::session::Session;

fn track(path: &str, title: &str) -> Track {
    Track {
        path: path.into(),
        title: Some(title.into()),
        mtime: 1,
        size: 1,
        ..Default::default()
    }
}

/// A session over a library file holding tracks a, b and c, and a playlist
/// "late" of a and b. The directory holds the library.
fn session() -> (Session, Vec<Track>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = db::open(&dir.path().join("library.db")).unwrap();
    let tracks: Vec<Track> = [("/m/a.flac", "A"), ("/m/b.flac", "B"), ("/m/c.flac", "C")]
        .iter()
        .map(|(path, title)| {
            let mut t = track(path, title);
            t.id = db::upsert(&conn, &t).unwrap();
            t
        })
        .collect();
    query::save_playlist(&mut conn, "late", &[tracks[0].id, tracks[1].id]).unwrap();
    (
        Session::new(conn, common::fake_player().0, event::ignore()),
        tracks,
        dir,
    )
}

fn paths(tracks: &[Track]) -> Vec<&str> {
    tracks.iter().map(|t| t.path.as_str()).collect()
}

fn playlist_id(session: &Session, name: &str) -> i64 {
    session
        .playlists()
        .iter()
        .find(|p| p.name == name)
        .unwrap()
        .id
}

#[test]
fn a_new_session_loads_the_library() {
    let (session, tracks, _dir) = session();
    assert_eq!(paths(session.tracks()), paths(&tracks));
    assert_eq!(session.playlists().len(), 1);
    assert!(session.selection().is_empty());
    assert!(session.has_library_file());
    assert_eq!(paths(&session.search("b")), ["/m/b.flac"]);
}

#[test]
fn the_selection_is_edited_by_track_and_by_index() {
    let (mut session, tracks, _dir) = session();
    let (a, c) = (tracks[0].clone(), tracks[2].clone());

    assert_eq!(
        session.toggle_selected(a.clone()),
        Outcome::AddedToSelection
    );
    assert_eq!(
        session.toggle_selected(c.clone()),
        Outcome::AddedToSelection
    );
    assert_eq!(paths(session.selection()), ["/m/a.flac", "/m/c.flac"]);
    assert_eq!(
        session.toggle_selected(a.clone()),
        Outcome::RemovedFromSelection
    );
    assert_eq!(paths(session.selection()), ["/m/c.flac"]);

    // A playlist adds only the tracks not selected yet.
    let late = playlist_id(&session, "late");
    assert_eq!(
        session.add_playlist_to_selection(late),
        Some(Outcome::AddedToSelection)
    );
    assert_eq!(
        paths(session.selection()),
        ["/m/c.flac", "/m/a.flac", "/m/b.flac"]
    );
    assert_eq!(
        session.add_playlist_to_selection(late),
        Some(Outcome::AlreadyInSelection)
    );
    assert_eq!(
        session.add_playlist_to_selection(9999),
        None,
        "no such playlist"
    );

    // c moves two places; a and b keep their order.
    assert_eq!(session.move_in_selection(0, 2), Some(2));
    assert_eq!(
        paths(session.selection()),
        ["/m/a.flac", "/m/b.flac", "/m/c.flac"]
    );
    assert_eq!(session.move_in_selection(0, -1), None);
    assert_eq!(session.move_in_selection(2, 1), None);
    assert_eq!(session.move_in_selection(7, -1), None);

    assert_eq!(
        session.remove_from_selection(1),
        Some(Outcome::RemovedTrack { title: "B".into() })
    );
    assert_eq!(session.remove_from_selection(5), None);
    assert_eq!(session.clear_selection(), Outcome::SelectionCleared);
    assert!(session.selection().is_empty());
}

#[test]
fn saving_refuses_until_it_can_and_asks_before_replacing() {
    let (mut session, tracks, _dir) = session();
    assert_eq!(session.check_save(), Err(Refusal::SelectionEmpty));

    // A file played from outside the library is selected but has no row.
    session.set_selection(vec![tracks[2].clone(), track("/elsewhere/x.flac", "X")]);
    assert_eq!(session.check_save(), Ok(()));
    assert_eq!(
        session.save_selection("  ", false),
        Refusal::NameEmpty.into()
    );
    assert_eq!(
        session.save_selection(" late ", false),
        Refusal::WouldReplace("late".into()).into()
    );
    assert_eq!(
        session.save_selection("late", true),
        Notice::Done(Outcome::Saved {
            name: "late".into(),
            tracks: 1,
            left_out: 1
        })
    );
    assert_eq!(
        session.playlists()[0].len,
        1,
        "the playlist list was not refreshed"
    );
    assert_eq!(
        session.save_selection("night", false),
        Notice::Done(Outcome::Saved {
            name: "night".into(),
            tracks: 1,
            left_out: 1
        })
    );
    assert_eq!(session.playlists().len(), 2);

    let mut in_memory = Session::new(
        db::open_memory().unwrap(),
        common::fake_player().0,
        event::ignore(),
    );
    in_memory.set_selection(vec![tracks[0].clone()]);
    assert_eq!(in_memory.check_save(), Err(Refusal::NoLibraryFile));
}

#[test]
fn playlists_are_renamed_and_deleted_by_id() {
    let (mut session, tracks, _dir) = session();
    session.set_selection(vec![tracks[2].clone()]);
    session.save_selection("early", false);
    let late = playlist_id(&session, "late");

    assert_eq!(
        session.rename_playlist(late, " "),
        Refusal::NameEmpty.into()
    );
    assert_eq!(
        session.rename_playlist(late, "late"),
        Refusal::NameUnchanged.into()
    );
    assert_eq!(
        session.rename_playlist(late, "early"),
        Refusal::NameTaken("early".into()).into()
    );
    assert_eq!(
        session.rename_playlist(late, "night"),
        Notice::Done(Outcome::Renamed {
            from: "late".into(),
            to: "night".into()
        })
    );
    assert_eq!(playlist_id(&session, "night"), late);

    assert_eq!(
        session.delete_playlist(late),
        Some(Notice::Done(Outcome::Deleted {
            name: "night".into()
        }))
    );
    assert_eq!(session.delete_playlist(late), None, "deleted twice");
    let names: Vec<&str> = session
        .playlists()
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["early"]);
}

/// Waits until the player reports a playing track with a known source.
fn wait_until_playing(session: &Session) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while session.playing_track().is_err() {
        assert!(Instant::now() < deadline, "never started playing");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn playlists_play_by_name_or_id() {
    let (mut session, _tracks, _dir) = session();
    assert_eq!(
        session.play_playlist_named("nope"),
        Refusal::NoPlaylistNamed("nope".into()).into()
    );
    assert_eq!(
        session.play_playlist_named(" late "),
        Notice::Done(Outcome::PlayingPlaylist {
            name: "late".into()
        })
    );
    let queue: Vec<PathBuf> = session.player().queue().to_vec();
    assert_eq!(
        queue,
        [PathBuf::from("/m/a.flac"), PathBuf::from("/m/b.flac")]
    );
    assert_eq!(session.play_playlist(9999), Refusal::PlaylistEmpty.into());
}

#[test]
fn modes_and_volume_are_set_through_the_session() {
    let (session, _tracks, _dir) = session();
    assert_eq!(
        session.set_mode(Mode::Shuffle),
        Outcome::Mode(Mode::Shuffle).into()
    );
    assert_eq!(session.cycle_mode(true), Outcome::Mode(Mode::Repeat).into());
    assert_eq!(
        session.cycle_mode(false),
        Outcome::Mode(Mode::Shuffle).into()
    );
    session.send(playr_core::audio::Cmd::SetVolume(0.95));
    session.volume_by(0.1);
    assert_eq!(session.player().volume(), 1.0, "volume went past full");
    session.volume_by(-0.25);
    assert!((session.player().volume() - 0.75).abs() < 1e-6);
}

/// A session playing 20 s of silence from a library file, and the file's path.
fn playing(dir: &Path) -> (Session, PathBuf) {
    let file = dir.join("long.wav");
    common::silence(&file, 8000, 20.0);
    let conn = db::open(&dir.join("library.db")).unwrap();
    let mut session = Session::new(conn, common::fake_player().0, event::ignore());
    session.play(&[track(&file.to_string_lossy(), "Long")], 0);
    wait_until_playing(&session);
    (session, file)
}

#[test]
fn marks_need_something_playing() {
    let (mut session, _tracks, _dir) = session();
    assert_eq!(session.add_mark(None), Refusal::NothingPlaying.into());
    assert_eq!(session.undo_mark(), Refusal::NothingPlaying.into());
    assert_eq!(session.seek_to_mark(true), Refusal::NothingPlaying.into());
    assert_eq!(session.marks_to_clear(), Err(Refusal::NothingPlaying));
    assert_eq!(
        session.slice_job(Cut::Region, None).map(|_| ()),
        Err(Refusal::NothingPlaying)
    );
    assert_eq!(
        session.clear_marks(Path::new("/m/a.flac")),
        None,
        "no marks to clear"
    );
}

#[test]
fn marks_can_be_allowed_closer_but_never_on_the_same_frame() {
    let dir = tempfile::tempdir().unwrap();
    let (mut session, _file) = playing(dir.path());
    let ms = Duration::from_millis;
    session.add_mark(Some(ms(5_000)));
    assert_eq!(
        session.add_mark_within(Some(ms(5_100)), Duration::ZERO),
        Notice::Done(Outcome::Marked {
            at: ms(5_100),
            kept: true
        })
    );
    // 8 kHz: 5.1001 s rounds to the same frame as 5.1 s.
    assert_eq!(
        session.add_mark_within(Some(Duration::from_micros(5_100_010)), Duration::ZERO),
        Refusal::AlreadyMarked { at: ms(5_100) }.into()
    );
}

#[test]
fn marks_are_added_undone_cleared_and_sought_on_the_playing_track() {
    let dir = tempfile::tempdir().unwrap();
    let (mut session, file) = playing(dir.path());
    let secs = Duration::from_secs;

    assert_eq!(
        session.add_mark(Some(secs(5))),
        Notice::Done(Outcome::Marked {
            at: secs(5),
            kept: true
        })
    );
    assert_eq!(
        session.add_mark(Some(Duration::from_millis(5_300))),
        Refusal::AlreadyMarked { at: secs(5) }.into()
    );
    session.add_mark(Some(secs(15)));
    session.add_mark(Some(secs(10)));
    assert_eq!(session.marks_to_clear(), Ok((file.clone(), 3)));
    let at: Vec<Duration> = session
        .marks_for(Some(&file))
        .iter()
        .map(|m| m.time())
        .collect();
    assert_eq!(at, [secs(5), secs(10), secs(15)]);

    // Undo takes marks off in the order they were added, not by position.
    assert_eq!(
        session.undo_mark(),
        Outcome::MarkRemoved { at: secs(10) }.into()
    );

    // From near the start, forward finds 0:05.
    assert_eq!(
        session.seek_to_mark(true),
        Outcome::AtMark { at: secs(5) }.into()
    );

    let job = session.slice_job(Cut::Equal(2), None).unwrap();
    session.set_samples_dir("/tmp/cuts".into());
    let job_after = session
        .slice_job(Cut::Region, Some((8_000, 16_000)))
        .unwrap();
    assert_eq!((job.path.clone(), job.rate), (file.clone(), 8000));
    assert_eq!(job.marks, [40_000, 120_000]);
    assert_eq!(job_after.samples, PathBuf::from("/tmp/cuts"));
    assert_eq!((job.range, job_after.range), (None, Some((8_000, 16_000))));

    assert_eq!(
        session.clear_marks(&file),
        Some(Outcome::MarksCleared.into())
    );
    assert_eq!(session.marks_to_clear(), Err(Refusal::NoMarks));
    assert_eq!(session.undo_mark(), Refusal::NoMarks.into());
    assert_eq!(session.seek_to_mark(false), Refusal::NoEarlierMark.into());
    assert_ne!(session.player().status().state, State::Stopped);
}
