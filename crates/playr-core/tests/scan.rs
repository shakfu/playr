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
fn scan_into_records_the_directory_as_a_root() {
    let dir = tempfile::tempdir().unwrap();
    let music = dir.path().join("music");
    std::fs::create_dir(&music).unwrap();
    common::silence(&music.join("a.wav"), 8000, 0.05);
    let library = dir.path().join("library.db");
    let report = scan::scan_into(&library, &music, |_| {}).unwrap();
    assert_eq!(report.stats.added, 1);
    let conn = db::open(&library).unwrap();
    assert_eq!(
        db::roots(&conn).unwrap(),
        vec![music.canonicalize().unwrap()]
    );

    // The test is on files seen, not files added, so a rescan that finds
    // nothing new keeps its root.
    let again = scan::scan_into(&library, &music, |_| {}).unwrap();
    assert_eq!((again.stats.added, again.stats.skipped), (0, 1));
    assert_eq!(
        db::roots(&conn).unwrap(),
        vec![music.canonicalize().unwrap()]
    );
}

#[test]
fn a_directory_holding_no_audio_is_not_recorded_as_a_root() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty");
    std::fs::create_dir_all(empty.join("sub")).unwrap();
    std::fs::write(empty.join("notes.txt"), b"x").unwrap();
    let library = dir.path().join("library.db");
    let report = scan::scan_into(&library, &empty, |_| {}).unwrap();
    assert_eq!(report.stats.seen, 0);
    let conn = db::open(&library).unwrap();
    assert!(
        db::roots(&conn).unwrap().is_empty(),
        "an empty directory became a root"
    );
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
    assert_eq!(db::prune_missing(&conn, root).unwrap().tracks, 1);
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

    assert_eq!(db::prune_missing(&conn, &music).unwrap().tracks, 1);
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

    assert_eq!(db::prune_missing(&conn, &music).unwrap().tracks, 1);
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

    assert_eq!(
        db::prune_missing(&conn, &drive).unwrap(),
        db::Pruned::default()
    );
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

#[test]
fn a_scan_does_not_hold_the_write_lock_while_it_reads_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("lib");
    std::fs::create_dir(&root).unwrap();
    for i in 0..3 {
        common::silence(&root.join(format!("{i}.wav")), 8000, 0.1);
    }
    let library = dir.path().join("library.db");
    let mut conn = db::open(&library).unwrap();
    // The session's connection, as a frontend writes a mark during a scan.
    // Without a busy timeout, a write that meets the scan's lock fails at once.
    let other = db::open(&library).unwrap();
    other.busy_timeout(std::time::Duration::ZERO).unwrap();

    let mut refused = Vec::new();
    let stats = scan::scan_dir(&mut conn, &root, |s, _| {
        let mark = query::Mark {
            frame: s.seen as u64,
            rate: 8000,
        };
        if let Err(e) = query::add_mark(&other, "/m/x.flac", mark) {
            refused.push((s.seen, e.to_string()));
        }
    })
    .unwrap();
    assert_eq!(stats.added, 3);
    assert!(refused.is_empty(), "writes refused mid-scan: {refused:?}");
    assert_eq!(query::marks(&other, "/m/x.flac").unwrap().len(), 3);
}

