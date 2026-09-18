//! What the page is sent and what it may do, read off a model with a library.

#[path = "../../playr-core/tests/common/mod.rs"]
mod common;

use playr_app::action::Action;
use playr_app::config::Config;
use playr_app::dispatch::Frontend;
use playr_app::model::Model;
use playr_app::{Theme, View};
use playr_core::audio::Mode;
use playr_core::db::{self, query, Track};
use playr_server::web::{self, allowed};
use serde_json::{json, Value};

/// A model over a library file of tracks a and b, and playlist "late" of both.
fn model() -> (Model, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = db::open(&dir.path().join("library.db")).unwrap();
    let ids: Vec<i64> = [("/m/a.flac", "Evans"), ("/m/b.flac", "Monk")]
        .iter()
        .map(|(path, artist)| {
            let track = Track {
                path: path.to_string(),
                title: Some(format!("{artist} tune")),
                artist: Some(artist.to_string()),
                duration_ms: Some(83_000),
                mtime: 1,
                size: 1,
                ..Default::default()
            };
            db::upsert(&conn, &track).unwrap()
        })
        .collect();
    query::save_playlist(&mut conn, "late", &ids).unwrap();
    let player = common::fake_player().0;
    (Model::new(conn, player, Vec::new(), Config::default()), dir)
}

#[test]
fn the_page_may_do_what_the_window_does_but_the_sampler_and_paths() {
    use Action::*;
    for action in [
        TogglePause,
        SetSpeed(-2),
        Mark,
        ClearMarks,
        Activate,
        Add,
        SaveAs("x".into()),
        DeletePlaylist,
        StartCommand,
        Theme(playr_app::Theme::Light),
        ShowView(View::Playlists),
        Rescan,
        ShowRoots,
        Prune(None),
    ] {
        assert!(allowed(&action), "{action:?}");
    }
    for action in [
        Quit,
        Scan("/".into()),
        Open(vec!["/".into()]),
        Prune(Some("/".into())),
        ForgetRoot("/".into()),
        ShowView(View::Sampler),
        Slice(playr_app::action::Slicing::Marks),
        WriteSlices,
        Loop(None),
    ] {
        assert!(!allowed(&action), "{action:?}");
    }
}

#[test]
fn keys_bound_per_view_with_refused_ones_taken_and_ignored() {
    let (model, _dir) = model();
    let keys = web::keys(&model);
    assert_eq!(keys["library"][":"], "command");
    assert_eq!(keys["library"]["a"], "toggle");
    assert_eq!(keys["playlists"]["a"], "add");
    assert_eq!(keys["selection"]["shift-down"], "move +1");
    assert_eq!(keys["library"]["shift-down"], "down");
    assert_eq!(keys["library"]["q"], Value::Null);
    assert_eq!(keys["library"]["4"], Value::Null);
    assert!(keys.get("sampler").is_none());
}

#[test]
fn help_lists_only_what_the_page_may_do() {
    let (model, _dir) = model();
    let rows = |commands| -> Vec<String> {
        web::help(&model, commands)
            .as_array()
            .unwrap()
            .iter()
            .map(|r| format!("{} {}", r[0].as_str().unwrap(), r[1].as_str().unwrap()))
            .collect()
    };
    let keys = rows(false);
    assert!(keys.contains(&"a :toggle".to_string()), "{keys:?}");
    assert!(keys.contains(&": :command".to_string()), "{keys:?}");
    assert!(!keys
        .iter()
        .any(|r| r.contains(":quit") || r.contains("sampler")));
    let commands = rows(true);
    assert!(commands.iter().any(|r| r.starts_with(":delete")));
    for refused in [
        ":quit", ":scan", ":open", ":slice", ":zoom", ":loop", ":map",
    ] {
        assert!(
            !commands.iter().any(|r| r.starts_with(refused)),
            "{refused} in {commands:?}"
        );
    }
    assert!(
        commands.iter().any(|r| r.starts_with(":prune")),
        "bare prune missing from {commands:?}"
    );
    assert!(!commands.iter().any(|r| r.trim() == "sampler"));
}

#[test]
fn rows_carry_what_each_view_draws() {
    let (mut model, _dir) = model();
    model.set_cursor(View::Library, Some(1));
    model.perform(Action::Add);
    let library = web::rows(&model, View::Library, 0, 10);
    assert_eq!(library["total"], 2);
    assert_eq!(
        library["rows"][1],
        json!({
            "key": "/m/b.flac", "title": "Monk tune", "artist": "Monk",
            "album": "Unknown Album", "time": "1:23", "selected": true,
        })
    );
    assert_eq!(library["rows"][0]["selected"], false);
    let selection = web::rows(&model, View::Selection, 0, 10);
    assert_eq!(selection["rows"][0]["key"], "/m/b.flac");
    let playlists = web::rows(&model, View::Playlists, 0, 10);
    let id = model.session().playlists()[0].id.to_string();
    assert_eq!(
        playlists["rows"],
        json!([{ "key": id, "name": "late", "tracks": 2 }])
    );
    assert_eq!(
        web::rows(&model, View::Library, 1, 10)["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(web::rows(&model, View::Library, 9, 10)["rows"], json!([]));
}

#[test]
fn the_screen_has_views_counts_input_and_playback() {
    let (mut model, _dir) = model();
    model.perform(Action::SetMode(Mode::RepeatOne));
    model.perform(Action::Theme(Theme::Light));
    model.perform(Action::ShowView(View::Playlists));
    model.perform(Action::DeletePlaylist);
    model.refresh();
    let s = web::screen(&model);
    assert_eq!(s["view"], "playlists");
    assert_eq!(
        s["counts"],
        json!({ "library": 2, "selection": 0, "playlists": 1 })
    );
    assert_eq!(
        s["input"],
        json!({ "kind": "confirm", "question": "delete playlist \"late\"?" })
    );
    assert_eq!(s["theme"], "light");
    // The name `:mode` takes, so the page can send it back.
    assert_eq!(s["mode"], "repeat-one");
    assert_eq!(
        (s["state"].as_str(), s["title"].as_str()),
        (Some("stopped"), None)
    );
    assert_eq!(s["marks"], json!([]));
    assert_eq!(s["volume"], 100.0);
}

#[test]
fn the_lists_revision_changes_with_what_the_lists_hold() {
    let (mut model, _dir) = model();
    let revision = |m: &Model| web::screen(m)["lists"].clone();
    let before = revision(&model);
    model.refresh();
    assert_eq!(revision(&model), before);
    model.perform(Action::Add);
    let selected = revision(&model);
    assert_ne!(selected, before);
    model.perform(Action::Search("evans".into()));
    assert_ne!(revision(&model), selected);
}

#[test]
fn a_command_line_is_checked_in_the_view_shown() {
    let (mut model, _dir) = model();
    assert_eq!(web::check(&model, "pause"), Ok(()));
    assert_eq!(web::check(&model, "quit").unwrap_err().0, 403);
    assert_eq!(web::check(&model, "delete").unwrap_err().0, 400);
    model.perform(Action::ShowView(View::Playlists));
    assert_eq!(web::check(&model, "delete"), Ok(()));
    assert_eq!(web::view_named("sampler"), None);
    assert_eq!(web::view_named("selection"), Some(View::Selection));
}
