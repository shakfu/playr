//! One playr at a time: the terminal, the window or the server.
//!
//! Each keeps the library's playlists and marks in memory and checks changes
//! against that copy, so a second process could replace the first one's
//! playlist without asking. The lock is per user, whichever library is open.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

/// Why a second playr does not start.
pub const ALREADY_RUNNING: &str =
    "playr is already running, in a terminal, a window or a server; quit it first";

/// The claim on being the one playr running. The system releases it when the
/// process ends, however it ends.
#[derive(Debug)]
pub struct Instance {
    _lock: Option<File>,
}

/// The lock file, beside the default library.
pub fn lock_path() -> PathBuf {
    playr_core::db::default_path().with_file_name("instance.lock")
}

/// Claims [`lock_path`]; see [`claim_at`].
pub fn claim() -> Result<Instance, String> {
    claim_at(&lock_path())
}

/// Claims the lock at `path`, or returns [`ALREADY_RUNNING`] if another
/// process holds it. Where the file cannot be created or locked, playr
/// starts without the claim rather than not at all.
pub fn claim_at(path: &Path) -> Result<Instance, String> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path);
    let Ok(file) = file else {
        return Ok(Instance { _lock: None });
    };
    match file.try_lock() {
        Ok(()) => Ok(Instance { _lock: Some(file) }),
        Err(TryLockError::WouldBlock) => Err(ALREADY_RUNNING.into()),
        Err(TryLockError::Error(_)) => Ok(Instance { _lock: None }),
    }
}
