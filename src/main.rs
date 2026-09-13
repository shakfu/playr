//! playr: a minimal TUI music player.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use playr::audio::Player;
use playr::db::{self, Track};
use playr::{scan, ui};

const USAGE: &str = "\
playr - a minimal TUI music player

usage:
  playr                        browse the library
  playr <path>...              play files or directories (recursively)
  playr scan <dir>...          add a directory to the library
  playr search <query>         play everything matching a search
  playr playlist <name>        play a saved playlist
  playr playlists              list saved playlists
  playr formats                show which formats this build can decode

options:
  --db <path>                  use a different library file
  -h, --help                   show this help
  -V, --version                show the version
";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("playr: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    let mut db_path = db::default_path();
    if let Some(i) = args.iter().position(|a| a == "--db") {
        let value = args.get(i + 1).ok_or("--db needs a path")?.clone();
        db_path = PathBuf::from(value);
        args.drain(i..=i + 1);
    }

    match args.first().map(String::as_str) {
        Some("-h") | Some("--help") => {
            print!("{USAGE}");
            return Ok(ExitCode::SUCCESS);
        }
        Some("-V") | Some("--version") => {
            println!("playr {}", env!("CARGO_PKG_VERSION"));
            return Ok(ExitCode::SUCCESS);
        }
        Some("formats") => {
            print_formats();
            return Ok(ExitCode::SUCCESS);
        }
        _ => {}
    }

    // Only `scan` creates the library. Without one, everything else runs on an
    // empty in-memory library, so playing a file leaves nothing behind.
    let mut conn = if args.first().map(String::as_str) == Some("scan") || db_path.try_exists()? {
        db::open(&db_path)?
    } else {
        db::open_memory()?
    };

    match args.first().map(String::as_str) {
        Some("scan") => {
            let dirs = &args[1..];
            if dirs.is_empty() {
                return Err("scan needs at least one directory".into());
            }
            return cmd_scan(&mut conn, dirs);
        }
        Some("playlists") => {
            for p in db::query::playlists(&conn)? {
                println!("{:<40} {:>4} tracks", p.name, p.len);
            }
            return Ok(ExitCode::SUCCESS);
        }
        _ => {}
    }

    // Everything else opens the interface, with a queue chosen by the arguments.
    let start: Vec<Track> = match args.first().map(String::as_str) {
        Some("search") => {
            let q = args[1..].join(" ");
            if q.trim().is_empty() {
                return Err("search needs a query".into());
            }
            let hits = db::query::search(&conn, &q)?;
            if hits.is_empty() {
                eprintln!("playr: nothing matches {q:?}");
                return Ok(ExitCode::FAILURE);
            }
            hits
        }
        Some("playlist") => {
            let name = args[1..].join(" ");
            let lists = db::query::playlists(&conn)?;
            let pl = db::query::find_playlist(&lists, name.trim())
                .ok_or_else(|| format!("no single playlist named {name:?}"))?;
            db::query::playlist_tracks(&conn, pl.id)?
        }
        Some(flag) if flag.starts_with('-') => {
            return Err(format!("unknown option {flag} (see playr --help)").into())
        }
        Some(_) => collect_paths(&conn, &args)?,
        None => Vec::new(),
    };

    let player = Player::new()?;

    // `ratatui::init` panics without a terminal; report it instead, since
    // running playr from a pipe or a service is an easy mistake to make.
    let mut terminal = ratatui::try_init().map_err(|e| format!("playr needs a terminal: {e}"))?;
    let app = ui::App::with_queue(conn, player, start);
    let result = app.run(&mut terminal);
    ratatui::restore();
    result?;
    Ok(ExitCode::SUCCESS)
}

