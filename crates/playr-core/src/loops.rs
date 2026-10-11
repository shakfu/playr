//! Scanning directories of precut loops into the loop library: frames,
//! tempo, key and tags from each file. See `docs/dev/loop-library.md`.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use walkdir::WalkDir;

use crate::db::loops::{self as lib, Loop, Origin, TempoFrom};
use crate::db::{self, Result};
use crate::scan::{is_audio, ScanStats, SCAN_BATCH};

/// The tempo `beats` give over `frames` at `rate`.
pub fn bpm(beats: u32, frames: u64, rate: u32) -> f64 {
    60.0 * f64::from(beats) * f64::from(rate) / frames.max(1) as f64
}

/// The whole beats `bpm` gives over `frames` at `rate`; `None` under one.
pub fn beats(bpm: f64, frames: u64, rate: u32) -> Option<u32> {
    let b = (bpm * frames as f64 / (60.0 * f64::from(rate.max(1)))).round();
    (b >= 1.0 && b <= f64::from(u32::MAX)).then_some(b as u32)
}

/// The beat count, a power of 2 from 1 to 128, whose tempo over `frames`
/// at `rate` is nearest 120 BPM on a log scale, and that tempo.
pub fn guess(frames: u64, rate: u32) -> Option<(u32, f64)> {
    if frames == 0 || rate == 0 {
        return None;
    }
    (0..8)
        .map(|k| {
            let b = 1u32 << k;
            (b, bpm(b, frames, rate))
        })
        .min_by(|a, b| {
            let d = |t: f64| (t / 120.0).log2().abs();
            d(a.1).total_cmp(&d(b.1))
        })
}

/// What a WAV's `acid` chunk holds. Layout from memory, not a spec: u32
/// flags, u16 root note, u16, f32, u32 beats, u16 and u16 meter, f32 tempo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Acid {
    pub one_shot: bool,
    /// The MIDI note, when the flags say it is set.
    pub root: Option<u8>,
    pub beats: u32,
    pub tempo: f32,
}

impl Acid {
    fn parse(b: &[u8]) -> Option<Acid> {
        if b.len() < 24 {
            return None;
        }
        let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().expect("4 bytes"));
        let flags = u32_at(0);
        let root = u16::from_le_bytes([b[4], b[5]]);
        Some(Acid {
            one_shot: flags & 1 != 0,
            root: (flags & 2 != 0 && root < 128).then_some(root as u8),
            beats: u32_at(12),
            tempo: f32::from_le_bytes(b[20..24].try_into().expect("4 bytes")),
        })
    }
}

/// What playr reads from a WAV's own chunks.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Wav {
    pub channels: u16,
    pub rate: u32,
    pub frames: u64,
    pub acid: Option<Acid>,
    /// The first loop of the `smpl` chunk, in frames, end exclusive.
    pub smpl: Option<(u64, u64)>,
}

/// Reads the `fmt `, `data`, `acid` and `smpl` chunks of the WAV at `path`.
/// `None` for a file that is not a RIFF WAVE, or has no `fmt ` or `data`.
pub fn read_wav(path: &Path) -> Option<Wav> {
    let mut f = File::open(path).ok()?;
    let mut head = [0u8; 12];
    f.read_exact(&mut head).ok()?;
    if &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return None;
    }
    let (mut wav, mut block, mut data) = (Wav::default(), 0u16, None);
    loop {
        let mut h = [0u8; 8];
        if f.read_exact(&mut h).is_err() {
            break;
        }
        let size = u64::from(u32::from_le_bytes(h[4..8].try_into().expect("4 bytes")));
        let id = [h[0], h[1], h[2], h[3]];
        let body = |f: &mut File| -> Option<Vec<u8>> {
            let mut b = vec![0u8; size.min(4096) as usize];
            f.read_exact(&mut b).ok()?;
            Some(b)
        };
        let at = f.stream_position().ok()?;
        match &id {
            b"fmt " => {
                let b = body(&mut f)?;
                if b.len() < 16 {
                    return None;
                }
                wav.channels = u16::from_le_bytes([b[2], b[3]]);
                wav.rate = u32::from_le_bytes(b[4..8].try_into().expect("4 bytes"));
                block = u16::from_le_bytes([b[12], b[13]]);
            }
            b"data" => data = Some(size),
            b"acid" => wav.acid = body(&mut f).as_deref().and_then(Acid::parse),
            b"smpl" => {
                let b = body(&mut f)?;
                let u32_at = |i: usize| {
                    b.get(i..i + 4)
                        .map(|s| u32::from_le_bytes(s.try_into().expect("4 bytes")))
                };
                // 36 bytes of header, then 24 bytes a loop: id, type, start, last.
                if u32_at(28).is_some_and(|n| n > 0) {
                    if let (Some(s), Some(l)) = (u32_at(44), u32_at(48)) {
                        wav.smpl = (l >= s).then_some((u64::from(s), u64::from(l) + 1));
                    }
                }
            }
            _ => {}
        }
        // Chunks start on even bytes.
        f.seek(SeekFrom::Start(at + size + size % 2)).ok()?;
    }
    if block == 0 || wav.rate == 0 {
        return None;
    }
    wav.frames = data? / u64::from(block);
    Some(wav)
}

