//! The loop library: `loop_lib` and `loop_roots`. See `docs/dev/loop-library.md`.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Row};

use super::Result;

/// Where a loop came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A file under a loop root.
    Scan,
    /// A track's loop slot, 1 to 8.
    Slot,
    /// The tape's Save loop.
    Tape,
}

impl Origin {
    pub fn name(self) -> &'static str {
        match self {
            Origin::Scan => "scan",
            Origin::Slot => "slot",
            Origin::Tape => "tape",
        }
    }

    fn named(s: &str) -> Origin {
        match s {
            "slot" => Origin::Slot,
            "tape" => Origin::Tape,
            _ => Origin::Scan,
        }
    }
}

/// Where a loop's tempo came from, most trusted first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempoFrom {
    /// Set with `:beats`.
    User,
    /// A BPM tag.
    Tag,
    /// A WAV `acid` chunk.
    Acid,
    /// The file name.
    Name,
    /// The loop's length alone: a guess.
    Length,
}

impl TempoFrom {
    pub fn name(self) -> &'static str {
        match self {
            TempoFrom::User => "user",
            TempoFrom::Tag => "tag",
            TempoFrom::Acid => "acid",
            TempoFrom::Name => "name",
            TempoFrom::Length => "length",
        }
    }

    fn named(s: &str) -> Option<TempoFrom> {
        Some(match s {
            "user" => TempoFrom::User,
            "tag" => TempoFrom::Tag,
            "acid" => TempoFrom::Acid,
            "name" => TempoFrom::Name,
            "length" => TempoFrom::Length,
            _ => return None,
        })
    }
}

/// One row of `loop_lib`. `id` is 0 for a record not yet inserted.
#[derive(Debug, Clone, PartialEq)]
pub struct Loop {
    pub id: i64,
    pub path: String,
    /// Source frames, end exclusive, at `rate`.
    pub start: u64,
    pub end: u64,
    pub rate: u32,
    pub channels: Option<u16>,
    pub slot: Option<u8>,
    pub origin: Origin,
    pub name: String,
    /// Words, space separated.
    pub tags: String,
    pub bpm: Option<f64>,
    pub beats: Option<u32>,
    pub bpm_from: Option<TempoFrom>,
    pub key: Option<String>,
    pub mtime: Option<i64>,
    pub size: Option<i64>,
}

impl Loop {
    pub fn frames(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.rate.max(1))
    }
}

const COLS: &str = "id, path, start, end, rate, channels, slot, origin, name, tags,
                    bpm, beats, bpm_from, key, mtime, size";

const ORDER: &str = "ORDER BY name COLLATE NOCASE, path, slot";

fn row_to_loop(r: &Row) -> rusqlite::Result<Loop> {
    Ok(Loop {
        id: r.get(0)?,
        path: r.get(1)?,
        start: r.get::<_, i64>(2)? as u64,
        end: r.get::<_, i64>(3)? as u64,
        rate: r.get(4)?,
        channels: r.get(5)?,
        slot: r.get(6)?,
        origin: Origin::named(&r.get::<_, String>(7)?),
        name: r.get(8)?,
        tags: r.get(9)?,
        bpm: r.get(10)?,
        beats: r.get(11)?,
        bpm_from: r
            .get::<_, Option<String>>(12)?
            .as_deref()
            .and_then(TempoFrom::named),
        key: r.get(13)?,
        mtime: r.get(14)?,
        size: r.get(15)?,
    })
}

/// Every loop, by name.
pub fn all(conn: &Connection) -> Result<Vec<Loop>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM loop_lib {ORDER}"))?;
    let rows = stmt.query_map([], row_to_loop)?;
    rows.collect()
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<Loop>> {
    conn.query_row(
        &format!("SELECT {COLS} FROM loop_lib WHERE id = ?1"),
        [id],
        row_to_loop,
    )
    .optional()
}

pub fn count(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM loop_lib", [], |r| r.get(0))
}

