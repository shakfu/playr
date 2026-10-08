//! `:sql`: a `SELECT` the listener writes, run on the library under limits.
//!
//! The statement reads stable views, not the tables, so a saved query
//! outlasts schema changes: `library`, one row per track with columns named as
//! the search fields are; `playlists`, one row per playlist entry; and
//! `marks`. It runs on its own read-only connection, and an authorizer
//! refuses anything but `SELECT`, reads through those views and the functions
//! in [`FUNCTIONS`]. That keeps out writes, `ATTACH`, which could open any
//! file the host can read, and `PRAGMA`. The page can send it, so it is also
//! stopped after [`TIME_LIMIT`], refused past [`ROW_LIMIT`] rows, and no value
//! it builds may exceed [`LENGTH_LIMIT`] bytes.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::limits::Limit;
use rusqlite::{Connection, OpenFlags};

use crate::analysis::{tempo, VERSION};

/// How long a statement may run before it is stopped.
pub const TIME_LIMIT: Duration = Duration::from_secs(2);
/// How many rows a statement may return.
pub const ROW_LIMIT: usize = 100_000;
/// The longest string or blob a statement may build, in bytes.
pub const LENGTH_LIMIT: i32 = 1_000_000;

/// The views a statement may read, and what each reads.
const VIEWS: [&str; 3] = ["library", "playlists", "marks"];

/// Functions a statement may call: text, number, date, aggregate and window
/// functions. Not `randomblob` or `zeroblob`, which build large values, nor
/// `load_extension`.
const FUNCTIONS: &[&str] = &[
    "abs",
    "avg",
    "char",
    "coalesce",
    "count",
    "date",
    "datetime",
    "glob",
    "group_concat",
    "ifnull",
    "iif",
    "instr",
    "julianday",
    "length",
    "like",
    "lower",
    "ltrim",
    "max",
    "min",
    "nullif",
    "printf",
    "format",
    "random",
    "replace",
    "round",
    "rtrim",
    "sign",
    "strftime",
    "string_agg",
    "substr",
    "substring",
    "sum",
    "time",
    "total",
    "trim",
    "typeof",
    "unicode",
    "unixepoch",
    "upper",
    "row_number",
    "rank",
    "dense_rank",
    "percent_rank",
    "cume_dist",
    "ntile",
    "lag",
    "lead",
    "first_value",
    "last_value",
    "nth_value",
];

/// The paths the rows of `sql` name in its `path` column, in its order, run
/// on the library file `library`. The error is SQLite's, or a limit's, in
/// words to show.
pub fn paths(library: &Path, sql: &str) -> Result<Vec<PathBuf>, String> {
    let conn = open(library).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let _ = conn.progress_handler(1000, Some(move || started.elapsed() > TIME_LIMIT));
    let mut stmt = conn.prepare(sql).map_err(|e| words(e, started))?;
    if !stmt.readonly() {
        return Err("only a SELECT can run".into());
    }
    let column = stmt
        .column_index("path")
        .map_err(|_| "the statement must select a column named path".to_string())?;
    let mut rows = stmt.query([]).map_err(|e| words(e, started))?;
    let mut paths = Vec::new();
    while let Some(row) = rows.next().map_err(|e| words(e, started))? {
        if paths.len() == ROW_LIMIT {
            return Err(format!("more than {ROW_LIMIT} rows; add a LIMIT"));
        }
        if let Ok(Some(path)) = row.get::<_, Option<String>>(column) {
            paths.push(PathBuf::from(path));
        }
    }
    Ok(paths)
}

/// A read-only connection to `library` with the views made and the
/// authorizer and limits in place.
fn open(library: &Path) -> rusqlite::Result<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(library, flags)?;
    // Temporary views are made in the connection's own temp schema, which a
    // read-only connection may still write. The names hide the tables of the
    // same name in `main`.
    conn.execute_batch(&format!(
        "CREATE TEMP VIEW library AS
         SELECT t.path, t.title, t.artist,
                COALESCE(t.album_artist, t.artist) AS album_artist,
                t.album, t.genre, t.disc_no AS disc, t.track_no AS track, t.year,
                t.duration_ms / 1000.0 AS time,
                t.sample_rate AS rate, t.channels, t.bit_depth AS bits, t.size,
                CASE WHEN a.error IS NULL THEN COALESCE(a.bpm_tag,
                  CASE WHEN a.bpm_conf >= {conf} THEN a.bpm END) END AS tempo,
                CASE WHEN a.error IS NULL THEN a.loudness END AS loudness,
                CASE WHEN a.error IS NULL THEN a.peak END AS peak,
                a.path IS NOT NULL AS analysed, a.error, a.lossless, a.cutoff_hz,
                a.bits_used, a.md5, a.skipped
           FROM main.tracks t LEFT JOIN main.analysis a
             ON a.path = t.path AND a.mtime = t.mtime AND a.size = t.size
            AND a.version = {VERSION};
         CREATE TEMP VIEW playlists AS
         SELECT p.name AS playlist, i.position, t.path
           FROM main.playlists p
           JOIN main.playlist_items i ON i.playlist_id = p.id
           JOIN main.tracks t ON t.id = i.track_id;
         CREATE TEMP VIEW marks AS
         SELECT path, frame * 1.0 / rate AS time, label FROM main.marks;
         PRAGMA query_only = ON;",
        conf = tempo::MIN_CONFIDENCE,
    ))?;
    conn.set_limit(Limit::SQLITE_LIMIT_LENGTH, LENGTH_LIMIT)?;
    conn.authorizer(Some(authorize))?;
    Ok(conn)
}

/// Allows a `SELECT`, reads of the views and by them, reads of the
/// statement's own `WITH` tables, and the listed functions.
fn authorize(context: AuthContext<'_>) -> Authorization {
    let view = |name: &str| VIEWS.contains(&name);
    match context.action {
        AuthAction::Select | AuthAction::Recursive => Authorization::Allow,
        AuthAction::Read { table_name, .. } => {
            let allowed = match (context.database_name, context.accessor) {
                // A stored table always has a database; a `WITH` table has none.
                (None, _) => true,
                (Some("temp"), None) => view(table_name),
                (Some(_), Some(accessor)) => view(accessor),
                (Some(_), None) => false,
            };
            match allowed {
                true => Authorization::Allow,
                false => Authorization::Deny,
            }
        }
        AuthAction::Function { function_name }
            if FUNCTIONS.contains(&function_name.to_ascii_lowercase().as_str()) =>
        {
            Authorization::Allow
        }
        _ => Authorization::Deny,
    }
}

/// `e` in words: a statement stopped by the time limit says so.
fn words(e: rusqlite::Error, started: Instant) -> String {
    if started.elapsed() > TIME_LIMIT {
        return format!("stopped after {} s", TIME_LIMIT.as_secs());
    }
    match e {
        rusqlite::Error::SqliteFailure(_, Some(message)) => message,
        rusqlite::Error::MultipleStatement => "one statement only".into(),
        e => e.to_string(),
    }
}
