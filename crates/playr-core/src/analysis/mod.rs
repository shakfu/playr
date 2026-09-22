//! `playr analyze`: each file decoded once, its checks, loudness and tempo
//! measured in the same pass and kept in the library.
//!
//! The library holds measurements, not verdicts: [`findings`] applies the
//! thresholds when read, so changing one needs no second decode. Files are
//! only read. `docs/dev/analyze.md` sets out the design.

pub mod cutoff;
pub mod loudness;
pub mod tempo;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use lofty::file::TaggedFileExt;
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use rusqlite::Connection;

use crate::audio::decode::AudioStream;
use crate::db::{self, Track};
use loudness::{Histogram, Loudness};
use tempo::Tempo;

/// The analysers' version. A row from an older one is analysed again.
///
/// 1: as built. 2: tempo records the metrical level either side of the one it
/// chose, which an older row has no column for.
pub const VERSION: i64 = 2;

/// Files written per transaction, as a scan writes them.
pub const BATCH: usize = crate::scan::SCAN_BATCH;

/// Below this cutoff a lossless file may come from a lossy one. It catches
/// LAME at 192 kbit/s (18.9 kHz) but not at 320 (20.3 kHz), which would
/// reach CD masters whose anti-alias filters start near 20 kHz.
pub const LOSSY_BELOW_HZ: u32 = 20_000;

/// Below this cutoff a file above 48 kHz may be upsampled. ffmpeg's default
/// resampler, taking 44.1 kHz to 96 kHz, puts its fall at 25.1 kHz.
pub const UPSAMPLED_BELOW_HZ: u32 = 26_000;

/// A fall across the cutoff at least this steep, in dB, is an encoder's or a
/// resampler's filter rather than a dark recording. Uncalibrated.
pub const STEEP_DB: f32 = 25.0;

/// Tracks this close in length, with the same title and artist, are duplicates.
pub const DUPLICATE_WITHIN_MS: i64 = 2000;

/// What a FLAC header's checksum says of the decoded audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Md5 {
    Ok,
    Bad,
    /// The header holds all zeros, as some encoders write.
    Absent,
}

impl Md5 {
    pub fn name(self) -> &'static str {
        match self {
            Md5::Ok => "ok",
            Md5::Bad => "bad",
            Md5::Absent => "absent",
        }
    }

    pub fn from_name(name: &str) -> Option<Md5> {
        [Md5::Ok, Md5::Bad, Md5::Absent]
            .into_iter()
            .find(|m| m.name() == name)
    }
}

/// What one decode of one file measured.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Analysis {
    /// Why decoding failed or stopped early. Loudness is not kept then, so a
    /// partial measurement never sets a gain.
    pub error: Option<String>,
    pub rate: u32,
    pub frames: u64,
    pub header_frames: Option<u64>,
    /// Packets that failed to decode and were skipped.
    pub skipped: u64,
    pub lossless: bool,
    /// Bits per sample the header gives, for integer formats.
    pub bits: Option<u32>,
    /// `None` when the file is not FLAC.
    pub md5: Option<Md5>,
    /// The FLAC header's MD5, for finding copies of the same audio.
    pub md5_hex: Option<String>,
    /// Bits a sample actually uses; less than `bits` when the low ones are
    /// always zero. `None` for silence and formats with no integer samples.
    pub bits_used: Option<u32>,
    pub cutoff: Option<cutoff::Measured>,
    /// Integrated loudness, LUFS; `None` for silence.
    pub loudness: Option<f32>,
    /// Sample peak, linear.
    pub peak: Option<f32>,
    pub histogram: Option<Histogram>,
    pub tempo: Option<tempo::Estimate>,
    /// A BPM tag, which [`Analysis::bpm`] prefers to the estimate.
    pub bpm_tag: Option<f32>,
}

impl Analysis {
    fn failed(error: String) -> Analysis {
        Analysis {
            error: Some(error),
            ..Default::default()
        }
    }

    /// The tempo: the tag when there is one, else a confident estimate.
    pub fn bpm(&self) -> Option<f32> {
        self.bpm_tag.or(self
            .tempo
            .filter(|t| t.confidence >= tempo::MIN_CONFIDENCE)
            .map(|t| t.bpm))
    }
}

/// OR of every sample as an integer of the header's width, whose trailing
/// zeros are bits the file never uses.
struct Bits {
    scale: f64,
    or: i64,
}

impl Bits {
    fn feed(&mut self, samples: &[f32]) {
        for &x in samples {
            self.or |= (x as f64 * self.scale).round() as i64;
        }
    }
}

