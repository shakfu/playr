//! Settings: `settings.toml`, read at startup on top of the defaults.
//!
//! `playr_core::settings` reads the file and applies the core's keys; this
//! module reads the `[keys]` tables it hands back. The default bindings are a
//! settings file too, `keys.toml` beside this module, read by the same code.
//! Key bindings are `:` command strings, checked by the command parser, so a
//! key and its command cannot mean different things. Any error stops playr
//! from starting, so a mistyped binding cannot go unnoticed.

use std::path::Path;

use playr_core::settings::toml::de::DeValue;
use playr_core::settings::toml::Spanned;
use playr_core::settings::{kind, Settings};

use crate::action::{Key, Keymap};
use crate::command::{self, view_name};
use crate::View;

pub use playr_core::settings::default_path;

/// The default key bindings, as shipped.
pub const DEFAULT_KEYS: &str = include_str!("keys.toml");

/// What the settings file sets.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub settings: Settings,
    pub keys: Keymap,
}

impl Default for Config {
    fn default() -> Self {
        let mut config = Config {
            settings: Settings::default(),
            keys: Keymap::empty(),
        };
        if let Err(errors) = config.apply(DEFAULT_KEYS) {
            panic!("bad default key bindings: {errors:?}");
        }
        config
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
        let (tables, mut errors) = self.settings.apply(text, &["keys"]);
        for (name, value) in tables {
            let DeValue::Table(keys) = value.get_ref() else {
                let message = format!("{} cannot be {}", name.get_ref(), kind(value.get_ref()));
                errors.add(value.span().start, message);
                continue;
            };
            for (key, target) in keys {
                // A table under `[keys]` holds one view's bindings.
                let (view, bindings) = match target.get_ref() {
                    DeValue::Table(bindings) => match command::view_named(key.get_ref()) {
                        Some(view) => (Some(view), bindings.iter().collect()),
                        None => {
                            errors.add(key.span().start, not_a_view(key.get_ref()));
                            continue;
                        }
                    },
                    _ => (None, vec![(key, target)]),
                };
                for (key, target) in bindings {
                    if let Err(e) = self.bind(view, key, target) {
                        errors.add(target.span().start, e);
                    }
                }
            }
        }
        errors.finish()
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

/// The error for a table under `[keys]` that names no view.
fn not_a_view(name: &str) -> String {
    format!("[keys.{name}] is not a view; views: library, selection, playlists, sampler")
}
