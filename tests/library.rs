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
fn under_path_matches_case_exactly() {
    let conn = db::open_memory().unwrap();
    db::upsert(&conn, &track("/mnt/music/a.flac", "A", "X", "Y")).unwrap();
    db::upsert(&conn, &track("/mnt/Music/b.flac", "B", "X", "Y")).unwrap();
    let hits = query::under_path(&conn, "/mnt/music/").unwrap();
    let paths: Vec<&str> = hits.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(paths, ["/mnt/music/a.flac"]);
}

#[test]
fn under_path_matches_non_ascii_prefixes() {
    // `substr` and `length` count characters, not bytes; both sides must agree.
    let conn = db::open_memory().unwrap();
    db::upsert(&conn, &track("/m/Bj\u{f6}rk/a.flac", "A", "X", "Y")).unwrap();
    db::upsert(&conn, &track("/m/Bj\u{f6}rn/b.flac", "B", "X", "Y")).unwrap();
    let hits = query::under_path(&conn, "/m/Bj\u{f6}rk/").unwrap();
    let paths: Vec<&str> = hits.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(paths, ["/m/Bj\u{f6}rk/a.flac"]);
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

#[test]
fn a_library_from_a_newer_playr_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let conn = db::open(&path).unwrap();
    conn.pragma_update(None, "user_version", db::SCHEMA_VERSION + 1)
        .unwrap();
    drop(conn);

    let err = db::open(&path).expect_err("a newer library was opened");
    assert!(err.to_string().contains("newer"), "error was {err}");
}

#[test]
fn renaming_a_playlist_keeps_its_tracks_and_refuses_a_taken_name() {
    let mut conn = seeded();
    let all = query::all(&conn).unwrap();
    let late = query::save_playlist(&mut conn, "late", &[all[0].id, all[1].id]).unwrap();
    query::save_playlist(&mut conn, "early", &[all[2].id]).unwrap();
    let names = |conn: &rusqlite::Connection| -> Vec<(String, i64)> {
        query::playlists(conn)
            .unwrap()
            .into_iter()
            .map(|p| (p.name, p.len))
            .collect()
    };

    query::rename_playlist(&conn, late, "night").unwrap();
    assert_eq!(
        names(&conn),
        [("early".to_string(), 1), ("night".to_string(), 2)]
    );

    assert!(query::rename_playlist(&conn, late, "early").is_err());
    assert_eq!(
        names(&conn),
        [("early".to_string(), 1), ("night".to_string(), 2)],
        "a failed rename changed a playlist"
    );
}

/// The seeded library plus a title that names an artist and one with a colon.
fn seeded_for_fields() -> rusqlite::Connection {
    let conn = seeded();
    db::upsert(
        &conn,
        &track("/m/e.flac", "Evans Theme", "Someone", "Tribute"),
    )
    .unwrap();
    db::upsert(&conn, &track("/m/o.flac", "Op: 1", "Composer", "Works")).unwrap();
    conn
}

fn titles(conn: &rusqlite::Connection, input: &str) -> Vec<String> {
    let mut t: Vec<String> = query::search(conn, input)
        .unwrap_or_else(|e| panic!("{input:?} raised {e}"))
        .into_iter()
        .map(|t| t.title.unwrap_or_default())
        .collect();
    t.sort();
    t
}

#[test]
fn a_field_limits_a_term_to_its_column() {
    let conn = seeded_for_fields();
    assert_eq!(
        titles(&conn, "evans"),
        ["Evans Theme", "My Foolish Heart", "Waltz for Debby"]
    );
    assert_eq!(
        titles(&conn, "artist:evans"),
        ["My Foolish Heart", "Waltz for Debby"]
    );
    assert_eq!(titles(&conn, "title:evans"), ["Evans Theme"]);
    assert_eq!(titles(&conn, "album:blue"), ["So What"]);
    assert_eq!(titles(&conn, "albumartist:someone"), ["Evans Theme"]);
    assert_eq!(titles(&conn, "album_artist:someone"), ["Evans Theme"]);
}

