//! Queries over the library: listing, full-text search, playlist CRUD.

use std::collections::{HashMap, HashSet};

use super::{Result, Track};
use crate::analysis;
use crate::columns::{cell, Cell, Column, Measures};
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

/// [`COLS`] and [`ORDER`] qualified, for the join with `analysis`, which has
/// a `path`, an `mtime` and a `size` of its own.
const COLS_T: &str = "t.id, t.path, t.title, t.artist, t.album, t.album_artist, t.track_no,
                      t.disc_no, t.year, t.genre, t.duration_ms, t.sample_rate, t.channels,
                      t.bit_depth, t.mtime, t.size";

const ORDER_T: &str = "ORDER BY t.album_artist IS NULL, t.album_artist, t.artist, t.album,
                                t.disc_no, t.track_no, t.title, t.path";

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

/// How far either side of a bare `bpm:128` a tempo still matches.
const BPM_WITHIN: f64 = 1.0;

/// What a `field:` prefix limits a search term to.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Field {
    /// An FTS5 column.
    Text(&'static str),
    /// The tempos `playr analyze` recorded, matched in SQL.
    Tempo,
    /// A column's value, matched in Rust after the query.
    Column(Column),
    /// A finding of `playr analyze`, or `duplicate` or `unanalysed`.
    Is,
}

/// The field `name` names: a column's name, as `:columns` takes it, or one
/// of the search-only names `file`, `is`, and `bpm` and `albumartist` kept
/// from before columns were searchable.
fn field(name: &str) -> Option<Field> {
    match name {
        "file" => Some(Field::Text("file")),
        "albumartist" => Some(Field::Text("album_artist")),
        "bpm" => Some(Field::Tempo),
        "is" => Some(Field::Is),
        _ => Some(match Column::named(name)? {
            Column::Title => Field::Text("title"),
            Column::Artist => Field::Text("artist"),
            Column::Album => Field::Text("album"),
            Column::AlbumArtist => Field::Text("album_artist"),
            Column::Tempo => Field::Tempo,
            column => Field::Column(column),
        }),
    }
}

/// Splits search input into terms: whitespace separates them except inside
/// double quotes, which are removed. A term that starts with a known `field:`,
/// before any quote, is limited to that field.
fn terms(input: &str) -> Vec<(Option<Field>, String)> {
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
        let field = colon.and_then(|at| Some((at, field(&text[..at].to_ascii_lowercase())?)));
        match field {
            Some((at, field)) => out.push((Some(field), text[at + 1..].to_string())),
            None => out.push((None, text)),
        }
    }
}

/// The inclusive range `text` names: `lo..hi`, `lo..`, `..hi`, or a bare
/// value for `near` either side of it. `None` when it names no range, which
/// matches nothing rather than everything.
fn range(text: &str, value: impl Fn(&str) -> Option<f64>, near: f64) -> Option<(f64, f64)> {
    let value = |t: &str| value(t.trim()).filter(|n| n.is_finite());
    match text.split_once("..") {
        Some(("", "")) => None,
        Some((lo, "")) => Some((value(lo)?, f64::INFINITY)),
        Some(("", hi)) => Some((f64::NEG_INFINITY, value(hi)?)),
        Some((lo, hi)) => Some((value(lo)?, value(hi)?)),
        None => {
            let at = value(text)?;
            Some((at - near, at + near))
        }
    }
}

/// Whole seconds in `5:00`, `1:02:03` or `300`.
fn seconds(text: &str) -> Option<f64> {
    text.split(':').try_fold(0.0, |total, part| {
        let n = part.parse::<u32>().ok()?;
        Some(total * 60.0 + n as f64)
    })
}

/// How far either side of a bare value a column still matches: half the
/// step the column is shown in, so `loudness:-14` matches what reads -14.0.
fn near(column: Column) -> f64 {
    match column {
        Column::Loudness => 0.05,
        Column::Peak => 0.0005,
        _ => 0.0,
    }
}

