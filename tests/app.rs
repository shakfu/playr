//! Key handling. `App` owns a `Player`, which plays to the fake device in `common`.

// The fake audio device and WAV writers live with playr-core's tests.
#[path = "../crates/playr-core/tests/common/mod.rs"]
mod common;

use playr::ui::{App, Input};
use playr_core::db::{self, query, Track};
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

#[test]
fn m_cycles_the_playback_mode_and_says_which() {
    use playr_core::audio::Mode;
    let (mut app, _dir) = app();
    let mode = |app: &mut App| {
        app.refresh();
        app.screen().snapshot.status.mode
    };
    // Quick presses, with no refresh between, must each take a step.
    press(&mut app, 'm');
    press(&mut app, 'm');
    // Before `refresh`, which reports the missing test files' errors over it.
    assert_eq!(app.screen().message, Some("mode: repeat"));
    assert_eq!(mode(&mut app), Mode::Repeat);
    press(&mut app, 'm');
    press(&mut app, 'm');
    assert_eq!(mode(&mut app), Mode::Normal, "the cycle does not wrap");
    press(&mut app, 'M');
    assert_eq!(mode(&mut app), Mode::RepeatOne);
}

fn typing(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, c);
    }
}

fn erase(app: &mut App, n: usize) {
    for _ in 0..n {
        key(app, KeyCode::Backspace, KeyModifiers::NONE);
    }
}

#[test]
fn r_renames_the_selected_playlist() {
    let (mut app, _dir) = app();
    press(&mut app, '3');
    press(&mut app, 'r');
    assert!(
        matches!(&app.screen().input, Input::RenamePlaylist { name, .. } if name == "late"),
        "the prompt did not start from the current name"
    );
    erase(&mut app, 4);
    typing(&mut app, "night");
    enter(&mut app);
    assert_eq!(playlist_lens(&mut app), [("night".to_string(), 1)]);
    assert_eq!(app.screen().message, Some("renamed \"late\" to \"night\""));
}

#[test]
fn renaming_refuses_a_taken_or_empty_name_and_follows_the_playlist() {
    let (mut app, _dir) = app();
    press(&mut app, 's');
    typing(&mut app, "early");
    enter(&mut app);
    press(&mut app, '3');
    press(&mut app, 'j');
    assert_eq!(
        app.screen().playlist_state.selected(),
        Some(1),
        "not on late"
    );

    press(&mut app, 'r');
    erase(&mut app, 4);
    typing(&mut app, "early");
    enter(&mut app);
    assert_eq!(
        app.screen().message,
        Some("a playlist named \"early\" already exists")
    );

    press(&mut app, 'r');
    erase(&mut app, 4);
    enter(&mut app);
    assert_eq!(app.screen().message, Some("playlist name cannot be empty"));

    press(&mut app, 'r');
    typing(&mut app, "x");
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(
        playlist_lens(&mut app),
        [("early".to_string(), 2), ("late".to_string(), 1)],
        "a refused or cancelled rename changed a playlist"
    );

    // Renamed to sort first, it moves up, and the cursor goes with it.
    press(&mut app, 'r');
    erase(&mut app, 4);
    typing(&mut app, "aaa");
    enter(&mut app);
    assert_eq!(
        playlist_lens(&mut app),
        [("aaa".to_string(), 1), ("early".to_string(), 2)]
    );
    assert_eq!(app.screen().playlist_state.selected(), Some(0));
}

