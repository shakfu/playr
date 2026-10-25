//! Where the library and the settings live, by platform.
//!
//! Linux keeps the XDG directories. macOS keeps both files in
//! `~/Library/Application Support/playr`; Windows keeps the library in
//! `%LOCALAPPDATA%\playr`, since its paths are this machine's, and the
//! settings in `%APPDATA%\playr`, which roams. An `XDG_DATA_HOME` or
//! `XDG_CONFIG_HOME` set to an absolute path wins on every platform.
//!
//! Up to 0.19 every platform used `~/.local/share` and `~/.config`. A file
//! there is used while the platform's directory holds none, so nothing moves.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The library's file name, under [`data_dir`].
pub const LIBRARY: &str = "library.db";
/// The settings' file name, under [`config_dir`].
pub const SETTINGS: &str = "settings.toml";

/// What the paths depend on, so tests can name a platform and a home.
pub struct Env<'a> {
    /// `std::env::consts::OS`'s value.
    pub os: &'a str,
    pub var: &'a dyn Fn(&str) -> Option<OsString>,
    pub home: Option<PathBuf>,
    /// Whether a file exists, for the fallback to the old directory.
    pub exists: &'a dyn Fn(&Path) -> bool,
}

impl Env<'_> {
    /// This process's platform, environment and home.
    pub fn current() -> Env<'static> {
        Env {
            os: std::env::consts::OS,
            var: &|name| std::env::var_os(name),
            home: std::env::home_dir(),
            exists: &|p| p.exists(),
        }
    }

    fn absolute(&self, name: &str) -> Option<PathBuf> {
        (self.var)(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    }
}

/// The library's directory, `playr` included. `None` without a home.
pub fn data_dir(env: &Env) -> Option<PathBuf> {
    if let Some(base) = env.absolute("XDG_DATA_HOME") {
        return Some(base.join("playr"));
    }
    let old = env.home.as_ref().map(|h| h.join(".local/share/playr"));
    let new = match env.os {
        "macos" => env
            .home
            .as_ref()
            .map(|h| h.join("Library/Application Support/playr")),
        "windows" => env.absolute("LOCALAPPDATA").map(|d| d.join("playr")),
        _ => return old,
    };
    either(env, new, old, LIBRARY)
}

/// The settings' directory, `playr` included. `None` without a home.
pub fn config_dir(env: &Env) -> Option<PathBuf> {
    if let Some(base) = env.absolute("XDG_CONFIG_HOME") {
        return Some(base.join("playr"));
    }
    let old = env.home.as_ref().map(|h| h.join(".config/playr"));
    let new = match env.os {
        "macos" => env
            .home
            .as_ref()
            .map(|h| h.join("Library/Application Support/playr")),
        "windows" => env.absolute("APPDATA").map(|d| d.join("playr")),
        _ => return old,
    };
    either(env, new, old, SETTINGS)
}

/// `new`, unless only `old` holds `file`.
fn either(env: &Env, new: Option<PathBuf>, old: Option<PathBuf>, file: &str) -> Option<PathBuf> {
    match (new, old) {
        (Some(new), Some(old))
            if !(env.exists)(&new.join(file)) && (env.exists)(&old.join(file)) =>
        {
            Some(old)
        }
        (Some(new), _) => Some(new),
        (None, old) => old,
    }
}
