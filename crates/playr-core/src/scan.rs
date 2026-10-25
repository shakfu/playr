//! Recursive directory scan: walk, read tags, write to the library.
//!
//! Files whose `(mtime, size)` match the stored row are skipped without opening
//! them, so a rescan of an unchanged library reads no tags.

use crate::db::{self, Track};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Extensions offered to the decoder. A superset of what is guaranteed to play:
/// a file that parses but will not decode is reported at playback time.
pub const AUDIO_EXTS: &[&str] = &[
    "flac", "mp3", "m4a", "mp4", "aac", "alac", "ogg", "oga", "opus", "wav", "wave", "aif", "aiff",
    "aifc", "caf", "mka", "webm", "mp1", "mp2", "mpa",
];

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_ascii_lowercase();
            AUDIO_EXTS.contains(&e.as_str())
        })
        .unwrap_or(false)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScanStats {
    pub seen: usize,
    pub added: usize,
    pub skipped: usize,
    /// Files that could not be read, and directories that could not be listed.
    pub failed: usize,
}

/// What a scan into a library found, and how many tracks the library holds after it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScanReport {
    pub stats: ScanStats,
    /// Tracks under the directory whose files are gone. A scan keeps them;
    /// [`db::prune_missing`] removes them.
    pub missing: usize,
    /// Directories the scan walked without finding one audio file, though the
    /// library holds tracks under them. A drive that is not mounted reads
    /// exactly like one emptied, and every track under it counts as missing,
    /// so a caller that prunes without asking leaves this alone.
    pub unavailable: usize,
    /// Tracks in the library once the scan is done.
    pub total: usize,
}

/// Files gathered to play, and what could not be gathered, in words.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Playable {
    pub tracks: Vec<Track>,
    pub problems: Vec<String>,
}

