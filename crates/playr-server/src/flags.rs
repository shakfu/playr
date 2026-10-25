//! The server's flags, from the command line over `[server]` in the settings.

use std::net::SocketAddr;

use playr_app::config::ServerSettings;

/// Where the page listens when neither the flag nor the settings say.
pub const LISTEN: ([u8; 4], u16) = ([127, 0, 0, 1], 8080);

/// What the server runs with.
#[derive(Debug, Clone, PartialEq)]
pub struct Flags {
    pub listen: SocketAddr,
    pub open: bool,
    pub hosts: Vec<String>,
    pub osc: Option<SocketAddr>,
    pub osc_reply: Option<String>,
}

impl Flags {
    /// Each of `given`, the command line's, or else `file`'s, or else the
    /// default. `--host` given once replaces the file's list.
    pub fn of(given: &ServerSettings, file: &ServerSettings) -> Flags {
        Flags {
            listen: given.listen.or(file.listen).unwrap_or(LISTEN.into()),
            open: given.open.or(file.open).unwrap_or(false),
            hosts: match given.hosts.is_empty() {
                true => file.hosts.clone(),
                false => given.hosts.clone(),
            },
            osc: given.osc.or(file.osc),
            osc_reply: given.osc_reply.clone().or_else(|| file.osc_reply.clone()),
        }
    }
}