/// Loops matching `input`: words matched as prefixes of the name, tags and
/// key, all of which must match; `bpm:120` or `bpm:118..124` by tempo, as
/// the library's `bpm:` matches; `key:Am` by key, ignoring case.
pub fn search(conn: &Connection, input: &str) -> Result<Vec<Loop>> {
    let mut words = Vec::new();
    let mut bpm: Vec<(f64, f64)> = Vec::new();
    let mut keys = Vec::new();
    let mut never = false;
    for term in input.split_whitespace() {
        let (field, text) = match term.split_once(':') {
            Some((f, t)) => (Some(f.to_ascii_lowercase()), t),
            None => (None, term),
        };
        match field.as_deref() {
            Some("bpm") => match super::query::range(text, |t| t.parse().ok(), 1.0) {
                Some(r) => bpm.push(r),
                None => never = true,
            },
            Some("key") => keys.push(text.to_lowercase()),
            _ => words.push((None, term.replace('"', ""))),
        }
    }
    if never {
        return Ok(Vec::new());
    }
    let mut loops = match super::query::fts_query(words) {
        Some(q) => {
            let mut stmt = conn.prepare(&format!(
                "SELECT {COLS} FROM loop_lib WHERE id IN
                   (SELECT rowid FROM loop_fts WHERE loop_fts MATCH ?1) {ORDER}"
            ))?;
            let rows = stmt.query_map([q], row_to_loop)?;
            rows.collect::<Result<Vec<_>>>()?
        }
        None => all(conn)?,
    };
    loops.retain(|l| {
        bpm.iter()
            .all(|(lo, hi)| l.bpm.is_some_and(|b| (*lo..=*hi).contains(&b)))
            && keys
                .iter()
                .all(|k| l.key.as_deref().is_some_and(|key| key.to_lowercase() == *k))
    });
    Ok(loops)
}

/// The `(mtime, size)` recorded for the loop file at `path`, if known.
pub fn stat_of(conn: &Connection, path: &str) -> Result<Option<(i64, i64)>> {
    conn.query_row(
        "SELECT mtime, size FROM loop_lib WHERE path = ?1 AND slot IS NULL",
        [path],
        |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?)),
    )
    .optional()
    .map(|r| match r {
        Some((Some(m), Some(s))) => Some((m, s)),
        _ => None,
    })
}

/// Inserts `l`, a whole-file loop, or replaces what the library holds for
/// its file. A tempo set with `:beats` and a key set with `:key` are kept.
/// Returns the row id.
pub fn upsert_file(conn: &Connection, l: &Loop) -> Result<i64> {
    conn.execute(
        "INSERT INTO loop_lib
           (path, start, end, rate, channels, slot, origin, name, tags,
            bpm, beats, bpm_from, key, mtime, size)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(path) WHERE slot IS NULL DO UPDATE SET
           start = excluded.start, end = excluded.end, rate = excluded.rate,
           channels = excluded.channels, origin = excluded.origin,
           name = excluded.name, tags = excluded.tags,
           bpm = CASE WHEN bpm_from = 'user' THEN bpm ELSE excluded.bpm END,
           beats = CASE WHEN bpm_from = 'user' THEN beats ELSE excluded.beats END,
           bpm_from = CASE WHEN bpm_from = 'user' THEN bpm_from ELSE excluded.bpm_from END,
           key = COALESCE(key, excluded.key),
           mtime = excluded.mtime, size = excluded.size",
        rusqlite::params![
            l.path,
            l.start as i64,
            l.end as i64,
            l.rate,
            l.channels,
            l.origin.name(),
            l.name,
            l.tags,
            l.bpm,
            l.beats,
            l.bpm_from.map(TempoFrom::name),
            l.key,
            l.mtime,
            l.size,
        ],
    )?;
    conn.query_row(
        "SELECT id FROM loop_lib WHERE path = ?1 AND slot IS NULL",
        [&l.path],
        |r| r.get(0),
    )
}

/// Takes loop `id` out of the library. Its file is left alone.
pub fn remove(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM loop_lib WHERE id = ?1", [id])? > 0)
}

/// Sets loop `id`'s beat count, and its tempo from it.
pub fn set_beats(conn: &Connection, id: i64, beats: u32) -> Result<bool> {
    let Some(l) = get(conn, id)? else {
        return Ok(false);
    };
    let bpm = crate::loops::bpm(beats, l.frames(), l.rate);
    Ok(conn.execute(
        "UPDATE loop_lib SET beats = ?2, bpm = ?3, bpm_from = 'user' WHERE id = ?1",
        rusqlite::params![id, beats, bpm],
    )? > 0)
}

/// Sets loop `id`'s key, or clears it.
pub fn set_key(conn: &Connection, id: i64, key: Option<&str>) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE loop_lib SET key = ?2 WHERE id = ?1",
        rusqlite::params![id, key],
    )? > 0)
}

/// A slot's name: the track's title, else its file's stem.
pub(crate) fn slot_name(conn: &Connection, path: &str) -> Result<String> {
    let title: Option<String> = conn
        .query_row(
            "SELECT title FROM tracks WHERE path = ?1 AND title IS NOT NULL AND title != ''",
            [path],
            |r| r.get(0),
        )
        .optional()?;
    Ok(title.unwrap_or_else(|| stem(path)))
}