/// Decodes `path` once and measures it.
pub fn analyse(path: &Path) -> Analysis {
    let mut stream = match AudioStream::open_verifying(path) {
        Ok(s) => s,
        Err(e) => return Analysis::failed(e.to_string()),
    };
    let mut a = Analysis {
        header_frames: stream.header_frames(),
        lossless: stream.lossless(),
        bits: stream.bits(),
        md5_hex: stream
            .md5()
            .map(|m| m.iter().map(|b| format!("{b:02x}")).collect()),
        bpm_tag: bpm_tag(path),
        ..Default::default()
    };

    // Rate and channels can be missing until the first packet decodes.
    let first = match stream.next_chunk() {
        Ok(chunk) => chunk.map(<[f32]>::to_vec).unwrap_or_default(),
        Err(e) => return Analysis::failed(e.to_string()),
    };
    let spec = stream.spec();
    if spec.rate == 0 || spec.channels == 0 {
        return Analysis::failed("unknown stream format".into());
    }
    a.rate = spec.rate;
    let channels = spec.channels as usize;
    let mut loud = Loudness::new(spec.rate, spec.channels);
    let mut beat = Tempo::new(spec.rate);
    let mut cut = cutoff::Cutoff::new(spec.rate, a.header_frames);
    // Integer samples up to 24 bits survive the f32 conversion exactly.
    let mut bits = a
        .bits
        .filter(|b| a.lossless && (1..=24).contains(b))
        .map(|b| Bits {
            scale: 2f64.powi(b as i32 - 1),
            or: 0,
        });

    let mut take = |chunk: &[f32], a: &mut Analysis| {
        a.frames += (chunk.len() / channels) as u64;
        loud.feed(chunk);
        beat.feed(chunk, channels);
        cut.feed(chunk, channels);
        if let Some(b) = bits.as_mut() {
            b.feed(chunk);
        }
    };
    take(&first, &mut a);
    loop {
        match stream.next_chunk() {
            Ok(Some(chunk)) => take(chunk, &mut a),
            Ok(None) => break,
            Err(e) => {
                a.error = Some(format!("stopped after {} frames: {e}", a.frames));
                break;
            }
        }
    }
    a.skipped = stream.skipped();
    if a.error.is_none() && stream.is_flac() {
        a.md5 = Some(match (a.md5_hex.is_some(), stream.finalize()) {
            (false, _) => Md5::Absent,
            (true, Some(true)) => Md5::Ok,
            (true, _) => Md5::Bad,
        });
    }
    a.bits_used = bits
        .zip(a.bits)
        .and_then(|(b, width)| (b.or != 0).then(|| width.saturating_sub(b.or.trailing_zeros())));
    a.cutoff = cut.finish();
    a.tempo = beat.finish();
    let measured = loud.finish();
    if a.error.is_none() {
        a.loudness = measured.lufs;
        a.peak = Some(measured.peak);
        a.histogram = Some(measured.histogram);
    }
    a
}

/// A BPM tag, as a number; some taggers write `120.5`, some `120`.
fn bpm_tag(path: &Path) -> Option<f32> {
    let tagged = Probe::open(path).ok()?.read().ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    [ItemKey::IntegerBpm, ItemKey::Bpm]
        .into_iter()
        .find_map(|k| tag.get_string(k)?.trim().parse::<f32>().ok())
        .filter(|b| *b > 0.0 && b.is_finite())
}

/// A problem [`findings`] reads from a measurement.
#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// Decoding failed, or stopped before the end.
    Unreadable(String),
    /// Packets were skipped, or the audio does not match the FLAC checksum.
    Damaged {
        skipped: u64,
        md5_bad: bool,
    },
    /// A FLAC file with no checksum to verify against.
    NoChecksum,
    /// A lossless file that decodes to a length its header does not give.
    WrongLength {
        decoded: u64,
        header: u64,
    },
    /// Samples wider than the bits they use.
    Padded {
        used: u32,
        bits: u32,
    },
    PossibleLossySource {
        hz: u32,
    },
    PossibleUpsampling {
        hz: u32,
    },
}

impl Finding {
    /// The finding's name in reports and JSON.
    pub fn name(&self) -> &'static str {
        match self {
            Finding::Unreadable(_) => "unreadable",
            Finding::Damaged { .. } => "damaged",
            Finding::NoChecksum => "no checksum",
            Finding::WrongLength { .. } => "wrong length",
            Finding::Padded { .. } => "padded",
            Finding::PossibleLossySource { .. } => "possible lossy source",
            Finding::PossibleUpsampling { .. } => "possible upsampling",
        }
    }
}

