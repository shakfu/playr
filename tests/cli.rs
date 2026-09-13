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

#[test]
fn an_unknown_option_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_playr"))
        .arg("--db")
        .arg(dir.path().join("library.db"))
        .arg("--bogus")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown option --bogus"),
        "stderr was {stderr:?}"
    );
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
