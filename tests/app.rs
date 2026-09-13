//! Key handling. `App` owns a `Player`, which plays to the fake device in `common`.

mod common;

use playr::db::{self, query, Track};
use playr::ui::{App, Input};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn press(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn enter(app: &mut App) {
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
}

/// An app over a library file whose selection holds two tracks, with one saved
/// playlist "late" of one track. The directory holds the file.
fn app() -> (App, tempfile::TempDir) {
    app_with(|dir| db::open(&dir.join("library.db")).unwrap())
}

fn app_with(
    open: impl FnOnce(&std::path::Path) -> rusqlite::Connection,
) -> (App, tempfile::TempDir) {
    let player = common::fake_player().0;
    let dir = tempfile::tempdir().unwrap();
    let mut conn = open(dir.path());
    let tracks: Vec<Track> = ["/m/a.flac", "/m/b.flac"]
        .iter()
        .map(|path| {
            let mut t = Track {
                path: path.to_string(),
                mtime: 1,
                size: 1,
                ..Default::default()
            };
            t.id = db::upsert(&conn, &t).unwrap();
            t
        })
        .collect();
    query::save_playlist(&mut conn, "late", &[tracks[0].id]).unwrap();
    (App::with_selection(conn, player, tracks), dir)
}

fn playlist_lens(app: &mut App) -> Vec<(String, i64)> {
    app.screen()
        .playlists
        .iter()
        .map(|p| (p.name.clone(), p.len))
        .collect()
}

fn confirming(app: &mut App) -> bool {
    matches!(app.screen().input, Input::Confirm(_))
}

#[test]
fn deleting_a_playlist_waits_for_y() {
    let (mut app, _dir) = app();
    press(&mut app, '3');

    press(&mut app, 'd');
    assert!(confirming(&mut app), "delete did not ask");
    press(&mut app, 'n');
    assert!(!confirming(&mut app));
    assert_eq!(playlist_lens(&mut app).len(), 1, "deleted without a y");

    press(&mut app, 'd');
    press(&mut app, 'y');
    assert!(playlist_lens(&mut app).is_empty(), "y did not delete");
}

#[test]
fn saving_over_an_existing_playlist_waits_for_y() {
    let (mut app, _dir) = app();

    press(&mut app, 's');
    for c in "late".chars() {
        press(&mut app, c);
    }
    enter(&mut app);
    assert!(confirming(&mut app), "overwrite did not ask");
    press(&mut app, 'q');
    assert_eq!(
        playlist_lens(&mut app),
        [("late".to_string(), 1)],
        "replaced without a y"
    );

    press(&mut app, 's');
    for c in "late".chars() {
        press(&mut app, c);
    }
    enter(&mut app);
    press(&mut app, 'y');
    assert_eq!(playlist_lens(&mut app), [("late".to_string(), 2)]);
}

#[test]
fn saving_under_a_new_name_does_not_ask() {
    let (mut app, _dir) = app();
    press(&mut app, 's');
    for c in "early".chars() {
        press(&mut app, c);
    }
    enter(&mut app);
    assert!(!confirming(&mut app));
    assert_eq!(
        playlist_lens(&mut app),
        [("early".to_string(), 2), ("late".to_string(), 1)]
    );
}

#[test]
fn saving_without_a_library_file_is_refused() {
    // `playr <path>` runs on an in-memory library; a playlist saved there
    // would be lost on exit.
    let (mut app, _dir) = app_with(|_| db::open_memory().unwrap());
    press(&mut app, 's');
    assert!(
        !matches!(app.screen().input, Input::SavePlaylist(_)),
        "the save prompt opened"
    );
    let message = app.screen().message.map(str::to_string);
    assert!(
        message.as_deref().is_some_and(|m| m.contains("playr scan")),
        "no explanation: {message:?}"
    );
}

fn chord(app: &mut App, modifiers: KeyModifiers, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), modifiers));
}

#[test]
fn ctrl_c_in_a_text_prompt_quits_instead_of_typing() {
    for open in ['/', 's'] {
        let (mut app, _dir) = app();
        press(&mut app, open);
        press(&mut app, 'a');
        chord(&mut app, KeyModifiers::CONTROL, 'c');
        assert!(app.quitting(), "ctrl-c after {open:?} did not quit");
        let text = match app.screen().input {
            Input::Search(t) | Input::SavePlaylist(t) => t.clone(),
            _ => panic!("the prompt after {open:?} closed"),
        };
        assert_eq!(text, "a", "ctrl-c after {open:?} was typed");
    }
}

#[test]
fn chords_are_not_typed_into_a_prompt() {
    let (mut app, _dir) = app();
    press(&mut app, 's');
    chord(&mut app, KeyModifiers::CONTROL, 'w');
    chord(&mut app, KeyModifiers::ALT, 'x');
    press(&mut app, 'b');
    assert!(matches!(app.screen().input, Input::SavePlaylist(t) if t == "b"));
}

