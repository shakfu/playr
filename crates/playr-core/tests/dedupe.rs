mod common;

use common::tone;
use playr_core::db::{self, dedupe, query, Track};
use playr_core::scan;
use std::path::Path;

/// A library scanned from `dir`, which holds `a/x.wav` and an identical
/// `b/x.wav`, and `c/y.wav` of the same length but other content.
fn copies(dir: &Path) -> rusqlite::Connection {
    for sub in ["a", "b", "c"] {
        std::fs::create_dir(dir.join(sub)).unwrap();
    }
    tone(&dir.join("a/x.wav"), 8000, 0.5, -6.0);
    std::fs::copy(dir.join("a/x.wav"), dir.join("b/x.wav")).unwrap();
    tone(&dir.join("c/y.wav"), 8000, 0.5, -12.0);
    let mut conn = db::open_memory().unwrap();
    scan::scan_dir(&mut conn, dir, |_, _| {}).unwrap();
    db::add_root(&conn, dir).unwrap();
    conn
}

fn id_of(conn: &rusqlite::Connection, path: &Path) -> i64 {
    let path = path.canonicalize().unwrap();
    query::by_path(conn, path.to_str().unwrap())
        .unwrap()
        .unwrap()
        .id
}

#[test]
fn identical_files_group_and_equal_sizes_alone_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let conn = copies(dir.path());
    let groups = dedupe::identical(&conn).unwrap();
    assert_eq!(groups.len(), 1, "{groups:?}");
    assert_eq!(groups[0].drop.len(), 1);
    let paths = [&groups[0].keep.path, &groups[0].drop[0].path];
    assert!(paths.iter().all(|p| p.ends_with("x.wav")), "{paths:?}");
}

#[test]
fn the_copy_in_a_playlist_is_kept_and_the_others_places_move_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = copies(dir.path());
    let (a, b) = (
        id_of(&conn, &dir.path().join("a/x.wav")),
        id_of(&conn, &dir.path().join("b/x.wav")),
    );
    query::save_playlist(&mut conn, "one", &[b]).unwrap();
    query::save_playlist(&mut conn, "two", &[b, a]).unwrap();
    let groups = dedupe::identical(&conn).unwrap();
    assert_eq!(groups[0].keep.id, b, "b has the most playlist places");

    assert_eq!(dedupe::apply(&conn, &groups).unwrap(), 1);
    assert_eq!(query::count(&conn).unwrap(), 2);
    let lists = query::playlists(&conn).unwrap();
    let two = lists.iter().find(|p| p.name == "two").unwrap();
    let ids: Vec<i64> = query::playlist_tracks(&conn, two.id)
        .unwrap()
        .iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(ids, [b, b], "a's place moves to b, in order");
}

#[test]
fn a_rescan_passes_over_a_dropped_copy_until_restored() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = copies(dir.path());
    let groups = dedupe::identical(&conn).unwrap();
    dedupe::apply(&conn, &groups).unwrap();
    assert!(Path::new(&groups[0].drop[0].path).exists(), "files stay");

    scan::scan_dir(&mut conn, dir.path(), |_, _| {}).unwrap();
    assert_eq!(query::count(&conn).unwrap(), 2);

    assert_eq!(dedupe::restore(&conn).unwrap(), 1);
    scan::scan_dir(&mut conn, dir.path(), |_, _| {}).unwrap();
    assert_eq!(query::count(&conn).unwrap(), 3);
}

#[test]
fn forgetting_a_root_forgets_its_exclusions() {
    let dir = tempfile::tempdir().unwrap();
    let conn = copies(dir.path());
    dedupe::apply(&conn, &dedupe::identical(&conn).unwrap()).unwrap();
    db::forget_root(&conn, dir.path()).unwrap().unwrap();
    assert!(dedupe::excluded(&conn).unwrap().is_empty());
}

/// A row at `path`, absolute on this host when it starts with `/`: Windows
/// wants a drive.
fn row(path: &str) -> Track {
    let path = match cfg!(windows) && path.starts_with('/') {
        true => format!("C:{path}"),
        false => path.to_string(),
    };
    Track {
        path,
        mtime: 1,
        size: 1,
        ..Default::default()
    }
}

