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
use playr_core::session::{DraftChoice, Session, DRAFT};
use playr_core::settings::Draft;

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
    // The draft playlist would show among the playlists these tests count.
    let mut session = Session::new(conn, common::fake_player().0, event::ignore());
    session.set_draft(Draft::Off);
    (session, tracks, dir)
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

/// A session over the library file in `dir`, whose files a, b and c exist.
fn session_with_files(dir: &Path) -> (Session, Vec<Track>) {
    let conn = db::open(&dir.join("library.db")).unwrap();
    let tracks: Vec<Track> = ["a", "b", "c"]
        .iter()
        .map(|name| {
            let path = dir.join(format!("{name}.wav"));
            if !path.exists() {
                common::silence(&path, 8000, 0.05);
            }
            let mut t = track(&path.to_string_lossy(), name);
            t.id = db::upsert(&conn, &t).unwrap();
            t
        })
        .collect();
    let session = Session::new(conn, common::fake_player().0, event::ignore());
    (session, tracks)
}

/// The titles of the playlist `name`, or `None` if there is none.
fn playlist(session: &Session, name: &str) -> Option<Vec<String>> {
    let id = session.playlists().iter().find(|p| p.name == name)?.id;
    Some(
        session
            .playlist_tracks(id)
            .iter()
            .map(|t| t.display_title())
            .collect(),
    )
}

#[test]
fn the_selection_is_written_as_the_draft_and_the_next_session_starts_empty() {
    let dir = tempfile::tempdir().unwrap();
    let (mut first, t) = session_with_files(dir.path());
    assert_eq!(playlist(&first, DRAFT), None);
    first.toggle_selected(t[0].clone());
    first.toggle_selected(t[1].clone());
    first.toggle_selected(t[2].clone());
    first.move_in_selection(2, -2);
    first.remove_from_selection(1);
    assert_eq!(playlist(&first, DRAFT).unwrap(), ["c", "b"]);
    first.clear_selection();
    assert_eq!(playlist(&first, DRAFT), None, "an empty draft is kept");
    first.toggle_selected(t[0].clone());
    drop(first);

    let (again, _) = session_with_files(dir.path());
    assert!(again.selection().is_empty());
    assert_eq!(playlist(&again, DRAFT).unwrap(), ["a"]);
}

/// A session whose earlier one left a draft of a and b, with `draft` set.
fn with_old_draft(dir: &Path, draft: Draft) -> (Session, Vec<Track>) {
    let (mut first, t) = session_with_files(dir);
    first.set_selection(t[..2].to_vec());
    drop(first);
    let (mut s, t) = session_with_files(dir);
    s.set_draft(draft);
    (s, t)
}

#[test]
fn an_old_draft_is_asked_about_once_a_change_comes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, t) = with_old_draft(dir.path(), Draft::Ask);
    assert_eq!(s.take_draft_question(), None, "asked before any change");
    s.toggle_selected(t[2].clone());
    assert_eq!(s.take_draft_question(), Some(2));
    assert_eq!(s.take_draft_question(), None, "asked twice for one change");
    assert_eq!(playlist(&s, DRAFT).unwrap(), ["a", "b"], "written unasked");
    // Unanswered, the next change asks again.
    s.toggle_selected(t[1].clone());
    assert_eq!(s.take_draft_question(), Some(2));
}