/// Tempo and key a file name gives, as loop packs write them: `120bpm`, or
/// `120` then `bpm`, or a bare number from 60 to 200; a key such as `Am`,
/// `C#m`, `Fmaj` or `Bb`. A lone letter is not taken as a key.
pub fn from_name(stem: &str) -> (Option<f64>, Option<String>) {
    let tokens: Vec<&str> = stem
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '#'))
        .filter(|t| !t.is_empty())
        .collect();
    let tempo = |t: &str| t.parse::<f64>().ok().filter(|b| (40.0..=300.0).contains(b));
    let mut bpm = None;
    let mut bare = None;
    for (i, t) in tokens.iter().enumerate() {
        let lower = t.to_ascii_lowercase();
        if let Some(n) = lower.strip_suffix("bpm").and_then(tempo) {
            bpm = Some(n);
            break;
        }
        if lower == "bpm" {
            if let Some(n) = i.checked_sub(1).and_then(|j| tempo(tokens[j])) {
                bpm = Some(n);
                break;
            }
        }
        let digits = t.len() >= 2 && t.len() <= 3 && t.bytes().all(|b| b.is_ascii_digit());
        if digits && bare.is_none() {
            bare = t.parse::<f64>().ok().filter(|b| (60.0..=200.0).contains(b));
        }
    }
    let key = tokens.iter().find_map(|t| key(t));
    (bpm.or(bare), key)
}

/// `t` as a key, `Am` or `F#`, if it names one: a note, with an accidental
/// or a mode, so a lone `A` does not count.
fn key(t: &str) -> Option<String> {
    let mut chars = t.chars();
    let note = chars.next().filter(|c| matches!(c, 'A'..='G'))?;
    let rest: String = chars.collect();
    let (acc, mode) = match rest.chars().next() {
        Some('#') => ("#", &rest[1..]),
        Some('b') => ("b", &rest[1..]),
        _ => ("", rest.as_str()),
    };
    let minor = match mode.to_ascii_lowercase().as_str() {
        "" => false,
        "m" | "min" | "minor" => true,
        "maj" | "major" => false,
        _ => return None,
    };
    if acc.is_empty() && mode.is_empty() {
        return None;
    }
    Some(format!("{note}{acc}{}", if minor { "m" } else { "" }))
}

/// A MIDI note's name, `C`, `C#` and so on, as a key without a mode.
pub fn note_name(midi: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    NAMES[usize::from(midi % 12)].to_string()
}

/// What reading a file in a loop root found.
#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    Loop(Loop),
    /// An `acid` chunk marks it a one-shot.
    OneShot,
}

/// Reads the loop file at `path` under `root`. `None` when it is not audio
/// playr can read.
pub fn read_loop(path: &Path, root: &Path) -> Option<Found> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0);
    let path_str = path.to_str()?.to_string();
    let wav = read_wav(path);
    if wav.and_then(|w| w.acid).is_some_and(|a| a.one_shot) {
        return Some(Found::OneShot);
    }
    let (channels, rate, frames) = match wav {
        Some(w) => (w.channels, w.rate, w.frames),
        None => stream_shape(path)?,
    };
    let (start, end) = wav
        .and_then(|w| w.smpl)
        .filter(|(s, e)| s < e && *e <= frames)
        .unwrap_or((0, frames));
    if end == 0 {
        return None;
    }
    let name = lib::stem(&path_str);
    let tags = path
        .parent()
        .and_then(|p| p.strip_prefix(root).ok())
        .map(|rel| {
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let (named_bpm, named_key) = from_name(&name);
    let acid = wav.and_then(|w| w.acid);
    let len = end - start;
    let tag = crate::analysis::bpm_tag(path).map(f64::from);
    let acid_beats = acid.map(|a| a.beats).filter(|b| *b > 0);
    let acid_tempo = acid
        .map(|a| f64::from(a.tempo))
        .filter(|t| *t > 0.0 && t.is_finite());
    let given = |b: f64, from| Some((b, beats(b, len, rate), from));
    let tempo = if let Some(b) = tag {
        given(b, TempoFrom::Tag)
    } else if let Some(b) = acid_beats {
        Some((bpm(b, len, rate), Some(b), TempoFrom::Acid))
    } else if let Some(t) = acid_tempo {
        given(t, TempoFrom::Acid)
    } else if let Some(b) = named_bpm {
        given(b, TempoFrom::Name)
    } else {
        guess(len, rate).map(|(b, t)| (t, Some(b), TempoFrom::Length))
    };
    let key = named_key.or_else(|| acid.and_then(|a| a.root).map(note_name));
    Some(Found::Loop(Loop {
        id: 0,
        path: path_str,
        start,
        end,
        rate,
        channels: Some(channels).filter(|c| *c > 0),
        slot: None,
        origin: Origin::Scan,
        name,
        tags,
        bpm: tempo.map(|t| t.0),
        beats: tempo.and_then(|t| t.1),
        bpm_from: tempo.map(|t| t.2),
        key,
        mtime: Some(mtime),
        size: Some(meta.len() as i64),
    }))
}

/// Channels, rate and frames of a file that is not a WAV playr reads: from
/// its header, or by decoding it when the header does not say.
fn stream_shape(path: &Path) -> Option<(u16, u32, u64)> {
    let mut stream = crate::audio::decode::AudioStream::open(path).ok()?;
    if stream.spec().rate == 0 {
        stream.next_chunk().ok()?;
    }
    let spec = stream.spec();
    let frames = match stream.header_frames() {
        Some(n) => n,
        None => {
            let c = usize::from(spec.channels.max(1));
            let mut n = 0u64;
            while let Ok(Some(chunk)) = stream.next_chunk() {
                n += (chunk.len() / c) as u64;
            }
            n
        }
    };
    (spec.rate > 0).then_some((spec.channels, spec.rate, frames))
}

/// What a loop scan did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LoopStats {
    pub scan: ScanStats,
    /// Files an `acid` chunk marks one-shot, left out.
    pub one_shots: usize,
    /// Tracks under the root taken out of the audio library.
    pub moved: usize,
}

