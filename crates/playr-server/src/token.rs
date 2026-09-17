//! The token a browser must present, kept in a file so a bookmark survives a
//! restart.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Bytes of randomness in a token, written as twice as many hex digits.
const BYTES: usize = 32;

/// The token file, beside the default library.
pub fn default_path() -> PathBuf {
    playr_core::db::default_path().with_file_name("server.token")
}

/// Reads the token at `path`, or creates one there, readable only by its owner.
pub fn load_or_create(path: &Path) -> io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let token = text.trim();
            if token.len() == BYTES * 2 && token.bytes().all(|b| b.is_ascii_hexdigit()) {
                Ok(token.to_string())
            } else {
                Err(io::Error::other(format!(
                    "{} does not hold a token; delete it to make a new one",
                    path.display()
                )))
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => create(path),
        Err(e) => Err(e),
    }
}

fn create(path: &Path) -> io::Result<String> {
    let mut bytes = [0u8; BYTES];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(token.as_bytes())?;
    Ok(token)
}

/// Whether `given` is `token`, taking the same time wherever they differ.
pub fn matches(token: &str, given: &str) -> bool {
    token.len() == given.len()
        && token
            .bytes()
            .zip(given.bytes())
            .fold(0, |differ, (a, b)| differ | (a ^ b))
            == 0
}
