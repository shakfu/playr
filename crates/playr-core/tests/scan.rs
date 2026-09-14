//! Scanner tests. Fixtures are generated with ffmpeg when it is available.

mod common;

use common::have_ffmpeg;
use playr_core::db::{self, query, Track};
use playr_core::scan;
use std::path::Path;
use std::process::Command;

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
    assert_eq!(db::prune_missing(&conn, root).unwrap(), 1);
    assert_eq!(query::count(&conn).unwrap(), 1);
    assert!(
        query::search(&conn, "A").unwrap().is_empty(),
        "pruned track still in the index"
    );
}

#[test]
fn a_relative_root_is_stored_as_absolute_paths() {
    if !have_ffmpeg() {
        return;
    }
    // Cargo runs tests from the package root, so `target/` is under the cwd.
    std::fs::create_dir_all("target").unwrap();
    let dir = tempfile::tempdir_in("target").unwrap();
    let cwd = std::env::current_dir().unwrap();
    let root = dir.path().strip_prefix(&cwd).unwrap();
    assert!(root.is_relative());
    make_flac(&root.join("a.flac"), "A", "X", "Y", 1);

    let mut conn = db::open_memory().unwrap();
    scan::scan_dir(&mut conn, root, |_, _| {}).unwrap();

    let tracks = query::all(&conn).unwrap();
    assert_eq!(tracks.len(), 1);
    let want = root.join("a.flac").canonicalize().unwrap();
    assert_eq!(
        Path::new(&tracks[0].path),
        want,
        "a relative row breaks playback and prune from any other directory"
    );
}

fn untagged(path: &Path) -> Track {
    Track {
        path: path.to_string_lossy().into_owned(),
        mtime: 1,
        size: 1,
        ..Default::default()
    }
}

#[test]
fn prune_leaves_rows_outside_the_scanned_root() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let music = base.join("music");
    std::fs::create_dir(&music).unwrap();

    let conn = db::open_memory().unwrap();
    db::upsert(&conn, &untagged(&music.join("gone.flac"))).unwrap();
    // Missing, but under a sibling whose name shares the root's prefix.
    db::upsert(&conn, &untagged(&base.join("music2/a.flac"))).unwrap();
    db::upsert(&conn, &untagged(&base.join("drive/b.flac"))).unwrap();

    assert_eq!(db::prune_missing(&conn, &music).unwrap(), 1);
    let left: Vec<String> = query::all(&conn)
        .unwrap()
        .into_iter()
        .map(|t| t.path)
        .collect();
    assert_eq!(left.len(), 2, "pruned outside the root: {left:?}");
}

#[test]
fn prune_leaves_rows_under_a_root_that_differs_only_in_case() {
    // On a case-sensitive file system `Music` is another directory, such as
    // the empty mount point of an unplugged drive.
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let music = base.join("music");
    std::fs::create_dir(&music).unwrap();

    let mut conn = db::open_memory().unwrap();
    db::upsert(&conn, &untagged(&music.join("gone.flac"))).unwrap();
    let id = db::upsert(&conn, &untagged(&base.join("Music/b.flac"))).unwrap();
    query::save_playlist(&mut conn, "keep", &[id]).unwrap();

    assert_eq!(db::prune_missing(&conn, &music).unwrap(), 1);
    assert_eq!(query::count(&conn).unwrap(), 1);
    assert_eq!(query::playlists(&conn).unwrap()[0].len, 1);
}

#[test]
fn prune_of_a_missing_root_removes_nothing() {
    // An unmounted drive looks like a root whose every file is gone.
    let dir = tempfile::tempdir().unwrap();
    let drive = dir.path().canonicalize().unwrap().join("drive");

    let mut conn = db::open_memory().unwrap();
    let id = db::upsert(&conn, &untagged(&drive.join("a.flac"))).unwrap();
    query::save_playlist(&mut conn, "keep", &[id]).unwrap();

    assert_eq!(db::prune_missing(&conn, &drive).unwrap(), 0);
    assert_eq!(query::count(&conn).unwrap(), 1);
    assert_eq!(query::playlists(&conn).unwrap()[0].len, 1);
}

