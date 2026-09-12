use playr::db::{self, query, Track};

fn track(path: &str, title: &str, artist: &str, album: &str) -> Track {
    Track {
        path: path.into(),
        title: Some(title.into()),
        artist: Some(artist.into()),
        album: Some(album.into()),
        album_artist: Some(artist.into()),
        duration_ms: Some(1000),
        mtime: 1,
        size: 100,
        ..Default::default()
    }
}

fn seeded() -> rusqlite::Connection {
    let conn = db::open_memory().unwrap();
    db::upsert(
        &conn,
        &track("/m/a.flac", "Waltz for Debby", "Bill Evans", "Sunday"),
    )
    .unwrap();
    db::upsert(
        &conn,
        &track("/m/b.flac", "My Foolish Heart", "Bill Evans", "Sunday"),
    )
    .unwrap();
    db::upsert(
        &conn,
        &track("/m/c.mp3", "So What", "Miles Davis", "Kind of Blue"),
    )
    .unwrap();
    conn
}

#[test]
fn upsert_is_idempotent_on_path() {
    let conn = db::open_memory().unwrap();
    let mut t = track("/m/a.flac", "One", "A", "X");
    let id1 = db::upsert(&conn, &t).unwrap();
    t.title = Some("Two".into());
    let id2 = db::upsert(&conn, &t).unwrap();
    assert_eq!(id1, id2, "same path must reuse the row");
    assert_eq!(query::count(&conn).unwrap(), 1);
    assert_eq!(query::all(&conn).unwrap()[0].title.as_deref(), Some("Two"));
}

#[test]
fn search_matches_title_and_artist() {
    let conn = seeded();
    let by_artist = query::search(&conn, "Bill Evans").unwrap();
    assert_eq!(by_artist.len(), 2);
    let by_title = query::search(&conn, "foolish").unwrap();
    assert_eq!(by_title.len(), 1);
    assert_eq!(by_title[0].title.as_deref(), Some("My Foolish Heart"));
}

#[test]
fn search_is_prefix_matching() {
    let conn = seeded();
    assert_eq!(query::search(&conn, "mil").unwrap().len(), 1);
    assert_eq!(query::search(&conn, "wal").unwrap().len(), 1);
}

#[test]
fn search_empty_input_returns_nothing() {
    let conn = seeded();
    assert!(query::search(&conn, "").unwrap().is_empty());
    assert!(query::search(&conn, "   ").unwrap().is_empty());
}

#[test]
fn search_treats_fts_syntax_as_literal() {
    let conn = seeded();
    // These would be syntax errors or operators if passed through unquoted.
    for input in ["\"", "*", "AND", "NEAR(", "a:b", "^x", "-"] {
        let r = query::search(&conn, input);
        assert!(r.is_ok(), "input {input:?} must not raise an FTS error");
    }
}

#[test]
fn fts_index_follows_update_and_delete() {
    let conn = db::open_memory().unwrap();
    let mut t = track("/m/a.flac", "Original", "A", "X");
    let id = db::upsert(&conn, &t).unwrap();
    assert_eq!(query::search(&conn, "Original").unwrap().len(), 1);

    t.title = Some("Renamed".into());
    db::upsert(&conn, &t).unwrap();
    assert!(
        query::search(&conn, "Original").unwrap().is_empty(),
        "stale index entry"
    );
    assert_eq!(query::search(&conn, "Renamed").unwrap().len(), 1);

    conn.execute("DELETE FROM tracks WHERE id = ?1", [id])
        .unwrap();
    assert!(query::search(&conn, "Renamed").unwrap().is_empty());
}

#[test]
fn playlist_roundtrip_preserves_order() {
    let mut conn = seeded();
    let all = query::all(&conn).unwrap();
    let ids: Vec<i64> = all.iter().rev().map(|t| t.id).collect();

    query::save_playlist(&mut conn, "mix", &ids).unwrap();
    let pls = query::playlists(&conn).unwrap();
    assert_eq!(pls.len(), 1);
    assert_eq!(pls[0].name, "mix");
    assert_eq!(pls[0].len, 3);

    let got: Vec<i64> = query::playlist_tracks(&conn, pls[0].id)
        .unwrap()
        .iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(got, ids, "playlist order must be preserved");
}

#[test]
fn saving_same_name_replaces_contents() {
    let mut conn = seeded();
    let all = query::all(&conn).unwrap();
    let id = query::save_playlist(&mut conn, "mix", &[all[0].id, all[1].id]).unwrap();
    let id2 = query::save_playlist(&mut conn, "mix", &[all[2].id]).unwrap();
    assert_eq!(id, id2);
    assert_eq!(query::playlist_tracks(&conn, id).unwrap().len(), 1);
    assert_eq!(query::playlists(&conn).unwrap().len(), 1);
}

#[test]
fn deleting_a_track_removes_it_from_playlists() {
    let mut conn = seeded();
    let all = query::all(&conn).unwrap();
    let pid = query::save_playlist(&mut conn, "mix", &[all[0].id, all[1].id]).unwrap();
    conn.execute("DELETE FROM tracks WHERE id = ?1", [all[0].id])
        .unwrap();
    assert_eq!(query::playlist_tracks(&conn, pid).unwrap().len(), 1);
}

#[test]
fn display_title_falls_back_to_file_stem() {
    let t = Track {
        path: "/m/untitled song.flac".into(),
        ..Default::default()
    };
    assert_eq!(t.display_title(), "untitled song");
    assert_eq!(t.display_artist(), "Unknown Artist");
}

#[test]
fn under_path_escapes_like_wildcards() {
    let conn = db::open_memory().unwrap();
    db::upsert(&conn, &track("/m/100%/a.flac", "A", "X", "Y")).unwrap();
    db::upsert(&conn, &track("/m/1009/b.flac", "B", "X", "Y")).unwrap();
    let hits = query::under_path(&conn, "/m/100%/").unwrap();
    assert_eq!(
        hits.len(),
        1,
        "`%` in the prefix must not act as a wildcard"
    );
    assert_eq!(hits[0].path, "/m/100%/a.flac");
}

#[test]
fn a_new_library_records_its_schema_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.db");
    let version = |conn: &rusqlite::Connection| -> i64 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(version(&db::open(&path).unwrap()), db::SCHEMA_VERSION);
    // Reopening must not reset it.
    assert_eq!(version(&db::open(&path).unwrap()), db::SCHEMA_VERSION);
}

#[test]
fn a_playlist_is_found_by_exact_name_before_case_folding() {
    let pl = |id, name: &str| query::Playlist {
        id,
        name: name.into(),
        len: 0,
    };
    let lists = [pl(1, "Late"), pl(2, "late"), pl(3, "Morning")];
    assert_eq!(query::find_playlist(&lists, "late").unwrap().id, 2);
    assert_eq!(query::find_playlist(&lists, "Late").unwrap().id, 1);
    assert_eq!(query::find_playlist(&lists, "morning").unwrap().id, 3);
    assert!(
        query::find_playlist(&lists, "LATE").is_none(),
        "an ambiguous name must not pick one"
    );
    assert!(query::find_playlist(&lists, "noon").is_none());
}