#[test]
fn a_ctrl_y_does_not_confirm() {
    let (mut app, _dir) = app();
    press(&mut app, '3');
    press(&mut app, 'd');
    chord(&mut app, KeyModifiers::CONTROL, 'y');
    assert_eq!(
        playlist_lens(&mut app).len(),
        1,
        "ctrl-y deleted the playlist"
    );
}

#[test]
fn question_mark_opens_help_and_the_next_key_only_closes_it() {
    let (mut app, _dir) = app();
    press(&mut app, '?');
    assert!(
        matches!(app.screen().input, Input::Help),
        "help did not open"
    );
    press(&mut app, 'q');
    assert!(
        matches!(app.screen().input, Input::None),
        "help did not close"
    );
    assert!(!app.quitting(), "the closing key also quit");
}

#[test]
fn saving_says_how_many_tracks_were_left_out() {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("library.db")).unwrap();
    let mut known = Track {
        path: "/m/a.flac".into(),
        mtime: 1,
        size: 1,
        ..Default::default()
    };
    known.id = db::upsert(&conn, &known).unwrap();
    // Selected by `playr <path>` from outside the library, so it has no row.
    let outside = Track {
        path: "/elsewhere/b.flac".into(),
        ..Default::default()
    };
    let mut app = App::with_selection(conn, common::fake_player().0, vec![known, outside]);

    press(&mut app, 's');
    for c in "mix".chars() {
        press(&mut app, c);
    }
    enter(&mut app);
    let message = app.screen().message.map(str::to_string).unwrap_or_default();
    assert!(
        message.contains("1 tracks, 1 not in the library left out"),
        "message was {message:?}"
    );
}

#[test]
fn errors_between_frames_are_counted() {
    // Both selected files are missing, so playing them fails twice at once.
    let (mut app, _dir) = app();
    std::thread::sleep(std::time::Duration::from_millis(300));
    app.refresh();
    let message = app.screen().message.map(str::to_string).unwrap_or_default();
    assert!(message.contains("b.flac"), "message was {message:?}");
    assert!(message.contains("(and 1 more)"), "message was {message:?}");
}

fn selection_paths(app: &mut App) -> Vec<String> {
    app.screen()
        .selection
        .iter()
        .map(|t| t.path.clone())
        .collect()
}

fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.on_key(KeyEvent::new(code, modifiers));
}

/// The player's list, as the app last sampled it.
fn playing_paths(app: &mut App) -> Vec<String> {
    app.refresh();
    let queue = app.screen().snapshot.status.queue.clone();
    queue
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn the_selection_starts_empty_and_playing_does_not_fill_it() {
    let (mut app, _dir) = app_with(|dir| db::open(&dir.join("library.db")).unwrap());
    // `app_with` selects two tracks, as `playr <path>` does; start over empty.
    press(&mut app, '2');
    press(&mut app, 'c');
    press(&mut app, 'y');
    assert!(selection_paths(&mut app).is_empty());

    press(&mut app, '1');
    enter(&mut app);
    assert_eq!(playing_paths(&mut app), ["/m/a.flac", "/m/b.flac"]);
    assert!(
        selection_paths(&mut app).is_empty(),
        "playing the library filled the selection"
    );

    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("library.db")).unwrap();
    let mut fresh = App::new(conn, common::fake_player().0);
    assert!(
        selection_paths(&mut fresh).is_empty(),
        "a new app has a selection"
    );
}

#[test]
fn adding_to_the_selection_leaves_playback_alone() {
    let (mut app, _dir) = app();
    press(&mut app, '2');
    press(&mut app, 'j');
    press(&mut app, 'd');
    press(&mut app, '1');
    enter(&mut app);
    let before = playing_paths(&mut app);

    press(&mut app, 'j');
    press(&mut app, 'a');
    assert_eq!(
        selection_paths(&mut app),
        ["/m/a.flac", "/m/b.flac"],
        "the track was not added"
    );
    assert_eq!(playing_paths(&mut app), before, "adding changed what plays");
}

#[test]
fn selected_tracks_can_be_moved_removed_and_cleared() {
    let (mut app, _dir) = app();
    press(&mut app, '2');

    // The cursor starts on a; shift moves the track with it.
    press(&mut app, 'J');
    assert_eq!(selection_paths(&mut app), ["/m/b.flac", "/m/a.flac"]);
    key(&mut app, KeyCode::Up, KeyModifiers::SHIFT);
    assert_eq!(selection_paths(&mut app), ["/m/a.flac", "/m/b.flac"]);

    press(&mut app, 'd');
    assert_eq!(selection_paths(&mut app), ["/m/b.flac"]);

    press(&mut app, 'c');
    assert!(confirming(&mut app), "clearing did not ask");
    press(&mut app, 'n');
    assert_eq!(
        selection_paths(&mut app),
        ["/m/b.flac"],
        "cleared without a y"
    );
    press(&mut app, 'c');
    press(&mut app, 'y');
    assert!(selection_paths(&mut app).is_empty());
}

