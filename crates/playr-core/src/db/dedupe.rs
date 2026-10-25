//! Rows that name one recording twice: byte-identical copies at two paths,
//! and the relative rows a 0.1.0 scan stored beside their absolute rescans.
//!
//! Only rows change. A dropped copy's path is recorded in `excluded`, so a
//! rescan does not add it back; files stay on disk.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use rusqlite::Connection;

use super::{query, roots, Result, Track};

/// Byte-identical tracks: the row to keep, and the rows to drop.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub keep: Track,
    pub drop: Vec<Track>,
}

/// Groups of tracks whose files are byte-identical. The kept row is the one
/// in the most playlists, then the shortest path, then the first in order.
///
/// A file that cannot be read is in no group. Name-only matches, as
/// `is:duplicate` lists, are left out: they may be different masters.
pub fn identical(conn: &Connection) -> Result<Vec<Group>> {
    let mut by_size: BTreeMap<i64, Vec<Track>> = BTreeMap::new();
    for t in query::all(conn)? {
        if t.size > 0 && Path::new(&t.path).is_absolute() {
            by_size.entry(t.size).or_default().push(t);
        }
    }
    let uses = playlist_uses(conn)?;
    let mut groups = Vec::new();
    for same in by_size.into_values().filter(|g| g.len() > 1) {
        let mut classes: Vec<Vec<Track>> = Vec::new();
        for t in same {
            match classes.iter_mut().find(|c| same_bytes(&c[0].path, &t.path)) {
                Some(c) => c.push(t),
                None if Path::new(&t.path).is_file() => classes.push(vec![t]),
                None => {}
            }
        }
        for mut class in classes.into_iter().filter(|c| c.len() > 1) {
            class.sort_by(|a, b| {
                let used = |t: &Track| uses.get(&t.id).copied().unwrap_or(0);
                used(b)
                    .cmp(&used(a))
                    .then(a.path.len().cmp(&b.path.len()))
                    .then(a.path.cmp(&b.path))
            });
            let keep = class.remove(0);
            groups.push(Group { keep, drop: class });
        }
    }
    Ok(groups)
}

/// Drops each group's other rows, moving their playlist places to the kept
/// row, and excludes their paths from later scans. Returns the rows dropped.
pub fn apply(conn: &Connection, groups: &[Group]) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let mut dropped = 0;
    for g in groups {
        for t in &g.drop {
            merge(&tx, t.id, g.keep.id)?;
            tx.execute(
                "INSERT OR IGNORE INTO excluded (path) VALUES (?1)",
                [&t.path],
            )?;
            dropped += 1;
        }
    }
    tx.commit()?;
    Ok(dropped)
}

/// Paths a scan passes over, as [`apply`] recorded them.
pub fn excluded(conn: &Connection) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT path FROM excluded")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    rows.collect()
}

/// Forgets every exclusion, so the next scan adds those files again.
pub fn restore(conn: &Connection) -> Result<usize> {
    conn.execute("DELETE FROM excluded", [])
}

/// What [`fix_relative`] did with the rows a 0.1.0 scan stored relative.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Relative {
    /// Merged into the one absolute row whose path ends with theirs.
    pub merged: usize,
    /// Given an absolute path: their file lies under exactly one root.
    pub resolved: usize,
    /// Neither; kept as they were.
    pub left: usize,
}

/// Repairs relative rows. Their directory was the scan's working directory,
/// which is not recorded, so a row is changed only when its target is
/// unambiguous. Playlist places move with the row.
pub fn fix_relative(conn: &Connection) -> Result<Relative> {
    let all = query::all(conn)?;
    let (relative, absolute): (Vec<&Track>, Vec<&Track>) =
        all.iter().partition(|t| Path::new(&t.path).is_relative());
    let mut done = Relative::default();
    if relative.is_empty() {
        return Ok(done);
    }
    let known: HashSet<&str> = absolute.iter().map(|t| t.path.as_str()).collect();
    let roots = roots(conn)?;
    let tx = conn.unchecked_transaction()?;
    for r in relative {
        // `./a.flac` ends no absolute path; `a.flac` may.
        let rel: PathBuf = Path::new(&r.path)
            .components()
            .filter(|c| *c != Component::CurDir)
            .collect();
        let rel = rel.as_path();
        let mut matches = absolute
            .iter()
            .filter(|a| Path::new(&a.path).ends_with(rel));
        if let (Some(a), None) = (matches.next(), matches.next()) {
            merge(&tx, r.id, a.id)?;
            done.merged += 1;
            continue;
        }
        let found: Vec<PathBuf> = roots
            .iter()
            .filter_map(|root| root.join(rel).canonicalize().ok())
            .filter(|p| p.is_file())
            .collect();
        match found.as_slice() {
            [p] if p.to_str().is_some_and(|s| !known.contains(s)) => {
                tx.execute(
                    "UPDATE tracks SET path = ?1 WHERE id = ?2",
                    rusqlite::params![p.to_str(), r.id],
                )?;
                done.resolved += 1;
            }
            _ => done.left += 1,
        }
    }
    tx.commit()?;
    Ok(done)
}

/// Moves track `from`'s playlist places to `to` and deletes `from`.
fn merge(conn: &Connection, from: i64, to: i64) -> Result<()> {
    conn.execute(
        "UPDATE playlist_items SET track_id = ?2 WHERE track_id = ?1",
        [from, to],
    )?;
    conn.execute("DELETE FROM tracks WHERE id = ?1", [from])?;
    Ok(())
}

/// Playlist places per track id.
fn playlist_uses(conn: &Connection) -> Result<HashMap<i64, usize>> {
    let mut stmt =
        conn.prepare("SELECT track_id, COUNT(*) FROM playlist_items GROUP BY track_id")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?;
    rows.collect()
}

/// Whether the files at `a` and `b` hold the same bytes. Unreadable is unequal.
fn same_bytes(a: &str, b: &str) -> bool {
    let (Ok(mut fa), Ok(mut fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else {
        return false;
    };
    let (mut ba, mut bb) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let (Ok(na), Ok(nb)) = (read_full(&mut fa, &mut ba), read_full(&mut fb, &mut bb)) else {
            return false;
        };
        if na != nb || ba[..na] != bb[..nb] {
            return false;
        }
        if na == 0 {
            return true;
        }
    }
}

/// Fills `buf` unless the file ends first; returns the bytes read.
fn read_full(f: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}
