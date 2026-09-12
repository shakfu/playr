//! Recursive directory scan: walk, read tags, write to the library.
//!
//! Files whose `(mtime, size)` match the stored row are skipped without opening
//! them, so a rescan of an unchanged library reads no tags.

use crate::db::{self, Track};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use rusqlite::Connection;
use std::path::Path;
use walkdir::WalkDir;

/// Extensions offered to the decoder. A superset of what is guaranteed to play;
/// unplayable files are reported at playback time, not hidden at scan time.
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
    pub failed: usize,
}

/// Reads tags and stream properties from `path` into a `Track`.
///
/// Returns `None` when the file cannot be parsed as audio. Tag absence is not a
/// failure: an untagged file still yields a row, so it remains playable.
pub fn read_track(path: &Path) -> Option<Track> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut t = Track {
        path: path.to_string_lossy().into_owned(),
        mtime,
        size: meta.len() as i64,
        ..Default::default()
    };

    let tagged = Probe::open(path).ok()?.read().ok()?;

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

/// Parses the leading integer of a tag like `"3"` or `"3/12"`.
fn parse_leading_num(s: &str) -> Option<u32> {
    let digits: String = s
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Scans `root` recursively into the library, calling `progress` per file seen.
pub fn scan_dir<F>(conn: &mut Connection, root: &Path, mut progress: F) -> db::Result<ScanStats>
where
    F: FnMut(&ScanStats, &Path),
{
    let mut stats = ScanStats::default();
    let files: Vec<_> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_audio(e.path()))
        .collect();

    // One transaction for the whole scan: an insert-per-commit is orders of
    // magnitude slower on a library of any size.
    let tx = conn.transaction()?;
    for entry in files {
        let path = entry.path();
        stats.seen += 1;
        let path_str = path.to_string_lossy();

        let disk = std::fs::metadata(path).ok().map(|m| {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            (mtime, m.len() as i64)
        });

        if let (Some(d), Ok(Some(known))) = (disk, db::stat_of(&tx, &path_str)) {
            if d == known {
                stats.skipped += 1;
                progress(&stats, path);
                continue;
            }
        }

        match read_track(path) {
            Some(t) => {
                db::upsert(&tx, &t)?;
                stats.added += 1;
            }
            None => stats.failed += 1,
        }
        progress(&stats, path);
    }
    tx.commit()?;
    Ok(stats)
}
