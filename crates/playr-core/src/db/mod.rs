//! Library database: schema, track upsert, search, playlists.
//!
//! Search uses an FTS5 external-content index kept in sync by triggers. Rescan
//! skips files whose `(mtime, size)` are unchanged, avoiding a tag re-read.

pub mod analysis;
pub mod query;

use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::time::Duration;

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
    // Neither pragma takes effect inside a transaction.
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    // One transaction, so a migration interrupted partway is redone on the
    // next open instead of leaving an index without the library's rows.
    let tx = conn.unchecked_transaction()?;
    // An index from before file names were searchable reads its text from
    // `tracks`, and its triggers write only four columns. Both are replaced,
    // and the index refilled once the schema has made the new ones.
    let old_index: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'tracks_fts')
           AND NOT EXISTS (SELECT 1 FROM pragma_table_info('tracks_fts') WHERE name = 'file')",
        [],
        |r| r.get(0),
    )?;
    // Triggers from playr 0.5.0 and 0.5.1 read only `/` as a separator, so on
    // Windows they indexed a track's whole path as its file name, and a search
    // matched the folders above it. They are replaced and the index refilled.
    let old_triggers: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master
                        WHERE name = 'tracks_ai' AND instr(sql, ':\\') = 0)",
        [],
        |r| r.get(0),
    )?;
    if old_index {
        tx.execute_batch(
            "DROP TRIGGER IF EXISTS tracks_ai;
             DROP TRIGGER IF EXISTS tracks_ad;
             DROP TRIGGER IF EXISTS tracks_au;
             DROP TABLE tracks_fts;",
        )?;
    } else if old_triggers {
        tx.execute_batch(
            "DROP TRIGGER IF EXISTS tracks_ai;
             DROP TRIGGER IF EXISTS tracks_au;",
        )?;
    }
    tx.execute_batch(include_str!("schema.sql"))?;
    // `CREATE TABLE IF NOT EXISTS` leaves a table made by an earlier playr as
    // it was, so a column added to one needs adding here too. The rows stay:
    // `analysis::VERSION` decides which are read again.
    let has_alt: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM pragma_table_info('analysis') WHERE name = 'bpm_alt')",
        [],
        |r| r.get(0),
    )?;
    if !has_alt {
        tx.execute_batch("ALTER TABLE analysis ADD COLUMN bpm_alt REAL;")?;
    }
    if old_index || old_triggers {
        // Updating every row in place fires the new trigger, which indexes it.
        tx.execute_batch("UPDATE tracks SET path = path;")?;
    }
    if version < SCHEMA_VERSION {
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    tx.commit()
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

/// What [`prune_missing`] removed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pruned {
    /// Tracks, and with them their places in playlists.
    pub tracks: usize,
    pub marks: usize,
}

/// Records `root` as a library root, once it resolves. A relative root that
/// does not resolve is ignored, as [`crate::scan::scan_dir`] would scan nothing.
///
/// Callers record a root only once a scan of it has seen an audio file. A
/// directory holding none is not a library root: it would be walked by every
/// later rescan and counted among the roots that make one possible. The test
/// is on files seen, not files added, so a rescan that finds nothing new
/// keeps its root.
///
/// A root that reads as empty is never dropped here. It may be a drive that is
/// not mounted, and forgetting it would make that unrecoverable. A root nested
/// inside another is dropped, since the wider one already covers its files:
/// keeping both walks them twice and counts what is missing twice.
pub fn add_root(conn: &Connection, root: &Path) -> Result<()> {
    let Some(path) = root
        .canonicalize()
        .ok()
        .as_ref()
        .and_then(|p| p.to_str())
        .map(str::to_owned)
    else {
        return Ok(());
    };
    // Component-wise, so `/music2` is not taken to be under `/music`.
    let new = Path::new(&path);
    let existing = roots(conn)?;
    if existing.iter().any(|r| new.starts_with(r)) {
        return Ok(());
    }
    for inner in existing.iter().filter(|r| r.starts_with(new)) {
        conn.execute(
            "DELETE FROM roots WHERE path = ?1",
            [inner.to_string_lossy()],
        )?;
    }
    conn.execute("INSERT OR IGNORE INTO roots (path) VALUES (?1)", [&path])?;
    Ok(())
}

/// Directories previously scanned into this library, in path order.
pub fn roots(conn: &Connection) -> Result<Vec<PathBuf>> {
    let mut stmt = conn.prepare("SELECT path FROM roots ORDER BY path")?;
    let paths = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>>>()?;
    Ok(paths.into_iter().map(PathBuf::from).collect())
}

/// Remembers `path` and how far into it playback had reached, replacing what
/// was there. The row is keyed by a constant, so there is only ever one.
///
/// A path that is not valid UTF-8 is dropped rather than stored lossily: a
/// mangled path would be offered at the next start and open nothing.
pub fn set_resume(conn: &Connection, path: &Path, position: Duration) -> Result<()> {
    let Some(path) = path.to_str() else {
        return Ok(());
    };
    conn.execute(
        "INSERT INTO resume (id, path, position) VALUES (0, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET path = excluded.path, position = excluded.position",
        rusqlite::params![path, position.as_millis() as i64],
    )?;
    Ok(())
}