/// The file name of `path` without its directory or extension.
pub fn stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Moves the rows of `loops`, the slots before the loop library, into
/// `loop_lib`, and empties it. A slot already in `loop_lib` is replaced: an
/// older playr saved it since.
pub(crate) fn migrate_slots(conn: &Connection) -> Result<()> {
    let rows: Vec<(String, u8, i64, i64, u32)> = conn
        .prepare("SELECT path, slot, start, end, rate FROM loops")?
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<_>>()?;
    // Writing only when there is something to move keeps an open read-only,
    // so it does not contend with another connection's writes.
    if rows.is_empty() {
        return Ok(());
    }
    for (path, slot, start, end, rate) in rows {
        let name = slot_name(conn, &path)?;
        put_slot(conn, &path, slot, (start as u64, end as u64), rate, &name)?;
    }
    conn.execute("DELETE FROM loops", [])?;
    Ok(())
}

/// Saves a track's loop slot, replacing what it held.
pub(crate) fn put_slot(
    conn: &Connection,
    path: &str,
    slot: u8,
    (start, end): (u64, u64),
    rate: u32,
    name: &str,
) -> Result<()> {
    let channels: Option<u16> = conn
        .query_row("SELECT channels FROM tracks WHERE path = ?1", [path], |r| {
            r.get(0)
        })
        .optional()?
        .flatten();
    conn.execute(
        "INSERT INTO loop_lib (path, start, end, rate, channels, slot, origin, name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'slot', ?7)
         ON CONFLICT(path, slot) WHERE slot IS NOT NULL DO UPDATE SET
           start = excluded.start, end = excluded.end, rate = excluded.rate,
           bpm = NULL, beats = NULL, bpm_from = NULL",
        rusqlite::params![path, start as i64, end as i64, rate, channels, slot, name],
    )?;
    Ok(())
}

/// Directories given to `playr loops scan`, in path order.
pub fn roots(conn: &Connection) -> Result<Vec<PathBuf>> {
    let mut stmt = conn.prepare("SELECT path FROM loop_roots ORDER BY path")?;
    let paths = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>>>()?;
    Ok(paths.into_iter().map(PathBuf::from).collect())
}

/// Records `root`, canonical, as a loop root. A root inside another is not
/// recorded, and roots inside it are dropped, as [`super::add_root`] does.
pub fn add_root(conn: &Connection, root: &Path) -> Result<()> {
    let Some(path) = root
        .canonicalize()
        .ok()
        .and_then(|p| p.to_str().map(str::to_owned))
    else {
        return Ok(());
    };
    let new = Path::new(&path);
    let existing = roots(conn)?;
    if existing.iter().any(|r| new.starts_with(r)) {
        return Ok(());
    }
    for inner in existing.iter().filter(|r| r.starts_with(new)) {
        conn.execute(
            "DELETE FROM loop_roots WHERE path = ?1",
            [inner.to_string_lossy()],
        )?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO loop_roots (path) VALUES (?1)",
        [&path],
    )?;
    Ok(())
}

/// Whether `path` lies under a loop root.
pub fn under_root(roots: &[PathBuf], path: &Path) -> bool {
    roots.iter().any(|r| path.starts_with(r))
}

/// Forgets loop root `root`, as stored or as it resolves, and the scanned
/// loops under it. Returns how many loops went, or `None` if no root matches.
pub fn forget_root(conn: &Connection, root: &Path) -> Result<Option<usize>> {
    let resolved = root.canonicalize().ok();
    let Some(stored) = roots(conn)?
        .into_iter()
        .find(|r| r == root || resolved.as_deref() == Some(r.as_path()))
    else {
        return Ok(None);
    };
    let prefix = stored.join("").to_string_lossy().into_owned();
    let tx = conn.unchecked_transaction()?;
    let n = tx.execute(
        "DELETE FROM loop_lib WHERE origin = 'scan' AND substr(path, 1, length(?1)) = ?1",
        [&prefix],
    )?;
    tx.execute(
        "DELETE FROM loop_roots WHERE path = ?1",
        [stored.to_string_lossy()],
    )?;
    tx.commit()?;
    Ok(Some(n))
}

/// Scanned loops under `root` whose files are gone. A root that does not
/// resolve has none, as [`super::missing_under`] has none.
pub fn missing_under(conn: &Connection, root: &Path) -> Result<Vec<Loop>> {
    let Ok(root) = root.canonicalize() else {
        return Ok(Vec::new());
    };
    let prefix = root.join("").to_string_lossy().into_owned();
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM loop_lib
         WHERE origin = 'scan' AND substr(path, 1, length(?1)) = ?1 {ORDER}"
    ))?;
    let rows = stmt.query_map([&prefix], row_to_loop)?;
    let mut loops = rows.collect::<Result<Vec<_>>>()?;
    loops.retain(|l| matches!(Path::new(&l.path).try_exists(), Ok(false)));
    Ok(loops)
}
