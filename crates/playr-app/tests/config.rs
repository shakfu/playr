//! Key names, the key map, and the settings file.

use playr_app::action::{Action, Key, Keymap};
use playr_app::config::{Config, DEFAULT_KEYS};
use playr_app::View::{Library, Playlists, Selection};
use playr_core::audio::Mode;
use playr_core::settings::{Settings, DEFAULT_SETTINGS};

fn key(text: &str) -> Key {
    Key::parse(text).unwrap()
}

#[test]
fn key_names_parse_and_print_the_same() {
    for name in [
        "j",
        "J",
        "?",
        "-",
        ":",
        "space",
        "enter",
        "esc",
        "tab",
        "backtab",
        "pagedown",
        "f1",
        "f12",
        "ctrl-s",
        "alt-x",
        "shift-right",
        "ctrl-alt-delete",
        "ctrl--",
    ] {
        assert_eq!(key(name).to_string(), name);
    }
    assert_eq!(key("PageDown"), key("pagedown"));
    for bad in ["", "jj", "f13", "ctrl-", "hyper-x", "shift-"] {
        assert!(Key::parse(bad).is_err(), "{bad:?} parsed");
    }
}

#[test]
fn a_view_binding_wins_and_nothing_stops_the_fallback() {
    let mut keys = Keymap::empty();
    keys.bind(None, key("d"), Some(Action::Stop));
    keys.bind(Some(Selection), key("d"), Some(Action::Remove));
    keys.bind(Some(Playlists), key("d"), None);
    assert_eq!(keys.lookup(key("d"), Library), Some(&Action::Stop));
    assert_eq!(keys.lookup(key("d"), Selection), Some(&Action::Remove));
    assert_eq!(keys.lookup(key("d"), Playlists), None);

    // Binding again replaces in place; unbinding a view falls back.
    keys.bind(None, key("d"), Some(Action::Next));
    assert_eq!(keys.bindings().len(), 3);
    assert_eq!(keys.lookup(key("d"), Library), Some(&Action::Next));
    assert!(keys.unbind(Some(Selection), key("d")));
    assert!(!keys.unbind(Some(Selection), key("d")));
    assert_eq!(keys.lookup(key("d"), Selection), Some(&Action::Next));
}

#[test]
fn an_empty_file_gives_the_defaults() {
    let config = Config::parse("").unwrap();
    assert_eq!(config, Config::default());
    assert_eq!(config.keys, Keymap::default());
    assert_eq!(config.settings, Settings::default());
    assert_eq!(Keymap::default().bindings().len(), 62);
}

#[test]
fn the_default_files_are_valid_user_files() {
    // Copied whole to ~/.config/playr/settings.toml, each changes nothing.
    assert_eq!(Config::parse(DEFAULT_SETTINGS), Ok(Config::default()));
    assert_eq!(Config::parse(DEFAULT_KEYS), Ok(Config::default()));
    let both = format!("{DEFAULT_SETTINGS}\n{DEFAULT_KEYS}");
    assert_eq!(Config::parse(&both), Ok(Config::default()));
}

#[test]
fn the_file_changes_keys_and_sets_how_playback_starts() {
    let text = r#"
        # comments and blank lines are skipped

        volume = 40
        mode = "shuffle"
        speed = -3

        [keys]
        right = "seek +10"
        ctrl-s = "save"
        q = "nop"
        "?" = "help"

        [keys.selection]
        ctrl-x = "remove"

        [keys.playlists]
        d = "nop"
    "#;
    let config = Config::parse(text).unwrap();
    assert_eq!(
        (
            config.settings.volume,
            config.settings.mode,
            config.settings.speed
        ),
        (0.4, Mode::Shuffle, -3)
    );
    let keys = &config.keys;
    assert_eq!(
        keys.lookup(key("right"), Library),
        Some(&Action::SeekBy(10))
    );
    assert_eq!(
        keys.lookup(key("ctrl-s"), Library),
        Some(&Action::StartSave)
    );
    assert_eq!(keys.lookup(key("?"), Library), Some(&Action::CommandHelp));
    assert_eq!(keys.lookup(key("ctrl-x"), Selection), Some(&Action::Remove));
    assert_eq!(keys.lookup(key("ctrl-x"), Library), None);
    assert_eq!(keys.lookup(key("d"), Playlists), None);
    assert_eq!(keys.lookup(key("d"), Selection), Some(&Action::Remove));
    assert_eq!(keys.lookup(key("q"), Library), None);
    // A default rebound keeps its place; a new key is added after the defaults.
    let at = |k: &str| {
        keys.bindings()
            .iter()
            .position(|b| b.view.is_none() && b.key == key(k))
    };
    assert_eq!(at("q"), Some(0));
    assert!(at("ctrl-s") > at(","));
}

#[test]
fn every_bad_setting_is_reported_with_its_line() {
    let text = r#"volume = 140
mode = "shufle"
speed = 1.5
colour = "red"

[keys]
d = "remove"
zz = "quit"
x = 3
y = "map q quit"

[keys.queue]
a = "add"

[keys.selection]
x = "delete"
"#;
    let errors = Config::parse(text).unwrap_err();
    assert_eq!(
        errors,
        [
            "line 1: volume is a number from 0 to 100",
            "line 2: unknown mode shufle; modes: normal, shuffle, repeat, repeat-one",
            "line 3: speed is a whole number from -12 to 12",
            "line 4: unknown setting: colour",
            "line 7: :remove works in the selection view; put d under [keys.selection]",
            "line 8: not a key: zz",
            "line 9: x must be a command string, not an integer",
            "line 10: a key cannot run :map or :unmap",
            "line 12: [keys.queue] is not a view; views: library, selection, playlists, sampler",
            "line 16: :delete works in the playlists view; put x under [keys.playlists]",
        ]
    );
    let syntax = Config::parse("volume = 60\nmode = shuffle\n").unwrap_err();
    assert_eq!(syntax.len(), 1);
    assert!(syntax[0].starts_with("line 2: "), "{syntax:?}");
    assert!(Config::parse("keys = 1").unwrap_err()[0].contains("keys cannot be an integer"));
}

#[test]
fn a_missing_file_is_an_error_only_when_named() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("settings.toml");
    assert_eq!(Config::load(&missing, false), Ok(Config::default()));
    let errors = Config::load(&missing, true).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].starts_with(&missing.display().to_string()),
        "{errors:?}"
    );

    std::fs::write(&missing, "volume = 50\nfrob = 1\n").unwrap();
    let errors = Config::load(&missing, false).unwrap_err();
    assert_eq!(
        errors,
        [format!(
            "{}: line 2: unknown setting: frob",
            missing.display()
        )]
    );
}