/// Reads tags and stream properties from `path` into a `Track`.
///
/// Returns `None` when the file cannot be parsed as audio. Tag absence is not a
/// failure: an untagged file still yields a row, so it remains playable.
pub fn read_track(path: &Path) -> Option<Track> {
    let meta = std::fs::metadata(path).ok()?;
    let (mtime, size) = stat(&meta);

    let mut t = Track {
        path: path.to_string_lossy().into_owned(),
        mtime,
        size,
        ..Default::default()
    };

    let Some(tagged) = Probe::open(path).ok().and_then(|p| p.read().ok()) else {
        // lofty has no reader for some containers Symphonia plays, such as CAF
        // and Matroska. Index those untagged rather than hide them.
        let mut stream = crate::audio::decode::AudioStream::open(path).ok()?;
        if stream.spec().rate == 0 {
            stream.next_chunk().ok()?;
        }
        let spec = stream.spec();
        t.sample_rate = Some(spec.rate).filter(|r| *r != 0);
        t.channels = Some(spec.channels).filter(|c| *c != 0);
        t.duration_ms = stream.duration().map(|d| d.as_millis() as i64);
        let tags = stream.tags();
        t.title = tags.title;
        t.artist = tags.artist;
        t.album = tags.album;
        t.album_artist = tags.album_artist;
        t.genre = tags.genre;
        t.track_no = tags.track;
        t.disc_no = tags.disc;
        t.year = tags.year;
        let info = caf_info(path);
        let get = |keys: &[&str]| {
            info.iter()
                .find(|(k, _)| keys.iter().any(|w| k.eq_ignore_ascii_case(w)))
                .map(|(_, v)| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        t.title = t.title.or(get(&["title"])).or_else(|| matroska_title(path));
        t.artist = t.artist.or(get(&["artist"]));
        t.album = t.album.or(get(&["album"]));
        t.genre = t.genre.or(get(&["genre"]));
        t.track_no =
            (t.track_no).or(get(&["track number", "track"]).and_then(|s| parse_leading_num(&s)));
        t.year = (t.year)
            .or(get(&["year", "date", "recorded date"]).and_then(|s| s.get(..4)?.parse().ok()));
        return Some(t);
    };

    let props = tagged.properties();
    t.duration_ms = Some(props.duration().as_millis() as i64);
    t.sample_rate = props.sample_rate();
    t.channels = props.channels().map(|c| c as u16);
    t.bit_depth = props.bit_depth();

    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
        let get = |k: ItemKey| {
            tag.get_string(k)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        t.title = get(ItemKey::TrackTitle);
        t.artist = get(ItemKey::TrackArtist);
        t.album = get(ItemKey::AlbumTitle);
        t.album_artist = get(ItemKey::AlbumArtist);
        t.genre = get(ItemKey::Genre);
        t.track_no = get(ItemKey::TrackNumber).and_then(|s| parse_leading_num(&s));
        t.disc_no = get(ItemKey::DiscNumber).and_then(|s| parse_leading_num(&s));
        t.year = get(ItemKey::RecordingDate)
            .or_else(|| get(ItemKey::Year))
            .and_then(|s| s.get(..4).and_then(|y| y.parse().ok()));
    }
    Some(t)
}

/// The key and value pairs of a CAF file's `info` chunk, which Symphonia
/// skips; empty for any other file. Apple's keys are `title`, `artist`,
/// `track number` and `year`; ffmpeg writes `track` and `date`.
fn caf_info(path: &Path) -> Vec<(String, String)> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut head = [0u8; 8];
    if f.read_exact(&mut head).is_err() || &head[..4] != b"caff" {
        return Vec::new();
    }
    let mut chunk = [0u8; 12];
    while f.read_exact(&mut chunk).is_ok() {
        let size = i64::from_be_bytes(chunk[4..].try_into().expect("8 bytes"));
        if &chunk[..4] == b"info" {
            // A tag chunk is small; a size past 1 MB is not one.
            let Some(len) = usize::try_from(size)
                .ok()
                .filter(|&n| (4..1 << 20).contains(&n))
            else {
                return Vec::new();
            };
            let mut data = vec![0u8; len];
            if f.read_exact(&mut data).is_err() {
                return Vec::new();
            }
            let n = u32::from_be_bytes(data[..4].try_into().expect("4 bytes")) as usize;
            let mut fields = data[4..]
                .split(|&b| b == 0)
                .map(|s| String::from_utf8_lossy(s).into_owned());
            let mut pairs = Vec::new();
            for _ in 0..n {
                match (fields.next(), fields.next()) {
                    (Some(k), Some(v)) => pairs.push((k, v)),
                    _ => break,
                }
            }
            return pairs;
        }
        // The audio data's size may be -1, to the end of the file.
        if size < 0 || f.seek(SeekFrom::Current(size)).is_err() {
            return Vec::new();
        }
    }
    Vec::new()
}

/// A Matroska file's title, which Symphonia does not expose: the segment
/// Info's, else the first track entry's name. `None` for any other file.
/// Reads up to the first Cluster, before which writers put both.
fn matroska_title(path: &Path) -> Option<String> {
    use std::io::{BufReader, Read, Seek, SeekFrom};
    const SEGMENT: u32 = 0x1853_8067;
    const INFO: u32 = 0x1549_A966;
    const TRACKS: u32 = 0x1654_AE6B;
    const TRACK_ENTRY: u32 = 0xAE;
    const CLUSTER: u32 = 0x1F43_B675;
    const TITLE: u32 = 0x7BA9;
    const NAME: u32 = 0x536E;

    /// An EBML variable-length integer: an ID keeps its length marker, a size
    /// drops it. `Some(None)` is a size of all ones, unknown.
    fn vint(f: &mut impl Read, keep_marker: bool) -> Option<Option<u64>> {
        let mut b = [0u8; 1];
        f.read_exact(&mut b).ok()?;
        let len = b[0].leading_zeros() as usize + 1;
        if len > 8 || (keep_marker && len > 4) {
            return None;
        }
        let mask = 0xFFu64 >> len;
        let mut v = u64::from(b[0]) & if keep_marker { 0xFF } else { mask };
        let mut ones = v == mask;
        for _ in 1..len {
            f.read_exact(&mut b).ok()?;
            ones &= b[0] == 0xFF;
            v = v << 8 | u64::from(b[0]);
        }
        Some((keep_marker || !ones).then_some(v))
    }
    /// The next element's ID and size, with `end` the parent's last byte.
    fn element<R: Read + Seek>(f: &mut R, end: u64) -> Option<(u32, Option<u64>)> {
        if f.stream_position().ok()? >= end {
            return None;
        }
        let id = vint(f, true)??;
        Some((id as u32, vint(f, false)?))
    }
    /// A string element's text, if it is short enough to be a title.
    fn text(f: &mut impl Read, size: u64) -> Option<String> {
        let mut data = vec![0u8; usize::try_from(size).ok().filter(|&n| n < 4096)?];
        f.read_exact(&mut data).ok()?;
        let s = String::from_utf8_lossy(&data);
        let s = s.trim_matches(char::from(0)).trim();
        (!s.is_empty()).then(|| s.to_string())
    }

    let mut f = BufReader::new(std::fs::File::open(path).ok()?);
    let (0x1A45_DFA3, Some(n)) = element(&mut f, u64::MAX)? else {
        return None;
    };
    f.seek_relative(n as i64).ok()?;
    let (SEGMENT, size) = element(&mut f, u64::MAX)? else {
        return None;
    };
    let seg_end = size.map_or(u64::MAX, |n| f.stream_position().unwrap_or(0) + n);
    // Calls `each` on every child of known size in `[pos, end)`, leaving the
    // reader after the child whatever `each` read; stops when it returns true.
    fn children<R: Read + Seek>(
        f: &mut R,
        end: u64,
        mut each: impl FnMut(&mut R, u32, u64) -> bool,
    ) -> Option<()> {
        while let Some((id, Some(size))) = element(f, end) {
            let child_end = f.stream_position().ok()?.saturating_add(size);
            if each(f, id, size) {
                break;
            }
            f.seek(SeekFrom::Start(child_end)).ok()?;
        }
        Some(())
    }

    let (mut title, mut name) = (None, None);
    children(&mut f, seg_end, |f, id, size| {
        let end = f.stream_position().unwrap_or(u64::MAX).saturating_add(size);
        match id {
            INFO => {
                children(f, end, |f, id, size| {
                    if id == TITLE {
                        title = text(f, size);
                    }
                    title.is_some()
                });
            }
            TRACKS if name.is_none() => {
                children(f, end, |f, id, size| {
                    if id == TRACK_ENTRY {
                        let entry_end = f.stream_position().unwrap_or(u64::MAX) + size;
                        children(f, entry_end, |f, id, size| {
                            if id == NAME {
                                name = text(f, size);
                            }
                            name.is_some()
                        });
                    }
                    name.is_some()
                });
            }
            _ => {}
        }
        title.is_some() || id == CLUSTER
    })?;
    title.or(name)
}

/// `(mtime, size)` as stored in the library, with mtime in nanoseconds.
///
/// Whole seconds missed a same-size rewrite within one second.
fn stat(meta: &std::fs::Metadata) -> (i64, i64) {
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0);
    (mtime, meta.len() as i64)
}