fn cmd_scan(
    conn: &mut rusqlite::Connection,
    dirs: &[String],
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut total = scan::ScanStats::default();
    let multi = dirs.len() > 1;
    let mut removed = 0;
    for dir in dirs {
        let path = Path::new(dir);
        if !path.is_dir() {
            eprintln!("playr: not a directory: {dir}");
            continue;
        }
        println!("scanning {dir}");
        let stats = scan::scan_dir(conn, path, |s, p| {
            // Overwrite one line rather than scrolling a wall of filenames.
            if s.seen % 25 == 0 {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default();
                print!("\r  {} files, {} added  {:<40.40}", s.seen, s.added, name);
                use std::io::Write;
                let _ = std::io::stdout().flush();
            }
        })?;
        println!(
            "\r  {} files, {} added, {} unchanged, {} unreadable{:<20}",
            stats.seen, stats.added, stats.skipped, stats.failed, ""
        );
        total.seen += stats.seen;
        total.added += stats.added;
        total.skipped += stats.skipped;
        total.failed += stats.failed;
        removed += db::prune_missing(conn, path)?;
    }
    if multi {
        println!(
            "total: {} files, {} added, {} unchanged, {} unreadable",
            total.seen, total.added, total.skipped, total.failed
        );
    }
    if removed > 0 {
        println!("removed {removed} tracks whose files are gone");
    }
    println!("library now holds {} tracks", db::query::count(conn)?);
    Ok(ExitCode::SUCCESS)
}

/// Expands the command line into a track list.
///
/// Directories are walked recursively. Tags come from the library when the file
/// is already known, and are read from disk otherwise, so playing an unscanned
/// directory still shows titles rather than file names.
fn collect_paths(
    conn: &rusqlite::Connection,
    args: &[String],
) -> Result<Vec<Track>, Box<dyn std::error::Error>> {
    let mut files: Vec<PathBuf> = Vec::new();
    for arg in args {
        // Canonical, to match the library's keys: `playr .` must find the
        // rows `playr scan ~/music` wrote.
        let Ok(path) = Path::new(arg).canonicalize() else {
            eprintln!("playr: no such file: {arg}");
            continue;
        };
        if path.is_dir() {
            let mut found: Vec<PathBuf> = Vec::new();
            for entry in walkdir::WalkDir::new(path).follow_links(false) {
                match entry {
                    Ok(e) if e.file_type().is_file() && scan::is_audio(e.path()) => {
                        found.push(e.into_path())
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("playr: cannot read {e}"),
                }
            }
            found.sort();
            files.extend(found);
        } else if path.is_file() {
            files.push(path);
        } else {
            eprintln!("playr: no such file: {arg}");
        }
    }
    // The queue carries paths as text, so a lossy name would open nothing.
    files.retain(|p| {
        let ok = p.to_str().is_some();
        if !ok {
            eprintln!("playr: skipping a path that is not UTF-8: {}", p.display());
        }
        ok
    });
    if files.is_empty() {
        return Err("nothing playable in those paths".into());
    }

    Ok(files
        .into_iter()
        .map(|p| {
            let key = p.to_string_lossy().into_owned();
            db::query::by_path(conn, &key)
                .ok()
                .flatten()
                .or_else(|| scan::read_track(&p))
                .unwrap_or(Track {
                    path: key,
                    ..Default::default()
                })
        })
        .collect())
}

fn print_formats() {
    let opus = cfg!(feature = "opus");
    println!("containers: WAV, AIFF, CAF, ISO/MP4 (m4a), MKV/WebM, OGG, FLAC, ADTS");
    print!("codecs:     FLAC, ALAC, MP1/MP2/MP3, AAC-LC, Vorbis, PCM, ADPCM");
    println!("{}", if opus { ", Opus" } else { "" });
    println!();
    if opus {
        println!("Opus is decoded by playr itself, in OGG and WebM; Symphonia 0.6");
        println!("demuxes it but ships no decoder. Mono and stereo only.");
    } else {
        println!("Opus is NOT in this build. It needs libopus, which needs cmake,");
        println!("so it is opt-in. To add it:");
        println!();
        println!("    cargo build --release --features opus");
    }
    println!();
    print!("not decodable: ");
    if !opus {
        print!("Opus, ");
    }
    println!("WavPack, WMA, Musepack, APE, DSD, TTA, TAK.");
    println!("playr reports such files and moves to the next track.");
}
