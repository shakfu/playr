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
use crate::mix::{Mix, Strip};
use crate::Theme;

/// Every value `persist` can name, as a session holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Values {
    /// dB, by band, in `Band::ALL` order.
    pub eq: [f32; 3],
    /// The faders. `Volume` keeps the master's position, `Mix` the rest.
    pub mix: Mix,
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
        // A position; `volume` held a gain, which `legacy_volume` reads.
        Persist::Volume => "master".into(),
        _ => p.name().to_string(),
    }
}

/// The value `p` names in `values`, as stored.
pub fn encode(p: Persist, values: &Values) -> String {
    let names = |names: Vec<String>| names.join(",");
    match p {
        Persist::Eq => values.eq.map(|db| db.to_string()).join(" "),
        Persist::Volume => values.mix.level(Strip::Master).to_string(),
        Persist::Mix => {
            let levels = MIXED.map(|s| values.mix.level(s).to_string());
            let mutes = Strip::ALL.map(|s| u8::from(values.mix.muted(s)).to_string());
            format!("{} {}", levels.join(" "), mutes.join(" "))
        }
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
                values.mix.set_level(Strip::Master, v);
            }
        }
        Persist::Mix => {
            let words: Vec<&str> = text.split(' ').collect();
            let levels: Option<Vec<f32>> = words
                .iter()
                .take(MIXED.len())
                .map(|w| w.parse().ok().filter(|v| (0.0..=1.0).contains(v)))
                .collect();
            let mutes: Option<Vec<bool>> = words
                .iter()
                .skip(MIXED.len())
                .map(|w| match *w {
                    "0" => Some(false),
                    "1" => Some(true),
                    _ => None,
                })
                .collect();
            let fits = words.len() == MIXED.len() + Strip::ALL.len();
            if let (true, Some(levels), Some(mutes)) = (fits, levels, mutes) {
                for (s, p) in MIXED.into_iter().zip(levels) {
                    values.mix.set_level(s, p);
                }
                for (s, on) in Strip::ALL.into_iter().zip(mutes) {
                    values.mix.set_muted(s, on);
                }
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

/// The strips `Mix` keeps the level of: all but the master.
const MIXED: [Strip; 4] = [Strip::Player, Strip::Tape, Strip::Decks, Strip::Headphones];

/// The master's position from the gain a version before the mixer stored
/// under `volume`, by the law in use, so it plays at the same level.
pub fn legacy_volume(text: &str, law: crate::mix::Law) -> Option<f32> {
    let g = text
        .parse::<f32>()
        .ok()
        .filter(|v| (0.0..=1.0).contains(v))?;
    Some(law.position(g))
}

/// The name `names` gives `value`.
fn name_of<T: PartialEq + Copy>(names: &[(&'static str, T)], value: T) -> &'static str {
    names.iter().find(|n| n.1 == value).map_or("", |n| n.0)
}

/// The value `names` gives `text`.
fn named<T: Copy>(names: &[(&'static str, T)], text: &str) -> Option<T> {
    names.iter().find(|n| n.0 == text).map(|n| n.1)
}
