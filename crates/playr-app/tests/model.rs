//! The shared model, driven with no drawing and no key events: what the
//! terminal does with a key, and a GUI with a control, arrives as these calls.

#[path = "../../playr-core/tests/common/mod.rs"]
mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use playr_app::action::Action;
use playr_app::command::CommandLine;
use playr_app::config::Config;
use playr_app::dispatch::{Confirm, Frontend};
use playr_app::message::Message;
use playr_app::model::{hold_peak, Input, Model, PEAK_HOLD};
use playr_app::View;
use playr_core::audio::Mode;
use playr_core::db::{self, query, Track};
use playr_core::notice::{Notice, Outcome, Task};

fn track(path: &str) -> Track {
    Track {
        path: path.into(),
        mtime: 1,
        size: 1,
        ..Default::default()
    }
}

/// A model over a library file of tracks a, b and c and playlists "early"
/// and "late", with its directory.
fn model() -> (Model, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = db::open(&dir.path().join("library.db")).unwrap();
    let ids: Vec<i64> = ["/m/a.flac", "/m/b.flac", "/m/c.flac"]
        .iter()
        .map(|p| db::upsert(&conn, &track(p)).unwrap())
        .collect();
    query::save_playlist(&mut conn, "late", &ids[..2]).unwrap();
    query::save_playlist(&mut conn, "early", &ids[2..]).unwrap();
    let player = common::fake_player().0;
    (Model::new(conn, player, Vec::new(), Config::default()), dir)
}

#[test]
fn tracks_handed_over_are_selected_played_and_shown() {
    let tracks = vec![track("/x/one.wav"), track("/x/two.wav")];
    let conn = db::open_memory().unwrap();
    let model = Model::new(
        conn,
        common::fake_player().0,
        tracks.clone(),
        Config::default(),
    );
    assert_eq!(model.view(), View::Selection);
    assert_eq!(model.session().selection(), &tracks[..]);
    assert_eq!(model.playing(), &tracks[..]);
    assert_eq!(model.cursors().selection, Some(0));
}

#[test]
fn a_question_waits_for_its_answer() {
    let (mut model, _dir) = model();
    model.perform(Action::ShowView(View::Playlists));
    model.perform(Action::DeletePlaylist);
    assert!(matches!(
        model.input(),
        Input::Confirm(Confirm::DeletePlaylist(p)) if p.name == "early"
    ));

    model.answer(false);
    assert_eq!(model.input(), &Input::None);
    assert_eq!(model.message(), Some(&Message::Cancelled));
    assert_eq!(model.session().playlists().len(), 2);

    model.perform(Action::DeletePlaylist);
    model.answer(true);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::Deleted {
            name: "early".into()
        })))
    );
    assert_eq!(model.session().playlists().len(), 1);

    // With no question open, an answer does nothing.
    model.answer(true);
    assert_eq!(model.session().playlists().len(), 1);
}

#[test]
fn command_lines_run_and_are_remembered_even_when_they_fail() {
    let (mut model, _dir) = model();
    model.set_input(Input::Command(CommandLine::default()));
    model.run_command("mode shuffle");
    assert_eq!(model.input(), &Input::None);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::Mode(Mode::Shuffle))))
    );
    model.run_command("frob");
    assert!(matches!(model.message(), Some(Message::Command(_))));
    model.run_command("   ");

    let mut line = CommandLine::default();
    line.recall(true, model.history());
    assert_eq!(line.text, "frob");
    line.recall(true, model.history());
    assert_eq!(line.text, "mode shuffle", "a blank line was remembered");
}

