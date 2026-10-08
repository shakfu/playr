//! Converting an export's `.sfz` kit to another sampler's format.
//!
//! The work is ConvertWithMoss's, run as a separate program: it reads SFZ and
//! writes many formats, resampling where a device needs it. playr writes
//! none of them itself. `docs/dev/hardware_samplers.md` has the reasons and a
//! test conversion of each name in [`FORMATS`].
//!
//! It is an extension: off until `convert-with-moss.enable` is set under
//! `[extensions]` in `settings.toml`, since it runs a program that is not
//! playr's.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The formats offered by name, as ConvertWithMoss's `-d` takes them. Any
/// other name it knows works too.
pub const FORMATS: [&str; 16] = [
    "1010music",
    "ableton",
    "bento",
    "deluge",
    "distingex",
    "emulti",
    "exs24",
    "mc707",
    "mpc",
    "nki",
    "opxy",
    "renoise",
    "s2400",
    "sf2",
    "sp404mk2",
    "sxt",
];

/// The most zones a preset of a format holds, where one is known. A kit with
/// more is converted in parts, as AudioHit splits `.ot` files.
/// `docs/dev/hardware_samplers.md` lists the devices' slice limits.
pub const LIMITS: [(&str, usize); 1] = [("opxy", 24)];

/// The most zones a preset of `format` holds, if known.
pub fn limit(format: &str) -> Option<usize> {
    LIMITS.iter().find(|(f, _)| *f == format).map(|(_, n)| *n)
}

/// Where ConvertWithMoss's installer puts its command line. Only the Linux
/// path is confirmed; the other two follow each installer's usual layout.
pub fn default_program() -> PathBuf {
    PathBuf::from(if cfg!(target_os = "macos") {
        "/Applications/ConvertWithMoss.app/Contents/MacOS/ConvertWithMoss"
    } else if cfg!(windows) {
        r"C:\Program Files\ConvertWithMoss\ConvertWithMossCLI.exe"
    } else {
        "/opt/convertwithmoss/bin/ConvertWithMoss"
    })
}

/// What to say when no ConvertWithMoss is at `program`.
pub fn not_installed(program: &Path) -> String {
    format!(
        "ConvertWithMoss is not at {}; install it, or set convert-with-moss.path in settings.toml",
        program.display()
    )
}

/// Whether `format` could be one of ConvertWithMoss's names. It becomes a
/// directory name, so nothing but lower-case letters and digits passes.
pub fn is_format_name(format: &str) -> bool {
    !format.is_empty()
        && format
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// A conversion that wrote its directory.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub dir: PathBuf,
    /// The presets written: more than one where the kit passed the format's
    /// [`limit`].
    pub parts: usize,
    /// What ConvertWithMoss said it left out, a line each, such as zones
    /// past a device's limit.
    pub warnings: Vec<String>,
}

/// Words ConvertWithMoss's messages use for content it did not convert, read
/// from its `Strings.properties`, its only language. Its progress lines use
/// none of them.
const LOSS: [&str; 5] = ["dropped", "skipped", "ignored", "truncated", "discarded"];