#[test]
fn a_scan_counts_missing_files_and_removes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let library = root.join("library.db");
    common::silence(&root.join("here.wav"), 8000, 0.1);
    let conn = db::open(&library).unwrap();
    let gone = db::upsert(&conn, &untagged(&root.join("gone.flac"))).unwrap();
    query::save_playlist(&mut db::open(&library).unwrap(), "keep", &[gone]).unwrap();
    query::add_mark(
        &conn,
        &root.join("gone.flac").to_string_lossy(),
        query::Mark {
            frame: 1,
            rate: 8000,
        },
    )
    .unwrap();

    let report = scan::scan_into(&library, &root, |_| {}).unwrap();
    assert_eq!(
        (report.stats.added, report.missing, report.total),
        (1, 1, 2)
    );
    assert_eq!(
        query::playlists(&conn).unwrap()[0].len,
        1,
        "a scan emptied a playlist"
    );
    assert_eq!(
        query::marks(&conn, &root.join("gone.flac").to_string_lossy())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn prune_removes_the_marks_of_missing_files_under_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let music = base.join("music");
    std::fs::create_dir(&music).unwrap();
    let here = music.join("here.wav");
    common::silence(&here, 8000, 0.1);
    let mark = |conn: &rusqlite::Connection, path: &Path, frame| {
        let path = path.to_string_lossy();
        query::add_mark(conn, &path, query::Mark { frame, rate: 8000 }).unwrap();
    };
    let marks = |conn: &rusqlite::Connection, path: &Path| {
        query::marks(conn, &path.to_string_lossy()).unwrap().len()
    };

    let conn = db::open_memory().unwrap();
    let gone = music.join("gone.flac");
    db::upsert(&conn, &untagged(&gone)).unwrap();
    db::upsert(&conn, &untagged(&here)).unwrap();
    mark(&conn, &gone, 1);
    mark(&conn, &gone, 2);
    mark(&conn, &here, 1);
    // A file played without being scanned, since deleted.
    let played = music.join("played.flac");
    mark(&conn, &played, 1);
    // Missing, but outside the root.
    let elsewhere = base.join("drive/a.flac");
    mark(&conn, &elsewhere, 1);

    let pruned = db::prune_missing(&conn, &music).unwrap();
    assert_eq!(
        pruned,
        db::Pruned {
            tracks: 1,
            marks: 3
        }
    );
    assert_eq!(query::count(&conn).unwrap(), 1);
    assert_eq!((marks(&conn, &gone), marks(&conn, &played)), (0, 0));
    assert_eq!(marks(&conn, &here), 1, "a present file lost its mark");
    assert_eq!(marks(&conn, &elsewhere), 1, "pruned outside the root");
}

#[test]
fn a_root_that_reads_empty_is_reported_unavailable_not_just_missing() {
    let dir = tempfile::tempdir().unwrap();
    let music = dir.path().join("music");
    std::fs::create_dir(&music).unwrap();
    common::silence(&music.join("a.wav"), 8000, 0.05);
    common::silence(&music.join("b.wav"), 8000, 0.05);
    let library = dir.path().join("library.db");
    let report = scan::scan_into(&library, &music, |_| {}).unwrap();
    assert_eq!((report.missing, report.unavailable), (0, 0));

    // The directory survives with nothing in it, as an unmounted drive does.
    std::fs::remove_file(music.join("a.wav")).unwrap();
    std::fs::remove_file(music.join("b.wav")).unwrap();
    let report = scan::scan_into(&library, &music, |_| {}).unwrap();
    assert_eq!(report.stats.seen, 0);
    assert_eq!(
        (report.missing, report.unavailable),
        (2, 1),
        "both tracks read as gone, so the root cannot be trusted"
    );

    // One file back is enough to trust it again.
    common::silence(&music.join("a.wav"), 8000, 0.05);
    let report = scan::scan_into(&library, &music, |_| {}).unwrap();
    assert_eq!((report.missing, report.unavailable), (1, 0));
}

#[test]
fn a_root_nested_in_another_does_not_become_a_second_root() {
    let dir = tempfile::tempdir().unwrap();
    let music = dir.path().join("music");
    let rock = music.join("rock");
    std::fs::create_dir_all(&rock).unwrap();
    common::silence(&music.join("a.wav"), 8000, 0.05);
    common::silence(&rock.join("b.wav"), 8000, 0.05);
    let library = dir.path().join("library.db");
    let conn = db::open(&library).unwrap();

    scan::scan_into(&library, &rock, |_| {}).unwrap();
    assert_eq!(
        db::roots(&conn).unwrap(),
        vec![rock.canonicalize().unwrap()]
    );

    // The wider directory replaces the one inside it.
    scan::scan_into(&library, &music, |_| {}).unwrap();
    assert_eq!(
        db::roots(&conn).unwrap(),
        vec![music.canonicalize().unwrap()]
    );

    // And scanning the inner one again does not add it back.
    scan::scan_into(&library, &rock, |_| {}).unwrap();
    assert_eq!(
        db::roots(&conn).unwrap(),
        vec![music.canonicalize().unwrap()]
    );

    // A sibling whose name extends the root's is its own root.
    let music2 = dir.path().join("music2");
    std::fs::create_dir(&music2).unwrap();
    common::silence(&music2.join("c.wav"), 8000, 0.05);
    scan::scan_into(&library, &music2, |_| {}).unwrap();
    let mut roots = db::roots(&conn).unwrap();
    roots.sort();
    assert_eq!(
        roots,
        vec![
            music.canonicalize().unwrap(),
            music2.canonicalize().unwrap()
        ]
    );
}
