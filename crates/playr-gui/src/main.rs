//! playr-gui: playr in a desktop window.

// No console window behind the GUI on Windows, except in debug builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use clap::Parser;
use eframe::egui;
use playr_app::config::{self, Config};
use playr_app::dispatch::Frontend;
use playr_app::instance::{self, Instance};
use playr_app::model::Model;
use playr_core::audio::Player;
use playr_core::db::{self, Track};
use playr_core::scan;
use playr_gui::{Errors, Gui};

/// playr - a music player, in a window
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Files or directories to play; directories are played recursively
    paths: Vec<PathBuf>,

    /// Use a different library file
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,

    /// Use a different settings file
    #[arg(long, value_name = "PATH")]
    settings: Option<PathBuf>,
}

/// What the window starts with.
struct Start {
    /// Held while the window is open; see `playr_app::instance`.
    instance: Option<Instance>,
    conn: rusqlite::Connection,
    player: Player,
    tracks: Vec<Track>,
    config: Config,
    library: PathBuf,
}

fn main() -> eframe::Result {
    let cli = Cli::parse();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("playr")
        .with_app_id("playr")
        .with_inner_size([1100.0, 720.0]);
    // The window's own icon; the platforms' bundles carry theirs.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/playr.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    match start(cli) {
        Ok(mut start) => {
            let _instance = start.instance.take();
            eframe::run_native(
                "playr",
                options,
                Box::new(move |cc| {
                    cc.egui_ctx.set_theme(egui::Theme::Dark);
                    let ctx = cc.egui_ctx.clone();
                    let mut model = Model::waking(
                        start.conn,
                        start.player,
                        start.tracks,
                        start.config,
                        move || ctx.request_repaint(),
                    );
                    // `:scan` creates the library here when there is none yet.
                    model.session_mut().set_library_path(start.library);
                    Ok(Box::new(Gui::new(model)))
                }),
            )
        }
        Err(errors) => eframe::run_native(
            "playr",
            options,
            Box::new(|cc| {
                cc.egui_ctx.set_theme(egui::Theme::Dark);
                Ok(Box::new(Errors(errors)))
            }),
        ),
    }
}

/// Opens the library, settings, files and audio device, as the terminal does.
/// A window has no visible stderr, so every problem is returned to be shown.
fn start(cli: Cli) -> Result<Start, Vec<String>> {
    let instance = instance::claim().map_err(|e| vec![e])?;
    let library = cli.db.unwrap_or_else(db::default_path);
    let fail = |e: &dyn std::fmt::Display| vec![e.to_string()];
    // Only a scan creates the library; without one playr runs on an empty one.
    let conn = match library.try_exists() {
        Ok(true) => db::open(&library),
        Ok(false) => db::open_memory(),
        Err(e) => return Err(fail(&e)),
    }
    .map_err(|e| fail(&e))?;
    let config = match (&cli.settings, config::default_path()) {
        (Some(path), _) => Config::load(path, true),
        (None, Some(path)) => Config::load(&path, false),
        (None, None) => Ok(Config::default()),
    }?;
    let mut tracks = Vec::new();
    if !cli.paths.is_empty() {
        let found = scan::playable(&cli.paths, |key| {
            db::query::by_path(&conn, key).ok().flatten()
        });
        if found.tracks.is_empty() {
            let mut problems = found.problems;
            problems.push("nothing playable in those paths".into());
            return Err(problems);
        }
        tracks = found.tracks;
    }
    let player = Player::new().map_err(|e| fail(&e))?;
    Ok(Start {
        instance: Some(instance),
        conn,
        player,
        tracks,
        config,
        library,
    })
}