/// The lines of a run that succeeded which report lost content: those on the
/// error output, and those on the output that name a loss. It reports both
/// kinds on the output, unmarked, so the words are all that tell them apart.
fn warnings(stdout: &str, stderr: &str) -> Vec<String> {
    let lost = |line: &str| {
        let lower = line.to_lowercase();
        LOSS.iter().any(|w| lower.contains(w))
    };
    stdout
        .lines()
        .filter(|l| lost(l))
        .chain(stderr.lines())
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// Converts the kit at `kit` to `format` with the ConvertWithMoss at
/// `program`, into a new directory named `format` beside the kit. A kit with
/// more regions than the format's [`limit`] is converted in parts of at most
/// that many, each a preset with its keys from the first again. A conversion
/// that fails leaves no directory.
pub fn convert(program: &Path, kit: &Path, format: &str) -> Result<Converted, String> {
    if !is_format_name(format) {
        return Err(format!("{format} is not a format name"));
    }
    if !program.is_file() {
        return Err(not_installed(program));
    }
    let text = fs::read_to_string(kit).map_err(|_| format!("no kit at {}", kit.display()))?;
    let dest = kit.with_file_name(format);
    // An export's slices never change, so neither would a second conversion.
    fs::create_dir(&dest).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => format!("{} exists already", dest.display()),
        _ => format!("cannot create {}: {e}", dest.display()),
    })?;
    let split = parts(&text, limit(format));
    let kits = match &split {
        None => Ok(vec![kit.to_path_buf()]),
        Some(parts) => write_parts(kit, parts),
    };
    let result = kits.and_then(|kits| {
        let mut warnings = Vec::new();
        for k in &kits {
            warnings.extend(run(program, k, format, &dest)?);
        }
        Ok((kits.len(), warnings))
    });
    // The parts are written beside the kit, so their samples are found; only
    // the presets made from them are kept.
    for n in 1..=split.map_or(0, |p| p.len()) {
        _ = fs::remove_file(part_path(kit, n));
    }
    match result {
        Ok((parts, warnings)) => Ok(Converted {
            dir: dest,
            parts,
            warnings,
        }),
        Err(e) => {
            let _ = fs::remove_dir_all(&dest);
            Err(e)
        }
    }
}

/// The kit's text in parts of at most `limit` regions, each region on its
/// key from the first again; `None` when it fits whole.
fn parts(text: &str, limit: Option<usize>) -> Option<Vec<String>> {
    let limit = limit.filter(|&n| n > 0)?;
    let regions: Vec<&str> = text
        .lines()
        .filter(|l| l.trim_start().starts_with("<region>"))
        .collect();
    if regions.len() <= limit {
        return None;
    }
    let rekey = |line: &str, i: usize| {
        let key = crate::samples::FIRST_KEY + i;
        line.split(' ')
            .map(|word| match word.starts_with("key=") {
                true => format!("key={key}"),
                false => word.to_string(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let parts = regions
        .chunks(limit)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .map(|(i, line)| rekey(line, i) + "\n")
                .collect()
        })
        .collect();
    Some(parts)
}

/// Where the parts of `kit` are written: beside it, numbered from 1.
fn part_path(kit: &Path, n: usize) -> PathBuf {
    let stem = kit.file_stem().unwrap_or_default().to_string_lossy();
    kit.with_file_name(format!("{stem}-{n}.sfz"))
}

/// Writes `parts` beside `kit`, and returns their paths.
fn write_parts(kit: &Path, parts: &[String]) -> Result<Vec<PathBuf>, String> {
    (1..)
        .zip(parts)
        .map(|(n, text)| {
            let path = part_path(kit, n);
            fs::write(&path, text)
                .map(|()| path.clone())
                .map_err(|e| format!("cannot write {}: {e}", path.display()))
        })
        .collect()
}

/// Runs ConvertWithMoss on one kit into `dest`, and returns what it said it
/// left out.
fn run(program: &Path, kit: &Path, format: &str, dest: &Path) -> Result<Vec<String>, String> {
    let before = fs::read_dir(dest).map_or(0, Iterator::count);
    // The kit itself, not its directory, which holds earlier conversions.
    let output = Command::new(program)
        .args(["-s", "sfz", "-d", format])
        .arg(kit)
        .arg(dest)
        .output()
        .map_err(|e| format!("cannot run {}: {e}", program.display()))?;
    let wrote = fs::read_dir(dest).map_or(0, Iterator::count) > before;
    if output.status.success() && wrote {
        return Ok(warnings(
            &String::from_utf8_lossy(&output.stdout),
            &String::from_utf8_lossy(&output.stderr),
        ));
    }
    // It exits 0 whatever happened, having written nothing, and says why on
    // its error output, first line first.
    let said = String::from_utf8_lossy(&output.stderr);
    Err(match said.lines().find(|l| !l.trim().is_empty()) {
        Some(line) => format!("ConvertWithMoss: {}", line.trim()),
        None => "ConvertWithMoss wrote nothing".into(),
    })
}
