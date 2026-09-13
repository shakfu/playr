//! Queries over the library: listing, full-text search, playlist CRUD.

use super::{Result, Track};
use rusqlite::{Connection, OptionalExtension, Row};

const COLS: &str = "id, path, title, artist, album, album_artist, track_no, disc_no,
                    year, genre, duration_ms, sample_rate, channels, bit_depth, mtime, size";

fn row_to_track(r: &Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: r.get(0)?,
        path: r.get(1)?,
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        album_artist: r.get(5)?,
        track_no: r.get(6)?,
        disc_no: r.get(7)?,
        year: r.get(8)?,
        genre: r.get(9)?,
        duration_ms: r.get(10)?,
        sample_rate: r.get(11)?,
        channels: r.get(12)?,
        bit_depth: r.get(13)?,
        mtime: r.get(14)?,
        size: r.get(15)?,
    })
}

/// Sort order used everywhere a track list is shown.
const ORDER: &str = "ORDER BY album_artist IS NULL, album_artist, artist, album,
                              disc_no, track_no, title, path";

/// All tracks, in library order.
pub fn all(conn: &Connection) -> Result<Vec<Track>> {
    let sql = format!("SELECT {COLS} FROM tracks {ORDER}");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_track)?;
    rows.collect()
}

pub fn count(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0))
}

/// Fields a search term can be limited to, as typed, and their FTS5 columns.
const FIELDS: &[(&str, &str)] = &[
    ("title", "title"),
    ("artist", "artist"),
    ("album", "album"),
    ("albumartist", "album_artist"),
    ("album_artist", "album_artist"),
];

/// Splits search input into terms: whitespace separates them except inside
/// double quotes, which are removed. A term that starts with a known `field:`,
/// before any quote, is limited to that field's column.
fn terms(input: &str) -> Vec<(Option<&'static str>, String)> {
    let mut out = Vec::new();
    let mut chars = input.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.peek().is_none() {
            return out;
        }
        let (mut text, mut quoted, mut seen_quote, mut colon) = (String::new(), false, false, None);
        while let Some(c) = chars.next_if(|c| quoted || !c.is_whitespace()) {
            match c {
                '"' => (quoted, seen_quote) = (!quoted, true),
                ':' if !seen_quote && colon.is_none() => {
                    colon = Some(text.len());
                    text.push(c);
                }
                _ => text.push(c),
            }
        }
        let field = colon.and_then(|at| {
            let name = text[..at].to_ascii_lowercase();
            FIELDS
                .iter()
                .find(|(typed, _)| *typed == name)
                .map(|(_, column)| (at, *column))
        });
        match field {
            Some((at, column)) => out.push((Some(column), text[at + 1..].to_string())),
            None => out.push((None, text)),
        }
    }
}

/// Turns search input into an FTS5 query of prefix terms, all of which must match.
///
/// Every term is quoted, so FTS operators in the input (`AND`, `*`, `"`) are
/// matched as text rather than parsed or raising an error. `artist:evans`
/// limits a term to one column, and `artist:"bill evans"` a phrase; a prefix
/// that names no field, as in `op:1`, stays part of the text.
fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = terms(input)
        .into_iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(column, text)| {
            let phrase = format!("\"{}\"*", text.replace('"', "\"\""));
            match column {
                Some(column) => format!("{column} : {phrase}"),
                None => phrase,
            }
        })
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// Full-text search over title, artist, album and album artist.
///
/// Results come in library order, so a matching album plays in track order.
/// Empty or whitespace-only input returns an empty vector rather than every
/// track.
pub fn search(conn: &Connection, input: &str) -> Result<Vec<Track>> {
    let Some(q) = fts_query(input) else {
        return Ok(Vec::new());
    };
    let sql = format!(
        "SELECT {COLS} FROM tracks
         WHERE id IN (SELECT rowid FROM tracks_fts WHERE tracks_fts MATCH ?1)
         {ORDER}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([&q], row_to_track)?;
    rows.collect()
}

/// The track stored for `path`, if it is in the library.
pub fn by_path(conn: &Connection, path: &str) -> Result<Option<Track>> {
    let sql = format!("SELECT {COLS} FROM tracks WHERE path = ?1");
    conn.query_row(&sql, [path], row_to_track).optional()
}

/// Tracks whose path starts with `prefix`, compared case-sensitively, in library order.
pub fn under_path(conn: &Connection, prefix: &str) -> Result<Vec<Track>> {
    // Not `LIKE`: it ignores ASCII case, so `/mnt/music/` also matched `/mnt/Music/`.
    let sql = format!("SELECT {COLS} FROM tracks WHERE substr(path, 1, length(?1)) = ?1 {ORDER}");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([prefix], row_to_track)?;
    rows.collect()
}

// --- playlists ---

#[derive(Debug, Clone, PartialEq)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub len: i64,
}

pub fn playlists(conn: &Connection) -> Result<Vec<Playlist>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, COUNT(i.track_id)
         FROM playlists p LEFT JOIN playlist_items i ON i.playlist_id = p.id
         GROUP BY p.id ORDER BY p.name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Playlist {
            id: r.get(0)?,
            name: r.get(1)?,
            len: r.get(2)?,
        })
    })?;
    rows.collect()
}