/// What a finding, `duplicate` or `unanalysed` names after `is:`, and the
/// finding's name in [`Finding::name`], if it is one.
const IS: &[(&str, Is)] = &[
    ("unreadable", Is::Finding("unreadable")),
    ("damaged", Is::Finding("damaged")),
    ("no-checksum", Is::Finding("no checksum")),
    ("wrong-length", Is::Finding("wrong length")),
    ("padded", Is::Finding("padded")),
    ("lossy", Is::Finding("possible lossy source")),
    ("upsampled", Is::Finding("possible upsampling")),
    ("duplicate", Is::Duplicate),
    ("unanalysed", Is::Unanalysed),
    ("unanalyzed", Is::Unanalysed),
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Is {
    Finding(&'static str),
    Duplicate,
    Unanalysed,
}

/// A term matched against each track after the query.
#[derive(Debug, Clone, PartialEq)]
enum Filter {
    /// A number within an inclusive range; seconds, for `time`.
    Within(Column, f64, f64),
    /// Text holding this, lowercased.
    Holds(Column, String),
    Is(Is),
    /// A value that names nothing, such as `year:soon`.
    Never,
}

impl Filter {
    fn new(field: Field, text: &str) -> Filter {
        let filter = match field {
            Field::Is => IS
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(text))
                .map(|(_, is)| Filter::Is(*is)),
            Field::Column(column) if column.numeric() => {
                let value = |t: &str| match column {
                    Column::Time => seconds(t),
                    _ => t.parse::<f64>().ok(),
                };
                range(text, value, near(column)).map(|(lo, hi)| Filter::Within(column, lo, hi))
            }
            Field::Column(column) => {
                (!text.is_empty()).then(|| Filter::Holds(column, text.to_lowercase()))
            }
            Field::Text(_) | Field::Tempo => None,
        };
        filter.unwrap_or(Filter::Never)
    }

    fn measured(&self) -> bool {
        matches!(self, Filter::Within(column, ..) if column.measured())
    }
}

/// Keeps the tracks every filter matches, reading only what the filters need.
fn filter(conn: &Connection, tracks: Vec<Track>, filters: &[Filter]) -> Result<Vec<Track>> {
    if filters.contains(&Filter::Never) {
        return Ok(Vec::new());
    }
    let measures = match filters.iter().any(Filter::measured) {
        true => super::analysis::measures(conn, &tracks)?,
        false => HashMap::new(),
    };
    let checks = filters.iter().any(|f| matches!(f, Filter::Is(_)));
    // Duplicates are found across the whole library, not the tracks at hand.
    let library = match checks {
        true => all(conn)?,
        false => Vec::new(),
    };
    let current = match checks {
        true => super::analysis::current(conn, &library)?,
        false => HashMap::new(),
    };
    let duplicates: HashSet<String> = match filters.contains(&Filter::Is(Is::Duplicate)) {
        true => analysis::duplicates(&library, &current)
            .into_iter()
            .flatten()
            .collect(),
        false => HashSet::new(),
    };
    let found = |t: &Track| -> Vec<&'static str> {
        current
            .get(&t.path)
            .map(|a| analysis::findings(a).iter().map(|f| f.name()).collect())
            .unwrap_or_default()
    };
    let matches = |t: &Track, f: &Filter| match f {
        Filter::Within(column, lo, hi) => {
            let measures = measures.get(&t.path).copied().unwrap_or_default();
            match cell(t, measures, *column) {
                Cell::Number(n) if *column == Column::Time => (lo..=hi).contains(&&n.floor()),
                Cell::Number(n) => (lo..=hi).contains(&&n),
                _ => false,
            }
        }
        Filter::Holds(column, text) => match cell(t, Measures::default(), *column) {
            Cell::Text(s) => s.to_lowercase().contains(text),
            _ => false,
        },
        Filter::Is(Is::Finding(name)) => found(t).contains(name),
        Filter::Is(Is::Duplicate) => duplicates.contains(&t.path),
        Filter::Is(Is::Unanalysed) => !current.contains_key(&t.path),
        Filter::Never => false,
    };
    Ok(tracks
        .into_iter()
        .filter(|t| filters.iter().all(|f| matches(t, f)))
        .collect())
}