/// What playr was playing when it last closed, if anything, and how far in.
pub fn resume(conn: &Connection) -> Result<Option<(PathBuf, Duration)>> {
    conn.query_row("SELECT path, position FROM resume WHERE id = 0", [], |r| {
        let path: String = r.get(0)?;
        let ms: i64 = r.get(1)?;
        Ok((PathBuf::from(path), Duration::from_millis(ms.max(0) as u64)))
    })
    .optional()
}

/// Forgets what was playing, so the next start offers nothing.
pub fn clear_resume(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM resume WHERE id = 0", [])?;
    Ok(())
}

/// The recorded root `root` names: the path as stored, a path that resolves
/// to it, or `None` when no root matches.
///
/// The literal spelling has to work, because a root whose directory is gone
/// cannot be canonicalized, and that is the root most worth forgetting.
pub(crate) fn stored_root(conn: &Connection, root: &Path) -> Result<Option<String>> {
    let resolved = root.canonicalize().ok();
    Ok(roots(conn)?
        .into_iter()
        .find(|r| r == root || resolved.as_deref() == Some(r.as_path()))
        .map(|r| r.to_string_lossy().into_owned()))
}

/// Forgets `root` and removes everything the library held under it: the
/// tracks, with their places in playlists, and the marks and analysis of
/// every file under it, in the library or not. `None` if no root matches.
///
/// Unlike [`prune_missing`] this does not ask the filesystem anything. A
/// directory that is no longer a root is no longer part of the library,
/// whether or not its files are still there.
pub fn forget_root(conn: &Connection, root: &Path) -> Result<Option<Pruned>> {
    let Some(stored) = stored_root(conn, root)? else {
        return Ok(None);
    };
    let prefix = Path::new(&stored).join("").to_string_lossy().into_owned();
    let tracks = query::under_path(conn, &prefix)?;
    let marked: Vec<String> = conn
        .prepare("SELECT DISTINCT path FROM marks WHERE substr(path, 1, length(?1)) = ?1")?
        .query_map([&prefix], |r| r.get(0))?
        .collect::<Result<_>>()?;
    let tx = conn.unchecked_transaction()?;
    for t in &tracks {
        tx.execute("DELETE FROM tracks WHERE id = ?1", [t.id])?;
    }
    tx.execute(
        "DELETE FROM analysis WHERE substr(path, 1, length(?1)) = ?1",
        [&prefix],
    )?;
    let mut marks = 0;
    for path in &marked {
        marks += query::clear_marks(&tx, path)?;
    }
    tx.execute("DELETE FROM roots WHERE path = ?1", [&stored])?;
    tx.commit()?;
    Ok(Some(Pruned {
        tracks: tracks.len(),
        marks,
    }))
}

/// `root` as a path prefix, or `None` if it does not resolve.
///
/// The trailing separator keeps `/music` from matching `/music2`.
fn prefix_of(root: &Path) -> Option<String> {
    let root = root.canonicalize().ok()?;
    Some(root.join("").to_string_lossy().into_owned())
}

/// Whether the file at `path` is known to be gone. One whose existence
/// cannot be checked is not.
fn gone(path: &str) -> bool {
    matches!(Path::new(path).try_exists(), Ok(false))
}

/// Tracks under `root` whose file is gone, in library order.
///
/// Only `root` is checked: an unmounted drive elsewhere looks exactly like
/// deleted files. For the same reason a `root` that does not resolve has none.
pub fn missing_under(conn: &Connection, root: &Path) -> Result<Vec<Track>> {
    let Some(prefix) = prefix_of(root) else {
        return Ok(Vec::new());
    };
    let mut tracks = query::under_path(conn, &prefix)?;
    tracks.retain(|t| gone(&t.path));
    Ok(tracks)
}

/// Removes the tracks [`missing_under`] finds, which also removes them from
/// playlists, and the marks and analysis of every file under `root` that is
/// gone, in the library or not.
///
/// Files are checked before the transaction opens, so the write lock is held
/// for the deletes alone.
pub fn prune_missing(conn: &Connection, root: &Path) -> Result<Pruned> {
    let Some(prefix) = prefix_of(root) else {
        return Ok(Pruned::default());
    };
    let tracks = missing_under(conn, root)?;
    let marked: Vec<String> = conn
        .prepare("SELECT DISTINCT path FROM marks WHERE substr(path, 1, length(?1)) = ?1")?
        .query_map([&prefix], |r| r.get(0))?
        .collect::<Result<_>>()?;
    let analysed: Vec<String> = conn
        .prepare("SELECT path FROM analysis WHERE substr(path, 1, length(?1)) = ?1")?
        .query_map([&prefix], |r| r.get(0))?
        .collect::<Result<Vec<String>>>()?
        .into_iter()
        .filter(|p| gone(p))
        .collect();
    let tx = conn.unchecked_transaction()?;
    for t in &tracks {
        tx.execute("DELETE FROM tracks WHERE id = ?1", [t.id])?;
    }
    analysis::delete(&tx, &analysed)?;
    let mut marks = 0;
    for path in marked.iter().filter(|p| gone(p)) {
        marks += query::clear_marks(&tx, path)?;
    }
    tx.commit()?;
    Ok(Pruned {
        tracks: tracks.len(),
        marks,
    })
}
