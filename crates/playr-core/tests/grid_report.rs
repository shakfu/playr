//! How well beat grids sit on the kicks of real music. Ignored, since it needs
//! a music directory and decodes every file in it twice:
//!
//! ```sh
//! PLAYR_GRID_DIR=~/Music cargo test --release -p playr-core --test grid_report -- --ignored --nocapture
//! ```
//!
//! For each file with a grid, the rise in low-band (kick) level is folded
//! onto the grid's beat. Where the fold peaks says where the kicks fall
//! against the beats; the same fold over the first and last quarters says
//! whether the grid drifts. A file whose fold has no clear peak has no
//! steady kick to judge by and is counted apart. See "A beat grid" in
//! `docs/dev/analyze.md` for the results.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use playr_core::analysis::analyse;
use playr_core::samples::read_frames;

/// Bins a beat is folded into, and the levels' hop, in frames.
const BINS: usize = 32;
const HOP: usize = 128;
/// A fold peaking this many times its mean has a steady kick.
const CLEAR: f64 = 2.0;
/// Kicks within this of a grid beat are on it.
const ON_MS: f64 = 35.0;

fn audio_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            audio_files(&p, out);
        } else if matches!(
            p.extension().and_then(|x| x.to_str()),
            Some("m4a" | "mp3" | "flac" | "wav" | "ogg" | "aiff" | "aif")
        ) {
            out.push(p);
        }
    }
}

/// The rise in log level below about 150 Hz, every [`HOP`] frames.
fn kick_rises(mono: &[f32], rate: u32) -> Vec<f64> {
    let k = 1.0 - (-std::f64::consts::TAU * 150.0 / f64::from(rate)).exp();
    let (mut a, mut b) = (0.0f64, 0.0f64);
    let levels: Vec<f64> = mono
        .chunks(HOP)
        .map(|c| {
            let sum: f64 = c
                .iter()
                .map(|&x| {
                    a += k * (f64::from(x) - a);
                    b += k * (a - b);
                    b * b
                })
                .sum();
            (sum / c.len() as f64 + 1e-12).ln()
        })
        .collect();
    levels.windows(2).map(|w| (w[1] - w[0]).max(0.0)).collect()
}

/// The fold of `rises[from..to]` onto beats `beat` seconds apart from `t0`:
/// the bin of its peak, as a fraction of a beat, and peak over mean.
fn fold(rises: &[f64], rate: u32, t0: f64, beat: f64, from: usize, to: usize) -> (f64, f64) {
    let mut bins = [0.0f64; BINS];
    for (i, r) in rises.iter().enumerate().take(to).skip(from) {
        // Rise `i` is into level `i + 1`, which starts (i + 1) hops in.
        let t = (i + 1) as f64 * HOP as f64 / f64::from(rate);
        let phase = ((t - t0) / beat).rem_euclid(1.0);
        bins[(phase * BINS as f64) as usize % BINS] += r;
    }
    let mean = bins.iter().sum::<f64>() / BINS as f64;
    let (peak, top) = bins
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .expect("bins");
    // The bin's middle, centred on the beat: -0.5 to 0.5.
    let at = (peak as f64 + 0.5) / BINS as f64;
    let at = if at > 0.5 { at - 1.0 } else { at };
    (at, if mean > 0.0 { top / mean } else { 0.0 })
}

#[derive(Debug)]
struct Row {
    file: String,
    bpm: f64,
    offset_ms: f64,
    clarity: f64,
    drift_ms: f64,
}

fn measure(path: &Path) -> Option<Row> {
    let a = analyse(path);
    let g = a.grid?;
    let rate = a.rate;
    let (samples, ch) = read_frames(path, rate, 0, a.frames).ok()?;
    let mono: Vec<f32> = samples
        .chunks(ch.max(1))
        .map(|f| f.iter().sum::<f32>() / f.len() as f32)
        .collect();
    let rises = kick_rises(&mono, rate);
    let beat = 60.0 / g.bpm;
    let n = rises.len();
    let (at, clarity) = fold(&rises, rate, g.t0, beat, 0, n);
    let (first, _) = fold(&rises, rate, g.t0, beat, 0, n / 4);
    let (last, _) = fold(&rises, rate, g.t0, beat, n - n / 4, n);
    let wrap = |d: f64| (d + 0.5).rem_euclid(1.0) - 0.5;
    Some(Row {
        file: path.file_name()?.to_string_lossy().into_owned(),
        bpm: g.bpm,
        offset_ms: at * beat * 1000.0,
        clarity,
        drift_ms: wrap(last - first) * beat * 1000.0,
    })
}

#[test]
#[ignore]
fn grids_against_the_kicks_of_a_music_directory() {
    let Ok(dir) = std::env::var("PLAYR_GRID_DIR") else {
        panic!("set PLAYR_GRID_DIR to a music directory");
    };
    let mut files = Vec::new();
    audio_files(Path::new(&dir), &mut files);
    files.sort();
    let next = AtomicUsize::new(0);
    let rows = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = files.get(i) else {
                    return;
                };
                let row = measure(path);
                rows.lock().unwrap().push(row);
            });
        }
    });
    let rows = rows.into_inner().unwrap();
    let total = rows.len();
    let gridded: Vec<Row> = rows.into_iter().flatten().collect();
    let beat_ms = |r: &Row| 60_000.0 / r.bpm;
    let mut sorted: Vec<&Row> = gridded.iter().collect();
    sorted.sort_by(|a, b| b.clarity.total_cmp(&a.clarity));
    for r in &sorted {
        eprintln!(
            "{:>8.2} BPM  kicks {:>+7.1} ms  drift {:>+7.1} ms  clarity {:>5.2}  {}",
            r.bpm, r.offset_ms, r.drift_ms, r.clarity, r.file
        );
    }
    eprintln!("\n{total} files; {} with a grid", gridded.len());
    for least in [1.2, 1.5, CLEAR] {
        let clear: Vec<&Row> = gridded.iter().filter(|r| r.clarity >= least).collect();
        let on = clear.iter().filter(|r| r.offset_ms.abs() <= ON_MS).count();
        let half = clear
            .iter()
            .filter(|r| (r.offset_ms.abs() - beat_ms(r) / 2.0).abs() <= ON_MS)
            .count();
        let drifting = clear.iter().filter(|r| r.drift_ms.abs() > ON_MS).count();
        eprintln!(
            "clarity >= {least}: {} files, {on} on the beat, {half} half a beat off, {} elsewhere; \
             {drifting} drift over {ON_MS} ms",
            clear.len(),
            clear.len() - on - half,
        );
    }
}