/// Turns search input into an FTS5 query of prefix terms, all of which must match.
///
/// Every term is quoted, so FTS operators in the input (`AND`, `*`, `"`) are
/// matched as text rather than parsed or raising an error. `artist:evans`
/// limits a term to one column, and `artist:"bill evans"` a phrase; a prefix
/// that names no field, as in `op:1`, stays part of the text.
fn fts_query(terms: Vec<(Option<&str>, String)>) -> Option<String> {
    let terms: Vec<String> = terms
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

/// Full-text search over title, artist, album and album artist; over the
/// tempos `playr analyze` recorded with `tempo:` or `bpm:`; over every other
/// column by its name; and over findings with `is:`.
///
/// A track matches on its BPM tag when it has one; otherwise on the tempo
/// measured or on the metrical level either side of it, so a track recorded
/// at 87 BPM is found by `bpm:174` as well.
///
/// Results come in library order, so a matching album plays in track order.
/// Empty or whitespace-only input returns an empty vector rather than every
/// track. A field term alone matches every track it admits; a track keeps
/// its place in library order either way.
pub fn search(conn: &Connection, input: &str) -> Result<Vec<Track>> {
    let (mut text, mut tempo, mut filters) = (Vec::new(), None, Vec::new());
    for (field, value) in terms(input) {
        match field {
            None => text.push((None, value)),
            Some(Field::Text(column)) => text.push((Some(column), value)),
            Some(Field::Tempo) => {
                tempo.get_or_insert(value);
            }
            Some(field) => filters.push(Filter::new(field, &value)),
        }
    }
    let fts = fts_query(text);
    let tracks = match (tempo, &fts) {
        (Some(tempo), _) => by_tempo(conn, &tempo, fts.as_deref())?,
        (None, Some(q)) => by_text(conn, q)?,
        (None, None) if !filters.is_empty() => all(conn)?,
        (None, None) => return Ok(Vec::new()),
    };
    match filters.is_empty() {
        true => Ok(tracks),
        false => filter(conn, tracks, &filters),
    }
}

fn by_text(conn: &Connection, q: &str) -> Result<Vec<Track>> {
    let sql = format!(
        "SELECT {COLS} FROM tracks
         WHERE id IN (SELECT rowid FROM tracks_fts WHERE tracks_fts MATCH ?1)
         {ORDER}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([q], row_to_track)?;
    rows.collect()
}

/// Tracks whose tempo is in the range `text` names, and that match `fts`: the
/// tempo shown, as `analysis::bpm_of` gives it, or what the analysis read,
/// at either level.
fn by_tempo(conn: &Connection, text: &str, fts: Option<&str>) -> Result<Vec<Track>> {
    let number = |t: &str| t.parse::<f64>().ok();
    // A range that parses as nothing, such as `bpm:fast`, matches nothing.
    let Some((lo, hi)) = range(text, number, BPM_WITHIN) else {
        return Ok(Vec::new());
    };
    let matching = match fts.is_some() {
        true => "AND t.id IN (SELECT rowid FROM tracks_fts WHERE tracks_fts MATCH ?4)",
        false => "",
    };
    let sql = format!(
        "SELECT {COLS_T} FROM tracks t
           JOIN analysis a ON a.path = t.path AND a.mtime = t.mtime AND a.size = t.size
           LEFT JOIN tempo_fix f ON f.path = a.path
          WHERE a.version = ?5
            AND (COALESCE(a.grid_bpm, a.bpm_tag, CASE WHEN a.bpm_conf >= ?3 THEN a.bpm END)
                   * COALESCE(f.factor, 1) BETWEEN ?1 AND ?2
                 OR CASE
                      WHEN a.bpm_tag IS NOT NULL THEN a.bpm_tag BETWEEN ?1 AND ?2
                      WHEN a.bpm_conf >= ?3 THEN a.bpm BETWEEN ?1 AND ?2
                                              OR a.bpm_alt BETWEEN ?1 AND ?2
                      ELSE 0
                    END)
            {matching}
          {ORDER_T}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params![
            lo,
            hi,
            crate::analysis::tempo::MIN_CONFIDENCE,
            fts.unwrap_or_default(),
            crate::analysis::VERSION,
        ],
        row_to_track,
    )?;
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

/// What a list of search results came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    /// As typed in the search box.
    Text(String),
    /// A `:sql` statement.
    Sql(String),
}

impl Query {
    pub fn text(&self) -> &str {
        match self {
            Query::Text(q) | Query::Sql(q) => q,
        }
    }
}

/// A search kept by name.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedSearch {
    pub id: i64,
    pub name: String,
    pub query: Query,
    /// Sort keys as `:sort` takes them, comma-separated; empty for SQL.
    pub sort: String,
}

/// Every saved search, by name.
pub fn searches(conn: &Connection) -> Result<Vec<SavedSearch>> {
    let mut stmt =
        conn.prepare("SELECT id, name, kind, query, sort FROM searches ORDER BY name")?;
    let rows = stmt.query_map([], |r| {
        let (kind, text): (String, String) = (r.get(2)?, r.get(3)?);
        Ok(SavedSearch {
            id: r.get(0)?,
            name: r.get(1)?,
            query: match kind.as_str() {
                "sql" => Query::Sql(text),
                _ => Query::Text(text),
            },
            sort: r.get(4)?,
        })
    })?;
    rows.collect()
}

/// Creates or replaces the saved search `name`.
pub fn save_search(conn: &Connection, name: &str, query: &Query, sort: &str) -> Result<()> {
    let kind = match query {
        Query::Text(_) => "search",
        Query::Sql(_) => "sql",
    };
    conn.execute(
        "INSERT INTO searches (name, kind, query, sort) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(name) DO UPDATE SET kind = excluded.kind, query = excluded.query,
           sort = excluded.sort",
        [name, kind, query.text(), sort],
    )?;
    Ok(())
}

pub fn delete_search(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM searches WHERE id = ?1", [id])?;
    Ok(())
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

/// Removes the marks at `remove` and adds `add` in the track at `path`, in one
/// transaction: all of them or, on an error, none.
pub fn change_marks(conn: &mut Connection, path: &str, remove: &[u64], add: &[Mark]) -> Result<()> {
    let tx = conn.transaction()?;
    for &frame in remove {
        remove_mark(&tx, path, frame)?;
    }
    for &mark in add {
        add_mark(&tx, path, mark)?;
    }
    tx.commit()
}

/// Removes the mark at `frame` in the track at `path`. Returns whether one
/// was there.
pub fn remove_mark(conn: &Connection, path: &str, frame: u64) -> Result<bool> {
    let gone = conn.execute(
        "DELETE FROM marks WHERE path = ?1 AND frame = ?2",
        rusqlite::params![path, frame as i64],
    )?;
    Ok(gone > 0)
}

/// Moves the mark at `from` to `to`, keeping its place in the order marks were
/// added, so undo still takes the most recent. Returns whether it moved: it
/// does not when `from` holds no mark, or `to` already holds one.
pub fn move_mark(conn: &Connection, path: &str, from: u64, to: u64) -> Result<bool> {
    if from == to {
        return Ok(false);
    }
    let moved = conn.execute(
        "UPDATE OR IGNORE marks SET frame = ?3 WHERE path = ?1 AND frame = ?2",
        rusqlite::params![path, from as i64, to as i64],
    )?;
    Ok(moved > 0)
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

/// The loops saved in the track at `path`: slot, start and end frame.
pub fn loops(conn: &Connection, path: &str) -> Result<Vec<(u8, u64, u64)>> {
    let mut stmt =
        conn.prepare("SELECT slot, start, end FROM loops WHERE path = ?1 ORDER BY slot")?;
    let rows = stmt.query_map([path], |r| {
        Ok((
            r.get(0)?,
            r.get::<_, i64>(1)? as u64,
            r.get::<_, i64>(2)? as u64,
        ))
    })?;
    rows.collect()
}

/// Saves frames `start..end` at `rate` as loop `slot` of the track at `path`,
/// replacing what the slot held.
pub fn save_loop(
    conn: &Connection,
    path: &str,
    slot: u8,
    (start, end): (u64, u64),
    rate: u32,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO loops (path, slot, start, end, rate) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![path, slot, start as i64, end as i64, rate],
    )?;
    Ok(())
}

/// Empties loop `slot` of the track at `path`. Returns whether it held one.
pub fn clear_loop(conn: &Connection, path: &str, slot: u8) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM loops WHERE path = ?1 AND slot = ?2",
        rusqlite::params![path, slot],
    )? > 0)
}

