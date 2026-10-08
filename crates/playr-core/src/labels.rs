//! A track's marks as a file other programs read: an Audacity label track, or
//! a cue sheet whose tracks start at the marks.

use std::path::Path;
use std::time::Duration;

/// A file of marks [`Session::export_marks`](crate::session::Session::export_marks) writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkFile {
    /// Audacity's label track text: start, end and label, tab-separated, in
    /// seconds. Imported with File, Import, Labels.
    Audacity,
    /// A cue sheet: a track at each mark, titled by its label.
    Cue,
}

impl MarkFile {
    pub const NAMES: [(&str, MarkFile); 2] =
        [("audacity", MarkFile::Audacity), ("cue", MarkFile::Cue)];

    pub fn name(self) -> &'static str {
        match self {
            MarkFile::Audacity => "audacity",
            MarkFile::Cue => "cue",
        }
    }

    /// What follows the track's name in the file's name.
    pub fn suffix(self) -> &'static str {
        match self {
            MarkFile::Audacity => "-labels.txt",
            MarkFile::Cue => ".cue",
        }
    }
}

/// Marks as Audacity point labels, a line each: the time twice, then the
/// label, which is empty for a mark without one.
pub fn audacity(marks: &[(Duration, Option<String>)]) -> String {
    marks
        .iter()
        .map(|(at, label)| {
            let s = at.as_secs_f64();
            // A tab or a line break would end the field or the line.
            let text = label
                .as_deref()
                .unwrap_or("")
                .replace(['\t', '\n', '\r'], " ");
            format!("{s:.6}\t{s:.6}\t{text}\n")
        })
        .collect()
}

/// A cue sheet for the track at `track`, a track starting at each mark, and
/// one from the start before the first mark if it is not at 0. A track is
/// titled by its mark's label, else by its number.
pub fn cue(track: &Path, marks: &[(Duration, Option<String>)]) -> String {
    let kind = match track
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .as_deref()
    {
        Some("mp3") => "MP3",
        Some("aif" | "aiff") => "AIFF",
        _ => "WAVE",
    };
    let mut starts: Vec<(Duration, Option<&str>)> = Vec::new();
    if marks.first().is_none_or(|(at, _)| !at.is_zero()) {
        starts.push((Duration::ZERO, None));
    }
    starts.extend(marks.iter().map(|(at, label)| (*at, label.as_deref())));
    // The whole path, so the sheet plays from anywhere; a title is cut to 80.
    let file = track.to_string_lossy().replace('"', "'");
    let mut out = format!("FILE \"{file}\" {kind}\n");
    for (n, (at, label)) in (1..).zip(starts) {
        let title = label.map_or_else(|| format!("Track {n:02}"), quoted);
        out.push_str(&format!(
            "  TRACK {n:02} AUDIO\n    TITLE \"{title}\"\n    INDEX 01 {}\n",
            index(at)
        ));
    }
    out
}

/// `text` safe inside a cue sheet's quotes, which it has no escape for, and
/// within the 80 characters the format allows a title.
fn quoted(text: &str) -> String {
    text.replace('"', "'")
        .replace(['\t', '\n', '\r'], " ")
        .chars()
        .take(80)
        .collect()
}

/// A time as a cue sheet's `mm:ss:ff`, in frames of 1/75 s, rounded down.
fn index(at: Duration) -> String {
    let frames = (at.as_secs_f64() * 75.0) as u64;
    let (minutes, rest) = (frames / (60 * 75), frames % (60 * 75));
    format!("{minutes:02}:{:02}:{:02}", rest / 75, rest % 75)
}
