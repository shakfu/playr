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
pub const FORMATS: [&str; 7] = [
    "1010music",
    "ableton",
    "bento",
    "distingex",
    "mc707",
    "renoise",
    "sf2",
];

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

/// Converts the kit at `kit` to `format` with the ConvertWithMoss at
/// `program`, into a new directory named `format` beside the kit, which it
/// returns. A conversion that fails leaves no directory.
pub fn convert(program: &Path, kit: &Path, format: &str) -> Result<PathBuf, String> {
    if !is_format_name(format) {
        return Err(format!("{format} is not a format name"));
    }
    if !program.is_file() {
        return Err(not_installed(program));
    }
    if !kit.is_file() {
        return Err(format!("no kit at {}", kit.display()));
    }
    let dest = kit.with_file_name(format);
    // An export's slices never change, so neither would a second conversion.
    fs::create_dir(&dest).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => format!("{} exists already", dest.display()),
        _ => format!("cannot create {}: {e}", dest.display()),
    })?;
    let fail = |error: String| {
        let _ = fs::remove_dir_all(&dest);
        Err(error)
    };
    // The kit itself, not its directory, which holds earlier conversions.
    let run = Command::new(program)
        .args(["-s", "sfz", "-d", format])
        .arg(kit)
        .arg(&dest)
        .output();
    let output = match run {
        Ok(output) => output,
        Err(e) => return fail(format!("cannot run {}: {e}", program.display())),
    };
    let wrote = fs::read_dir(&dest).is_ok_and(|mut entries| entries.next().is_some());
    if output.status.success() && wrote {
        return Ok(dest);
    }
    // It exits 0 whatever happened, having written nothing, and says why on
    // its error output, first line first.
    let said = String::from_utf8_lossy(&output.stderr);
    fail(match said.lines().find(|l| !l.trim().is_empty()) {
        Some(line) => format!("ConvertWithMoss: {}", line.trim()),
        None => "ConvertWithMoss wrote nothing".into(),
    })
}