/// Why a loop scan did not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The directory holds a track root, whose tracks it would take away.
    HoldsTrackRoot(PathBuf),
    Db(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::HoldsTrackRoot(r) => write!(
                f,
                "it holds the library directory {}; scan a directory of loops inside it",
                r.display()
            ),
            Refused::Db(e) => write!(f, "{e}"),
        }
    }
}

impl From<rusqlite::Error> for Refused {
    fn from(e: rusqlite::Error) -> Self {
        Refused::Db(e.to_string())
    }
}

/// Scans `root` into the loop library, recording it as a loop root once a
/// file is seen. Tracks under it leave the audio library. Refused when
/// `root` holds a track root.
pub fn scan_dir(
    conn: &mut Connection,
    root: &Path,
    mut progress: impl FnMut(&ScanStats),
) -> std::result::Result<LoopStats, Refused> {
    let mut stats = LoopStats::default();
    let Ok(root) = root.canonicalize() else {
        return Ok(stats);
    };
    if let Some(r) = db::roots(conn)?.into_iter().find(|r| r.starts_with(&root)) {
        return Err(Refused::HoldsTrackRoot(r));
    }
    let mut files = Vec::new();
    for entry in WalkDir::new(&root).follow_links(false) {
        match entry {
            Ok(e) if e.file_type().is_file() && is_audio(e.path()) => files.push(e.into_path()),
            Ok(_) => {}
            Err(_) => stats.scan.failed += 1,
        }
    }
    for batch in files.chunks(SCAN_BATCH) {
        let mut read = Vec::new();
        for path in batch {
            stats.scan.seen += 1;
            let Some(path_str) = path.to_str() else {
                stats.scan.failed += 1;
                progress(&stats.scan);
                continue;
            };
            let disk = std::fs::metadata(path).ok().map(|m| {
                let mtime = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos() as i64)
                    .unwrap_or(0);
                (mtime, m.len() as i64)
            });
            if disk.is_some() && disk == lib::stat_of(conn, path_str)? {
                stats.scan.skipped += 1;
                progress(&stats.scan);
                continue;
            }
            match read_loop(path, &root) {
                Some(Found::Loop(l)) => {
                    read.push(l);
                    stats.scan.added += 1;
                }
                Some(Found::OneShot) => stats.one_shots += 1,
                None => stats.scan.failed += 1,
            }
            progress(&stats.scan);
        }
        let tx = conn.transaction()?;
        for l in &read {
            lib::upsert_file(&tx, l)?;
        }
        tx.commit()?;
    }
    if stats.scan.seen > 0 {
        lib::add_root(conn, &root)?;
    }
    stats.moved = remove_tracks_under(conn, &root)?;
    Ok(stats)
}

/// Takes the tracks under `root` out of the audio library, with their
/// places in playlists. Returns how many.
fn remove_tracks_under(conn: &Connection, root: &Path) -> Result<usize> {
    let prefix = root.join("").to_string_lossy().into_owned();
    conn.execute(
        "DELETE FROM tracks WHERE substr(path, 1, length(?1)) = ?1",
        [&prefix],
    )
}

/// Removes the scanned loops under `root` whose files are gone. Returns how many.
pub fn prune_missing(conn: &Connection, root: &Path) -> Result<usize> {
    let gone = lib::missing_under(conn, root)?;
    let tx = conn.unchecked_transaction()?;
    for l in &gone {
        tx.execute("DELETE FROM loop_lib WHERE id = ?1", [l.id])?;
    }
    tx.commit()?;
    Ok(gone.len())
}