/// The problems `a` shows, by the thresholds above.
pub fn findings(a: &Analysis) -> Vec<Finding> {
    let mut found = Vec::new();
    if let Some(e) = &a.error {
        found.push(Finding::Unreadable(e.clone()));
    }
    let md5_bad = a.md5 == Some(Md5::Bad);
    if a.skipped > 0 || md5_bad {
        found.push(Finding::Damaged {
            skipped: a.skipped,
            md5_bad,
        });
    }
    if a.md5 == Some(Md5::Absent) {
        found.push(Finding::NoChecksum);
    }
    if let (true, None, Some(header)) = (a.lossless, &a.error, a.header_frames) {
        if header != a.frames {
            found.push(Finding::WrongLength {
                decoded: a.frames,
                header,
            });
        }
    }
    if let (Some(used), Some(bits)) = (a.bits_used, a.bits) {
        if used < bits {
            found.push(Finding::Padded { used, bits });
        }
    }
    if let Some(c) = a.cutoff.filter(|c| c.fall_db >= STEEP_DB) {
        if a.rate > 48_000 && c.hz < UPSAMPLED_BELOW_HZ {
            found.push(Finding::PossibleUpsampling { hz: c.hz });
        } else if a.lossless && c.hz < LOSSY_BELOW_HZ {
            found.push(Finding::PossibleLossySource { hz: c.hz });
        }
    }
    found
}

/// Groups of two or more paths that look like the same recording: equal FLAC
/// checksums, or the same title and artist within [`DUPLICATE_WITHIN_MS`].
pub fn duplicates(tracks: &[Track], rows: &HashMap<String, Analysis>) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut by_md5: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for t in tracks {
        if let Some(hex) = rows.get(&t.path).and_then(|a| a.md5_hex.as_deref()) {
            by_md5.entry(hex).or_default().push(t.path.clone());
        }
    }
    groups.extend(by_md5.into_values().filter(|g| g.len() > 1));

    let mut by_name: BTreeMap<(String, String), Vec<&Track>> = BTreeMap::new();
    for t in tracks {
        if let (Some(title), Some(artist), Some(_)) = (&t.title, &t.artist, t.duration_ms) {
            let key = (title.to_lowercase(), artist.to_lowercase());
            by_name.entry(key).or_default().push(t);
        }
    }
    for mut same in by_name.into_values().filter(|g| g.len() > 1) {
        same.sort_by_key(|t| t.duration_ms);
        let mut run: Vec<&Track> = Vec::new();
        for t in same {
            let near = run.last().is_some_and(|l| {
                t.duration_ms.unwrap_or(0) - l.duration_ms.unwrap_or(0) <= DUPLICATE_WITHIN_MS
            });
            if !near && run.len() > 1 {
                groups.push(run.iter().map(|t| t.path.clone()).collect());
            }
            if !near {
                run.clear();
            }
            run.push(t);
        }
        if run.len() > 1 {
            groups.push(run.iter().map(|t| t.path.clone()).collect());
        }
    }
    groups
}

/// The key an album's tracks share: the album, and its album artist or,
/// when that is empty, the directory. `None` without an album tag.
pub fn album_key(t: &Track) -> Option<String> {
    let album = t.album.as_deref().filter(|a| !a.trim().is_empty())?;
    let by = match t.album_artist.as_deref().filter(|a| !a.trim().is_empty()) {
        Some(artist) => format!("artist:{artist}"),
        None => {
            let dir = Path::new(&t.path).parent().unwrap_or(Path::new(""));
            format!("dir:{}", dir.to_string_lossy())
        }
    };
    Some(format!("{album}\u{1f}{by}"))
}

/// Whether `t`'s stored analysis describes the file as the library knows it.
pub fn is_current(t: &Track, stat: Option<&db::analysis::Stat>) -> bool {
    stat.is_some_and(|s| s.mtime == t.mtime && s.size == t.size && s.version == VERSION)
}

/// The library's tracks at or under each of `paths`, in library order, and
/// the paths no track lies under. Every track when `paths` is empty.
pub fn select<'a>(library: &'a [Track], paths: &'a [PathBuf]) -> (Vec<Track>, Vec<&'a PathBuf>) {
    if paths.is_empty() {
        return (library.to_vec(), Vec::new());
    }
    let under = |t: &Track, root: &Path| Path::new(&t.path).starts_with(root);
    let resolved: Vec<PathBuf> = paths
        .iter()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
        .collect();
    let missing = paths
        .iter()
        .zip(&resolved)
        .filter(|(_, root)| !library.iter().any(|t| under(t, root)))
        .map(|(given, _)| given)
        .collect();
    let chosen = library
        .iter()
        .filter(|t| resolved.iter().any(|root| under(t, root)))
        .cloned()
        .collect();
    (chosen, missing)
}

/// Tracks of `chosen` whose stored analysis does not describe them.
pub fn pending(conn: &Connection, chosen: &[Track]) -> db::Result<Vec<Track>> {
    let stats = db::analysis::stats(conn)?;
    Ok(chosen
        .iter()
        .filter(|t| !is_current(t, stats.get(&t.path)))
        .cloned()
        .collect())
}

