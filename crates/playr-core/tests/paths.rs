use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use playr_core::paths::{config_dir, data_dir, Env};

/// `s`, absolute on this host: Windows wants a drive.
fn root(s: &str) -> String {
    match cfg!(windows) && s.starts_with('/') {
        true => format!("C:{s}"),
        false => s.to_string(),
    }
}

/// `os` with `vars` set, a home at /h, and `files` existing.
fn dirs(os: &str, vars: &[(&str, &str)], files: &[&str]) -> (Option<PathBuf>, Option<PathBuf>) {
    let vars: Vec<(String, String)> = vars.iter().map(|(k, v)| (k.to_string(), root(v))).collect();
    let files: HashSet<PathBuf> = files.iter().map(|f| PathBuf::from(root(f))).collect();
    let var = move |name: &str| {
        vars.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| OsString::from(v))
    };
    let exists = move |p: &Path| files.contains(p);
    let env = Env {
        os,
        var: &var,
        home: Some(PathBuf::from(root("/h"))),
        exists: &exists,
    };
    (data_dir(&env), config_dir(&env))
}

fn p(s: &str) -> Option<PathBuf> {
    Some(PathBuf::from(root(s)))
}

#[test]
fn each_platform_has_its_own_directories() {
    assert_eq!(
        dirs("linux", &[], &[]),
        (p("/h/.local/share/playr"), p("/h/.config/playr"))
    );
    let mac = p("/h/Library/Application Support/playr");
    assert_eq!(dirs("macos", &[], &[]), (mac.clone(), mac));
    let windows = dirs("windows", &[("LOCALAPPDATA", "/l"), ("APPDATA", "/r")], &[]);
    assert_eq!(windows, (p("/l/playr"), p("/r/playr")));
}

#[test]
fn an_absolute_xdg_variable_wins_everywhere() {
    let vars = [("XDG_DATA_HOME", "/d"), ("XDG_CONFIG_HOME", "/c")];
    for os in ["linux", "macos", "windows"] {
        assert_eq!(dirs(os, &vars, &[]), (p("/d/playr"), p("/c/playr")), "{os}");
    }
    // A relative one is ignored, as the XDG specification says.
    let relative = [("XDG_DATA_HOME", "d")];
    assert_eq!(dirs("linux", &relative, &[]).0, p("/h/.local/share/playr"));
}

#[test]
fn files_in_the_old_directories_are_used_until_the_new_hold_some() {
    let old = [
        "/h/.local/share/playr/library.db",
        "/h/.config/playr/settings.toml",
    ];
    assert_eq!(
        dirs("macos", &[], &old),
        (p("/h/.local/share/playr"), p("/h/.config/playr"))
    );
    let both = [
        old[0],
        old[1],
        "/h/Library/Application Support/playr/library.db",
        "/h/Library/Application Support/playr/settings.toml",
    ];
    let mac = p("/h/Library/Application Support/playr");
    assert_eq!(dirs("macos", &[], &both), (mac.clone(), mac));
    // Each file on its own: an old library beside new settings.
    let windows = dirs(
        "windows",
        &[("LOCALAPPDATA", "/l"), ("APPDATA", "/r")],
        &old[..1],
    );
    assert_eq!(windows, (p("/h/.local/share/playr"), p("/r/playr")));
}
