//! The `analysis` and `album_loudness` tables, and the gains read from them.

use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension, Row};

use super::{Result, Track};
use crate::analysis::loudness::Histogram;
use crate::analysis::{album_key, cutoff, is_current, tempo, Analysis, Md5};
use crate::columns::Measures;
use crate::gain::{Gain, Gains};

/// What a stored row was measured against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    pub mtime: i64,
    pub size: i64,
    pub version: i64,
}

/// Every stored row's stat, by path.
pub fn stats(conn: &Connection) -> Result<HashMap<String, Stat>> {
    let mut stmt = conn.prepare("SELECT path, mtime, size, version FROM analysis")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get(0)?,
            Stat {
                mtime: r.get(1)?,
                size: r.get(2)?,
                version: r.get(3)?,
            },
        ))
    })?;
    rows.collect()
}

/// Stores `a` for `t`, replacing its row, at the analysers' version.
pub fn put(conn: &Connection, t: &Track, a: Analysis) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO analysis
           (path, mtime, size, version, error, rate, frames, header_frames,
            skipped, lossless, bits, md5, md5_hex, bits_used, cutoff_hz,
            cutoff_db, loudness, peak, histogram, bpm, bpm_alt, bpm_conf, bpm_tag)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)",
        params![
            t.path,
            t.mtime,
            t.size,
            crate::analysis::VERSION,
            a.error,
            a.rate,
            a.frames as i64,
            a.header_frames.map(|f| f as i64),
            a.skipped as i64,
            a.lossless,
            a.bits,
            a.md5.map(Md5::name),
            a.md5_hex,
            a.bits_used,
            a.cutoff.map(|c| c.hz),
            a.cutoff.map(|c| c.fall_db),
            a.loudness,
            a.peak,
            a.histogram.as_ref().map(Histogram::to_bytes),
            a.tempo.map(|t| t.bpm),
            a.tempo.and_then(|t| t.alt),
            a.tempo.map(|t| t.confidence),
            a.bpm_tag,
        ],
    )?;
    Ok(())
}

const COLS: &str = "path, mtime, size, version, error, rate, frames, header_frames,
                    skipped, lossless, bits, md5, md5_hex, bits_used, cutoff_hz,
                    cutoff_db, loudness, peak, histogram, bpm, bpm_alt, bpm_conf, bpm_tag";

fn row_to_analysis(r: &Row) -> rusqlite::Result<(String, Stat, Analysis)> {
    let hz: Option<u32> = r.get(14)?;
    let fall: Option<f32> = r.get(15)?;
    let bpm: Option<f32> = r.get(19)?;
    let alt: Option<f32> = r.get(20)?;
    let conf: Option<f32> = r.get(21)?;
    let histogram: Option<Vec<u8>> = r.get(18)?;
    let md5: Option<String> = r.get(11)?;
    Ok((
        r.get(0)?,
        Stat {
            mtime: r.get(1)?,
            size: r.get(2)?,
            version: r.get(3)?,
        },
        Analysis {
            error: r.get(4)?,
            rate: r.get(5)?,
            frames: r.get::<_, i64>(6)?.max(0) as u64,
            header_frames: r.get::<_, Option<i64>>(7)?.map(|f| f.max(0) as u64),
            skipped: r.get::<_, i64>(8)?.max(0) as u64,
            lossless: r.get(9)?,
            bits: r.get(10)?,
            md5: md5.as_deref().and_then(Md5::from_name),
            md5_hex: r.get(12)?,
            bits_used: r.get(13)?,
            cutoff: hz
                .zip(fall)
                .map(|(hz, fall_db)| cutoff::Measured { hz, fall_db }),
            loudness: r.get(16)?,
            peak: r.get(17)?,
            histogram: histogram.as_deref().and_then(Histogram::from_bytes),
            tempo: bpm.zip(conf).map(|(bpm, confidence)| tempo::Estimate {
                bpm,
                confidence,
                alt,
            }),
            bpm_tag: r.get(22)?,
        },
    ))
}

/// Every stored row, current or not, by path.
pub fn rows(conn: &Connection) -> Result<HashMap<String, (Stat, Analysis)>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM analysis"))?;
    let rows = stmt.query_map([], row_to_analysis)?;
    rows.map(|r| r.map(|(path, stat, a)| (path, (stat, a))))
        .collect()
}

/// The row stored for `path`, current or not.
pub fn row_of(conn: &Connection, path: &str) -> Result<Option<(Stat, Analysis)>> {
    let sql = format!("SELECT {COLS} FROM analysis WHERE path = ?1");
    conn.query_row(&sql, [path], row_to_analysis)
        .optional()
        .map(|row| row.map(|(_, stat, a)| (stat, a)))
}

/// The histogram and peak stored for `path`, when it decoded to the end.
pub fn loudness_of(conn: &Connection, path: &str) -> Result<Option<(Histogram, f32)>> {
    let row: Option<(Option<Vec<u8>>, Option<f32>)> = conn
        .query_row(
            "SELECT histogram, peak FROM analysis WHERE path = ?1 AND error IS NULL",
            [path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(h, p)| Some((Histogram::from_bytes(&h?)?, p?))))
}

