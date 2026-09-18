//! Settings: `settings.toml`, read at startup on top of the defaults.
//!
//! The core owns the file's top-level keys. A frontend owns the tables it
//! names, such as `[keys]`: [`Settings::apply`] hands those back unread, and
//! reports every other name it does not know. The defaults are a settings file
//! too, `settings.toml` beside this module, read by the same code.

use std::borrow::Cow;
use std::path::PathBuf;

pub use toml;
use toml::de::{DeTable, DeValue};
use toml::Spanned;

use crate::audio::Mode;

/// The core's default settings, as shipped.
pub const DEFAULT_SETTINGS: &str = include_str!("settings.toml");

/// What the settings file sets for the core.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// From 0 to 1.
    pub volume: f32,
    pub mode: Mode,
    /// Semitones.
    pub speed: i32,
    /// Where exported slices are written.
    pub samples: PathBuf,
    /// The onset sensitivity `:slice onsets` uses when given none, 0 to 1.
    pub onset_sensitivity: f32,
    /// After a scan, remove tracks whose files are gone, without asking.
    pub auto_prune: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let mut settings = Settings {
            volume: 1.0,
            mode: Mode::Normal,
            speed: 0,
            samples: PathBuf::new(),
            onset_sensitivity: 0.5,
            auto_prune: false,
        };
        let (_, errors) = settings.apply(DEFAULT_SETTINGS, &[]);
        if let Err(errors) = errors.finish() {
            panic!("bad default settings: {errors:?}");
        }
        settings
    }
}

/// A top-level entry of the file that a frontend named: its name and value.
pub type Table<'a> = (Spanned<Cow<'a, str>>, Spanned<DeValue<'a>>);

/// Problems found in one settings file, each at a byte offset into it.
#[derive(Debug)]
pub struct Errors<'a> {
    text: &'a str,
    found: Vec<(usize, String)>,
}

impl Errors<'_> {
    /// Records `message` about the setting at byte `offset`.
    pub fn add(&mut self, offset: usize, message: impl Into<String>) {
        self.found.push((offset, message.into()));
    }

    /// `Ok` if nothing was found; otherwise each problem as `line N: message`,
    /// in file order.
    pub fn finish(mut self) -> Result<(), Vec<String>> {
        if self.found.is_empty() {
            return Ok(());
        }
        self.found.sort_by_key(|(offset, _)| *offset);
        let text = self.text;
        let line = |offset: usize| text[..offset.min(text.len())].matches('\n').count() + 1;
        Err(self
            .found
            .into_iter()
            .map(|(offset, message)| format!("line {}: {message}", line(offset)))
            .collect())
    }
}

/// A TOML value's kind, for messages.
pub fn kind(value: &DeValue) -> &'static str {
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

impl Settings {
    /// Applies the top-level keys of `text` on top of `self`. Returns the
    /// entries named in `tables`, in file order, for the frontend to read,
    /// and the problems found so far, to which it adds its own.
    pub fn apply<'a>(&mut self, text: &'a str, tables: &[&str]) -> (Vec<Table<'a>>, Errors<'a>) {
        let mut errors = Errors {
            text,
            found: Vec::new(),
        };
        let table = match DeTable::parse(text) {
            Ok(table) => table.into_inner(),
            Err(e) => {
                let at = e.span().map_or(0, |s| s.start);
                errors.add(at, e.message().trim());
                return (Vec::new(), errors);
            }
        };
        let mut named = Vec::new();
        for (name, value) in table {
            let at = value.span().start;
            match (name.get_ref().as_ref(), value.get_ref()) {
                (other, _) if tables.contains(&other) => named.push((name, value)),
                ("volume", v) => match number(v) {
                    Some(n) if (0.0..=100.0).contains(&n) => self.volume = (n / 100.0) as f32,
                    _ => errors.add(at, "volume is a number from 0 to 100"),
                },
                ("mode", DeValue::String(s)) => {
                    match Mode::NAMES.iter().find(|m| m.0.eq_ignore_ascii_case(s)) {
                        Some(&(_, mode)) => self.mode = mode,
                        None => {
                            let names: Vec<&str> = Mode::NAMES.iter().map(|m| m.0).collect();
                            errors.add(at, format!("unknown mode {s}; modes: {}", names.join(", ")))
                        }
                    }
                }
                ("speed", v) => match number(v) {
                    Some(n) if n.fract() == 0.0 && (-12.0..=12.0).contains(&n) => {
                        self.speed = n as i32
                    }
                    _ => errors.add(at, "speed is a whole number from -12 to 12"),
                },
                ("onset_sensitivity", v) => match number(v) {
                    Some(n) if (0.0..=1.0).contains(&n) => self.onset_sensitivity = n as f32,
                    _ => errors.add(at, "onset_sensitivity is a number from 0 to 1"),
                },
                ("auto_prune", DeValue::Boolean(b)) => self.auto_prune = *b,
                ("samples", DeValue::String(s)) => match expand_home(s) {
                    Some(path) => self.samples = path,
                    None => errors.add(at, "samples must be an absolute path or start with ~/"),
                },
                ("mode" | "samples" | "auto_prune", v) => {
                    errors.add(at, format!("{} cannot be {}", name.get_ref(), kind(v)))
                }
                (other, _) => errors.add(name.span().start, format!("unknown setting: {other}")),
            }
        }
        (named, errors)
    }
}

/// `path` with a leading `~/` replaced by the home directory, if it is then absolute.
fn expand_home(path: &str) -> Option<PathBuf> {
    let path = match path.strip_prefix("~/") {
        Some(rest) => std::env::home_dir()?.join(rest),
        None => PathBuf::from(path),
    };
    path.is_absolute().then_some(path)
}

/// `$XDG_CONFIG_HOME/playr/settings.toml`, else `~/.config/playr/settings.toml`.
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::home_dir().map(|h| h.join(".config")))?;
    Some(base.join("playr/settings.toml"))
}