#[test]
fn enter_in_the_selection_plays_it() {
    let (mut app, _dir) = app();
    press(&mut app, '2');
    press(&mut app, 'J');
    enter(&mut app);
    assert_eq!(playing_paths(&mut app), ["/m/b.flac", "/m/a.flac"]);
}

#[test]
fn a_on_a_playlist_adds_its_tracks_not_already_selected() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = db::open(&dir.path().join("library.db")).unwrap();
    let id = |conn: &rusqlite::Connection, path: &str| {
        let t = Track {
            path: path.into(),
            mtime: 1,
            size: 1,
            ..Default::default()
        };
        db::upsert(conn, &t).unwrap()
    };
    let (a, b) = (id(&conn, "/m/a.flac"), id(&conn, "/m/b.flac"));
    query::save_playlist(&mut conn, "motif", &[a, b, a]).unwrap();
    let mut app = App::with_selection(conn, common::fake_player().0, Vec::new());

    // Into an empty selection the playlist's own repeat survives.
    press(&mut app, '3');
    press(&mut app, 'a');
    assert_eq!(
        selection_paths(&mut app),
        ["/m/a.flac", "/m/b.flac", "/m/a.flac"]
    );
    // Added again, every track is already there.
    press(&mut app, 'a');
    assert_eq!(app.screen().message, Some("already in selection"));
    assert_eq!(selection_paths(&mut app).len(), 3);

    // Into a selection holding b, only the tracks it lacks are added.
    press(&mut app, '2');
    press(&mut app, 'c');
    press(&mut app, 'y');
    press(&mut app, '1');
    press(&mut app, 'j');
    press(&mut app, 'a');
    press(&mut app, '3');
    press(&mut app, 'a');
    assert_eq!(
        selection_paths(&mut app),
        ["/m/b.flac", "/m/a.flac", "/m/a.flac"]
    );
}

#[test]
fn shift_arrows_seek_further_than_arrows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.wav");
    common::silence(&path, 8000, 60.0);
    let track = Track {
        path: path.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let conn = db::open_memory().unwrap();
    let mut app = App::with_selection(conn, common::fake_player().0, vec![track]);

    let position_after = |app: &mut App, code: KeyCode, modifiers: KeyModifiers| {
        app.on_key(KeyEvent::new(code, modifiers));
        std::thread::sleep(std::time::Duration::from_millis(300));
        app.refresh();
        app.screen().snapshot.position.as_secs_f64()
    };
    let after_right = position_after(&mut app, KeyCode::Right, KeyModifiers::NONE);
    assert!((5.0..6.0).contains(&after_right), "right: {after_right}");
    let after_jump = position_after(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
    assert!(
        (35.0..36.5).contains(&after_jump),
        "shift-right: {after_jump}"
    );
    let after_back = position_after(&mut app, KeyCode::Left, KeyModifiers::SHIFT);
    assert!((5.0..7.0).contains(&after_back), "shift-left: {after_back}");
}

#[test]
fn quick_volume_presses_all_count() {
    // Between frames the snapshot is stale, so each press must start from
    // the volume the one before it set.
    let (mut app, _dir) = app();
    for _ in 0..10 {
        press(&mut app, '-');
    }
    app.refresh();
    let volume = app.screen().snapshot.volume;
    assert!(
        (volume - 0.5).abs() < 1e-3,
        "volume {volume} after ten steps down"
    );
}

#[test]
fn a_toggles_a_track_and_moves_the_cursor_down() {
    let (mut app, _dir) = app();
    press(&mut app, '2');
    press(&mut app, 'c');
    press(&mut app, 'y');

    press(&mut app, '1');
    press(&mut app, 'a');
    assert_eq!(app.screen().message, Some("added to selection"));
    press(&mut app, 'a');
    assert_eq!(selection_paths(&mut app), ["/m/a.flac", "/m/b.flac"]);

    // On the last row the cursor stays, so a second press unselects b.
    press(&mut app, 'a');
    assert_eq!(app.screen().message, Some("removed from selection"));
    assert_eq!(selection_paths(&mut app), ["/m/a.flac"]);

    // Back on a: unselect it, and the cursor moves on to b, which is selected again.
    press(&mut app, 'k');
    press(&mut app, 'a');
    assert!(selection_paths(&mut app).is_empty());
    press(&mut app, 'a');
    assert_eq!(selection_paths(&mut app), ["/m/b.flac"]);
}
