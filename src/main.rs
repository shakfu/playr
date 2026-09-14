//! playr: a minimal TUI music player.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use playr::ui;
use playr_core::audio::Player;
use playr_core::db::{self, Track};
use playr_core::scan;

/// playr - a minimal TUI music player
///
/// With no command, browses the library; with paths, plays them.
#[derive(Parser)]
#[command(
    version,
    // Each subcommand name is a file `playr <path>` cannot play; `help` need not be one.
    disable_help_subcommand = true,
    override_usage = "playr [OPTIONS] [PATHS]...\n       playr [OPTIONS] <COMMAND>"
)]
struct Cli {
    /// Files or directories to play; directories are played recursively
    paths: Vec<PathBuf>,

    /// Use a different library file
    #[arg(long, global = true, value_name = "PATH")]
    db: Option<PathBuf>,

    /// Use a different settings file
    #[arg(long, global = true, value_name = "PATH")]
    settings: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Add directories to the library, creating it if there is none
    Scan {
        /// Directories to scan, recursively
        #[arg(required = true, value_name = "DIR")]
        dirs: Vec<PathBuf>,
    },
    /// Play everything matching a search; put a query that starts with - after --
    Search {
        /// Print the matches as a JSON array instead of playing them
        #[arg(long)]
        json: bool,
        /// Words to match, as in the / prompt; field:word matches one field
        #[arg(required = true, value_name = "QUERY")]
        query: Vec<String>,
    },
    /// Play a saved playlist
    Playlist {
        /// The playlist's name; quotes are optional
        #[arg(required = true, value_name = "NAME")]
        name: Vec<String>,
    },
    /// List saved playlists
    Playlists,
    /// Show which formats this build can decode
    Formats,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("playr: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if let Some(Command::Formats) = cli.command {
        print_formats();
        return Ok(ExitCode::SUCCESS);
    }

    // Only `scan` creates the library. Without one, everything else runs on an
    // empty in-memory library, so playing a file leaves nothing behind.
    let db_path = cli.db.unwrap_or_else(db::default_path);
    let scanning = matches!(cli.command, Some(Command::Scan { .. }));
    let mut conn = if scanning || db_path.try_exists()? {
        db::open(&db_path)?
    } else {
        db::open_memory()?
    };

    // Everything else opens the interface, with tracks chosen by the arguments.
    let start: Vec<Track> = match cli.command {
        Some(Command::Formats) => unreachable!("handled above"),
        Some(Command::Scan { dirs }) => return cmd_scan(&mut conn, &dirs),
        Some(Command::Playlists) => {
            for p in db::query::playlists(&conn)? {
                println!("{:<40} {:>4} tracks", p.name, p.len);
            }
            return Ok(ExitCode::SUCCESS);
        }
        Some(Command::Search { json, query }) => {
            let q = query.join(" ");
            if q.trim().is_empty() {
                return Err("search needs a query".into());
            }
            let hits = db::query::search(&conn, &q)?;
            if json {
                // `[]` for no matches, so a script can always parse the output.
                println!("{}", tracks_json(&hits));
                return Ok(if hits.is_empty() {
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                });
            }
            if hits.is_empty() {
                eprintln!("playr: nothing matches {q:?}");
                return Ok(ExitCode::FAILURE);
            }
            hits
        }
        Some(Command::Playlist { name }) => {
            let name = name.join(" ");
            let lists = db::query::playlists(&conn)?;
            let pl = db::query::find_playlist(&lists, name.trim())
                .ok_or_else(|| format!("no single playlist named {name:?}"))?;
            db::query::playlist_tracks(&conn, pl.id)?
        }
        None if cli.paths.is_empty() => Vec::new(),
        None => collect_paths(&conn, &cli.paths)?,
    };

    // Before the terminal is taken over, so every error can be read.
    let config = match (&cli.settings, playr_app::config::default_path()) {
        (Some(path), _) => playr_app::config::Config::load(path, true),
        (None, Some(path)) => playr_app::config::Config::load(&path, false),
        (None, None) => Ok(playr_app::config::Config::default()),
    };
    let config = match config {
        Ok(config) => config,
        Err(errors) => {
            for e in errors {
                eprintln!("playr: {e}");
            }
            return Ok(ExitCode::FAILURE);
        }
    };

    let player = Player::new()?;

    // `ratatui::init` panics without a terminal; report it instead, since
    // running playr from a pipe or a service is an easy mistake to make.
    let mut terminal = ratatui::try_init().map_err(|e| format!("playr needs a terminal: {e}"))?;
    let app = ui::App::configured(conn, player, start, config);
    let result = app.run(&mut terminal);
    ratatui::restore();
    result?;
    Ok(ExitCode::SUCCESS)
}

/// `tracks` as a JSON array of objects, one field per library column, with
/// `null` for a tag the file lacks.
fn tracks_json(tracks: &[Track]) -> String {
    let rows: Vec<serde_json::Value> = tracks
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "path": t.path,
                "title": t.title,
                "artist": t.artist,
                "album": t.album,
                "album_artist": t.album_artist,
                "track_no": t.track_no,
                "disc_no": t.disc_no,
                "year": t.year,
                "genre": t.genre,
                "duration_ms": t.duration_ms,
                "sample_rate": t.sample_rate,
                "channels": t.channels,
                "bit_depth": t.bit_depth,
                "mtime": t.mtime,
                "size": t.size,
            })
        })
        .collect();
    serde_json::Value::Array(rows).to_string()
}

fn cmd_scan(
    conn: &mut rusqlite::Connection,
    dirs: &[PathBuf],
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut total = scan::ScanStats::default();
    let multi = dirs.len() > 1;
    let mut removed = 0;
    for path in dirs {
        if !path.is_dir() {
            eprintln!("playr: not a directory: {}", path.display());
            continue;
        }
        println!("scanning {}", path.display());
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
    args: &[PathBuf],
) -> Result<Vec<Track>, Box<dyn std::error::Error>> {
    let mut files: Vec<PathBuf> = Vec::new();
    for arg in args {
        // Canonical, to match the library's keys: `playr .` must find the
        // rows `playr scan ~/music` wrote.
        let Ok(path) = arg.canonicalize() else {
            eprintln!("playr: no such file: {}", arg.display());
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
            eprintln!("playr: no such file: {}", arg.display());
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
