//! playr: a minimal TUI music player.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

mod analyze;

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

    /// Play to this output device instead of the default, overriding the
    /// device setting; `playr devices` lists them
    #[arg(long, value_name = "ID")]
    device: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Add directories to the library, creating it if there is none
    Scan {
        /// Directories to scan, recursively. With none, re-scans those recorded.
        #[arg(value_name = "DIR")]
        dirs: Vec<PathBuf>,
    },
    /// List the directories the library covers, or change them
    Roots {
        #[command(subcommand)]
        op: Option<RootsOp>,
    },
    /// Remove tracks under directories whose files are gone, with their marks
    Prune {
        /// Directories to check. With none, checks every directory previously scanned.
        #[arg(value_name = "DIR")]
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
    /// List output devices, by the ID --device and the device setting take
    Devices,
    /// Decode library tracks once, recording checks, loudness and tempo;
    /// files are only read. Runs beside a playing playr
    Analyze {
        /// Files or directories in the library. With none, every track.
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
        /// Analyse tracks again even when nothing changed since
        #[arg(long)]
        force: bool,
        /// Print what is recorded, decoding nothing
        #[arg(long)]
        report: bool,
        /// Print the findings as JSON
        #[arg(long)]
        json: bool,
        /// Files to decode at once; the default leaves one processor free
        #[arg(long, value_name = "N")]
        jobs: Option<usize>,
    },
}

#[derive(Subcommand)]
enum RootsOp {
    /// Record a directory and scan it, as `playr scan DIR` does
    Add {
        #[arg(required = true, value_name = "DIR")]
        dirs: Vec<PathBuf>,
    },
    /// Forget a directory, and the tracks and marks under it
    Rm {
        #[arg(required = true, value_name = "DIR")]
        dirs: Vec<PathBuf>,
    },
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
    if let Some(Command::Devices) = cli.command {
        print_devices()?;
        return Ok(ExitCode::SUCCESS);
    }

    // One playr at a time, terminal or window; reading the library needs no
    // claim. Held until `run` returns.
    // `analyze` writes only its own tables, which no running playr caches.
    let reads_only = matches!(
        cli.command,
        Some(
            Command::Playlists
                | Command::Search { json: true, .. }
                | Command::Roots { op: None }
                | Command::Analyze { .. }
        )
    );
    let _instance = if reads_only {
        None
    } else {
        Some(playr_app::instance::claim()?)
    };

    // Only `scan` creates the library. Without one, everything else runs on an
    // empty in-memory library, so playing a file leaves nothing behind.
    let db_path = cli.db.unwrap_or_else(db::default_path);
    let scanning = matches!(
        cli.command,
        Some(
            Command::Scan { .. }
                | Command::Roots {
                    op: Some(RootsOp::Add { .. })
                }
        )
    );
    if matches!(
        cli.command,
        Some(
            Command::Prune { .. }
                | Command::Analyze { .. }
                | Command::Roots {
                    op: None | Some(RootsOp::Rm { .. })
                }
        )
    ) && !db_path.try_exists()?
    {
        return Err(format!("no library at {}", db_path.display()).into());
    }
    // A bare `playr scan` covers the recorded roots, and without a library
    // there are none. Checked before the library is opened, which would
    // otherwise leave an empty file behind. `prune` already returned above.
    if let Some(Command::Scan { ref dirs }) = cli.command {
        if dirs.is_empty() && !db_path.try_exists()? {
            eprintln!("{NO_SCAN_DIRS}");
            return Ok(ExitCode::FAILURE);
        }
    }
    let mut conn = if scanning || db_path.try_exists()? {
        db::open(&db_path)?
    } else {
        db::open_memory()?
    };