#[test]
fn an_old_draft_is_overwritten_appended_or_saved_as_answered() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, t) = with_old_draft(dir.path(), Draft::Ask);
    s.toggle_selected(t[2].clone());
    s.settle_draft(DraftChoice::Overwrite);
    assert_eq!(playlist(&s, DRAFT).unwrap(), ["c"]);
    drop(s);

    let dir = tempfile::tempdir().unwrap();
    let (mut s, t) = with_old_draft(dir.path(), Draft::Ask);
    s.toggle_selected(t[2].clone());
    assert_eq!(
        s.settle_draft(DraftChoice::Append),
        Notice::Done(Outcome::DraftAppended)
    );
    assert_eq!(paths(s.selection()), paths(&t));
    assert_eq!(playlist(&s, DRAFT).unwrap(), ["a", "b", "c"]);
    drop(s);

    let dir = tempfile::tempdir().unwrap();
    let (mut s, t) = with_old_draft(dir.path(), Draft::Ask);
    s.toggle_selected(t[2].clone());
    // The draft's name is reserved, and a refused name asks again.
    assert_eq!(
        s.settle_draft(DraftChoice::SaveAs("Draft".into())),
        Notice::Refused(Refusal::NameReserved("Draft".into()))
    );
    assert_eq!(s.take_draft_question(), Some(2));
    s.settle_draft(DraftChoice::SaveAs("kept".into()));
    assert_eq!(playlist(&s, "kept").unwrap(), ["a", "b"]);
    assert_eq!(playlist(&s, DRAFT).unwrap(), ["c"]);
}

#[test]
fn the_draft_setting_can_answer_for_the_listener_or_keep_no_draft() {
    for (draft, expected) in [
        (Draft::Overwrite, Some(vec!["c"])),
        (Draft::Append, Some(vec!["a", "b", "c"])),
        (Draft::Off, Some(vec!["a", "b"])),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (mut s, t) = with_old_draft(dir.path(), draft);
        s.toggle_selected(t[2].clone());
        assert_eq!(s.take_draft_question(), None, "{draft:?} asked");
        let expected = expected.map(|v| v.into_iter().map(String::from).collect::<Vec<_>>());
        assert_eq!(playlist(&s, DRAFT), expected, "{draft:?}");
    }
}

#[test]
fn no_other_playlist_takes_the_draft_s_name() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, t) = session_with_files(dir.path());
    s.toggle_selected(t[0].clone());
    assert_eq!(
        s.save_selection("draft", false),
        Notice::Refused(Refusal::NameReserved("draft".into()))
    );
    s.save_selection("mine", false);
    let id = s.playlists().iter().find(|p| p.name == "mine").unwrap().id;
    assert_eq!(
        s.rename_playlist(id, "DRAFT"),
        Notice::Refused(Refusal::NameReserved("DRAFT".into()))
    );
}

#[test]
fn a_queue_not_kept_is_neither_stored_nor_offered() {
    let dir = tempfile::tempdir().unwrap();
    let (mut first, t) = session_with_files(dir.path());
    let path = PathBuf::from(&t[1].path);
    first.remember(&path, Duration::from_secs(1));
    db::set_resume_queue(
        &db::open(&dir.path().join("library.db")).unwrap(),
        &db::SavedQueue {
            played: vec![PathBuf::from(&t[0].path)],
            ..Default::default()
        },
        &path,
    )
    .unwrap();
    assert!(!first.resumable().unwrap().2.is_empty());
    // What was stored is forgotten once the queue is not kept.
    first.keep_queue(false);
    first.remember(&path, Duration::from_secs(1));
    drop(first);

    let (again, _) = session_with_files(dir.path());
    let (_, _, queue) = again.resumable().unwrap();
    assert!(queue.is_empty(), "{queue:?}");
}

/// The id of playlist `name`.
fn id_of(session: &Session, name: &str) -> i64 {
    session
        .playlists()
        .iter()
        .find(|p| p.name == name)
        .unwrap()
        .id
}

