//! Settings: `settings.toml`, read at startup on top of the defaults.
//!
//! The core owns the file's top-level keys and `[extensions]`. A frontend owns the tables it
//! names, such as `[keys]`: [`Settings::apply`] hands those back unread, and
//! reports every other name it does not know. The defaults are a settings file
//! too, `settings.toml` beside this module, read by the same code.

use std::borrow::Cow;
use std::path::PathBuf;

pub use toml;
use toml::de::{DeTable, DeValue};
use toml::Spanned;

use crate::audio::{AfterQueue, Mode};
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

/// `convert-with-moss` under `[extensions]`: `:convert`, which runs
/// ConvertWithMoss.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvertWithMoss {
    pub enable: bool,
    /// Its command line.
    pub path: PathBuf,
}

/// What the settings file sets for the core.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// From 0 to 1.
    pub volume: f32,
    pub mode: Mode,
    /// What plays once the queued tracks have.
    pub after_queue: AfterQueue,
    /// Semitones.
    pub speed: i32,
    /// Where exported slices are written.
    pub samples: PathBuf,
    pub convert_with_moss: ConvertWithMoss,
    /// The onset sensitivity `:slice onsets` uses when given none, 0 to 1.
    pub onset_sensitivity: f32,
    /// The levels, in dBFS, below which the DJ master's and the tape's
    /// write head's soft clips pass the signal unchanged.
    pub dj_knee: f32,
    pub tape_knee: f32,
    /// What an export does at slice edges, and how long a fade takes.
    pub slice_edges: crate::samples::Edges,
    pub slice_fades: crate::samples::Fades,
    /// Write an Octatrack `.ot` file with each export.
    pub slice_ot_file: bool,
    /// After a scan, remove tracks whose files are gone, without asking.
    pub auto_prune: bool,
    /// Store the queue, to offer with the track playing at the next start.
    pub keep_queue: bool,
    /// What a session does with the draft playlist an earlier one left.
    pub draft: Draft,
    /// After a scan, analyse the tracks it added or found changed.
    pub analyze_on_scan: bool,
    /// The output device's ID, or `None` for the default.
    pub device: Option<String>,
    pub replaygain: ReplayGain,
    /// The columns a track list shows, in order.
    pub columns: Vec<Column>,
    /// The keys the library is sorted by, most important first.
    pub sort: Vec<SortKey>,
    /// Session values to remember in the library between runs.
    pub persist: Vec<Persist>,
}

/// What a session does with a draft playlist an earlier session left, once
/// its own selection first changes. See `Session::set_draft`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Draft {
    /// Ask whether to overwrite it, append to it, or save it under a name.
    #[default]
    Ask,
    Overwrite,
    /// Put its tracks back in the selection, before the new ones.
    Append,
    /// Keep no draft: the selection is not saved.
    Off,
}

impl Draft {
    pub const NAMES: [(&'static str, Draft); 4] = [
        ("ask", Draft::Ask),
        ("overwrite", Draft::Overwrite),
        ("append", Draft::Append),
        ("off", Draft::Off),
    ];
}

/// A session value `persist` can name. While it is named, a remembered value
/// wins over the setting of the same name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persist {
    Eq,
    Volume,
    Mode,
    ReplayGain,
    Theme,
    /// Remembered for each program, as a program's table may set them.
    Columns,
    Sort,
    /// `:` command lines, shared by every program.
    History,
}

impl Persist {
    pub const NAMES: [(&'static str, Persist); 8] = [
        ("eq", Persist::Eq),
        ("volume", Persist::Volume),
        ("mode", Persist::Mode),
        ("replaygain", Persist::ReplayGain),
        ("theme", Persist::Theme),
        ("columns", Persist::Columns),
        ("sort", Persist::Sort),
        ("history", Persist::History),
    ];

    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(_, p)| *p == self)
            .expect("every value")
            .0
    }
}

