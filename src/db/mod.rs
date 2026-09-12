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
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
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

/// In-memory library, for tests.
pub fn open_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    init(&conn)?;
    Ok(conn)
}

fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(include_str!("schema.sql"))
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

/// Removes rows whose file no longer exists on disk. Returns the count removed.
pub fn prune_missing(conn: &Connection) -> Result<usize> {
    let gone: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT id, path FROM tracks")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        rows.filter_map(|r| r.ok())
            .filter(|(_, p)| !Path::new(p).exists())
            .map(|(id, _)| id)
            .collect()
    };
    for id in &gone {
        conn.execute("DELETE FROM tracks WHERE id = ?1", [id])?;
    }
    Ok(gone.len())
}