/// Parses the leading integer of a tag like `"3"` or `"3/12"`.
fn parse_leading_num(s: &str) -> Option<u32> {
    let digits: String = s
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Files committed per transaction during a scan.
pub const SCAN_BATCH: usize = 500;

/// Scans `root` recursively into the library, calling `progress` per file seen.
///
/// Rows are keyed by canonical path. A relative root would store paths that
/// resolve only from the directory the scan ran in. A root that does not
/// resolve scans nothing.
pub fn scan_dir<F>(conn: &mut Connection, root: &Path, mut progress: F) -> db::Result<ScanStats>
where
    F: FnMut(&ScanStats, &Path),
{
    let mut stats = ScanStats::default();
    let Ok(root) = root.canonicalize() else {
        return Ok(stats);
    };
    // Files under a loop root are loops, kept out of the audio library.
    let loop_roots = db::loops::roots(conn)?;
    // Copies `playr dedupe` dropped.
    let excluded = db::dedupe::excluded(conn)?;
    let mut files = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        match entry {
            Ok(e) if e.file_type().is_file() && is_audio(e.path()) => {
                let skip = e.path().to_str().is_some_and(|p| excluded.contains(p));
                if !skip && !db::loops::under_root(&loop_roots, e.path()) {
                    files.push(e)
                }
            }
            Ok(_) => {}
            // A directory without read permission, for one; its files are never seen.
            Err(_) => stats.failed += 1,
        }
    }

    // Transactions of `SCAN_BATCH` files: a commit per insert is orders of
    // magnitude slower, and one commit for the whole scan loses every row
    // when the scan is interrupted. Files are read before the transaction
    // opens, so the write lock is held for the inserts alone.
    for batch in files.chunks(SCAN_BATCH) {
        let mut read = Vec::new();
        for entry in batch {
            let path = entry.path();
            stats.seen += 1;
            // Rows hold paths as text. A lossy conversion would store a path
            // that opens nothing, so such a file is counted as unreadable.
            let Some(path_str) = path.to_str() else {
                stats.failed += 1;
                progress(&stats, path);
                continue;
            };

            let disk = std::fs::metadata(path).ok().map(|m| stat(&m));

            if let (Some(d), Ok(Some(known))) = (disk, db::stat_of(conn, path_str)) {
                if d == known {
                    stats.skipped += 1;
                    progress(&stats, path);
                    continue;
                }
            }

            match read_track(path) {
                Some(t) => {
                    read.push(t);
                    stats.added += 1;
                }
                None => stats.failed += 1,
            }
            progress(&stats, path);
        }
        let tx = conn.transaction()?;
        for t in &read {
            db::upsert(&tx, t)?;
        }
        tx.commit()?;
    }
    Ok(stats)
}