#[test]
fn field_terms_match_prefixes_ignore_field_case_and_combine() {
    let conn = seeded_for_fields();
    assert_eq!(
        titles(&conn, "artist:ev"),
        ["My Foolish Heart", "Waltz for Debby"]
    );
    assert_eq!(titles(&conn, "ARTIST:miles"), ["So What"]);
    assert_eq!(titles(&conn, "artist:evans foolish"), ["My Foolish Heart"]);
}

#[test]
fn a_quoted_value_is_a_phrase() {
    let conn = seeded_for_fields();
    assert_eq!(
        titles(&conn, "artist:\"bill evans\""),
        ["My Foolish Heart", "Waltz for Debby"]
    );
    assert!(
        titles(&conn, "artist:\"evans bill\"").is_empty(),
        "word order ignored"
    );
    // Unquoted, only the first word is limited to the field.
    assert_eq!(titles(&conn, "artist:bill theme"), Vec::<String>::new());
    assert_eq!(titles(&conn, "\"waltz for\""), ["Waltz for Debby"]);
}

#[test]
fn a_prefix_that_names_no_field_is_text() {
    let conn = seeded_for_fields();
    assert_eq!(titles(&conn, "op:1"), ["Op: 1"]);
    // A quote before the colon makes it text too.
    assert!(titles(&conn, "\"artist:evans\"").is_empty());
}

#[test]
fn an_empty_field_or_operators_in_a_value_are_harmless() {
    let conn = seeded_for_fields();
    assert!(titles(&conn, "artist:").is_empty());
    assert!(titles(&conn, "artist:\"\"").is_empty());
    for input in [
        "artist:\"a AND b\"",
        "title:*",
        "album:NEAR(",
        "artist:\"unclosed",
    ] {
        let _ = titles(&conn, input);
    }
}

#[test]
fn marks_are_kept_per_path_in_order() {
    use query::Mark;
    let conn = db::open_memory().unwrap();
    let at = |ms| Mark::at_time(std::time::Duration::from_millis(ms), 44100);
    query::add_mark(&conn, "/m/a.flac", at(90_000)).unwrap();
    query::add_mark(&conn, "/m/a.flac", at(15_000)).unwrap();
    query::add_mark(&conn, "/m/a.flac", at(15_000)).unwrap();
    query::add_mark(&conn, "/m/b.flac", at(1_000)).unwrap();

    let a = query::marks(&conn, "/m/a.flac").unwrap();
    assert_eq!(
        a,
        [at(15_000), at(90_000)],
        "unordered, or the duplicate was kept"
    );
    assert_eq!(a[0].frame, 15 * 44100);
    assert_eq!(a[1].time(), std::time::Duration::from_secs(90));

    assert_eq!(query::clear_marks(&conn, "/m/a.flac").unwrap(), 2);
    assert!(query::marks(&conn, "/m/a.flac").unwrap().is_empty());
    assert_eq!(
        query::marks(&conn, "/m/b.flac").unwrap().len(),
        1,
        "cleared another track"
    );
}

#[test]
fn a_library_from_before_marks_gains_the_table_and_keeps_its_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let conn = db::open(&path).unwrap();
    conn.execute_batch("DROP TABLE marks").unwrap();
    drop(conn);

    let conn = db::open(&path).unwrap();
    assert!(query::marks(&conn, "/m/a.flac").unwrap().is_empty());
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        version,
        db::SCHEMA_VERSION,
        "the version moved, locking out older playr"
    );
}

#[test]
fn marks_are_undone_in_the_order_they_were_added() {
    use query::Mark;
    let conn = db::open_memory().unwrap();
    let at = |s| Mark::at_time(std::time::Duration::from_secs(s), 44100);
    for s in [90, 15, 40] {
        query::add_mark(&conn, "/m/a.flac", at(s)).unwrap();
    }
    query::add_mark(&conn, "/m/b.flac", at(5)).unwrap();

    let undone: Vec<Option<Mark>> = (0..4)
        .map(|_| query::remove_last_mark(&conn, "/m/a.flac").unwrap())
        .collect();
    assert_eq!(undone, [Some(at(40)), Some(at(15)), Some(at(90)), None]);
    assert_eq!(
        query::marks(&conn, "/m/b.flac").unwrap(),
        [at(5)],
        "undid another track's mark"
    );
}
