//! `playr analyze`: decoding the library once, and reporting what it found.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use playr_app::message::finding_detail;
use playr_core::analysis::{self, tempo, Analysis};
use playr_core::db::{self, Track};
use serde_json::json;

/// Estimates within this fraction of a tag agree with it.
const BPM_AGREES: f32 = 0.02;

pub struct Options {
    pub paths: Vec<PathBuf>,
    pub force: bool,
    pub report: bool,
    pub json: bool,
    pub jobs: Option<usize>,
}

pub fn run(
    conn: &mut rusqlite::Connection,
    opts: Options,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let library = db::query::all(conn)?;
    let (chosen, missing) = analysis::select(&library, &opts.paths);
    for path in missing {
        eprintln!("playr: no library tracks at {}", path.display());
    }
    if chosen.is_empty() {
        eprintln!("playr: no tracks to analyse; `playr scan DIR` adds them");
        return Ok(ExitCode::FAILURE);
    }

    if !opts.report {
        let todo = match opts.force {
            true => chosen.clone(),
            false => analysis::pending(conn, &chosen)?,
        };
        let workers = opts.jobs.unwrap_or_else(analysis::default_workers).max(1);
        let total = todo.len();
        let started = std::time::Instant::now();
        if !opts.json {
            println!(
                "analysing {total} of {} tracks, {workers} at a time",
                chosen.len()
            );
        }
        let done = analysis::run(conn, &library, &todo, workers, |n, path| {
            if !opts.json && (n % 5 == 0 || n == total) {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default();
                print!("\r  {n}/{total}  {name:<40.40}");
                let _ = std::io::stdout().flush();
            }
        })?;
        if !opts.json && total > 0 {
            let secs = started.elapsed().as_secs_f64().max(0.001);
            println!(
                "\r  {} analysed, {} unreadable, {} albums, {:.1} files a second{:<20}",
                done.analysed,
                done.failed,
                done.albums,
                done.analysed as f64 / secs,
                ""
            );
        }
    }

    let rows = db::analysis::rows(conn)?;
    let current: HashMap<String, Analysis> = chosen
        .iter()
        .filter_map(|t| {
            let (stat, a) = rows.get(&t.path)?;
            analysis::is_current(t, Some(stat)).then(|| (t.path.clone(), a.clone()))
        })
        .collect();
    let dupes = analysis::duplicates(&chosen, &current);
    if opts.json {
        println!("{}", report_json(&chosen, &current, &dupes));
    } else {
        print_report(&chosen, &current, &dupes);
    }
    Ok(ExitCode::SUCCESS)
}

fn print_report(tracks: &[Track], rows: &HashMap<String, Analysis>, dupes: &[Vec<String>]) {
    let mut counts: Vec<(&'static str, usize)> = Vec::new();
    for t in tracks {
        let Some(a) = rows.get(&t.path) else { continue };
        for f in analysis::findings(a) {
            println!("{:<22} {}  ({})", f.name(), t.path, finding_detail(&f));
            match counts.iter_mut().find(|c| c.0 == f.name()) {
                Some(c) => c.1 += 1,
                None => counts.push((f.name(), 1)),
            }
        }
    }
    if !dupes.is_empty() {
        println!("duplicates");
        for group in dupes {
            for path in group {
                println!("  {path}");
            }
            println!();
        }
    }

    let missing = tracks.len() - rows.len();
    print!("{} tracks analysed", rows.len());
    if missing > 0 {
        print!(", {missing} not yet or changed since");
    }
    for (name, n) in &counts {
        print!("; {n} {name}");
    }
    if !dupes.is_empty() {
        print!("; {} groups of duplicates", dupes.len());
    }
    println!();
    if let Some(line) = tempo_check(rows.values()) {
        println!("{line}");
    }
}

/// How the estimates compare with BPM tags, on the tracks that have both.
fn tempo_check<'a>(rows: impl Iterator<Item = &'a Analysis>) -> Option<String> {
    let (mut tagged, mut agree, mut octave, mut unsure) = (0, 0, 0, 0);
    for a in rows {
        let Some(tag) = a.bpm_tag else { continue };
        tagged += 1;
        let Some(est) = a.tempo.filter(|t| t.confidence >= tempo::MIN_CONFIDENCE) else {
            unsure += 1;
            continue;
        };
        let near = |ratio: f32| (est.bpm / tag / ratio - 1.0).abs() <= BPM_AGREES;
        if near(1.0) {
            agree += 1;
        } else if near(0.5) || near(2.0) {
            octave += 1;
        }
    }
    (tagged > 0).then(|| {
        format!(
            "tempo against BPM tags: {agree} of {tagged} within 2%, {octave} at half or double, \
             {unsure} without a confident estimate"
        )
    })
}

fn report_json(
    tracks: &[Track],
    rows: &HashMap<String, Analysis>,
    dupes: &[Vec<String>],
) -> serde_json::Value {
    let analysed: Vec<serde_json::Value> = tracks
        .iter()
        .filter_map(|t| {
            let a = rows.get(&t.path)?;
            let findings: Vec<serde_json::Value> = analysis::findings(a)
                .iter()
                .map(|f| json!({ "finding": f.name(), "detail": finding_detail(f) }))
                .collect();
            Some(json!({
                "path": t.path,
                "findings": findings,
                "error": a.error,
                "rate": a.rate,
                "frames": a.frames,
                "header_frames": a.header_frames,
                "skipped": a.skipped,
                "lossless": a.lossless,
                "bits": a.bits,
                "bits_used": a.bits_used,
                "md5": a.md5.map(|m| m.name()),
                "cutoff_hz": a.cutoff.map(|c| c.hz),
                "cutoff_db": a.cutoff.map(|c| c.fall_db),
                "loudness": a.loudness,
                "peak": a.peak,
                "bpm": a.bpm(),
                "bpm_estimate": a.tempo.map(|t| t.bpm),
                "bpm_alt": a.tempo.and_then(|t| t.alt),
                "bpm_confidence": a.tempo.map(|t| t.confidence),
                "bpm_tag": a.bpm_tag,
            }))
        })
        .collect();
    json!({
        "tracks": analysed,
        "duplicates": dupes,
        "not_analysed": tracks.len() - rows.len(),
    })
}
