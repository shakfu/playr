//! Settings: `settings.toml`, read at startup on top of the defaults.
//!
//! The defaults are a settings file too, `src/ui/settings.toml`, read by the
//! same code. A user file then only needs the lines it changes. Key bindings
//! are `:` command strings, checked by the command parser, so a key and its
//! command cannot mean different things. Any error stops playr from starting,
//! so a mistyped binding cannot go unnoticed.

use std::path::{Path, PathBuf};

use toml::de::{DeTable, DeValue};
use toml::Spanned;

use super::action::{Key, Keymap};
use super::command::{self, view_name};
use super::View;
use crate::audio::Mode;

/// The default settings, as shipped.
pub const DEFAULT_SETTINGS: &str = include_str!("settings.toml");

/// What the settings file sets.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub keys: Keymap,
    /// From 0 to 1.
    pub volume: f32,
    pub mode: Mode,
    /// Semitones.
    pub speed: i32,
}

impl Default for Config {
    fn default() -> Self {
        let mut config = Config {
            keys: Keymap::empty(),
            volume: 1.0,
            mode: Mode::Normal,
            speed: 0,
        };
        if let Err(errors) = config.apply(DEFAULT_SETTINGS) {
            panic!("bad default settings: {errors:?}");
        }
        config
    }
}

/// A TOML value's kind, for messages.
fn kind(value: &DeValue) -> &'static str {
    match value {
        DeValue::String(_) => "a string",
        DeValue::Integer(_) => "an integer",
        DeValue::Float(_) => "a number",
        DeValue::Boolean(_) => "a boolean",
        DeValue::Datetime(_) => "a date",
        DeValue::Array(_) => "an array",
        DeValue::Table(_) => "a table",
    }
}

fn number(value: &DeValue) -> Option<f64> {
    match value {
        DeValue::Integer(n) => i64::from_str_radix(&n.as_str().replace('_', ""), n.radix())
            .ok()
            .map(|n| n as f64),
        DeValue::Float(n) => n.as_str().replace('_', "").parse().ok(),
        _ => None,
    }
}

impl Config {
    /// The defaults with `text` applied on top. Errors are `line N: message`,
    /// one for every bad setting.
    pub fn parse(text: &str) -> Result<Config, Vec<String>> {
        let mut config = Config::default();
        config.apply(text).map(|()| config)
    }

    /// Reads the file at `path`. A missing file gives the defaults unless
    /// `required`, as for a path given with `--settings`. Errors name the file.
    pub fn load(path: &Path, required: bool) -> Result<Config, Vec<String>> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if !required && e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Config::default())
            }
            Err(e) => return Err(vec![format!("{}: {e}", path.display())]),
        };
        Config::parse(&text).map_err(|errors| {
            errors
                .into_iter()
                .map(|e| format!("{}: {e}", path.display()))
                .collect()
        })
    }

    fn apply(&mut self, text: &str) -> Result<(), Vec<String>> {
        let line = |offset: usize| text[..offset.min(text.len())].matches('\n').count() + 1;
        let table = match DeTable::parse(text) {
            Ok(table) => table.into_inner(),
            Err(e) => {
                let at = e.span().map_or(1, |s| line(s.start));
                return Err(vec![format!("line {at}: {}", e.message().trim())]);
            }
        };
        let mut errors = Vec::new();
        let mut fail = |span: std::ops::Range<usize>, message: String| {
            errors.push(format!("line {}: {message}", line(span.start)));
        };
        for (name, value) in &table {
            let span = value.span();
            match (name.get_ref().as_ref(), value.get_ref()) {
                ("volume", v) => match number(v) {
                    Some(n) if (0.0..=100.0).contains(&n) => self.volume = (n / 100.0) as f32,
                    _ => fail(span, "volume is a number from 0 to 100".into()),
                },
                ("mode", DeValue::String(s)) => match command::mode_named(s) {
                    Ok(mode) => self.mode = mode,
                    Err(e) => fail(span, format!("unknown mode {s}; {e}")),
                },
                ("speed", v) => match number(v) {
                    Some(n) if n.fract() == 0.0 && (-12.0..=12.0).contains(&n) => {
                        self.speed = n as i32
                    }
                    _ => fail(span, "speed is a whole number from -12 to 12".into()),
                },
                ("keys", DeValue::Table(keys)) => {
                    for (key, target) in keys {
                        match target.get_ref() {
                            DeValue::Table(bindings) => match command::view_named(key.get_ref()) {
                                Some(view) => {
                                    for (key, target) in bindings {
                                        if let Err(e) = self.bind(Some(view), key, target) {
                                            fail(target.span(), e);
                                        }
                                    }
                                }
                                None => fail(
                                    key.span(),
                                    format!(
                                        "[keys.{}] is not a view; views: library, selection, playlists",
                                        key.get_ref()
                                    ),
                                ),
                            },
                            _ => {
                                if let Err(e) = self.bind(None, key, target) {
                                    fail(target.span(), e);
                                }
                            }
                        }
                    }
                }
                ("mode" | "keys", v) => {
                    fail(span, format!("{} cannot be {}", name.get_ref(), kind(v)))
                }
                (other, _) => fail(name.span(), format!("unknown setting: {other}")),
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Binds the key named `key` in `view` to the command in `target`.
    fn bind(
        &mut self,
        view: Option<View>,
        key: &Spanned<std::borrow::Cow<str>>,
        target: &Spanned<DeValue>,
    ) -> Result<(), String> {
        let name = key.get_ref();
        let key = Key::parse(name)?;
        let DeValue::String(target) = target.get_ref() else {
            return Err(format!(
                "{name} must be a command string, not {}",
                kind(target.get_ref())
            ));
        };
        let action = command::key_target(target, view).map_err(|e| {
            match command::only_view(target).filter(|v| view != Some(*v)) {
                Some(v) => format!("{e}; put {name} under [keys.{}]", view_name(v)),
                None => e,
            }
        })?;
        self.keys.bind(view, key, action);
        Ok(())
    }
}

/// `$XDG_CONFIG_HOME/playr/settings.toml`, else `~/.config/playr/settings.toml`.
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("playr/settings.toml"))
}
