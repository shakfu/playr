//! Scanner tests. Fixtures are generated with ffmpeg when it is available.

use playr::db::{self, query};
use playr::scan;
use std::path::Path;
use std::process::Command;

fn have_ffmpeg() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Writes a 1 second tagged FLAC at `path`.
fn make_flac(path: &Path, title: &str, artist: &str, album: &str, track_no: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100:duration=1",
            "-ac",
            "2",
        ])
        .args(["-metadata", &format!("title={title}")])
        .args(["-metadata", &format!("artist={artist}")])
        .args(["-metadata", &format!("album={album}")])
        .args(["-metadata", &format!("album_artist={artist}")])
        .args(["-metadata", &format!("track={track_no}")])
        .args(["-c:a", "flac"])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success(), "ffmpeg failed to write {path:?}");
}

#[test]
fn extension_filter_accepts_audio_and_rejects_the_rest() {
    assert!(scan::is_audio(Path::new("/m/a.flac")));
    assert!(
        scan::is_audio(Path::new("/m/a.MP3")),
        "extension match must be case-insensitive"
    );
    assert!(scan::is_audio(Path::new("/m/a.m4a")));
    assert!(!scan::is_audio(Path::new("/m/cover.jpg")));
    assert!(!scan::is_audio(Path::new("/m/notes.txt")));
    assert!(!scan::is_audio(Path::new("/m/noext")));
}

#[test]
fn scan_reads_tags_recursively_and_skips_unchanged_files() {
    if !have_ffmpeg() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    make_flac(
        &root.join("Evans/Sunday/01 Gloria.flac"),
        "Gloria's Step",
        "Bill Evans",
        "Sunday",
        1,
    );
    make_flac(
        &root.join("Evans/Sunday/02 Waltz.flac"),
        "Waltz for Debby",
        "Bill Evans",
        "Sunday",
        2,
    );
    make_flac(
        &root.join("Davis/Blue/01 So What.flac"),
        "So What",
        "Miles Davis",
        "Kind of Blue",
        1,
    );
    std::fs::write(root.join("Evans/Sunday/cover.jpg"), b"not audio").unwrap();

    let mut conn = db::open_memory().unwrap();
    let stats = scan::scan_dir(&mut conn, root, |_, _| {}).unwrap();
    assert_eq!(stats.seen, 3, "the jpg must not be counted");
    assert_eq!(stats.added, 3);
    assert_eq!(stats.failed, 0);

    let tracks = query::all(&conn).unwrap();
    assert_eq!(tracks.len(), 3);
    let waltz = tracks
        .iter()
        .find(|t| t.title.as_deref() == Some("Waltz for Debby"))
        .unwrap();
    assert_eq!(waltz.artist.as_deref(), Some("Bill Evans"));
    assert_eq!(waltz.album.as_deref(), Some("Sunday"));
    assert_eq!(waltz.track_no, Some(2));
    assert_eq!(waltz.sample_rate, Some(44100));
    assert_eq!(waltz.channels, Some(2));
    assert!(waltz.duration_ms.unwrap() > 900, "duration not read");

    // A second scan must open nothing.
    let again = scan::scan_dir(&mut conn, root, |_, _| {}).unwrap();
    assert_eq!(again.skipped, 3, "unchanged files were re-read");
    assert_eq!(again.added, 0);
    assert_eq!(query::count(&conn).unwrap(), 3, "rescan duplicated rows");
}

#[test]
fn a_changed_file_is_picked_up_again() {
    if !have_ffmpeg() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let path = root.join("a.flac");
    make_flac(&path, "Before", "A", "X", 1);

    let mut conn = db::open_memory().unwrap();
    scan::scan_dir(&mut conn, root, |_, _| {}).unwrap();
    assert_eq!(query::search(&conn, "Before").unwrap().len(), 1);

    make_flac(&path, "After", "A", "X", 1);
    let stats = scan::scan_dir(&mut conn, root, |_, _| {}).unwrap();
    assert_eq!(stats.added, 1, "rewritten file was not re-read");
    assert_eq!(
        query::count(&conn).unwrap(),
        1,
        "re-read created a duplicate row"
    );
    assert_eq!(query::search(&conn, "After").unwrap().len(), 1);
    assert!(
        query::search(&conn, "Before").unwrap().is_empty(),
        "old title still indexed"
    );
}

#[test]
fn unreadable_files_are_counted_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("broken.flac"), b"this is not a flac file").unwrap();

    let mut conn = db::open_memory().unwrap();
    let stats = scan::scan_dir(&mut conn, root, |_, _| {}).unwrap();
    assert_eq!(stats.seen, 1);
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.added, 0);
    assert_eq!(query::count(&conn).unwrap(), 0);
}

#[test]
fn prune_removes_rows_for_deleted_files() {
    if !have_ffmpeg() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    make_flac(&root.join("a.flac"), "A", "X", "Y", 1);
    make_flac(&root.join("b.flac"), "B", "X", "Y", 2);

    let mut conn = db::open_memory().unwrap();
    scan::scan_dir(&mut conn, root, |_, _| {}).unwrap();
    assert_eq!(query::count(&conn).unwrap(), 2);

    std::fs::remove_file(root.join("a.flac")).unwrap();
    assert_eq!(db::prune_missing(&conn).unwrap(), 1);
    assert_eq!(query::count(&conn).unwrap(), 1);
    assert!(
        query::search(&conn, "A").unwrap().is_empty(),
        "pruned track still in the index"
    );
}
