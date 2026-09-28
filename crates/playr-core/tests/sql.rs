//! `:sql` statements: what they may read, and every limit on them.

use std::path::{Path, PathBuf};

use playr_core::db::{self, query, sql, Track};

fn track(path: &str, title: &str, year: i32) -> Track {
    Track {
        path: path.into(),
        title: Some(title.into()),
        year: Some(year),
        mtime: 1,
        size: 1,
        ..Default::default()
    }
}

/// A library file of three tracks, one analysed at 124 BPM, a playlist of
/// two and a mark.
fn library() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("library.db");
    let mut conn = db::open(&file).unwrap();
    let ids: Vec<i64> = [
        ("/m/a.flac", "A", 1959),
        ("/m/b.flac", "B", 1965),
        ("/m/c.flac", "C", 1971),
    ]
    .iter()
    .map(|(p, t, y)| db::upsert(&conn, &track(p, t, *y)).unwrap())
    .collect();
    conn.execute(
        "INSERT INTO analysis (path, mtime, size, version, rate, frames, skipped, lossless, bpm, bpm_conf)
         VALUES ('/m/b.flac', 1, 1, ?1, 44100, 1, 0, 1, 124.0, 0.9)",
        [playr_core::analysis::VERSION],
    )
    .unwrap();
    query::save_playlist(&mut conn, "late", &[ids[2], ids[0]]).unwrap();
    conn.execute(
        "INSERT INTO marks (path, frame, rate) VALUES ('/m/a.flac', 88200, 44100)",
        [],
    )
    .unwrap();
    (dir, file)
}

fn run(file: &Path, sql: &str) -> Result<Vec<String>, String> {
    sql::paths(file, sql).map(|ps| {
        ps.iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    })
}

#[test]
fn the_views_answer_with_paths_in_the_statement_s_order() {
    let (_dir, file) = library();
    assert_eq!(
        run(
            &file,
            "SELECT path FROM library WHERE year < 1970 ORDER BY year DESC"
        ),
        Ok(vec!["/m/b.flac".into(), "/m/a.flac".into()])
    );
    assert_eq!(
        run(
            &file,
            "SELECT path FROM library WHERE tempo BETWEEN 120 AND 130"
        ),
        Ok(vec!["/m/b.flac".into()])
    );
    assert_eq!(
        run(
            &file,
            "SELECT path FROM playlists WHERE playlist = 'late' ORDER BY position"
        ),
        Ok(vec!["/m/c.flac".into(), "/m/a.flac".into()])
    );
    assert_eq!(
        run(
            &file,
            "SELECT l.path FROM library l JOIN marks m USING (path) WHERE m.time = 2"
        ),
        Ok(vec!["/m/a.flac".into()])
    );
    assert_eq!(
        run(&file, "SELECT title FROM library"),
        Err("the statement must select a column named path".into())
    );
}

#[test]
fn only_a_select_through_the_views_runs() {
    let (_dir, file) = library();
    for statement in [
        "DELETE FROM tracks",
        "UPDATE library SET year = 1",
        "SELECT path FROM main.tracks",
        "SELECT name AS path FROM sqlite_master",
        "WITH t AS (SELECT path FROM main.tracks) SELECT path FROM t",
        "ATTACH DATABASE '/etc/hosts' AS x",
        "PRAGMA query_only = OFF",
        "CREATE TEMP TABLE x (path TEXT)",
        "SELECT load_extension('x') AS path",
        "SELECT hex(randomblob(10)) AS path",
        "SELECT path FROM library; DELETE FROM tracks",
    ] {
        assert!(run(&file, statement).is_err(), "{statement} ran");
    }
    assert_eq!(
        run(&file, "SELECT path FROM library; SELECT path FROM library"),
        Err("one statement only".into())
    );
    // Nothing was changed along the way.
    let conn = db::open(&file).unwrap();
    assert_eq!(query::all(&conn).unwrap().len(), 3);
}

#[test]
fn rows_values_and_time_are_limited() {
    let (_dir, file) = library();
    let rows = format!(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i <= {})
         SELECT 'x' AS path FROM n",
        sql::ROW_LIMIT
    );
    assert_eq!(
        run(&file, &rows),
        Err(format!("more than {} rows; add a LIMIT", sql::ROW_LIMIT))
    );
    let long = format!(
        "SELECT printf('%.*c', {}, 'x') AS path",
        sql::LENGTH_LIMIT + 1
    );
    assert!(run(&file, &long).is_err(), "a value past the length limit");
    let endless = "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n)
                   SELECT 'x' AS path FROM n WHERE i < 0";
    assert_eq!(run(&file, endless), Err("stopped after 2 s".into()));
}