#[test]
fn an_interrupted_scan_keeps_the_batches_it_finished() {
    if !have_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let one = dir.path().join("one.flac");
    make_flac(&one, "A", "X", "Y", 1);
    let root = dir.path().join("lib");
    std::fs::create_dir(&root).unwrap();
    for i in 0..scan::SCAN_BATCH + 10 {
        std::fs::copy(&one, root.join(format!("{i:04}.flac"))).unwrap();
    }

    let mut conn = db::open_memory().unwrap();
    let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        scan::scan_dir(&mut conn, &root, |s, _| {
            if s.seen == scan::SCAN_BATCH + 5 {
                panic!("interrupted");
            }
        })
    }));
    assert!(interrupted.is_err());
    assert_eq!(
        query::count(&conn).unwrap(),
        scan::SCAN_BATCH as i64,
        "the finished batch was not committed"
    );
}

#[cfg(unix)]
#[test]
fn a_path_that_is_not_utf8_is_counted_unreadable() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let name = std::ffi::OsStr::from_bytes(b"bad-\xff.flac");
    // APFS and some other filesystems refuse such names outright.
    if std::fs::write(dir.path().join(name), b"x").is_err() {
        eprintln!("skipping: filesystem rejects non-UTF-8 names");
        return;
    }

    let mut conn = db::open_memory().unwrap();
    let stats = scan::scan_dir(&mut conn, dir.path(), |_, _| {}).unwrap();
    assert_eq!(stats.seen, 1);
    assert_eq!(stats.failed, 1);
    assert_eq!(query::count(&conn).unwrap(), 0);
}

#[test]
fn a_playable_file_without_readable_tags_is_still_indexed() {
    if !have_ffmpeg() {
        return;
    }
    // Symphonia plays these containers; the tag reader does not parse them.
    let dir = tempfile::tempdir().unwrap();
    for (name, codec) in [("a.caf", "pcm_s16le"), ("b.mka", "flac")] {
        let status = Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
            .arg("sine=frequency=440:sample_rate=48000:duration=1")
            .args(["-ac", "2", "-c:a", codec])
            .arg(dir.path().join(name))
            .status()
            .unwrap();
        assert!(status.success(), "ffmpeg failed to write {name}");
    }

    let mut conn = db::open_memory().unwrap();
    let stats = scan::scan_dir(&mut conn, dir.path(), |_, _| {}).unwrap();
    assert_eq!((stats.added, stats.failed), (2, 0), "{stats:?}");
    for t in query::all(&conn).unwrap() {
        assert_eq!(t.sample_rate, Some(48000), "{}", t.path);
        assert_eq!(t.channels, Some(2), "{}", t.path);
        // Symphonia's Matroska reader reports no duration.
        match t.duration_ms {
            Some(ms) => assert!((900..1100).contains(&ms), "{}: {ms}ms", t.path),
            None => assert!(t.path.ends_with(".mka"), "{}: no duration", t.path),
        }
    }
}

/// Sets the modification time of `path` to `secs` plus `nanos`.
fn set_mtime(path: &Path, secs: u64, nanos: u32) {
    let t = std::time::UNIX_EPOCH + std::time::Duration::new(secs, nanos);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

#[test]
fn a_same_size_rewrite_within_one_second_is_picked_up() {
    if !have_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.flac");
    make_flac(&path, "Before", "A", "X", 1);
    let size = std::fs::metadata(&path).unwrap().len();
    set_mtime(&path, 1_700_000_000, 100);

    let mut conn = db::open_memory().unwrap();
    scan::scan_dir(&mut conn, dir.path(), |_, _| {}).unwrap();

    // Same tag length, so the same size; only the sub-second mtime differs.
    make_flac(&path, "Behind", "A", "X", 1);
    assert_eq!(std::fs::metadata(&path).unwrap().len(), size);
    set_mtime(&path, 1_700_000_000, 900);
    if std::fs::metadata(&path).unwrap().modified().unwrap()
        == std::time::UNIX_EPOCH + std::time::Duration::new(1_700_000_000, 100)
    {
        eprintln!("skipping: filesystem stores whole-second mtimes");
        return;
    }

    let stats = scan::scan_dir(&mut conn, dir.path(), |_, _| {}).unwrap();
    assert_eq!(stats.added, 1, "the rewrite was skipped as unchanged");
    assert_eq!(query::search(&conn, "Behind").unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn a_directory_that_cannot_be_listed_is_counted_unreadable() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Root reads it anyway, so there is nothing to test.
    if std::fs::read_dir(&locked).is_ok() {
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("skipping: permissions do not restrict this user");
        return;
    }

    let mut conn = db::open_memory().unwrap();
    let stats = scan::scan_dir(&mut conn, dir.path(), |_, _| {}).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(stats.failed, 1, "{stats:?}");
}
