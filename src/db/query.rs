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

/// Turns free-form user input into an FTS5 prefix query.
///
/// Each whitespace-separated token becomes a quoted prefix term, so input
/// containing FTS operators (`AND`, `*`, `"`, `:`) is matched literally rather
/// than being parsed as syntax or raising an error.
fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
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

pub fn delete_playlist(conn: &Connection, playlist_id: i64) -> Result<()> {
    conn.execute("DELETE FROM playlists WHERE id = ?1", [playlist_id])?;
    Ok(())
}
