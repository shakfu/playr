//! Key handling. `App` owns a `Player`, so these skip without an output device.
//! Nothing is played.

use playr::audio::Player;
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
fn app() -> Option<(App, tempfile::TempDir)> {
    app_with(|dir| db::open(&dir.join("library.db")).unwrap())
}

fn app_with(
    open: impl FnOnce(&std::path::Path) -> rusqlite::Connection,
) -> Option<(App, tempfile::TempDir)> {
    let player = match Player::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skipping: {e}");
            return None;
        }
    };
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
    Some((App::with_queue(conn, player, queue), dir))
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
    let Some((mut app, _dir)) = app() else { return };
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
    let Some((mut app, _dir)) = app() else { return };

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
    let Some((mut app, _dir)) = app() else { return };
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
    let Some((mut app, _dir)) = app_with(|_| db::open_memory().unwrap()) else {
        return;
    };
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
