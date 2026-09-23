//! Settings: `settings.toml`, read at startup on top of the defaults.
//!
//! `playr_core::settings` reads the file and applies the core's keys; this
//! module reads the `[keys]` tables and the `theme` key it hands back. The default bindings are a
//! settings file too, `keys.toml` beside this module, read by the same code.
//! Key bindings are `:` command strings, checked by the command parser, so a
//! key and its command cannot mean different things. Any error stops playr
//! from starting, so a mistyped binding cannot go unnoticed.

use std::path::Path;

use playr_core::settings::toml::de::DeValue;
use playr_core::settings::toml::Spanned;
use playr_core::settings::{self, kind, Errors, Settings};

use crate::action::{Key, Keymap};
use crate::command::{self, view_name};
use crate::{Theme, View};

pub use playr_core::settings::default_path;

/// The default key bindings, as shipped.
pub const DEFAULT_KEYS: &str = include_str!("keys.toml");

/// What the settings file sets.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub settings: Settings,
    pub keys: Keymap,
    pub theme: Theme,
}

impl Default for Config {
    fn default() -> Self {
        let mut config = Config {
            settings: Settings::default(),
            keys: Keymap::empty(),
            theme: Theme::Dark,
        };
        if let Err(errors) = config.apply(DEFAULT_KEYS) {
            panic!("bad default key bindings: {errors:?}");
        }
        config
    }
}

/// Which program is reading the settings, for the table it owns.
///
/// One `settings.toml` serves all three, so `[gui]` sets the window's columns
/// without the terminal reading them. A program's table wins over the
/// top-level key it repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Program {
    Terminal,
    Gui,
    Server,
}

impl Program {
    pub fn table(self) -> &'static str {
        match self {
            Program::Terminal => "terminal",
            Program::Gui => "gui",
            Program::Server => "server",
        }
    }
}

impl Config {
    /// The defaults with `text` applied on top. Errors are `line N: message`,
    /// one for every bad setting.
    pub fn parse(text: &str) -> Result<Config, Vec<String>> {
        Config::parse_for(Program::Terminal, text)
    }

    /// The same, reading `program`'s own table as well.
    pub fn parse_for(program: Program, text: &str) -> Result<Config, Vec<String>> {
        let mut config = Config::for_program(program);
        config.apply_for(Some(program), text).map(|()| config)
    }

    /// The defaults as `program` reads them: the shipped file is applied
    /// again with its table, which `Settings::default` cannot do, since it
    /// does not know which program is asking.
    pub fn for_program(program: Program) -> Config {
        let mut config = Config::default();
        if let Err(errors) = config.apply_for(Some(program), settings::DEFAULT_SETTINGS) {
            panic!("bad default settings: {errors:?}");
        }
        config
    }

    /// Reads the file at `path`. A missing file gives the defaults unless
    /// `required`, as for a path given with `--settings`. Errors name the file.
    pub fn load(path: &Path, required: bool) -> Result<Config, Vec<String>> {
        Config::load_for(Program::Terminal, path, required)
    }

    /// The same, reading `program`'s own table as well.
    pub fn load_for(program: Program, path: &Path, required: bool) -> Result<Config, Vec<String>> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if !required && e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Config::for_program(program))
            }
            Err(e) => return Err(vec![format!("{}: {e}", path.display())]),
        };
        Config::parse_for(program, &text).map_err(|errors| {
            errors
                .into_iter()
                .map(|e| format!("{}: {e}", path.display()))
                .collect()
        })
    }

    fn apply(&mut self, text: &str) -> Result<(), Vec<String>> {
        self.apply_for(None, text)
    }

    fn apply_for(&mut self, program: Option<Program>, text: &str) -> Result<(), Vec<String>> {
        let mine = program.map(Program::table).unwrap_or("keys");
        let (tables, mut errors) = self.settings.apply(text, &["keys", "theme", mine]);
        for (name, value) in tables {
            if program.is_some_and(|p| name.get_ref() == p.table()) {
                self.apply_program(value.get_ref(), value.span().start, &mut errors);
                continue;
            }
            if name.get_ref() == "theme" {
                if let Err(e) = self.set_theme(value.get_ref()) {
                    errors.add(value.span().start, e);
                }
                continue;
            }
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

    /// Reads a program's own table: the settings it may set for itself.
    fn apply_program(&mut self, value: &DeValue, at: usize, errors: &mut Errors<'_>) {
        let DeValue::Table(entries) = value else {
            errors.add(
                at,
                format!("a program's settings are a table, not {}", kind(value)),
            );
            return;
        };
        for (key, value) in entries {
            let at = value.span().start;
            match key.get_ref().as_ref() {
                "columns" => match settings::columns_value(value.get_ref()) {
                    Ok(columns) => self.settings.columns = columns,
                    Err(e) => errors.add(at, e),
                },
                "sort" => match settings::sort_value(value.get_ref()) {
                    Ok(sort) => self.settings.sort = sort,
                    Err(e) => errors.add(at, e),
                },
                // The server's own flags are not settings yet; see TODO.md.
                other => errors.add(key.span().start, format!("unknown setting: {other}")),
            }
        }
    }

    fn set_theme(&mut self, value: &DeValue) -> Result<(), String> {
        let names = || Theme::NAMES.map(|t| t.0).join(", ");
        let DeValue::String(s) = value else {
            return Err(format!("theme cannot be {}", kind(value)));
        };
        let theme = Theme::NAMES.iter().find(|t| t.0.eq_ignore_ascii_case(s));
        self.theme = theme
            .ok_or_else(|| format!("unknown theme {s}; themes: {}", names()))?
            .1;
        Ok(())
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
