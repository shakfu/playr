//! Session values remembered between runs, as the `persist` setting chooses.
//!
//! `settings.toml` holds settings, which the user writes; a remembered value
//! is state, which playr writes, so it lives in `library.db`. Each is stored
//! as text in the form its command or setting takes.

use playr_core::audio::Mode;
use playr_core::columns::{Column, SortKey};
use playr_core::gain::ReplayGain;
use playr_core::settings::Persist;

use crate::config::Program;
use crate::Theme;

/// Every value `persist` can name, as a session holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Values {
    /// dB, by band, in `Band::ALL` order.
    pub eq: [f32; 3],
    /// From 0 to 1.
    pub volume: f32,
    pub mode: Mode,
    pub replaygain: ReplayGain,
    pub theme: Theme,
    pub columns: Vec<Column>,
    pub sort: Vec<SortKey>,
    /// `:` command lines, oldest first.
    pub history: Vec<String>,
}

/// The key `p` is stored under. Columns and sort are kept for each program,
/// as each program's table may set its own.
pub fn key(p: Persist, program: Program) -> String {
    match p {
        Persist::Columns | Persist::Sort => format!("{}.{}", p.name(), program.table()),
        _ => p.name().to_string(),
    }
}

/// The value `p` names in `values`, as stored.
pub fn encode(p: Persist, values: &Values) -> String {
    let names = |names: Vec<String>| names.join(",");
    match p {
        Persist::Eq => values.eq.map(|db| db.to_string()).join(" "),
        Persist::Volume => values.volume.to_string(),
        Persist::Mode => name_of(&Mode::NAMES, values.mode).into(),
        Persist::ReplayGain => values.replaygain.name().into(),
        Persist::Theme => values.theme.name().into(),
        Persist::Columns => names(values.columns.iter().map(|c| c.name().into()).collect()),
        Persist::Sort => names(values.sort.iter().map(|k| k.text()).collect()),
        // A command line is one line, so a newline cannot occur in one.
        Persist::History => values.history.join("\n"),
    }
}

/// Sets `p` in `values` from its stored `text`. A value that no longer
/// reads, as after an upgrade renames something, is ignored.
pub fn decode(p: Persist, text: &str, values: &mut Values) {
    match p {
        Persist::Eq => {
            let db: Vec<f32> = text
                .split(' ')
                .filter_map(|n| n.parse().ok())
                .filter(|n: &f32| n.abs() <= playr_core::audio::eq::RANGE_DB)
                .collect();
            if let Ok(eq) = <[f32; 3]>::try_from(db) {
                values.eq = eq;
            }
        }
        Persist::Volume => {
            if let Some(v) = text.parse::<f32>().ok().filter(|v| (0.0..=1.0).contains(v)) {
                values.volume = v;
            }
        }
        Persist::Mode => values.mode = named(&Mode::NAMES, text).unwrap_or(values.mode),
        Persist::ReplayGain => {
            values.replaygain = named(&ReplayGain::NAMES, text).unwrap_or(values.replaygain)
        }
        Persist::Theme => values.theme = named(&Theme::NAMES, text).unwrap_or(values.theme),
        Persist::Columns => {
            let columns: Option<Vec<Column>> = text.split(',').map(Column::named).collect();
            if let Some(columns) = columns.filter(|c| !c.is_empty()) {
                values.columns = columns;
            }
        }
        Persist::Sort => {
            let sort: Option<Vec<SortKey>> = match text {
                "" => Some(Vec::new()),
                _ => text.split(',').map(SortKey::named).collect(),
            };
            if let Some(sort) = sort {
                values.sort = sort;
            }
        }
        Persist::History => {
            values.history = text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(String::from)
                .collect()
        }
    }
}

/// The name `names` gives `value`.
fn name_of<T: PartialEq + Copy>(names: &[(&'static str, T)], value: T) -> &'static str {
    names.iter().find(|n| n.1 == value).map_or("", |n| n.0)
}

/// The value `names` gives `text`.
fn named<T: Copy>(names: &[(&'static str, T)], text: &str) -> Option<T> {
    names.iter().find(|n| n.0 == text).map(|n| n.1)
}