/// Refreshes until `done` holds for the app's snapshot, or five seconds pass.
fn refresh_until(app: &mut App, done: impl Fn(&playr::ui::Snapshot) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        app.refresh();
        if done(app.screen().snapshot) || std::time::Instant::now() > deadline {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn b_marks_and_comma_and_period_seek_between_marks() {
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library.db");
    let conn = db::open(&library).unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 60.0);
    let path = file.to_string_lossy().into_owned();
    let track = Track {
        path: path.clone(),
        ..Default::default()
    };
    let mut app = App::with_selection(conn, common::fake_player().0, vec![track]);
    let seconds = |app: &mut App| {
        std::thread::sleep(Duration::from_millis(300));
        app.refresh();
        app.screen().snapshot.position.as_secs_f64()
    };
    refresh_until(&mut app, |s| s.position > Duration::from_millis(200));

    press(&mut app, 'b');
    assert_eq!(app.screen().message, Some("marked 0:00"));
    press(&mut app, 'b');
    assert_eq!(app.screen().message, Some("already marked at 0:00"));

    key(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
    assert!(seconds(&mut app) > 30.0);
    press(&mut app, 'b');
    assert_eq!(app.screen().message, Some("marked 0:30"));

    // Stored in the library, so another connection sees them.
    let other = db::open(&library).unwrap();
    assert_eq!(query::marks(&other, &path).unwrap().len(), 2);

    // Half a second past the 0:30 mark, within a second of it, so `,` skips it.
    std::thread::sleep(Duration::from_millis(500));
    press(&mut app, ',');
    let back = seconds(&mut app);
    assert!(back < 2.0, "comma went to {back}s, not the first mark");
    press(&mut app, '.');
    let forward = seconds(&mut app);
    assert!((30.0..31.5).contains(&forward), "period went to {forward}s");
    press(&mut app, '.');
    assert_eq!(app.screen().message, Some("no later mark"));

    // A mark added last but earlier in the track: `B` removes it, not 0:30.
    key(&mut app, KeyCode::Left, KeyModifiers::SHIFT);
    key(&mut app, KeyCode::Right, KeyModifiers::NONE);
    let _ = seconds(&mut app);
    press(&mut app, 'b');
    assert_eq!(app.screen().message, Some("marked 0:05"));
    press(&mut app, 'B');
    assert_eq!(app.screen().message, Some("removed mark at 0:05"));
    let left: Vec<u64> = query::marks(&other, &path)
        .unwrap()
        .iter()
        .map(|m| m.time().as_secs())
        .collect();
    assert_eq!(left, [0, 30], "undo removed the wrong mark");

    press(&mut app, 'C');
    assert!(confirming(&mut app), "clearing did not ask");
    press(&mut app, 'y');
    assert_eq!(app.screen().message, Some("marks cleared"));
    assert!(query::marks(&other, &path).unwrap().is_empty());
    app.refresh();
    assert!(app.screen().snapshot.marks.is_empty());
}

#[test]
fn marks_need_something_playing() {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("library.db")).unwrap();
    let mut app = App::new(conn, common::fake_player().0);
    for c in ['b', ',', '.', 'B', 'C'] {
        press(&mut app, c);
        assert_eq!(
            app.screen().message,
            Some("nothing is playing"),
            "after {c:?}"
        );
    }
}

/// Types `:line` and Enter.
fn command(app: &mut App, line: &str) {
    press(app, ':');
    typing(app, line);
    enter(app);
}

fn command_text(app: &mut App) -> Option<String> {
    match app.screen().input {
        Input::Command(line) => Some(line.text.clone()),
        _ => None,
    }
}

#[test]
fn colon_commands_do_what_their_keys_do() {
    use playr::ui::View;
    use playr_core::audio::Mode;
    let (mut app, _dir) = app();

    command(&mut app, "mode shuffle");
    assert_eq!(app.screen().message, Some("mode: shuffle"));
    app.refresh();
    assert_eq!(app.screen().snapshot.status.mode, Mode::Shuffle);

    command(&mut app, "vol 30");
    app.refresh();
    assert!((app.screen().snapshot.volume - 0.3).abs() < 1e-3);
    command(&mut app, "volume -10");
    app.refresh();
    assert!((app.screen().snapshot.volume - 0.2).abs() < 1e-3);

    command(&mut app, "save night");
    assert_eq!(
        playlist_lens(&mut app),
        [("late".to_string(), 1), ("night".to_string(), 2)]
    );
    command(&mut app, "save late");
    assert!(confirming(&mut app), "replacing by command did not ask");
    press(&mut app, 'n');

    command(&mut app, "rename dusk");
    assert_eq!(
        app.screen().message,
        Some(":rename works in the playlists view")
    );
    command(&mut app, "view playlists");
    assert_eq!(app.screen().view, View::Playlists);
    command(&mut app, "rename dusk");
    assert_eq!(
        playlist_lens(&mut app),
        [("dusk".to_string(), 1), ("night".to_string(), 2)]
    );

    command(&mut app, "playlist dusk");
    assert_eq!(app.screen().message, Some("playing \"dusk\""));
    assert_eq!(playing_paths(&mut app), ["/m/a.flac"]);
    command(&mut app, "playlist nope");
    assert_eq!(
        app.screen().message,
        Some("no single playlist named \"nope\"")
    );

    command(&mut app, "q");
    assert!(app.quitting());
}

#[test]
fn a_bad_command_says_why_and_closes_the_prompt() {
    let (mut app, _dir) = app();
    command(&mut app, "seek soon");
    assert_eq!(
        app.screen().message,
        Some("not a time: soon (try 1:23 or 90)")
    );
    assert!(command_text(&mut app).is_none());

    // Esc and deleting past the colon both close it without running anything.
    press(&mut app, ':');
    typing(&mut app, "q");
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(command_text(&mut app).is_none());
    press(&mut app, ':');
    typing(&mut app, "q");
    erase(&mut app, 1);
    assert_eq!(command_text(&mut app).as_deref(), Some(""));
    erase(&mut app, 1);
    assert!(command_text(&mut app).is_none());
    assert!(!app.quitting());
}

#[test]
fn keys_in_the_command_prompt_edit_it_rather_than_act() {
    use playr::ui::View;
    let (mut app, _dir) = app();
    let view = app.screen().view;
    press(&mut app, ':');
    typing(&mut app, "mo");
    key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(command_text(&mut app).as_deref(), Some("mode"));
    assert_eq!(app.screen().view, view, "tab switched view");
    typing(&mut app, " sh");
    key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(command_text(&mut app).as_deref(), Some("mode shuffle"));
    enter(&mut app);

    // Playlist names complete from the library.
    press(&mut app, ':');
    typing(&mut app, "playlist l");
    key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(command_text(&mut app).as_deref(), Some("playlist late"));
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    command(&mut app, "view library");
    command(&mut app, "view selection");
    assert_eq!(app.screen().view, View::Selection);

    // Up recalls; a cancelled line was not recorded.
    press(&mut app, ':');
    key(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(command_text(&mut app).as_deref(), Some("view selection"));
    key(&mut app, KeyCode::Up, KeyModifiers::NONE);
    key(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(command_text(&mut app).as_deref(), Some("mode shuffle"));
    key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(command_text(&mut app).as_deref(), Some("view library"));
    enter(&mut app);
    assert_eq!(app.screen().view, View::Library);
    assert_eq!(
        app.screen().selection_state.selected(),
        Some(0),
        "up or down moved the cursor"
    );
}

#[test]
fn help_command_lists_commands_and_the_next_key_only_closes_it() {
    let (mut app, _dir) = app();
    command(&mut app, "help");
    assert!(matches!(app.screen().input, Input::CommandHelp));
    press(&mut app, 'q');
    assert!(matches!(app.screen().input, Input::None));
    assert!(!app.quitting());
}

#[test]
fn seek_and_mark_commands_take_times() {
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library.db");
    let conn = db::open(&library).unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 60.0);
    let path = file.to_string_lossy().into_owned();
    let track = Track {
        path: path.clone(),
        ..Default::default()
    };
    let mut app = App::with_selection(conn, common::fake_player().0, vec![track]);
    refresh_until(&mut app, |s| s.position > Duration::from_millis(100));
    let seconds = |app: &mut App| {
        std::thread::sleep(Duration::from_millis(300));
        app.refresh();
        app.screen().snapshot.position.as_secs_f64()
    };

    command(&mut app, "seek 0:40");
    let at = seconds(&mut app);
    assert!((40.0..41.0).contains(&at), ":seek 0:40 went to {at}s");
    command(&mut app, "seek -30");
    let at = seconds(&mut app);
    assert!((10.0..11.5).contains(&at), ":seek -30 went to {at}s");

    command(&mut app, "mark 0:25");
    assert_eq!(app.screen().message, Some("marked 0:25"));
    let marks: Vec<u64> = query::marks(&db::open(&library).unwrap(), &path)
        .unwrap()
        .iter()
        .map(|m| m.time().as_secs())
        .collect();
    assert_eq!(marks, [25]);
}

#[test]
fn view_commands_act_on_the_view_they_belong_to() {
    use playr::ui::View;
    let (mut app, _dir) = app();
    assert_eq!(app.screen().view, View::Selection);

    // Selection: move, remove, clear.
    command(&mut app, "move +1");
    assert_eq!(selection_paths(&mut app), ["/m/b.flac", "/m/a.flac"]);
    command(&mut app, "first");
    command(&mut app, "remove");
    assert_eq!(selection_paths(&mut app), ["/m/a.flac"]);
    command(&mut app, "delete");
    assert_eq!(
        app.screen().message,
        Some(":delete works in the playlists view")
    );
    command(&mut app, "clear");
    assert!(confirming(&mut app), ":clear did not ask");
    press(&mut app, 'y');
    assert!(selection_paths(&mut app).is_empty());

    // Library: toggle, with the cursor moved by command.
    command(&mut app, "view library");
    command(&mut app, "last");
    command(&mut app, "toggle");
    assert_eq!(selection_paths(&mut app), ["/m/b.flac"]);
    command(&mut app, "up");
    command(&mut app, "toggle");
    assert_eq!(selection_paths(&mut app), ["/m/b.flac", "/m/a.flac"]);
    command(&mut app, "search zzz");
    assert!(app.screen().results.is_some());
    command(&mut app, "clear-search");
    assert!(app.screen().results.is_none());

    // Playlists: add, rename, delete.
    command(&mut app, "next-view");
    command(&mut app, "next-view");
    assert_eq!(app.screen().view, View::Playlists);
    command(&mut app, "clear");
    assert_eq!(
        app.screen().message,
        Some(":clear works in the selection view")
    );
    command(&mut app, "add");
    assert_eq!(app.screen().message, Some("already in selection"));
    command(&mut app, "delete");
    assert!(confirming(&mut app), ":delete did not ask");
    press(&mut app, 'y');
    assert!(playlist_lens(&mut app).is_empty());
}

#[test]
fn tab_completes_only_commands_that_work_in_this_view() {
    let (mut app, _dir) = app();
    press(&mut app, ':');
    typing(&mut app, "re");
    key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(command_text(&mut app).as_deref(), Some("remove"));
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    press(&mut app, '3');
    press(&mut app, ':');
    typing(&mut app, "re");
    key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(command_text(&mut app).as_deref(), Some("rename"));
}

#[test]
fn help_lists_scroll_with_j_and_k_and_other_keys_close_them() {
    let (mut app, _dir) = app();
    for (line, open) in [("help", "commands"), ("keys", "keys")] {
        command(&mut app, line);
        let showing = |app: &mut App| match app.screen().input {
            Input::CommandHelp => Some("commands"),
            Input::Help => Some("keys"),
            _ => None,
        };
        assert_eq!(showing(&mut app), Some(open));
        press(&mut app, 'j');
        key(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
        assert_eq!(*app.screen().help_scroll, 11);
        press(&mut app, 'k');
        key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(*app.screen().help_scroll, 9);
        assert_eq!(showing(&mut app), Some(open), "scrolling closed it");
        press(&mut app, 'x');
        assert_eq!(showing(&mut app), None);
    }
    // Reopening starts at the top.
    command(&mut app, "help");
    assert_eq!(*app.screen().help_scroll, 0);
}

#[test]
fn the_config_sets_keys_and_startup_before_anything_plays() {
    use playr::ui::config::Config;
    use playr_core::audio::Mode;
    let config =
        Config::parse("volume = 30\nmode = 'repeat'\n[keys.selection]\nctrl-x = 'remove'").unwrap();
    let tracks: Vec<Track> = ["/m/a.flac", "/m/b.flac"]
        .iter()
        .map(|p| Track {
            path: p.to_string(),
            ..Default::default()
        })
        .collect();
    let mut app = App::configured(
        db::open_memory().unwrap(),
        common::fake_player().0,
        tracks,
        config,
    );
    assert_eq!(
        app.screen().message,
        None,
        "a startup setting was announced"
    );
    app.refresh();
    assert!((app.screen().snapshot.volume - 0.3).abs() < 1e-3);
    assert_eq!(app.screen().snapshot.status.mode, Mode::Repeat);

    key(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL);
    assert_eq!(selection_paths(&mut app), ["/m/b.flac"]);
    // Ctrl-C quits whatever the key map says.
    chord(&mut app, KeyModifiers::CONTROL, 'c');
    assert!(app.quitting());
}

#[test]
fn map_and_unmap_change_keys_while_running() {
    let (mut app, _dir) = app();
    command(&mut app, "map ctrl-x clear");
    assert_eq!(
        app.screen().message,
        Some(":clear works in the selection view; use map selection ctrl-x clear")
    );
    command(&mut app, "map selection ctrl-x clear");
    assert_eq!(app.screen().message, Some("map selection ctrl-x clear"));
    key(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL);
    assert!(confirming(&mut app), "the mapped key did not run :clear");
    press(&mut app, 'n');

    command(&mut app, "map selection d nop");
    press(&mut app, 'd');
    assert_eq!(selection_paths(&mut app).len(), 2, "d still removed");

    // Unmapping falls back to a binding for all views, not to the default,
    // and no default binds d in all views.
    command(&mut app, "unmap selection d");
    assert_eq!(app.screen().message, Some("unmapped d"));
    press(&mut app, 'd');
    assert_eq!(
        selection_paths(&mut app).len(),
        2,
        "d fell back to a default"
    );
    command(&mut app, "map d first");
    command(&mut app, "last");
    press(&mut app, 'd');
    assert_eq!(
        app.screen().selection_state.selected(),
        Some(0),
        "no fallback to all views"
    );
    command(&mut app, "map selection d remove");
    press(&mut app, 'd');
    assert_eq!(
        selection_paths(&mut app).len(),
        1,
        "remapping did not restore d"
    );
    command(&mut app, "unmap selection d");
    command(&mut app, "unmap selection d");
    assert_eq!(
        app.screen().message,
        Some("d has no binding in the selection view")
    );
}

#[test]
fn export_commands_write_slices_of_the_playing_track_in_the_background() {
    use playr::ui::config::Config;
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 20.0);
    let samples = dir.path().join("samples");
    let settings = format!("samples = {:?}", samples.to_str().unwrap());
    let track = Track {
        path: file.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let mut app = App::configured(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track],
        Config::parse(&settings).unwrap(),
    );
    refresh_until(&mut app, |s| s.position > Duration::from_millis(100));

    // Marks at 5 s and 10 s, then back inside them.
    command(&mut app, "mark 0:05");
    command(&mut app, "mark 0:10");
    command(&mut app, "seek 0:07");
    std::thread::sleep(Duration::from_millis(300));
    command(&mut app, "slice region");
    assert_eq!(app.screen().message, Some("exporting"));
    let first = samples.join("long");
    assert_eq!(
        wait_for_export(&mut app),
        format!("exported 1 slice to {}", first.display())
    );
    let wav = first.join("000-long_S00.wav");
    assert_eq!(hound::WavReader::open(&wav).unwrap().duration(), 5 * 8000);

    command(&mut app, "slice marks");
    assert_eq!(
        wait_for_export(&mut app),
        format!("exported 3 slices to {}", samples.join("long-2").display())
    );

    command(&mut app, "stop");
    refresh_until(&mut app, |s| {
        s.status.state == playr_core::audio::State::Stopped
    });
    command(&mut app, "slice 4");
    assert_eq!(app.screen().message, Some("nothing is playing"));
}

#[test]
fn paths_under_home_are_shown_from_tilde() {
    use playr::ui::home_as_tilde;
    use std::path::{Path, PathBuf};
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    assert_eq!(
        home_as_tilde(&home.join("Music/playr/samples/amen")),
        "~/Music/playr/samples/amen"
    );
    assert_eq!(home_as_tilde(Path::new("/opt/cuts")), "/opt/cuts");
}

/// The message an export reports, once it has.
fn wait_for_export(app: &mut App) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        app.refresh();
        let message = app.screen().message.unwrap_or_default().to_string();
        if message.starts_with("export") && message != "exporting" {
            return message;
        }
        assert!(std::time::Instant::now() < deadline, "no report");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn slice_onsets_uses_the_setting_unless_given_a_sensitivity() {
    use playr::ui::config::Config;
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    // Noise at -40 dB with a hit 10 dB louder: found at sensitivity 1, not 0.
    let file = dir.path().join("soft.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&file, spec).unwrap();
    for i in 0..88_200 {
        let amp = if (20_000..24_000).contains(&i) {
            1036
        } else {
            328
        };
        w.write_sample(if i % 2 == 0 { amp } else { -amp } as i16)
            .unwrap();
    }
    w.finalize().unwrap();

    for (setting, typed, slices) in [
        (0.0, "slice onsets", "1 slice"),
        (0.0, "slice onsets 1", "2 slices"),
        (1.0, "slice onsets", "2 slices"),
        (1.0, "slice onsets 0", "1 slice"),
    ] {
        let settings = format!(
            "onset_sensitivity = {setting:.1}\nsamples = {:?}",
            dir.path().join("samples").to_str().unwrap()
        );
        let track = Track {
            path: file.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let mut app = App::configured(
            db::open_memory().unwrap(),
            common::fake_player().0,
            vec![track],
            Config::parse(&settings).unwrap(),
        );
        refresh_until(&mut app, |s| s.position > Duration::from_millis(50));
        command(&mut app, typed);
        let message = wait_for_export(&mut app);
        assert!(
            message.starts_with(&format!("exported {slices} to")),
            "{typed:?} with onset_sensitivity {setting}: {message}"
        );
    }
}

/// Refreshes until `done` holds for the sampler's state, or five seconds pass.
fn wait_for_sampler(app: &mut App, done: impl Fn(&playr::ui::sampler::Sampler) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        app.refresh();
        if done(app.screen().sampler) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "sampler never got there"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn wave_of(sampler: &playr::ui::sampler::Sampler) -> Option<(String, u64)> {
    match &sampler.wave {
        playr::ui::sampler::Wave::Ready { path, peaks } => Some((
            path.file_name().unwrap().to_string_lossy().into_owned(),
            peaks.frames,
        )),
        _ => None,
    }
}

#[test]
fn the_sampler_reads_the_waveform_once_opened_and_follows_the_track() {
    use playr::ui::View;
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let tracks: Vec<Track> = [("a.wav", 3.0), ("b.wav", 4.0)]
        .iter()
        .map(|(name, secs)| {
            let file = dir.path().join(name);
            common::silence(&file, 8000, *secs);
            Track {
                path: file.to_string_lossy().into_owned(),
                ..Default::default()
            }
        })
        .collect();
    let mut app = App::with_selection(db::open_memory().unwrap(), common::fake_player().0, tracks);
    refresh_until(&mut app, |s| s.position > Duration::from_millis(50));
    app.refresh();
    assert!(
        matches!(app.screen().sampler.wave, playr::ui::sampler::Wave::None),
        "read before the view opened"
    );

    press(&mut app, '4');
    assert_eq!(app.screen().view, View::Sampler);
    wait_for_sampler(&mut app, |s| wave_of(s).is_some());
    assert_eq!(
        wave_of(app.screen().sampler),
        Some(("a.wav".into(), 24_000))
    );

    press(&mut app, 'n');
    wait_for_sampler(&mut app, |s| {
        wave_of(s).is_some_and(|(name, _)| name == "b.wav")
    });
    assert_eq!(
        wave_of(app.screen().sampler),
        Some(("b.wav".into(), 32_000))
    );
}

#[test]
fn zoom_and_display_keys_work_only_in_the_sampler() {
    use playr::ui::sampler::Display;
    let (mut app, _dir) = app();
    press(&mut app, 'z');
    assert_eq!(app.screen().sampler.zoom, 0, "z zoomed outside the sampler");

    press(&mut app, '4');
    press(&mut app, 'z');
    press(&mut app, 'z');
    assert_eq!(app.screen().sampler.zoom, 2);
    press(&mut app, 'Z');
    assert_eq!(app.screen().sampler.zoom, 1);
    press(&mut app, '0');
    assert_eq!(app.screen().sampler.zoom, 0);
    press(&mut app, 'Z');
    assert_eq!(app.screen().sampler.zoom, 0);

    press(&mut app, 'w');
    assert_eq!(app.screen().sampler.display, Display::Decibels);
    assert_eq!(app.screen().message, Some("display: db"));
    press(&mut app, 'w');
    assert_eq!(app.screen().sampler.display, Display::Braille);
    assert_eq!(app.screen().message, Some("display: braille"));
    command(&mut app, "display db");
    assert_eq!(app.screen().sampler.display, Display::Decibels);
    press(&mut app, 'w');
    press(&mut app, 'w');
    command(&mut app, "display envelope");
    assert_eq!(app.screen().sampler.display, Display::Envelope);
    command(&mut app, "zoom all");
    press(&mut app, '1');
    command(&mut app, "zoom +");
    assert_eq!(
        app.screen().message,
        Some(":zoom works in the sampler view")
    );
}

#[test]
fn in_the_sampler_slices_are_planned_then_written_or_discarded() {
    use playr::ui::config::Config;
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let samples = dir.path().join("samples");
    let tracks: Vec<Track> = ["long.wav", "next.wav"]
        .iter()
        .map(|name| {
            let file = dir.path().join(name);
            common::silence(&file, 8000, 20.0);
            Track {
                path: file.to_string_lossy().into_owned(),
                ..Default::default()
            }
        })
        .collect();
    let settings = format!("samples = {:?}", samples.to_str().unwrap());
    let mut app = App::configured(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        tracks,
        Config::parse(&settings).unwrap(),
    );
    refresh_until(&mut app, |s| s.position > Duration::from_millis(50));
    command(&mut app, "mark 0:05");
    command(&mut app, "mark 0:10");
    command(&mut app, "seek 0:07");
    refresh_until(&mut app, |s| s.position > Duration::from_secs(6));
    press(&mut app, '4');
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        app.screen().message,
        Some("no slices planned; :slice plans them")
    );

    command(&mut app, "slice 4");
    assert_eq!(app.screen().message, Some("planning slices"));
    wait_for_sampler(&mut app, |s| s.pending.is_some());
    let spans = app.screen().sampler.pending.as_ref().unwrap().spans.clone();
    assert_eq!(
        spans,
        [
            (40_000, Some(50_000)),
            (50_000, Some(60_000)),
            (60_000, Some(70_000)),
            (70_000, Some(80_000))
        ]
    );
    assert_eq!(
        app.screen().message,
        Some("4 slices planned: enter writes, esc discards")
    );
    assert!(!samples.exists(), "planning wrote files");

    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.screen().sampler.pending.is_none());
    let first = samples.join("long");
    assert_eq!(
        wait_for_export(&mut app),
        format!("exported 4 slices to {}", first.display())
    );
    let lengths: Vec<u32> = (0..4)
        .map(|i| {
            let file = first.join(format!("{i:03}-long_S{i:02}.wav"));
            hound::WavReader::open(file).unwrap().duration()
        })
        .collect();
    assert_eq!(lengths, [10_000; 4]);

    command(&mut app, "slice marks");
    wait_for_sampler(&mut app, |s| s.pending.is_some());
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.screen().message, Some("slices discarded"));
    assert!(
        !samples.join("long-2").exists(),
        "discarded slices were written"
    );

    // A plan belongs to its track.
    command(&mut app, "slice 2");
    wait_for_sampler(&mut app, |s| s.pending.is_some());
    press(&mut app, 'n');
    wait_for_sampler(&mut app, |s| s.pending.is_none());

    // Outside the sampler, :slice writes at once, as before.
    press(&mut app, '1');
    command(&mut app, "slice region");
    assert!(wait_for_export(&mut app).starts_with("exported 1 slice to"));
}
