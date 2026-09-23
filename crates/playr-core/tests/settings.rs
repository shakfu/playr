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
    let text = "[keys]\nq = 'quit'\n\n[gui]\ncolumns = ['title']\n\n[colours]\nred = 1\n";
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

/// One settings file serves all three programs, so each passes over the
/// tables it does not read. Without this a `[gui]` table stopped the terminal.
#[test]
fn another_frontends_table_is_passed_over_not_refused() {
    // Core keys come first: after a table header, TOML reads bare keys as
    // that table's.
    let text = "volume = 40\n\n[gui]\ncolumns = ['title']\n\n[server]\nlisten = '0.0.0.0:8080'\n";
    let mut settings = Settings::default();
    let (tables, errors) = settings.apply(text, &["keys"]);
    assert!(tables.is_empty(), "a table this frontend never asked for");
    assert_eq!(errors.finish(), Ok(()));
    assert_eq!(settings.volume, 0.4, "core keys still apply");

    // A typo is still caught: only the names frontends own are passed over.
    let mut settings = Settings::default();
    let (_, errors) = settings.apply("[guii]\ncolumns = []\n", &["keys"]);
    assert_eq!(
        errors.finish().unwrap_err(),
        ["line 1: unknown setting: guii"]
    );

    // And a frontend's name used for something that is not a table.
    let mut settings = Settings::default();
    let (_, errors) = settings.apply("gui = 3\n", &["keys"]);
    assert_eq!(
        errors.finish().unwrap_err(),
        ["line 1: gui is another program's settings, and must be a table, not an integer"]
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
    let home = std::env::home_dir().unwrap();
    assert_eq!(
        Settings::default().samples,
        home.join("Music/playr/samples")
    );
    let settings = parse("samples = \"~/loops\"").unwrap();
    assert_eq!(settings.samples, home.join("loops"));
    // An absolute path as the platform writes one: `C:\...` on Windows, where
    // `/tmp/cuts` has no drive and is not absolute. A TOML literal string
    // keeps the backslashes.
    let cuts = std::env::temp_dir().join("cuts");
    let settings = parse(&format!("samples = '{}'", cuts.display())).unwrap();
    assert_eq!(settings.samples, cuts);
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

#[test]
fn auto_prune_defaults_off_and_takes_a_boolean() {
    assert!(!Settings::default().auto_prune);
    assert!(parse("auto_prune = true").unwrap().auto_prune);
    assert!(!parse("auto_prune = false").unwrap().auto_prune);
    assert_eq!(
        parse("auto_prune = 1").unwrap_err(),
        ["line 1: auto_prune cannot be an integer"]
    );
}

#[test]
fn analyze_on_scan_defaults_off_and_takes_a_boolean() {
    assert!(!Settings::default().analyze_on_scan);
    assert!(parse("analyze_on_scan = true").unwrap().analyze_on_scan);
    assert_eq!(
        parse("analyze_on_scan = 1").unwrap_err(),
        ["line 1: analyze_on_scan cannot be an integer"]
    );
}

#[test]
fn replaygain_defaults_off_and_takes_a_setting() {
    use playr_core::gain::ReplayGain;
    assert_eq!(Settings::default().replaygain, ReplayGain::Off);
    assert_eq!(
        parse("replaygain = \"Auto\"").unwrap().replaygain,
        ReplayGain::Auto
    );
    assert_eq!(
        parse("replaygain = \"loud\"").unwrap_err(),
        ["line 1: unknown replaygain loud; choices: off, track, album, auto"]
    );
    assert_eq!(
        parse("replaygain = true").unwrap_err(),
        ["line 1: replaygain cannot be a boolean"]
    );
}

#[test]
fn the_device_defaults_to_none_and_takes_an_id() {
    assert_eq!(Settings::default().device, None);
    assert_eq!(parse("device = \"\"").unwrap().device, None);
    assert_eq!(
        parse("device = \"alsa:hw:CARD=DAC,DEV=0\"")
            .unwrap()
            .device
            .as_deref(),
        Some("alsa:hw:CARD=DAC,DEV=0")
    );
    assert_eq!(
        parse("device = 2").unwrap_err(),
        ["line 1: device cannot be an integer"]
    );
}

#[test]
fn columns_and_sort_are_lists_of_column_names() {
    use playr_core::columns::{Column, SortKey};
    let settings = Settings::default();
    assert_eq!(
        settings.columns,
        [Column::Artist, Column::Album, Column::Title, Column::Time]
    );
    assert_eq!(
        settings.sort.first().map(|k| k.column),
        Some(Column::AlbumArtist)
    );

    let settings = parse("columns = ['title', 'tempo']\nsort = ['loudness desc']").unwrap();
    assert_eq!(settings.columns, [Column::Title, Column::Tempo]);
    assert_eq!(
        settings.sort,
        [SortKey {
            column: Column::Loudness,
            descending: true
        }]
    );
    // An empty sort leaves the library as it was read.
    assert_eq!(parse("sort = []").unwrap().sort, []);

    assert_eq!(
        parse("columns = ['loudest']").unwrap_err()[0],
        "line 1: unknown column loudest; choices: title, artist, album_artist, album, disc, track, year, time, tempo, loudness, peak, path"
    );
    assert_eq!(
        parse("columns = []").unwrap_err(),
        ["line 1: columns needs at least one column"]
    );
    assert_eq!(
        parse("columns = 'title'").unwrap_err(),
        ["line 1: columns is a list of names, not a string"]
    );
}