/// Scans `dir` into the library file at `library`, creating it if there is
/// none, and counts the tracks under `dir` whose files are gone. `progress`
/// is called per file seen.
pub fn scan_into(
    library: &Path,
    dir: &Path,
    mut progress: impl FnMut(&ScanStats),
) -> Result<ScanReport, String> {
    let fail = |e: rusqlite::Error| format!("{}: {e}", library.display());
    let mut conn = db::open(library).map_err(fail)?;
    let stats = scan_dir(&mut conn, dir, |s, _| progress(s)).map_err(fail)?;
    if stats.seen > 0 {
        db::add_root(&conn, dir).map_err(fail)?;
    }
    db::dedupe::fix_relative(&conn).map_err(fail)?;
    let missing = db::missing_under(&conn, dir).map_err(fail)?.len();
    let total = db::query::count(&conn).map_err(fail)? as usize;
    Ok(ScanReport {
        stats,
        missing,
        unavailable: usize::from(stats.seen == 0 && missing > 0),
        total,
    })
}

/// Scans every `root` into `library`, reporting progress across them as one
/// run. Each root is recorded; missing tracks are counted under each.
pub fn scan_roots(
    library: &Path,
    roots: &[PathBuf],
    mut progress: impl FnMut(&ScanStats),
) -> Result<ScanReport, String> {
    let fail = |e: rusqlite::Error| format!("{}: {e}", library.display());
    let mut conn = db::open(library).map_err(fail)?;
    let mut stats = ScanStats::default();
    let (mut missing, mut unavailable) = (0, 0);
    for root in roots {
        let batch = scan_dir(&mut conn, root, |s, _| {
            let mut shown = stats;
            shown.seen += s.seen;
            shown.added += s.added;
            shown.skipped += s.skipped;
            shown.failed += s.failed;
            progress(&shown);
        })
        .map_err(fail)?;
        stats.seen += batch.seen;
        stats.added += batch.added;
        stats.skipped += batch.skipped;
        stats.failed += batch.failed;
        if batch.seen > 0 {
            db::add_root(&conn, root).map_err(fail)?;
        }
        // Counted per root: one unmounted drive among several would otherwise
        // hide behind the files the others found.
        let gone = db::missing_under(&conn, root).map_err(fail)?.len();
        if batch.seen == 0 && gone > 0 {
            unavailable += 1;
        }
        missing += gone;
    }
    db::dedupe::fix_relative(&conn).map_err(fail)?;
    let total = db::query::count(&conn).map_err(fail)? as usize;
    Ok(ScanReport {
        stats,
        missing,
        unavailable,
        total,
    })
}

/// The tracks to play for `paths`: files as given, and the audio files under
/// directories, recursively and sorted. Tags come from `known` when it has the
/// file, as the library does, and are read from disk otherwise, so an
/// unscanned directory still shows titles rather than file names.
pub fn playable(paths: &[PathBuf], known: impl Fn(&str) -> Option<Track>) -> Playable {
    let mut problems = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    for arg in paths {
        // Canonical, to match the library's keys: `playr .` must find the
        // rows `playr scan ~/music` wrote.
        let Ok(path) = arg.canonicalize() else {
            problems.push(format!("no such file: {}", arg.display()));
            continue;
        };
        if path.is_dir() {
            let mut found: Vec<PathBuf> = Vec::new();
            for entry in WalkDir::new(path).follow_links(false) {
                match entry {
                    Ok(e) if e.file_type().is_file() && is_audio(e.path()) => {
                        found.push(e.into_path())
                    }
                    Ok(_) => {}
                    Err(e) => problems.push(format!("cannot read {e}")),
                }
            }
            found.sort();
            files.extend(found);
        } else if path.is_file() {
            files.push(path);
        } else {
            problems.push(format!("no such file: {}", arg.display()));
        }
    }
    // The queue carries paths as text, so a lossy name would open nothing.
    files.retain(|p| {
        let ok = p.to_str().is_some();
        if !ok {
            problems.push(format!(
                "skipping a path that is not UTF-8: {}",
                p.display()
            ));
        }
        ok
    });
    let tracks = files
        .into_iter()
        .map(|p| {
            let key = p.to_string_lossy().into_owned();
            known(&key).or_else(|| read_track(&p)).unwrap_or(Track {
                path: key,
                ..Default::default()
            })
        })
        .collect();
    Playable { tracks, problems }
}
