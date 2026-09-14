//! Library database: schema, track upsert, search, playlists.
//!
//! Search uses an FTS5 external-content index kept in sync by triggers. Rescan
//! skips files whose `(mtime, size)` are unchanged, avoiding a tag re-read.

pub mod query;

use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, rusqlite::Error>;

/// One row of `tracks`. `id` is 0 for a record not yet inserted.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub id: i64,
    pub path: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub duration_ms: Option<i64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub bit_depth: Option<u8>,
    pub mtime: i64,
    pub size: i64,
}

impl Track {
    /// Display title, falling back to the file stem when untagged.
    pub fn display_title(&self) -> String {
        match self.title.as_deref() {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => Path::new(&self.path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.path.clone()),
        }
    }

    pub fn display_artist(&self) -> &str {
        self.artist
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("Unknown Artist")
    }

    pub fn display_album(&self) -> &str {
        self.album
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("Unknown Album")
    }
}

/// Default library path: `$XDG_DATA_HOME/playr/library.db`, else `~/.local/share`.
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::home_dir().map(|h| h.join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("playr").join("library.db")
}

/// Opens (creating if needed) the library at `path` and applies the schema.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(path)?;
    init(&conn)?;
    Ok(conn)
}

/// Empty in-memory library, used when no library file exists yet.
pub fn open_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    init(&conn)?;
    Ok(conn)
}

/// Schema version, kept in `PRAGMA user_version`.
///
/// 0: playr 0.1.0, `mtime` in seconds. 1: `mtime` in nanoseconds. A version 0
/// row never matches a nanosecond mtime, so its file is re-read on the next
/// scan; no migration is needed.
pub const SCHEMA_VERSION: i64 = 1;

fn init(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    // Checked before the schema runs, which could otherwise add tables and
    // triggers this version knows to a library it does not understand.
    if version > SCHEMA_VERSION {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
            Some(format!(
                "library schema version {version} is newer than this playr supports ({SCHEMA_VERSION})"
            )),
        ));
    }
    // An index from before file names were searchable reads its text from
    // `tracks`, and its triggers write only four columns. Both are replaced,
    // and the index refilled once the schema has made the new ones.
    let old_index: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'tracks_fts')
           AND NOT EXISTS (SELECT 1 FROM pragma_table_info('tracks_fts') WHERE name = 'file')",
        [],
        |r| r.get(0),
    )?;
    // Triggers from playr 0.5.0 and 0.5.1 read only `/` as a separator, so on
    // Windows they indexed a track's whole path as its file name, and a search
    // matched the folders above it. They are replaced and the index refilled.
    let old_triggers: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master
                        WHERE name = 'tracks_ai' AND instr(sql, ':\\') = 0)",
        [],
        |r| r.get(0),
    )?;
    if old_index {
        conn.execute_batch(
            "DROP TRIGGER IF EXISTS tracks_ai;
             DROP TRIGGER IF EXISTS tracks_ad;
             DROP TRIGGER IF EXISTS tracks_au;
             DROP TABLE tracks_fts;",
        )?;
    } else if old_triggers {
        conn.execute_batch(
            "DROP TRIGGER IF EXISTS tracks_ai;
             DROP TRIGGER IF EXISTS tracks_au;",
        )?;
    }
    conn.execute_batch(include_str!("schema.sql"))?;
    if old_index || old_triggers {
        // Updating every row in place fires the new trigger, which indexes it.
        conn.execute_batch("BEGIN; UPDATE tracks SET path = path; COMMIT;")?;
    }
    if version < SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}

/// Returns the `(mtime, size)` recorded for `path`, if the file is known.
pub fn stat_of(conn: &Connection, path: &str) -> Result<Option<(i64, i64)>> {
    conn.query_row(
        "SELECT mtime, size FROM tracks WHERE path = ?1",
        [path],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
}

/// Inserts `t`, or replaces the existing row for the same path. Returns the row id.
pub fn upsert(conn: &Connection, t: &Track) -> Result<i64> {
    conn.execute(
        "INSERT INTO tracks
           (path, title, artist, album, album_artist, track_no, disc_no, year,
            genre, duration_ms, sample_rate, channels, bit_depth, mtime, size)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
         ON CONFLICT(path) DO UPDATE SET
           title=excluded.title, artist=excluded.artist, album=excluded.album,
           album_artist=excluded.album_artist, track_no=excluded.track_no,
           disc_no=excluded.disc_no, year=excluded.year, genre=excluded.genre,
           duration_ms=excluded.duration_ms, sample_rate=excluded.sample_rate,
           channels=excluded.channels, bit_depth=excluded.bit_depth,
           mtime=excluded.mtime, size=excluded.size",
        rusqlite::params![
            t.path,
            t.title,
            t.artist,
            t.album,
            t.album_artist,
            t.track_no,
            t.disc_no,
            t.year,
            t.genre,
            t.duration_ms,
            t.sample_rate,
            t.channels,
            t.bit_depth,
            t.mtime,
            t.size,
        ],
    )?;
    conn.query_row("SELECT id FROM tracks WHERE path = ?1", [&t.path], |r| {
        r.get(0)
    })
}

/// Removes rows under `root` whose file is gone. Returns the count removed.
///
/// Only `root` is checked: an unmounted drive elsewhere looks exactly like
/// deleted files, and deleting its rows also empties its playlists. For the
/// same reason a `root` that does not resolve prunes nothing, and a file whose
/// existence cannot be checked is kept.
pub fn prune_missing(conn: &Connection, root: &Path) -> Result<usize> {
    let Ok(root) = root.canonicalize() else {
        return Ok(0);
    };
    // The trailing separator keeps `/music` from matching `/music2`.
    let prefix = root.join("");
    let gone: Vec<i64> = query::under_path(conn, &prefix.to_string_lossy())?
        .into_iter()
        .filter(|t| matches!(Path::new(&t.path).try_exists(), Ok(false)))
        .map(|t| t.id)
        .collect();
    for id in &gone {
        conn.execute("DELETE FROM tracks WHERE id = ?1", [id])?;
    }
    Ok(gone.len())
}