/// The hot cues set in the track at `path`: slot and time in seconds.
pub fn hot_cues(conn: &Connection, path: &str) -> Result<Vec<(u8, f64)>> {
    let mut stmt = conn.prepare("SELECT slot, at FROM hot_cues WHERE path = ?1 ORDER BY slot")?;
    let rows = stmt.query_map([path], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// Sets hot cue `slot` of the track at `path` to `at` seconds, or empties it.
pub fn set_hot_cue(conn: &Connection, path: &str, slot: u8, at: Option<f64>) -> Result<()> {
    match at {
        Some(at) => conn.execute(
            "INSERT OR REPLACE INTO hot_cues (path, slot, at) VALUES (?1, ?2, ?3)",
            rusqlite::params![path, slot, at],
        )?,
        None => conn.execute(
            "DELETE FROM hot_cues WHERE path = ?1 AND slot = ?2",
            rusqlite::params![path, slot],
        )?,
    };
    Ok(())
}

/// Empties every loop slot of the track at `path`, returning how many held one.
pub fn clear_loops(conn: &Connection, path: &str) -> Result<usize> {
    conn.execute("DELETE FROM loops WHERE path = ?1", [path])
}

/// Removes every mark in the track at `path`, returning how many there were.
pub fn clear_marks(conn: &Connection, path: &str) -> Result<usize> {
    conn.execute("DELETE FROM marks WHERE path = ?1", [path])
}

pub fn delete_playlist(conn: &Connection, playlist_id: i64) -> Result<()> {
    conn.execute("DELETE FROM playlists WHERE id = ?1", [playlist_id])?;
    Ok(())
}
