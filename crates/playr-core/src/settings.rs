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
use crate::columns::{Column, SortKey};
use crate::gain::ReplayGain;

/// The core's default settings, as shipped.
pub const DEFAULT_SETTINGS: &str = include_str!("settings.toml");

/// Entries a frontend may own, in the order they are documented.
///
/// A frontend asks [`Settings::apply`] for the ones it reads; the rest are
/// passed over rather than reported, so one settings file serves all three
/// programs. Without this a `[gui]` table stopped the terminal starting.
///
/// Nothing here is checked by the core: a frontend validates its own table
/// when it runs, so a mistake inside `[gui]` is reported by the window and
/// not by the terminal.
pub const FRONTEND_TABLES: &[&str] = &["keys", "theme", "terminal", "gui", "server"];

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
    /// After a scan, analyse the tracks it added or found changed.
    pub analyze_on_scan: bool,
    /// The output device's ID, or `None` for the default.
    pub device: Option<String>,
    pub replaygain: ReplayGain,
    /// The columns a track list shows, in order.
    pub columns: Vec<Column>,
    /// The keys the library is sorted by, most important first.
    pub sort: Vec<SortKey>,
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
            analyze_on_scan: false,
            device: None,
            replaygain: ReplayGain::Off,
            columns: Vec::new(),
            sort: Vec::new(),
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
    ///
    /// An entry in [`FRONTEND_TABLES`] that `tables` does not name belongs to
    /// another frontend and is passed over. Every other unknown name is an
    /// error, so a typo is still caught.
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
                // Another frontend's. It must still be a table, so a stray
                // `gui = 3` is reported rather than passed over.
                (other, DeValue::Table(_)) if FRONTEND_TABLES.contains(&other) => {}
                (other, v) if FRONTEND_TABLES.contains(&other) => errors.add(
                    at,
                    format!(
                        "{other} is another program's settings, and must be a table, not {}",
                        kind(v)
                    ),
                ),
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
                ("analyze_on_scan", DeValue::Boolean(b)) => self.analyze_on_scan = *b,
                ("samples", DeValue::String(s)) => match expand_home(s) {
                    Some(path) => self.samples = path,
                    None => errors.add(at, "samples must be an absolute path or start with ~/"),
                },
                // Checked when the device opens, since a settings file may be
                // read on a machine without that device.
                ("device", DeValue::String(s)) => {
                    self.device = (!s.is_empty()).then(|| s.to_string())
                }
                ("columns", v) => match columns_value(v) {
                    Ok(columns) => self.columns = columns,
                    Err(e) => errors.add(at, e),
                },
                ("sort", v) => match sort_value(v) {
                    Ok(sort) => self.sort = sort,
                    Err(e) => errors.add(at, e),
                },
                ("replaygain", DeValue::String(s)) => {
                    match ReplayGain::NAMES
                        .iter()
                        .find(|r| r.0.eq_ignore_ascii_case(s))
                    {
                        Some(&(_, replaygain)) => self.replaygain = replaygain,
                        None => {
                            let names: Vec<&str> = ReplayGain::NAMES.iter().map(|r| r.0).collect();
                            errors.add(
                                at,
                                format!("unknown replaygain {s}; choices: {}", names.join(", ")),
                            )
                        }
                    }
                }
                (
                    "mode" | "samples" | "auto_prune" | "analyze_on_scan" | "device" | "replaygain",
                    v,
                ) => errors.add(at, format!("{} cannot be {}", name.get_ref(), kind(v))),
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

/// The columns a `columns = [...]` value names, or why it is not one.
///
/// A frontend reads the same key from its own table, so the wording of a
/// mistake is the same wherever it is made.
pub fn columns_value(value: &DeValue) -> Result<Vec<Column>, String> {
    let DeValue::Array(items) = value else {
        return Err(format!("columns is a list of names, not {}", kind(value)));
    };
    match named_list(items, Column::named)? {
        columns if columns.is_empty() => Err("columns needs at least one column".into()),
        columns => Ok(columns),
    }
}

/// The keys a `sort = [...]` value names, or why it is not one. An empty
/// list leaves the library in the order it was read.
pub fn sort_value(value: &DeValue) -> Result<Vec<SortKey>, String> {
    let DeValue::Array(items) = value else {
        return Err(format!("sort is a list of names, not {}", kind(value)));
    };
    named_list(items, SortKey::named)
}

/// Every column name, for an error that lists the choices.
fn column_names() -> String {
    Column::NAMES
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

fn unknown(what: &str, bad: &str, choices: String) -> String {
    format!("unknown {what} {bad}; choices: {choices}")
}

/// Reads an array of names with `parse`, reporting the first it does not know.
fn named_list<T>(
    items: &[Spanned<DeValue>],
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Vec<T>, String> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let DeValue::String(text) = item.get_ref() else {
            return Err(format!("a name, not {}", kind(item.get_ref())));
        };
        match parse(text) {
            Some(value) => out.push(value),
            None => return Err(unknown("column", text, column_names())),
        }
    }
    Ok(out)
}