/// The playlist called `name`: an exact match, else the only case-insensitive one.
///
/// Names are unique case-sensitively, so `Late` and `late` can both exist;
/// matching case-insensitively first would always pick one of them.
pub fn find_playlist<'a>(lists: &'a [Playlist], name: &str) -> Option<&'a Playlist> {
    if let Some(p) = lists.iter().find(|p| p.name == name) {
        return Some(p);
    }
    let mut folded = lists.iter().filter(|p| p.name.eq_ignore_ascii_case(name));
    match (folded.next(), folded.next()) {
        (Some(p), None) => Some(p),
        _ => None,
    }
}

/// Creates or replaces the playlist `name` with `track_ids`, in order.
pub fn save_playlist(conn: &mut Connection, name: &str, track_ids: &[i64]) -> Result<i64> {
    let tx = conn.transaction()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    tx.execute(
        "INSERT INTO playlists (name, created_at) VALUES (?1, ?2)
         ON CONFLICT(name) DO UPDATE SET name = excluded.name",
        rusqlite::params![name, now],
    )?;
    let id: i64 = tx.query_row("SELECT id FROM playlists WHERE name = ?1", [name], |r| {
        r.get(0)
    })?;
    tx.execute("DELETE FROM playlist_items WHERE playlist_id = ?1", [id])?;
    {
        let mut ins = tx.prepare(
            "INSERT INTO playlist_items (playlist_id, position, track_id) VALUES (?1, ?2, ?3)",
        )?;
        for (pos, tid) in track_ids.iter().enumerate() {
            ins.execute(rusqlite::params![id, pos as i64, tid])?;
        }
    }
    tx.commit()?;
    Ok(id)
}

/// Tracks of a playlist, in stored order.
pub fn playlist_tracks(conn: &Connection, playlist_id: i64) -> Result<Vec<Track>> {
    let sql = format!(
        "SELECT {COLS} FROM tracks t
         JOIN playlist_items i ON i.track_id = t.id
         WHERE i.playlist_id = ?1 ORDER BY i.position"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([playlist_id], row_to_track)?;
    rows.collect()
}

/// Renames a playlist, keeping its tracks. Fails if another playlist is
/// already called `name`, since names are unique.
pub fn rename_playlist(conn: &Connection, playlist_id: i64, name: &str) -> Result<()> {
    conn.execute(
        "UPDATE playlists SET name = ?1 WHERE id = ?2",
        rusqlite::params![name, playlist_id],
    )?;
    Ok(())
}

// --- marks ---

/// A marked position in a track, as a source frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub frame: u64,
    /// The source sample rate `frame` counts in.
    pub rate: u32,
}

impl Mark {
    /// A mark at `at` into a track sampled at `rate`.
    pub fn at_time(at: std::time::Duration, rate: u32) -> Self {
        Mark {
            frame: (at.as_secs_f64() * rate as f64).round() as u64,
            rate,
        }
    }

    /// How far into the track the mark is.
    pub fn time(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(self.frame as f64 / self.rate.max(1) as f64)
    }
}

/// The marks in the track at `path`, earliest first.
pub fn marks(conn: &Connection, path: &str) -> Result<Vec<Mark>> {
    let mut stmt = conn.prepare("SELECT frame, rate FROM marks WHERE path = ?1 ORDER BY frame")?;
    let rows = stmt.query_map([path], |r| {
        Ok(Mark {
            // SQLite integers are signed; a frame count stays far below i64::MAX.
            frame: r.get::<_, i64>(0)? as u64,
            rate: r.get(1)?,
        })
    })?;
    rows.collect()
}

/// Adds a mark; a mark already at the same frame is left as it is.
pub fn add_mark(conn: &Connection, path: &str, mark: Mark) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO marks (path, frame, rate) VALUES (?1, ?2, ?3)",
        rusqlite::params![path, mark.frame as i64, mark.rate],
    )?;
    Ok(())
}

/// Removes the mark most recently added to the track at `path`, and returns it.
///
/// Marks are removed in the reverse of the order they were added, whatever
/// their positions. The row id records that order, so it survives a restart.
pub fn remove_last_mark(conn: &Connection, path: &str) -> Result<Option<Mark>> {
    let last = conn
        .query_row(
            "SELECT rowid, frame, rate FROM marks WHERE path = ?1 ORDER BY rowid DESC LIMIT 1",
            [path],
            |r| {
                let mark = Mark {
                    frame: r.get::<_, i64>(1)? as u64,
                    rate: r.get(2)?,
                };
                Ok((r.get::<_, i64>(0)?, mark))
            },
        )
        .optional()?;
    let Some((rowid, mark)) = last else {
        return Ok(None);
    };
    conn.execute("DELETE FROM marks WHERE rowid = ?1", [rowid])?;
    Ok(Some(mark))
}

/// Removes every mark in the track at `path`, returning how many there were.
pub fn clear_marks(conn: &Connection, path: &str) -> Result<usize> {
    conn.execute("DELETE FROM marks WHERE path = ?1", [path])
}

pub fn delete_playlist(conn: &Connection, playlist_id: i64) -> Result<()> {
    conn.execute("DELETE FROM playlists WHERE id = ?1", [playlist_id])?;
    Ok(())
}
