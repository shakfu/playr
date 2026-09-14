//! The settings file's top-level keys, read by the core with no frontend, and
//! the tables it hands back to one.

use playr_core::audio::Mode;
use playr_core::settings::{Settings, DEFAULT_SETTINGS};

/// The defaults with `text` applied, for a frontend that names no tables.
fn parse(text: &str) -> Result<Settings, Vec<String>> {
    let mut settings = Settings::default();
    let (tables, errors) = settings.apply(text, &[]);
    assert!(tables.is_empty());
    errors.finish().map(|()| settings)
}

#[test]
fn the_defaults_file_sets_the_defaults() {
    let settings = parse(DEFAULT_SETTINGS).unwrap();
    assert_eq!(settings, Settings::default());
    assert_eq!(
        (settings.volume, settings.mode, settings.speed),
        (1.0, Mode::Normal, 0)
    );
    let settings = parse("volume = 40\nmode = \"Repeat-One\"\nspeed = -3").unwrap();
    assert_eq!(
        (settings.volume, settings.mode, settings.speed),
        (0.4, Mode::RepeatOne, -3)
    );
}

#[test]
fn a_mode_is_named_in_full() {
    // A prefix that is unique today would stop parsing when a mode is added.
    assert_eq!(
        parse("mode = \"shuf\"").unwrap_err(),
        ["line 1: unknown mode shuf; modes: normal, shuffle, repeat, repeat-one"]
    );
    assert_eq!(
        parse("mode = 1").unwrap_err(),
        ["line 1: mode cannot be an integer"]
    );
}

#[test]
fn tables_a_frontend_names_come_back_and_others_are_errors() {
    let text = "[keys]\nq = 'quit'\n\n[gui]\ntheme = 'dark'\n\n[colours]\nred = 1\n";
    let mut settings = Settings::default();
    let (tables, errors) = settings.apply(text, &["gui", "keys"]);
    let names: Vec<&str> = tables.iter().map(|(n, _)| n.get_ref().as_ref()).collect();
    assert_eq!(names, ["keys", "gui"], "not in file order");
    assert_eq!(
        errors.finish().unwrap_err(),
        ["line 7: unknown setting: colours"]
    );

    // A frontend's errors, added after the core's, are listed with them by line.
    let text = "keys = { q = 3 }\nvolume = 400\n";
    let mut settings = Settings::default();
    let (tables, mut errors) = settings.apply(text, &["keys"]);
    errors.add(tables[0].1.span().start, "q must be a command string");
    assert_eq!(
        errors.finish().unwrap_err(),
        [
            "line 1: q must be a command string",
            "line 2: volume is a number from 0 to 100",
        ]
    );
}

#[test]
fn a_syntax_error_is_the_only_error() {
    let errors = parse("volume = 60\nmode = shuffle\nfrob = 1\n").unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].starts_with("line 2: "), "{errors:?}");
}

#[test]
fn the_samples_directory_defaults_to_music_and_expands_home() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    assert_eq!(
        Settings::default().samples,
        home.join("Music/playr/samples")
    );
    let settings = parse("samples = \"~/loops\"").unwrap();
    assert_eq!(settings.samples, home.join("loops"));
    let settings = parse("samples = \"/tmp/cuts\"").unwrap();
    assert_eq!(settings.samples, std::path::PathBuf::from("/tmp/cuts"));
    assert_eq!(
        parse("samples = \"cuts\"").unwrap_err(),
        ["line 1: samples must be an absolute path or start with ~/"]
    );
    assert_eq!(
        parse("samples = 3").unwrap_err(),
        ["line 1: samples cannot be an integer"]
    );
}

#[test]
fn onset_sensitivity_defaults_to_the_middle_and_stays_in_range() {
    assert_eq!(Settings::default().onset_sensitivity, 0.5);
    assert_eq!(
        parse("onset_sensitivity = 0.8").unwrap().onset_sensitivity,
        0.8
    );
    assert_eq!(
        parse("onset_sensitivity = 1").unwrap().onset_sensitivity,
        1.0
    );
    for bad in [
        "onset_sensitivity = 1.5",
        "onset_sensitivity = -0.1",
        "onset_sensitivity = \"high\"",
    ] {
        assert_eq!(
            parse(bad).unwrap_err(),
            ["line 1: onset_sensitivity is a number from 0 to 1"],
            "{bad}"
        );
    }
}