/// Opens `library`, analyses the tracks under `paths` that need it, and
/// reports `(done, total)` as each file finishes. For a session job, which
/// has no connection of its own to lend. `force` analyses them all again.
pub fn run_into(
    library: &Path,
    paths: &[PathBuf],
    force: bool,
    workers: usize,
    mut progress: impl FnMut(usize, usize),
) -> Result<RunStats, String> {
    let mut conn = db::open(library).map_err(|e| e.to_string())?;
    let tracks = db::query::all(&conn).map_err(|e| e.to_string())?;
    let (chosen, _) = select(&tracks, paths);
    let todo = match force {
        true => chosen,
        false => pending(&conn, &chosen).map_err(|e| e.to_string())?,
    };
    let total = todo.len();
    run(&mut conn, &tracks, &todo, workers, |done, _| {
        progress(done, total)
    })
    .map_err(|e| e.to_string())
}

/// Files decoded at once by default: one fewer than the processors, so
/// playback beside a run keeps one.
pub fn default_workers() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1).max(1))
}

/// What a run did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RunStats {
    pub analysed: usize,
    /// Of those, the ones that could not be decoded to the end.
    pub failed: usize,
    /// Albums whose loudness was worked out again.
    pub albums: usize,
}

/// Analyses `todo` on `workers` threads, writing each result to the library
/// as it arrives, [`BATCH`] to a transaction, then works out the loudness of
/// every album they belong to. `progress` hears each file as it is done.
///
/// An interrupted run keeps every batch it committed.
pub fn run(
    conn: &mut Connection,
    library: &[Track],
    todo: &[Track],
    workers: usize,
    mut progress: impl FnMut(usize, &Path),
) -> db::Result<RunStats> {
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let mut stats = RunStats::default();
    let result = std::thread::scope(|scope| {
        let (tx, rx) = std::sync::mpsc::channel();
        for _ in 0..workers.max(1) {
            let tx = tx.clone();
            let (next, stop) = (&next, &stop);
            scope.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= todo.len() || stop.load(Ordering::Relaxed) {
                    return;
                }
                let a = analyse(Path::new(&todo[i].path));
                if tx.send((i, a)).is_err() {
                    return;
                }
            });
        }
        drop(tx);
        let mut batch = Vec::with_capacity(BATCH);
        let mut write = |batch: &mut Vec<(usize, Analysis)>| -> db::Result<()> {
            let tx = conn.transaction()?;
            for (i, a) in batch.drain(..) {
                db::analysis::put(&tx, &todo[i], a.clone())?;
            }
            tx.commit()
        };
        for (i, a) in rx {
            stats.analysed += 1;
            stats.failed += a.error.is_some() as usize;
            progress(stats.analysed, Path::new(&todo[i].path));
            batch.push((i, a));
            if batch.len() >= BATCH {
                if let Err(e) = write(&mut batch) {
                    stop.store(true, Ordering::Relaxed);
                    return Err(e);
                }
            }
        }
        write(&mut batch)
    });
    result?;

    let touched: HashSet<String> = todo.iter().filter_map(album_key).collect();
    stats.albums = update_albums(conn, library, &touched)?;
    Ok(stats)
}

/// Works out each album in `keys` again from its tracks' histograms, or
/// removes it when a track has none current. Returns how many were written.
pub fn update_albums(
    conn: &mut Connection,
    library: &[Track],
    keys: &HashSet<String>,
) -> db::Result<usize> {
    if keys.is_empty() {
        return Ok(0);
    }
    let mut members: HashMap<String, Vec<&Track>> = HashMap::new();
    for t in library {
        if let Some(k) = album_key(t).filter(|k| keys.contains(k)) {
            members.entry(k).or_default().push(t);
        }
    }
    let stats = db::analysis::stats(conn)?;
    let tx = conn.transaction()?;
    let mut written = 0;
    for key in keys {
        let tracks = members.get(key).map(Vec::as_slice).unwrap_or_default();
        let mut pooled = Histogram::default();
        let mut peak = 0f32;
        let mut complete = !tracks.is_empty();
        for t in tracks {
            let row = is_current(t, stats.get(&t.path))
                .then(|| db::analysis::loudness_of(&tx, &t.path).ok().flatten())
                .flatten();
            match row {
                Some((h, p)) => {
                    pooled.pool(&h);
                    peak = peak.max(p);
                }
                None => complete = false,
            }
        }
        match pooled.integrated().filter(|_| complete) {
            Some(lufs) => {
                db::analysis::put_album(&tx, key, lufs, peak, tracks.len())?;
                written += 1;
            }
            None => db::analysis::delete_album(&tx, key)?,
        }
    }
    tx.commit()?;
    Ok(written)
}