#[test]
fn a_relative_row_merges_into_the_one_absolute_row_ending_with_it() {
    let mut conn = db::open_memory().unwrap();
    let rel = db::upsert(&conn, &row("./music/a.flac")).unwrap();
    let abs = db::upsert(&conn, &row("/home/u/music/a.flac")).unwrap();
    query::save_playlist(&mut conn, "p", &[rel]).unwrap();

    let fixed = dedupe::fix_relative(&conn).unwrap();
    assert_eq!(fixed.merged, 1);
    assert_eq!(query::count(&conn).unwrap(), 1);
    let pl = &query::playlists(&conn).unwrap()[0];
    assert_eq!(query::playlist_tracks(&conn, pl.id).unwrap()[0].id, abs);
}

#[test]
fn an_ambiguous_relative_row_is_left() {
    let conn = db::open_memory().unwrap();
    db::upsert(&conn, &row("a.flac")).unwrap();
    db::upsert(&conn, &row("/x/a.flac")).unwrap();
    db::upsert(&conn, &row("/y/a.flac")).unwrap();
    let fixed = dedupe::fix_relative(&conn).unwrap();
    assert_eq!((fixed.merged, fixed.resolved, fixed.left), (0, 0, 1));
    assert_eq!(query::count(&conn).unwrap(), 3);
}

#[test]
fn a_relative_row_under_one_root_gets_its_absolute_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("al")).unwrap();
    tone(&dir.path().join("al/t.wav"), 8000, 0.1, -6.0);
    let conn = db::open_memory().unwrap();
    db::add_root(&conn, dir.path()).unwrap();
    db::upsert(&conn, &row("al/t.wav")).unwrap();

    let fixed = dedupe::fix_relative(&conn).unwrap();
    assert_eq!(fixed.resolved, 1);
    let want = dir.path().join("al/t.wav").canonicalize().unwrap();
    assert_eq!(query::all(&conn).unwrap()[0].path, want.to_str().unwrap());
}

/// CAF and Matroska rows scanned before their tags were read are read
/// again by the next scan, once.
#[test]
fn caf_and_matroska_rows_are_reread_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    {
        let conn = db::open(&path).unwrap();
        for p in ["/m/a.mkv", "/m/b.WEBM", "/m/c.caf", "/m/d.flac"] {
            db::upsert(
                &conn,
                &Track {
                    path: p.into(),
                    mtime: 5,
                    size: 1,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        conn.execute("DELETE FROM state WHERE key = 'reread_caf_mkv'", [])
            .unwrap();
        conn.execute("DELETE FROM state WHERE key = 'reread_mkv_title'", [])
            .unwrap();
    }
    let mtimes = |conn: &rusqlite::Connection| -> Vec<i64> {
        query::all(conn).unwrap().iter().map(|t| t.mtime).collect()
    };
    let conn = db::open(&path).unwrap();
    let mut by_path = query::all(&conn).unwrap();
    by_path.sort_by(|a, b| a.path.cmp(&b.path));
    assert_eq!(
        by_path.iter().map(|t| t.mtime).collect::<Vec<_>>(),
        [0, 0, 0, 5]
    );
    conn.execute("UPDATE tracks SET mtime = 5", []).unwrap();
    drop(conn);
    let conn = db::open(&path).unwrap();
    assert!(mtimes(&conn).iter().all(|&m| m == 5), "once only");
}

/// Matroska rows read before their titles were are read again, once, by a
/// library that already reread its CAF and Matroska tags.
#[test]
fn matroska_rows_are_reread_for_their_titles() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    {
        let conn = db::open(&path).unwrap();
        for p in ["/m/a.mka", "/m/c.caf"] {
            db::upsert(
                &conn,
                &Track {
                    path: p.into(),
                    mtime: 5,
                    size: 1,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        conn.execute("DELETE FROM state WHERE key = 'reread_mkv_title'", [])
            .unwrap();
    }
    let conn = db::open(&path).unwrap();
    let mut rows = query::all(&conn).unwrap();
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    assert_eq!(rows.iter().map(|t| t.mtime).collect::<Vec<_>>(), [0, 5]);
}