#[test]
fn an_edit_saves_over_its_playlist_without_asking_and_then_ends() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, t) = session_with_files(dir.path());
    s.set_selection(t[..2].to_vec());
    s.save_selection("mine", false);
    s.clear_selection();

    let mine = id_of(&s, "mine");
    assert_eq!(
        s.edit_playlist(mine),
        Some(Outcome::Editing {
            name: "mine".into()
        })
    );
    assert_eq!(paths(s.selection()), paths(&t[..2]));
    assert_eq!(s.editing().map(|p| p.name.as_str()), Some("mine"));
    s.remove_from_selection(0);
    s.toggle_selected(t[2].clone());
    assert!(matches!(s.save_selection("mine", false), Notice::Done(_)));
    assert_eq!(playlist(&s, "mine").unwrap(), ["b", "c"]);
    assert_eq!(s.editing(), None, "still editing once saved");

    // Another playlist of that name is still asked about, and clearing ends
    // an edit.
    s.edit_playlist(id_of(&s, "mine"));
    s.clear_selection();
    assert_eq!(s.editing(), None);
    s.toggle_selected(t[0].clone());
    assert_eq!(
        s.save_selection("mine", false),
        Notice::Refused(Refusal::WouldReplace("mine".into()))
    );
}

#[test]
fn an_edit_goes_on_in_a_later_session_when_its_draft_is_taken_up() {
    let dir = tempfile::tempdir().unwrap();
    let (mut first, t) = session_with_files(dir.path());
    first.set_selection(t[..2].to_vec());
    first.save_selection("mine", false);
    first.clear_selection();
    first.edit_playlist(id_of(&first, "mine"));
    first.toggle_selected(t[2].clone());
    drop(first);

    let (mut again, _) = session_with_files(dir.path());
    assert_eq!(again.editing(), None);
    let draft = id_of(&again, DRAFT);
    assert_eq!(again.edit_playlist(draft), Some(Outcome::DraftAppended));
    assert_eq!(paths(again.selection()), paths(&t));
    assert_eq!(again.editing().map(|p| p.name.as_str()), Some("mine"));
    assert_eq!(
        again.edit_playlist(draft),
        None,
        "the draft is the selection"
    );
    drop(again);

    // Overwritten, the old draft's edit is dropped with it.
    let (mut third, t) = session_with_files(dir.path());
    third.toggle_selected(t[0].clone());
    third.settle_draft(DraftChoice::Overwrite);
    drop(third);
    let (mut fourth, _) = session_with_files(dir.path());
    fourth.edit_playlist(id_of(&fourth, DRAFT));
    assert_eq!(fourth.editing(), None);
}

#[test]
fn a_saved_search_finds_what_is_there_now_in_the_order_it_was_saved_with() {
    use playr_core::columns::SortKey;
    use playr_core::db::query::Query;

    let (mut s, _, _dir) = session();
    assert_eq!(
        s.save_search("mine", false),
        Notice::Refused(Refusal::NoSearch)
    );
    s.set_shown(Some(Query::Text("path:/m/".into())));
    s.set_sort(vec![SortKey::named("title desc").unwrap()]);
    assert_eq!(
        s.save_search(" mine ", false),
        Notice::Done(Outcome::SearchSaved {
            name: "mine".into()
        })
    );
    // Sorted as saved, whatever the library's sort is now.
    s.set_sort(Vec::new());
    let search = s.searches()[0].clone();
    assert_eq!(
        paths(&s.run_search(&search)),
        ["/m/c.flac", "/m/b.flac", "/m/a.flac"]
    );
    // Names are shared with playlists, and the draft's is reserved.
    assert_eq!(
        s.save_search("mine", false),
        Notice::Refused(Refusal::WouldReplace("mine".into()))
    );
    assert!(matches!(s.save_search("mine", true), Notice::Done(_)));
    assert_eq!(
        s.save_search("late", false),
        Notice::Refused(Refusal::NameTaken("late".into()))
    );
    assert_eq!(
        s.save_search("draft", false),
        Notice::Refused(Refusal::NameReserved("draft".into()))
    );
    s.set_selection(vec![track("/m/a.flac", "A")]);
    assert_eq!(
        s.save_selection("mine", false),
        Notice::Refused(Refusal::NameTaken("mine".into()))
    );
    let late = id_of(&s, "late");
    assert_eq!(
        s.rename_playlist(late, "mine"),
        Notice::Refused(Refusal::NameTaken("mine".into()))
    );

    assert!(s.delete_search(search.id).is_some());
    assert!(s.searches().is_empty());
}
