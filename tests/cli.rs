//! Command-line behaviour, run against the built binary.
//!
//! No command here reaches playback: the files are not audio, and without a
//! terminal the interface refuses to start.

use std::path::Path;
use std::process::{Command, Stdio};

fn playr(db: &Path, args: &[&str]) {
    Command::new(env!("CARGO_BIN_EXE_playr"))
        // Not the config of whoever runs the tests.
        .env("XDG_CONFIG_HOME", db.parent().unwrap())
        .arg("--db")
        .arg(db)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
}

#[test]
fn only_scan_creates_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("playr/library.db");
    let file = dir.path().join("x.mp3");
    std::fs::write(&file, b"x").unwrap();

    for args in [
        vec![file.to_str().unwrap()],
        vec![],
        vec!["search", "anything"],
        vec!["playlist", "late"],
        vec!["playlists"],
    ] {
        playr(&db, &args);
        assert!(
            !db.exists(),
            "`playr {}` created the library",
            args.join(" ")
        );
    }

    playr(&db, &["scan", dir.path().to_str().unwrap()]);
    assert!(db.exists(), "`playr scan` did not create the library");
}

/// Runs playr on the library `db`, with no terminal and no user settings,
/// returning its exit code, stdout and stderr.
fn output(db: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_playr"))
        .env("XDG_CONFIG_HOME", db.parent().unwrap())
        .arg("--db")
        .arg(db)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn bad_arguments_are_refused_before_anything_runs() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.db");
    for (args, error) in [
        (&["--bogus"][..], "unexpected argument '--bogus'"),
        (&["playlists", "--bogus"], "unexpected argument '--bogus'"),
        (&["formats", "extra"], "unexpected argument 'extra'"),
        (&["search", "rock", "--db"], "a value is required for '--db"),
        (&["search"], "<QUERY>"),
        (&["scan"], "<DIR>"),
    ] {
        let (code, _, stderr) = output(&db, args);
        assert_eq!(code, Some(2), "playr {args:?}: {stderr}");
        assert!(stderr.contains(error), "playr {args:?}: {stderr}");
    }
    assert!(!db.exists());
}

#[test]
fn each_subcommand_has_its_own_help() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.db");
    for (command, text) in [("scan", "Usage: playr scan"), ("search", "--json")] {
        let (code, stdout, _) = output(&db, &[command, "--help"]);
        assert_eq!(code, Some(0));
        assert!(stdout.contains(text), "playr {command} --help: {stdout}");
    }
    // Scanning `--help` would have created the library.
    assert!(!db.exists(), "playr scan --help scanned");
}

#[test]
fn search_json_prints_every_match_with_its_columns() {
    use playr_core::db::{self, Track};
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("library.db");
    let conn = db::open(&db_path).unwrap();
    for track in [
        Track {
            path: "/m/peace piece.flac".into(),
            title: Some("Peace Piece".into()),
            artist: Some("Bill Evans".into()),
            track_no: Some(3),
            duration_ms: Some(393_000),
            mtime: 1,
            size: 2,
            ..Default::default()
        },
        Track {
            path: "/m/evans-untagged.wav".into(),
            mtime: 1,
            size: 2,
            ..Default::default()
        },
        Track {
            path: "/m/other.flac".into(),
            title: Some("Other".into()),
            mtime: 1,
            size: 2,
            ..Default::default()
        },
    ] {
        db::upsert(&conn, &track).unwrap();
    }
    drop(conn);

    let (code, stdout, stderr) = output(&db_path, &["search", "--json", "evans"]);
    assert_eq!(code, Some(0), "{stderr}");
    let rows: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let rows = rows.as_array().unwrap();
    let paths: Vec<&str> = rows.iter().map(|r| r["path"].as_str().unwrap()).collect();
    assert_eq!(paths.len(), 2, "{stdout}");
    assert!(paths.contains(&"/m/evans-untagged.wav"), "{stdout}");
    let piece = rows
        .iter()
        .find(|r| r["path"] == "/m/peace piece.flac")
        .unwrap();
    assert_eq!(piece["title"], "Peace Piece");
    assert_eq!(piece["artist"], "Bill Evans");
    assert_eq!(piece["track_no"], 3);
    assert_eq!(piece["duration_ms"], 393_000);
    assert!(piece["album"].is_null(), "{piece}");
    assert!(piece["id"].as_i64().unwrap() > 0);
    assert_eq!(piece.as_object().unwrap().len(), 16, "{piece}");

    // No match still prints valid JSON, and fails as a plain search does.
    let (code, stdout, _) = output(&db_path, &["search", "--json", "coltrane"]);
    assert_eq!((code, stdout.trim()), (Some(1), "[]"));
    // After --, a query may look like an option.
    let (code, stdout, stderr) = output(&db_path, &["search", "--json", "--", "--db"]);
    assert_eq!((code, stdout.trim()), (Some(1), "[]"), "{stderr}");
}

/// Runs playr with a settings directory and no terminal, returning its exit status and stderr.
fn with_config(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_playr"))
        .env("XDG_CONFIG_HOME", dir)
        .arg("--db")
        .arg(dir.join("library.db"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn bad_settings_stop_playr_before_it_starts_and_list_every_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("playr")).unwrap();
    let default = dir.path().join("playr/settings.toml");
    std::fs::write(&default, "volume = 50\nfrob = 1\n[keys]\nd = 'remove'\n").unwrap();

    let (ok, stderr) = with_config(dir.path(), &[]);
    assert!(!ok);
    let path = default.display();
    assert!(
        stderr.contains(&format!("playr: {path}: line 2: unknown setting: frob")),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("playr: {path}: line 4: :remove works")),
        "{stderr}"
    );
    // Stopped at the settings, not at the missing terminal.
    assert!(!stderr.contains("terminal"), "{stderr}");

    // --settings replaces the default file, and must exist.
    let other = dir.path().join("other.toml");
    std::fs::write(&other, "volume = 50\n").unwrap();
    let (_, stderr) = with_config(dir.path(), &["--settings", other.to_str().unwrap()]);
    assert!(
        !stderr.contains("line 2"),
        "the default file was read: {stderr}"
    );
    let (ok, stderr) = with_config(dir.path(), &["--settings", "/nonexistent/playr.toml"]);
    assert!(!ok);
    assert!(
        stderr.contains("playr: /nonexistent/playr.toml:"),
        "{stderr}"
    );
}
