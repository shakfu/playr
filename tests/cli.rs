//! Command-line behaviour, run against the built binary.
//!
//! No command here reaches playback: the files are not audio, and without a
//! terminal the interface refuses to start.

use std::path::Path;
use std::process::{Command, Stdio};

fn playr(db: &Path, args: &[&str]) {
    Command::new(env!("CARGO_BIN_EXE_playr"))
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