impl Default for Settings {
    fn default() -> Self {
        let mut settings = Settings {
            volume: 1.0,
            mode: Mode::Normal,
            after_queue: AfterQueue::Resume,
            speed: 0,
            samples: PathBuf::new(),
            convert_with_moss: ConvertWithMoss {
                enable: false,
                path: PathBuf::new(),
            },
            onset_sensitivity: 0.5,
            dj_knee: 0.0,
            tape_knee: 0.0,
            slice_edges: crate::samples::Edges::Exact,
            slice_fades: crate::samples::Fades::default(),
            slice_ot_file: false,
            auto_prune: false,
            keep_queue: true,
            draft: Draft::Ask,
            analyze_on_scan: false,
            device: None,
            replaygain: ReplayGain::Off,
            columns: Vec::new(),
            sort: Vec::new(),
            persist: Vec::new(),
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

impl Settings {
    /// Applies the `[extensions]` table.
    fn extensions(&mut self, table: &DeTable, errors: &mut Errors) {
        for (name, value) in table {
            let at = value.span().start;
            match (name.get_ref().as_ref(), value.get_ref()) {
                ("convert-with-moss", DeValue::Table(keys)) => {
                    for (key, value) in keys {
                        self.convert_with_moss_key(key.get_ref(), value, errors);
                    }
                }
                ("convert-with-moss", v) => {
                    errors.add(at, format!("convert-with-moss is a table, not {}", kind(v)))
                }
                (other, _) => errors.add(name.span().start, format!("unknown extension: {other}")),
            }
        }
    }

    fn convert_with_moss_key(&mut self, key: &str, value: &Spanned<DeValue>, errors: &mut Errors) {
        let at = value.span().start;
        let moss = &mut self.convert_with_moss;
        match (key, value.get_ref()) {
            ("enable", DeValue::Boolean(b)) => moss.enable = *b,
            ("enable", v) => errors.add(
                at,
                format!("convert-with-moss.enable is true or false, not {}", kind(v)),
            ),
            // Checked when it is run: it may be installed later.
            ("path", DeValue::String(s)) if s.is_empty() => {
                moss.path = crate::convertwithmoss::default_program()
            }
            ("path", DeValue::String(s)) => match expand_home(s) {
                Some(path) => moss.path = path,
                // A bare name would be looked up on PATH.
                None => errors.add(
                    at,
                    "convert-with-moss.path must be an absolute path or start with ~/",
                ),
            },
            ("path", v) => errors.add(at, format!("convert-with-moss.path cannot be {}", kind(v))),
            (other, _) => errors.add(at, format!("unknown convert-with-moss setting: {other}")),
        }
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
                let message = e.message().trim();
                // Matched on the line, not the message, which toml may reword.
                if line_at(text, at).contains('\\') {
                    errors.add(at, format!("{message}; {BACKSLASHES}"));
                } else {
                    errors.add(at, message);
                }
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
                ("after_queue", DeValue::String(s)) => {
                    match AfterQueue::NAMES
                        .iter()
                        .find(|a| a.0.eq_ignore_ascii_case(s))
                    {
                        Some(&(_, after)) => self.after_queue = after,
                        None => errors.add(
                            at,
                            format!("unknown after_queue {s}; choices: resume, stop"),
                        ),
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
                (name @ ("dj_knee" | "tape_knee"), v) => match number(v) {
                    Some(n) if (-24.0..=-0.1).contains(&n) => match name {
                        "dj_knee" => self.dj_knee = n as f32,
                        _ => self.tape_knee = n as f32,
                    },
                    _ => errors.add(at, format!("{name} is a number from -24 to -0.1 dBFS")),
                },
                ("slice_edges", DeValue::String(s)) => {
                    let names = crate::samples::Edges::NAMES;
                    match names.iter().find(|e| e.0.eq_ignore_ascii_case(s)) {
                        Some(&(_, edges)) => self.slice_edges = edges,
                        None => errors.add(
                            at,
                            format!("unknown slice_edges {s}; choices: exact, zero, fade"),
                        ),
                    }
                }
                ("slice_fade_in" | "slice_fade_out", v) => match number(v) {
                    Some(ms) if (0.0..=100.0).contains(&ms) => {
                        let d = std::time::Duration::from_secs_f64(ms / 1000.0);
                        match name.get_ref().as_ref() {
                            "slice_fade_in" => self.slice_fades.fade_in = d,
                            _ => self.slice_fades.fade_out = d,
                        }
                    }
                    _ => errors.add(
                        at,
                        format!("{} is milliseconds, from 0 to 100", name.get_ref()),
                    ),
                },
                ("auto_prune", DeValue::Boolean(b)) => self.auto_prune = *b,
                ("keep_queue", DeValue::Boolean(b)) => self.keep_queue = *b,
                ("draft", DeValue::String(s)) => {
                    match Draft::NAMES.iter().find(|d| d.0.eq_ignore_ascii_case(s)) {
                        Some(&(_, draft)) => self.draft = draft,
                        None => {
                            let names = Draft::NAMES.map(|d| d.0).join(", ");
                            errors.add(at, format!("unknown draft {s}; choices: {names}"))
                        }
                    }
                }
                ("analyze_on_scan", DeValue::Boolean(b)) => self.analyze_on_scan = *b,
                ("slice_ot_file", DeValue::Boolean(b)) => self.slice_ot_file = *b,
                // A control character is a backslash read as an escape: `\n`
                // in "C:\new". No path means one.
                ("samples", DeValue::String(s)) if s.chars().any(char::is_control) => errors.add(
                    at,
                    format!("samples holds a control character; {BACKSLASHES}"),
                ),
                ("samples", DeValue::String(s)) => match expand_home(s) {
                    Some(path) => self.samples = path,
                    None => errors.add(at, "samples must be an absolute path or start with ~/"),
                },
                ("extensions", DeValue::Table(table)) => self.extensions(table, &mut errors),
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
                ("persist", DeValue::Array(items)) => {
                    let named = |s: &str| {
                        Persist::NAMES
                            .iter()
                            .find(|p| p.0.eq_ignore_ascii_case(s))
                            .map(|p| p.1)
                    };
                    let names = || Persist::NAMES.map(|p| p.0).join(", ");
                    let mut persist = Vec::new();
                    for item in items {
                        match item.get_ref() {
                            DeValue::String(s) => match named(s) {
                                Some(p) => persist.push(p),
                                None => errors.add(
                                    item.span().start,
                                    format!("unknown persist {s}; choices: {}", names()),
                                ),
                            },
                            v => errors.add(item.span().start, format!("a name, not {}", kind(v))),
                        }
                    }
                    self.persist = persist;
                }
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
                    "mode" | "samples" | "extensions" | "slice_edges" | "auto_prune"
                    | "analyze_on_scan" | "slice_ot_file" | "device" | "replaygain" | "persist"
                    | "after_queue" | "keep_queue" | "draft",
                    v,
                ) => errors.add(at, format!("{} cannot be {}", name.get_ref(), kind(v))),
                (other, _) => errors.add(name.span().start, format!("unknown setting: {other}")),
            }
        }
        (named, errors)
    }
}

/// `path` with a leading `~/` replaced by the home directory, if it is then absolute.
/// The fix for a Windows path written in a TOML basic string.
const BACKSLASHES: &str =
    "a backslash in \"...\" starts an escape, so write a Windows path as 'C:\\Music' or \"C:/Music\"";

/// The line of `text` holding byte `at`.
fn line_at(text: &str, at: usize) -> &str {
    let start = text
        .get(..at)
        .and_then(|t| t.rfind('\n'))
        .map_or(0, |i| i + 1);
    text[start..].lines().next().unwrap_or("")
}

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