/// What the columns show, for every track with a current row: loudness,
/// peak and the tempo to show, which is the tag when there is one.
pub fn measures(conn: &Connection, library: &[Track]) -> Result<HashMap<String, Measures>> {
    let mut stmt = conn.prepare(
        "SELECT a.path, a.mtime, a.size, a.version, a.loudness, a.peak,
                COALESCE(a.bpm_tag, CASE WHEN a.bpm_conf >= ?1 THEN a.bpm END)
           FROM analysis a WHERE a.error IS NULL",
    )?;
    let rows = stmt.query_map([tempo::MIN_CONFIDENCE], |r| {
        Ok((
            r.get::<_, String>(0)?,
            Stat {
                mtime: r.get(1)?,
                size: r.get(2)?,
                version: r.get(3)?,
            },
            Measures {
                loudness: r.get(4)?,
                peak: r.get(5)?,
                bpm: r.get(6)?,
            },
        ))
    })?;
    let mut stored: HashMap<String, (Stat, Measures)> = HashMap::new();
    for row in rows {
        let (path, stat, measures) = row?;
        stored.insert(path, (stat, measures));
    }
    // Only rows that still describe the file the library knows.
    Ok(library
        .iter()
        .filter_map(|t| {
            let (stat, measures) = stored.get(&t.path)?;
            is_current(t, Some(stat)).then(|| (t.path.clone(), *measures))
        })
        .collect())
}

/// The tempo to show for `path`: its BPM tag, else a confident estimate.
/// `None` unless the row describes the file as the library knows it.
pub fn bpm_of(conn: &Connection, path: &str) -> Result<Option<f32>> {
    conn.query_row(
        "SELECT COALESCE(a.bpm_tag, CASE WHEN a.bpm_conf >= ?2 THEN a.bpm END)
           FROM analysis a
           JOIN tracks t ON t.path = a.path AND t.mtime = a.mtime AND t.size = a.size
          WHERE a.path = ?1 AND a.version = ?3",
        params![path, tempo::MIN_CONFIDENCE, crate::analysis::VERSION],
        |r| r.get(0),
    )
    .optional()
    .map(Option::flatten)
}

pub fn put_album(conn: &Connection, key: &str, lufs: f32, peak: f32, tracks: usize) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO album_loudness (album_key, loudness, peak, tracks)
         VALUES (?1, ?2, ?3, ?4)",
        params![key, lufs, peak, tracks as i64],
    )?;
    Ok(())
}

pub fn delete_album(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM album_loudness WHERE album_key = ?1", [key])?;
    Ok(())
}

/// Removes the rows of `paths`, as pruning removes their tracks.
pub(crate) fn delete(conn: &Connection, paths: &[String]) -> Result<usize> {
    let mut n = 0;
    for p in paths {
        n += conn.execute("DELETE FROM analysis WHERE path = ?1", [p])?;
    }
    Ok(n)
}

/// ReplayGain for each track of `library` with a current, loud enough
/// analysis. An album's gain is given only while every one of its tracks
/// has one, and none was added since the album was worked out.
pub fn gains(conn: &Connection, library: &[Track]) -> Result<HashMap<PathBuf, Gains>> {
    let mut loud: HashMap<String, (Stat, f32, f32)> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT path, mtime, size, version, loudness, peak FROM analysis
         WHERE error IS NULL AND loudness IS NOT NULL AND peak IS NOT NULL",
    )?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            Stat {
                mtime: r.get(1)?,
                size: r.get(2)?,
                version: r.get(3)?,
            },
            r.get::<_, f32>(4)?,
            r.get::<_, f32>(5)?,
        ))
    })? {
        let (path, stat, lufs, peak) = row?;
        loud.insert(path, (stat, lufs, peak));
    }

    let mut albums: HashMap<String, (f32, f32, i64)> = HashMap::new();
    let mut stmt = conn.prepare("SELECT album_key, loudness, peak, tracks FROM album_loudness")?;
    for row in stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?)))
    })? {
        let (key, album) = row?;
        albums.insert(key, album);
    }

    let track_gain = |t: &Track| {
        let (stat, lufs, peak) = loud.get(&t.path)?;
        is_current(t, Some(stat)).then(|| Gain::for_loudness(*lufs, *peak))
    };
    // Per album: tracks in the library, and whether each has a gain.
    let mut members: HashMap<String, (i64, bool)> = HashMap::new();
    for t in library {
        if let Some(key) = album_key(t) {
            let m = members.entry(key).or_insert((0, true));
            m.0 += 1;
            m.1 &= track_gain(t).is_some();
        }
    }
    let album_gain = |t: &Track| {
        let key = album_key(t)?;
        let (count, all) = members.get(&key)?;
        let (lufs, peak, tracks) = albums.get(&key)?;
        (*all && tracks == count).then(|| Gain::for_loudness(*lufs, *peak))
    };
    Ok(library
        .iter()
        .filter_map(|t| {
            let track = track_gain(t)?;
            Some((
                PathBuf::from(&t.path),
                Gains {
                    track: Some(track),
                    album: album_gain(t),
                },
            ))
        })
        .collect())
}