    // Everything else opens the interface, with tracks chosen by the arguments.
    let start: Vec<Track> = match cli.command {
        Some(Command::Formats | Command::Devices) => unreachable!("handled above"),
        Some(Command::Scan { dirs }) => return cmd_scan(&mut conn, &dirs),
        // `roots add` is `scan`: recording a root without scanning it would
        // leave a root the library holds nothing for.
        Some(Command::Roots {
            op: Some(RootsOp::Add { dirs }),
        }) => return cmd_scan(&mut conn, &dirs),
        Some(Command::Roots {
            op: Some(RootsOp::Rm { dirs }),
        }) => return cmd_forget(&conn, &dirs),
        Some(Command::Roots { op: None }) => {
            for root in db::roots(&conn)? {
                println!("{}", root.display());
            }
            return Ok(ExitCode::SUCCESS);
        }
        Some(Command::Prune { dirs }) => return cmd_prune(&conn, &dirs),
        Some(Command::Analyze {
            paths,
            force,
            report,
            json,
            jobs,
        }) => {
            let opts = analyze::Options {
                paths,
                force,
                report,
                json,
                jobs,
            };
            return analyze::run(&mut conn, opts);
        }
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

    // crossterm opens /dev/tty when stdin is not a terminal, so only stdout
    // shows whether the interface can be seen. Checked before the device opens.
    if !std::io::stdout().is_terminal() {
        return Err("playr needs a terminal: stdout is not one".into());
    }
    let player = Player::new(cli.device.as_deref().or(config.settings.device.as_deref()))?;

    // `ratatui::init` panics without a terminal; report it instead, since
    // running playr from a pipe or a service is an easy mistake to make.
    let mut terminal = ratatui::try_init().map_err(|e| format!("playr needs a terminal: {e}"))?;
    let mut app = ui::App::configured(conn, player, start, config);
    // crossterm drops colour under NO_COLOR (https://no-color.org) with an SGR
    // reset, which also clears the cursor row's reverse video, so playr drops it.
    app.set_colour(std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()));
    ratatui::crossterm::style::force_color_output(true);
    // `:scan` creates the library here when there is none yet.
    app.set_library_path(db_path);
    // The terminal and the window gain media keys together, under the parity
    // rule in docs/dev/gui.md.
    app.attach_media();
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

/// Refusing a bare `playr scan`, from `run` before the library is opened and
/// from `cmd_scan` once it is.
const NO_SCAN_DIRS: &str = "playr: no directories to scan; give one, as `playr scan DIR`";

fn cmd_scan(
    conn: &mut rusqlite::Connection,
    dirs: &[PathBuf],
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let recorded;
    let dirs = if dirs.is_empty() {
        recorded = db::roots(conn)?;
        if recorded.is_empty() {
            eprintln!("{NO_SCAN_DIRS}");
            return Ok(ExitCode::FAILURE);
        }
        recorded.as_slice()
    } else {
        dirs
    };
    let mut total = scan::ScanStats::default();
    let multi = dirs.len() > 1;
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
        if stats.seen > 0 {
            db::add_root(conn, path)?;
        }
        println!(
            "\r  {} files, {} added, {} unchanged, {} unreadable{:<20}",
            stats.seen, stats.added, stats.skipped, stats.failed, ""
        );
        total.seen += stats.seen;
        total.added += stats.added;
        total.skipped += stats.skipped;
        total.failed += stats.failed;
        let missing = db::missing_under(conn, path)?.len();
        if missing > 0 {
            let path = path.display();
            println!("  {missing} tracks missing; `playr prune {path}` removes them");
        }
    }
    if multi {
        println!(
            "total: {} files, {} added, {} unchanged, {} unreadable",
            total.seen, total.added, total.skipped, total.failed
        );
    }
    println!("library now holds {} tracks", db::query::count(conn)?);
    // The interface can analyse as it scans, with `analyze_on_scan`; here the
    // settings are not read, so say what is waiting instead.
    let waiting = playr_core::analysis::pending(conn, &db::query::all(conn)?)?.len();
    if waiting > 0 {
        println!("  {waiting} tracks not analysed; `playr analyze` measures them");
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_prune(
    conn: &rusqlite::Connection,
    dirs: &[PathBuf],
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let recorded;
    let dirs = if dirs.is_empty() {
        recorded = db::roots(conn)?;
        if recorded.is_empty() {
            eprintln!("playr: no directories to prune; give one, or scan one first");
            return Ok(ExitCode::FAILURE);
        }
        recorded.as_slice()
    } else {
        dirs
    };
    for path in dirs {
        if !path.is_dir() {
            eprintln!("playr: not a directory: {}", path.display());
            continue;
        }
        let pruned = db::prune_missing(conn, path)?;
        println!(
            "pruning {}: removed {} tracks and {} marks of missing files",
            path.display(),
            pruned.tracks,
            pruned.marks
        );
    }
    println!("library now holds {} tracks", db::query::count(conn)?);
    Ok(ExitCode::SUCCESS)
}

/// Forgets each root named, with the tracks and marks under it. A path that
/// is not a root is reported and the rest still run.
fn cmd_forget(
    conn: &rusqlite::Connection,
    dirs: &[PathBuf],
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut code = ExitCode::SUCCESS;
    for path in dirs {
        match db::forget_root(conn, path)? {
            Some(removed) => println!(
                "forgot {}: removed {} tracks and {} marks",
                path.display(),
                removed.tracks,
                removed.marks
            ),
            None => {
                eprintln!(
                    "playr: not one of the library's directories: {}",
                    path.display()
                );
                code = ExitCode::FAILURE;
            }
        }
    }
    println!("library now holds {} tracks", db::query::count(conn)?);
    Ok(code)
}

/// The tracks the paths on the command line name, with each problem reported.
fn collect_paths(
    conn: &rusqlite::Connection,
    paths: &[PathBuf],
) -> Result<Vec<Track>, Box<dyn std::error::Error>> {
    let found = scan::playable(paths, |key| db::query::by_path(conn, key).ok().flatten());
    for problem in &found.problems {
        eprintln!("playr: {problem}");
    }
    if found.tracks.is_empty() {
        return Err("nothing playable in those paths".into());
    }
    Ok(found.tracks)
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

/// One output device a line, its ID then its name, with the default marked `*`.
fn print_devices() -> Result<(), Box<dyn std::error::Error>> {
    let devices = playr_core::audio::output::devices()?;
    let width = devices.iter().map(|d| d.id.len()).max().unwrap_or(0);
    for d in &devices {
        let mark = if d.default { '*' } else { ' ' };
        println!("{mark} {:<width$}  {}", d.id, d.name);
    }
    Ok(())
}
