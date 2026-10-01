//! Converting a kit with ConvertWithMoss, stood in for by a shell script.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use playr_core::convertwithmoss::{convert, is_format_name};

/// Held by each test: a script still open for writing in one thread when
/// another starts a process cannot be run, "Text file busy".
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn alone() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}

/// An executable script at `dir/program` with `body`, which sees the
/// arguments ConvertWithMoss would.
fn program(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("program");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// An export directory `dir/amen` holding `amen.sfz`, which it returns.
fn kit(dir: &Path) -> PathBuf {
    let export = dir.join("amen");
    std::fs::create_dir(&export).unwrap();
    let kit = export.join("amen.sfz");
    std::fs::write(&kit, "<region> sample=000-amen_S00.wav key=36\n").unwrap();
    kit
}

#[test]
fn a_kit_is_converted_into_a_directory_named_after_the_format() {
    let _alone = alone();
    let dir = tempfile::tempdir().unwrap();
    // The destination is the last argument; the script records the rest there.
    let program = program(
        dir.path(),
        r#"for last; do :; done; echo "$@" > "$last/args""#,
    );
    let kit = kit(dir.path());
    let converted = convert(&program, &kit, "sf2").unwrap();
    assert!(converted.warnings.is_empty());
    let dest = converted.dir;
    assert_eq!(dest, dir.path().join("amen/sf2"));
    let args = std::fs::read_to_string(dest.join("args")).unwrap();
    assert_eq!(
        args.trim(),
        format!("-s sfz -d sf2 {} {}", kit.display(), dest.display())
    );
    // The slices have not changed, so a second conversion is refused.
    let again = convert(&program, &kit, "sf2").unwrap_err();
    assert!(again.ends_with("sf2 exists already"), "{again}");
    assert!(dest.join("args").is_file(), "the first conversion is kept");
}

#[test]
fn what_a_conversion_left_out_is_reported_with_its_result() {
    let _alone = alone();
    let dir = tempfile::tempdir().unwrap();
    // ConvertWithMoss 20.3.0's output for 40 slices to opxy, shortened: the
    // loss is a plain line among the progress, and the exit is 0.
    let program = program(
        dir.path(),
        r#"for last; do :; done; touch "$last/patch.json"
echo "ConvertWithMoss 20.3.0"
echo "Analyzing: amen.sfz"
echo "Using default volume envelope for category 'Unknown'."
echo "The preset has 40 regions but the device plays at most 24, the rest is dropped."
echo "Re-sampling from 24 bit / 44100 Hz to 16 bit / 44100 Hz as required by the destination format..."
echo "Done"
echo "Could not read the loop of 003.wav" >&2"#,
    );
    let converted = convert(&program, &kit(dir.path()), "opxy").unwrap();
    assert_eq!(
        converted.warnings,
        [
            "The preset has 40 regions but the device plays at most 24, the rest is dropped.",
            "Could not read the loop of 003.wav",
        ]
    );
}

#[test]
fn a_conversion_that_fails_leaves_no_directory_and_says_why() {
    let _alone = alone();
    let dir = tempfile::tempdir().unwrap();
    let kit = kit(dir.path());
    let dest = dir.path().join("amen/nope");

    // As ConvertWithMoss does: the reason first on its error output, exit 0.
    let failing = program(
        dir.path(),
        "echo 'Invalid value for destination format: nope' >&2; echo 'Allowed values are: [sf2]' >&2",
    );
    let error = convert(&failing, &kit, "nope").unwrap_err();
    assert_eq!(
        error,
        "ConvertWithMoss: Invalid value for destination format: nope"
    );
    assert!(!dest.exists());

    let silent = program(dir.path(), "echo 'Finished.'");
    let error = convert(&silent, &kit, "nope").unwrap_err();
    assert_eq!(error, "ConvertWithMoss wrote nothing");
    assert!(!dest.exists());

    // A file written does not excuse a failing exit.
    let partial = program(
        dir.path(),
        r#"for last; do :; done; touch "$last/half"; exit 1"#,
    );
    assert!(convert(&partial, &kit, "nope").is_err());
    assert!(!dest.exists());
}

#[test]
fn nothing_runs_without_the_program_a_kit_and_a_plain_format_name() {
    let _alone = alone();
    let dir = tempfile::tempdir().unwrap();
    let kit = kit(dir.path());
    let program = program(dir.path(), "exit 0");

    let missing = dir.path().join("not-installed");
    let error = convert(&missing, &kit, "sf2").unwrap_err();
    assert!(
        error.starts_with("ConvertWithMoss is not at ") && error.contains("not-installed"),
        "{error}"
    );

    let error = convert(&program, &dir.path().join("amen/gone.sfz"), "sf2").unwrap_err();
    assert!(error.starts_with("no kit at "), "{error}");

    // A format is a directory name: it cannot climb out of the export.
    for name in ["../up", "a/b", "", "SF2", "sf 2"] {
        assert!(!is_format_name(name), "{name}");
        assert!(convert(&program, &kit, name).is_err(), "{name}");
    }
    assert!(!dir.path().join("up").exists());
    assert_eq!(
        std::fs::read_dir(dir.path().join("amen")).unwrap().count(),
        1
    );
}