#[test]
fn a_search_shows_results_as_typed_and_ends_kept_or_cleared() {
    let (mut model, _dir) = model();
    model.search_as_typed("b".into());
    assert_eq!(model.input(), &Input::Search("b".into()));
    assert_eq!(model.results().map(<[Track]>::len), Some(1));

    model.end_search(true);
    assert_eq!(model.input(), &Input::None);
    assert_eq!(model.results().map(<[Track]>::len), Some(1));
    assert_eq!(model.message(), None);

    model.search_as_typed("zzz".into());
    model.end_search(true);
    assert_eq!(model.message(), Some(&Message::NoMatches));

    model.end_search(false);
    assert_eq!(model.results(), None);
    assert_eq!(model.listed().len(), 3);
}

/// Refreshes until the message showing matches `done`, or five seconds pass.
fn refresh_until(model: &mut Model, done: impl Fn(&Message) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        model.refresh();
        if model.message().is_some_and(&done) {
            return;
        }
        assert!(Instant::now() < deadline, "saw {:?}", model.message());
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Three short WAV files in `dir`.
fn music(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    for name in ["a.wav", "b.wav", "c.wav"] {
        common::silence(&dir.join(name), 8000, 0.05);
    }
}

#[test]
fn a_scan_fills_a_library_that_started_empty() {
    let dir = tempfile::tempdir().unwrap();
    let songs = dir.path().join("music");
    music(&songs);
    let conn = db::open_memory().unwrap();
    let mut model = Model::new(conn, common::fake_player().0, Vec::new(), Config::default());
    model
        .session_mut()
        .set_library_path(dir.path().join("library.db"));

    model.perform(Action::Scan(songs.clone()));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::ScanStarted {
            dir: songs.clone()
        })))
    );
    refresh_until(&mut model, |m| {
        matches!(m, Message::Core(Notice::Done(Outcome::Scanned { .. })))
    });
    assert_eq!(model.session().tracks().len(), 3);
    assert!(model.session().has_library_file());
    assert_eq!(
        model.cursors().library,
        Some(0),
        "no cursor on the new rows"
    );
}

#[test]
fn opened_files_are_added_to_the_selection_and_played() {
    let dir = tempfile::tempdir().unwrap();
    let songs = dir.path().join("music");
    music(&songs);
    let (mut model, _library) = model();
    model.perform(Action::Add);
    assert_eq!(model.session().selection().len(), 1);

    model.perform(Action::Open(vec![songs.clone()]));
    refresh_until(&mut model, |m| {
        matches!(m, Message::Core(Notice::Done(Outcome::Opened { .. })))
    });
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::Opened {
            tracks: 3,
            skipped: 0
        })))
    );
    assert_eq!(model.view(), View::Selection);
    assert_eq!(model.session().selection().len(), 4);
    assert_eq!(
        model.cursors().selection,
        Some(1),
        "not on the first opened"
    );
    let playing: Vec<&str> = model.playing().iter().map(|t| t.path.as_str()).collect();
    assert_eq!(playing.len(), 3);
    assert!(playing[0].ends_with("a.wav"), "{playing:?}");

    model.perform(Action::Open(vec![dir.path().join("gone")]));
    refresh_until(&mut model, |m| {
        matches!(m, Message::Core(Notice::Failed { .. }))
    });
    assert!(matches!(
        model.message(),
        Some(Message::Core(Notice::Failed {
            task: Task::Open,
            ..
        }))
    ));
    assert_eq!(
        model.session().selection().len(),
        4,
        "a failed open changed the selection"
    );
}

#[test]
fn a_peak_is_held_then_released() {
    let start = Instant::now();
    let held = hold_peak(None, 0.9, start);
    assert_eq!(held, Some((0.9, start)));
    // A lower reading inside the hold keeps the peak.
    let soon = start + PEAK_HOLD / 2;
    assert_eq!(hold_peak(held, 0.2, soon), held);
    // A higher one replaces it at once.
    assert_eq!(hold_peak(held, 0.95, soon), Some((0.95, soon)));
    // Once the hold has passed, the current reading shows.
    let later = start + PEAK_HOLD;
    assert_eq!(hold_peak(held, 0.2, later), Some((0.2, later)));
    assert_eq!(hold_peak(held, 0.0, later), None);
}
