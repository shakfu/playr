//! M3U playlists: writing one from tracks, and reading the tracks one lists.
//!
//! Written as extended M3U in UTF-8, with absolute paths, which other players
//! on the same machine read. Read leniently, since other players write
//! variants: comments and blank lines are skipped, a relative path is taken
//! from the playlist's directory, and a `file://` URL is decoded.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::db::{query, Result, Track};

/// `tracks` as an extended M3U playlist called `name`, and how many were left
/// out because their path holds a line break, which a line cannot carry.
pub fn write(name: &str, tracks: &[Track]) -> (String, usize) {
    // A tag with a line break would end its line early.
    let line = |s: &str| s.replace(['\n', '\r'], " ");
    let mut text = format!("#EXTM3U\n#PLAYLIST:{}\n", line(name));
    let mut left_out = 0;
    for t in tracks {
        if t.path.contains(['\n', '\r']) {
            left_out += 1;
            continue;
        }
        let secs = t.duration_ms.map_or(-1, |ms| (ms + 500) / 1000);
        let title = match t.artist.as_deref().filter(|a| !a.is_empty()) {
            Some(artist) => format!("{artist} - {}", t.display_title()),
            None => t.display_title(),
        };
        text.push_str(&format!(
            "#EXTINF:{secs},{}\n{}\n",
            line(&title),
            plain(&t.path)
        ));
    }
    (text, left_out)
}

/// `path` without the `\\?\` prefix a Windows canonical path carries, which
/// the library stores and other players may not read: `\\?\C:\m\a.flac` as
/// `C:\m\a.flac`, and `\\?\UNC\host\share\a.flac` as `\\host\share\a.flac`.
/// Any other path is returned as it is. Import matches either form.
pub fn plain(path: &str) -> std::borrow::Cow<'_, str> {
    let Some(rest) = path.strip_prefix(r"\\?\") else {
        return path.into();
    };
    if let Some(unc) = rest.strip_prefix(r"UNC\") {
        return format!(r"\\{unc}").into();
    }
    let drive = rest.as_bytes();
    match drive {
        [letter, b':', b'\\', ..] if letter.is_ascii_alphabetic() => rest.into(),
        _ => path.into(),
    }
}

/// The name a `#PLAYLIST:` line gives, if any.
pub fn name(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("#PLAYLIST:"))
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
}

/// The paths `text` lists, in order, with relative ones taken from `dir`.
pub fn paths(text: &str, dir: &Path) -> Vec<PathBuf> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| match l.strip_prefix("file://") {
            // `file:///m/a.flac`, or `file://localhost/m/a.flac`.
            Some(rest) => Some(PathBuf::from(decode(
                rest.strip_prefix("localhost").unwrap_or(rest),
            ))),
            // Another scheme, such as a stream's URL, names no file.
            None if l.contains("://") => None,
            None => Some(dir.join(l)),
        })
        .collect()
}

/// `%XX` escapes decoded as bytes; an escape that is not two hex digits is
/// kept as written.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The library's track for each of `paths`, in order, and the paths that
/// name no track in it. A path is matched as written, then with its links
/// and `..` resolved.
pub fn resolve(conn: &Connection, paths: &[PathBuf]) -> Result<(Vec<Track>, Vec<PathBuf>)> {
    let (mut found, mut missing) = (Vec::new(), Vec::new());
    for path in paths {
        let as_written = path.to_string_lossy();
        let track = match query::by_path(conn, &as_written)? {
            Some(t) => Some(t),
            None => match std::fs::canonicalize(path) {
                Ok(real) => query::by_path(conn, &real.to_string_lossy())?,
                Err(_) => None,
            },
        };
        match track {
            Some(t) => found.push(t),
            None => missing.push(path.clone()),
        }
    }
    Ok((found, missing))
}
