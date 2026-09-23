//! playr-server: playr without a screen, controlled from a web page and OSC.

use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{mpsc, Arc};

use clap::{Parser, Subcommand};
use playr_app::config::{self, Config, Program};
use playr_app::dispatch::Frontend;
use playr_app::instance;
use playr_app::model::Model;
use playr_core::audio::Player;
use playr_core::db;
use playr_server::state::Latest;
use playr_server::{http, osc, owner, token, web};

/// playr - a music player, controlled from a web page and OSC
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Use a different library file
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,

    /// Use a different settings file
    #[arg(long, value_name = "PATH")]
    settings: Option<PathBuf>,

    /// Play to this output device instead of the default, overriding the
    /// device setting; `playr devices` lists them
    #[arg(long, value_name = "ID")]
    device: Option<String>,

    /// Address and port for the web page; 0.0.0.0 reaches it from the network
    #[arg(long, value_name = "ADDR:PORT", default_value = "127.0.0.1:8080")]
    listen: SocketAddr,

    /// Serve the page without a token, on a network whose every device is
    /// trusted: anyone who can reach the address controls playr
    #[arg(long)]
    open: bool,

    /// A host name the page may be opened by, besides IP addresses, localhost
    /// and .local names; may be repeated
    #[arg(long = "host", value_name = "NAME")]
    hosts: Vec<String>,

    /// Address and port to receive OSC on; off unless given
    #[arg(long, value_name = "ADDR:PORT")]
    osc: Option<SocketAddr>,

    /// Where to send playback state as OSC, such as a tablet running TouchOSC
    #[arg(long, value_name = "ADDR:PORT")]
    osc_reply: Option<SocketAddr>,
}

#[derive(Subcommand)]
enum Command {
    /// Print the OSC addresses received and sent, as JSON
    OscSchema,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Some(Command::OscSchema) = cli.command {
        println!("{:#}", osc::schema());
        return ExitCode::SUCCESS;
    }
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(errors) => {
            for e in errors {
                eprintln!("playr-server: {e}");
            }
            ExitCode::FAILURE
        }
    }
}

/// Opens the library, settings, audio device and listening socket, then runs
/// the model until it quits.
fn run(cli: Cli) -> Result<(), Vec<String>> {
    let fail = |e: &dyn std::fmt::Display| vec![e.to_string()];
    let _instance = instance::claim().map_err(|e| vec![e])?;
    let token = match cli.open {
        true => None,
        false => {
            let path = token::default_path();
            let token = token::load_or_create(&path)
                .map_err(|e| vec![format!("{}: {e}", path.display())])?;
            Some(token)
        }
    };
    let listener =
        TcpListener::bind(cli.listen).map_err(|e| vec![format!("{}: {e}", cli.listen)])?;
    let osc_socket = match cli.osc {
        Some(addr) => Some(UdpSocket::bind(addr).map_err(|e| vec![format!("{addr}: {e}")])?),
        None => None,
    };
    let mut feedback = match cli.osc_reply {
        Some(to) => Some(osc::Feedback::new(to).map_err(|e| vec![format!("{to}: {e}")])?),
        None => None,
    };
    let library = cli.db.unwrap_or_else(db::default_path);
    // Only a scan creates the library; without one playr runs on an empty one.
    let conn = match library.try_exists() {
        Ok(true) => db::open(&library),
        Ok(false) => db::open_memory(),
        Err(e) => return Err(fail(&e)),
    }
    .map_err(|e| fail(&e))?;
    let config = match (&cli.settings, config::default_path()) {
        (Some(path), _) => Config::load_for(Program::Server, path, true),
        (None, Some(path)) => Config::load_for(Program::Server, &path, false),
        (None, None) => Ok(Config::default()),
    }?;
    let player = Player::new(cli.device.as_deref().or(config.settings.device.as_deref()))
        .map_err(|e| fail(&e))?;
    let mut model = Model::new(conn, player, Vec::new(), config);
    model.session_mut().set_library_path(library);

    let (requests, received) = mpsc::channel();
    let latest = Arc::new(Latest::default());
    let server = http::Config {
        token,
        hosts: cli.hosts,
        // Read once: `playr scan` cannot run while the server holds the lock.
        rescan: !model.session().roots().is_empty(),
    };
    match &server.token {
        Some(token) => println!("playr-server: http://{}/?token={token}", cli.listen),
        None => println!("playr-server: http://{}/ (open: no token)", cli.listen),
    }
    if let Some(socket) = osc_socket {
        let requests = requests.clone();
        std::thread::spawn(move || osc::listen(socket, requests));
    }
    let serving = latest.clone();
    std::thread::spawn(move || {
        if let Err(e) = http::serve(listener, server, requests, serving) {
            eprintln!("playr-server: {e}");
        }
    });
    owner::run(model, received, |model| {
        latest.set(web::screen(model).to_string());
        if let Some(feedback) = &mut feedback {
            feedback.send(model);
        }
    });
    Ok(())
}
