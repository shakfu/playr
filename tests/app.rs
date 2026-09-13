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

/// An app over a library file whose queue holds two tracks, with one saved
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
    let queue: Vec<Track> = ["/m/a.flac", "/m/b.flac"]
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
    query::save_playlist(&mut conn, "late", &[queue[0].id]).unwrap();
    (App::with_queue(conn, player, queue), dir)
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
    // Queued by `playr <path>` from outside the library, so it has no row.
    let outside = Track {
        path: "/elsewhere/b.flac".into(),
        ..Default::default()
    };
    let mut app = App::with_queue(conn, common::fake_player().0, vec![known, outside]);

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
    // Both queued files are missing, so playing them fails twice at once.
    let (mut app, _dir) = app();
    std::thread::sleep(std::time::Duration::from_millis(300));
    app.refresh();
    let message = app.screen().message.map(str::to_string).unwrap_or_default();
    assert!(message.contains("b.flac"), "message was {message:?}");
    assert!(message.contains("(and 1 more)"), "message was {message:?}");
}

fn queue_paths(app: &mut App) -> Vec<String> {
    app.screen().queue.iter().map(|t| t.path.clone()).collect()
}

#[test]
fn the_queue_view_shows_the_players_queue_at_once() {
    let (mut app, _dir) = app();
    assert_eq!(queue_paths(&mut app), ["/m/a.flac", "/m/b.flac"]);

    press(&mut app, '1');
    press(&mut app, 'a');
    assert_eq!(
        queue_paths(&mut app),
        ["/m/a.flac", "/m/b.flac", "/m/a.flac"],
        "the queued track is not shown"
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
    let mut app = App::with_queue(conn, common::fake_player().0, vec![track]);

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
