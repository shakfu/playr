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
        (
            settings.master,
            settings.volume,
            settings.mode,
            settings.speed
        ),
        (None, None, Mode::Normal, 0)
    );
    let settings = parse("master = 40\nmode = \"Repeat-One\"\nspeed = -3").unwrap();
    assert_eq!(
        (settings.master, settings.mode, settings.speed),
        (Some(0.4), Mode::RepeatOne, -3)
    );
}

/// `volume` was a gain before `master` took its place as a fader position;
/// a file that still sets it keeps its level, and one setting both is refused.
#[test]
fn volume_is_read_as_a_gain_and_refused_beside_master() {
    let settings = parse("volume = 50").unwrap();
    assert_eq!((settings.volume, settings.master), (Some(0.5), None));
    assert_eq!(
        parse("master = 80\nvolume = 50").unwrap_err(),
        ["line 2: volume and master are both set; master replaces volume"]
    );
    assert_eq!(
        parse("master = 101").unwrap_err(),
        ["line 1: master is a number from 0 to 100"]
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
    assert_eq!(settings.volume, Some(0.4), "core keys still apply");

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
fn an_ot_file_is_written_only_when_asked_for() {
    assert!(!Settings::default().slice_ot_file);
    assert!(parse("slice_ot_file = true").unwrap().slice_ot_file);
    assert_eq!(
        parse("slice_ot_file = 1").unwrap_err(),
        ["line 1: slice_ot_file cannot be an integer"]
    );
}

#[test]
fn the_convert_with_moss_extension_is_off_until_enabled() {
    let installed = playr_core::convertwithmoss::default_program();
    let moss = Settings::default().convert_with_moss;
    assert!(
        !moss.enable,
        "an extension runs another program: off as shipped"
    );
    assert_eq!(moss.path, installed);
    assert!(installed.is_absolute());

    // Enabling it keeps the installer's path.
    let moss = parse("[extensions]\nconvert-with-moss.enable = true")
        .unwrap()
        .convert_with_moss;
    assert!(moss.enable);
    assert_eq!(moss.path, installed);

    let home = std::env::home_dir().unwrap();
    let moss = parse("[extensions.convert-with-moss]\nenable = true\npath = \"~/bin/convert-wm\"")
        .unwrap()
        .convert_with_moss;
    assert_eq!(
        (moss.enable, moss.path),
        (true, home.join("bin/convert-wm"))
    );
    let moss = parse("[extensions]\nconvert-with-moss.path = \"\"")
        .unwrap()
        .convert_with_moss;
    assert_eq!((moss.enable, moss.path), (false, installed));

    for (text, error) in [
        // A bare name would be looked up on PATH, which playr does not do.
        (
            "[extensions]\nconvert-with-moss.path = \"convert-wm\"",
            "line 2: convert-with-moss.path must be an absolute path or start with ~/",
        ),
        (
            "[extensions]\nconvert-with-moss.enable = \"yes\"",
            "line 2: convert-with-moss.enable is true or false, not a string",
        ),
        (
            "[extensions]\nconvert-with-moss.path = 3",
            "line 2: convert-with-moss.path cannot be an integer",
        ),
        (
            "[extensions]\nconvert-with-moss.enabled = true",
            "line 2: unknown convert-with-moss setting: enabled",
        ),
        (
            "[extensions]\nconvert-with-moss = true",
            "line 2: convert-with-moss is a table, not a boolean",
        ),
        (
            "[extensions]\nffmpeg.enable = true",
            "line 2: unknown extension: ffmpeg",
        ),
        (
            "extensions = true",
            "line 1: extensions cannot be a boolean",
        ),
    ] {
        assert_eq!(parse(text).unwrap_err(), [error], "{text}");
    }
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
fn a_windows_path_in_double_quotes_says_to_use_single_quotes() {
    let hint = "a backslash in \"...\" starts an escape, so write a Windows path as 'C:\\Music' or \"C:/Music\"";
    // `\U` and `\m` are not escapes TOML knows, so the file does not parse.
    for path in [r"C:\Users\me", r"D:\music"] {
        let errors = parse(&format!("volume = 60\nsamples = \"{path}\"")).unwrap_err();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].starts_with("line 2: "), "{errors:?}");
        assert!(errors[0].ends_with(hint), "{errors:?}");
    }
    // Every backslash an escape, so it parses, to control characters. The
    // second is absolute on Windows, with a tab in it.
    for path in [r"C:\new", r"C:\\Music\temp"] {
        assert_eq!(
            parse(&format!("samples = \"{path}\"")).unwrap_err(),
            [format!("line 1: samples holds a control character; {hint}")]
        );
    }
    // A syntax error with no backslash on its line gets no hint.
    let errors = parse("samples = 'C:\\Music'\nmode = shuffle").unwrap_err();
    assert!(errors[0].starts_with("line 2: "), "{errors:?}");
    assert!(!errors[0].contains("backslash"), "{errors:?}");
}

#[test]
fn the_knees_are_in_dbfs_and_stay_in_range() {
    let s = Settings::default();
    assert_eq!((s.dj_knee, s.tape_knee), (-0.9, -6.0));
    let s = parse("dj_knee = -3\ntape_knee = -0.5").unwrap();
    assert_eq!((s.dj_knee, s.tape_knee), (-3.0, -0.5));
    for (bad, name) in [
        ("dj_knee = 0", "dj_knee"),
        ("tape_knee = -30", "tape_knee"),
        ("dj_knee = \"soft\"", "dj_knee"),
    ] {
        assert_eq!(
            parse(bad).unwrap_err(),
            [format!("line 1: {name} is a number from -24 to -0.1 dBFS")],
            "{bad}"
        );
    }
}

#[test]
fn fade_defaults_to_none_and_stays_in_range() {
    use std::time::Duration;
    assert_eq!(Settings::default().fade, Duration::ZERO);
    assert_eq!(parse("fade = 2").unwrap().fade, Duration::from_secs(2));
    assert_eq!(
        parse("fade = 0.5").unwrap().fade,
        Duration::from_millis(500)
    );
    for bad in ["fade = 11", "fade = -1", "fade = \"long\""] {
        assert_eq!(
            parse(bad).unwrap_err(),
            ["line 1: fade is a number of seconds from 0 to 10"],
            "{bad}"
        );
    }
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
fn the_queue_is_kept_and_the_draft_asked_about_unless_set_otherwise() {
    use playr_core::settings::Draft;
    let defaults = Settings::default();
    assert!(defaults.keep_queue);
    assert_eq!(defaults.draft, Draft::Ask);
    let set = parse("keep_queue = false\ndraft = 'Append'").unwrap();
    assert!(!set.keep_queue);
    assert_eq!(set.draft, Draft::Append);
    // Naming values to remember leaves both alone.
    assert!(parse("persist = ['eq']").unwrap().keep_queue);
    assert_eq!(
        parse("keep_queue = 'no'\ndraft = 'later'").unwrap_err(),
        [
            "line 1: keep_queue cannot be a string",
            "line 2: unknown draft later; choices: ask, overwrite, append, off",
        ]
    );
    assert_eq!(
        parse("draft = true").unwrap_err(),
        ["line 1: draft cannot be a boolean"]
    );
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
        "line 1: unknown column loudest; choices: title, artist, album_artist, album, genre, disc, track, year, time, tempo, loudness, peak, path"
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

#[test]
fn slice_edges_default_exact_with_short_fades_and_take_a_name_and_milliseconds() {
    use playr_core::samples::{Edges, Fades};
    use std::time::Duration;
    let defaults = Settings::default();
    assert_eq!(defaults.slice_edges, Edges::Exact);
    assert_eq!(defaults.slice_fades, Fades::default());
    assert_eq!(
        defaults.slice_fades.fade_in,
        Duration::from_millis(1),
        "the defaults file and Fades::default agree"
    );
    assert_eq!(
        parse("slice_edges = \"Zero\"").unwrap().slice_edges,
        Edges::Zero
    );
    let fades = parse("slice_fade_in = 0.5\nslice_fade_out = 20")
        .unwrap()
        .slice_fades;
    assert_eq!(
        (fades.fade_in, fades.fade_out),
        (Duration::from_micros(500), Duration::from_millis(20))
    );
    assert_eq!(
        parse("slice_edges = \"soft\"").unwrap_err(),
        ["line 1: unknown slice_edges soft; choices: exact, zero, fade"]
    );
    assert_eq!(
        parse("slice_edges = 1").unwrap_err(),
        ["line 1: slice_edges cannot be an integer"]
    );
    for bad in [
        "slice_fade_out = 101",
        "slice_fade_out = -1",
        "slice_fade_out = \"5\"",
    ] {
        assert_eq!(
            parse(bad).unwrap_err(),
            ["line 1: slice_fade_out is milliseconds, from 0 to 100"],
            "{bad}"
        );
    }
}

#[test]
fn persist_names_the_values_to_remember() {
    use playr_core::settings::Persist;
    assert_eq!(
        parse("").unwrap().persist,
        [],
        "nothing is remembered by default"
    );
    assert_eq!(
        parse("persist = ['eq', 'Volume', 'sort']").unwrap().persist,
        [Persist::Eq, Persist::Volume, Persist::Sort]
    );
    assert_eq!(
        parse("persist = ['eq', 'speed', 3]").unwrap_err(),
        [
            "line 1: unknown persist speed; choices: eq, volume, mode, replaygain, theme, columns, sort, history, mix",
            "line 1: a name, not an integer",
        ]
    );
    assert_eq!(
        parse("persist = true").unwrap_err(),
        ["line 1: persist cannot be a boolean"]
    );
}

#[test]
fn state_is_remembered_by_key_and_replaced() {
    use playr_core::db;
    let conn = db::open_memory().unwrap();
    assert_eq!(db::state(&conn, "eq").unwrap(), None);
    db::set_state(&conn, "eq", "3 0 -2").unwrap();
    db::set_state(&conn, "eq", "1 0 0").unwrap();
    assert_eq!(db::state(&conn, "eq").unwrap().as_deref(), Some("1 0 0"));
}
