//! The columns a track list shows, and the order it is sorted in.
//!
//! A column is also a sort key, so `columns` and `sort` in `settings.toml`
//! name the same set. Sorting happens here rather than in SQL: the loudness
//! and tempo columns come from `analysis`, keyed by path and only counted
//! while they still describe the file, and the list a frontend shows is the
//! list it plays, so one order has to serve both.

use std::cmp::Ordering;

use crate::db::Track;

/// A column of a track list, and a key to sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Title,
    Artist,
    AlbumArtist,
    Album,
    Disc,
    TrackNo,
    Year,
    Time,
    /// From `playr analyze`.
    Tempo,
    Loudness,
    Peak,
    Path,
}

impl Column {
    /// Each column's name in `settings.toml` and in `:columns`.
    pub const NAMES: [(&'static str, Column); 12] = [
        ("title", Column::Title),
        ("artist", Column::Artist),
        ("album_artist", Column::AlbumArtist),
        ("album", Column::Album),
        ("disc", Column::Disc),
        ("track", Column::TrackNo),
        ("year", Column::Year),
        ("time", Column::Time),
        ("tempo", Column::Tempo),
        ("loudness", Column::Loudness),
        ("peak", Column::Peak),
        ("path", Column::Path),
    ];

    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(_, c)| *c == self)
            .map(|(name, _)| *name)
            .unwrap_or("title")
    }

    /// The heading a table shows, which is the name with a capital and the
    /// unit where the number needs one.
    pub fn heading(self) -> &'static str {
        match self {
            Column::Title => "Title",
            Column::Artist => "Artist",
            Column::AlbumArtist => "Album artist",
            Column::Album => "Album",
            Column::Disc => "Disc",
            Column::TrackNo => "No.",
            Column::Year => "Year",
            Column::Time => "Time",
            Column::Tempo => "BPM",
            Column::Loudness => "LUFS",
            Column::Peak => "Peak",
            Column::Path => "File",
        }
    }

    /// Numbers are drawn narrow and to the right; text takes the room left.
    pub fn numeric(self) -> bool {
        matches!(
            self,
            Column::Disc
                | Column::TrackNo
                | Column::Year
                | Column::Time
                | Column::Tempo
                | Column::Loudness
                | Column::Peak
        )
    }

    /// Cells a number needs, for a terminal's fixed-width row.
    pub fn width(self) -> usize {
        match self {
            Column::Disc | Column::TrackNo => 3,
            Column::Year | Column::Tempo => 5,
            Column::Time | Column::Loudness | Column::Peak => 6,
            _ => 0,
        }
    }

    /// Whether the column reads what `playr analyze` measured, which is
    /// missing until a track is analysed.
    pub fn measured(self) -> bool {
        matches!(self, Column::Tempo | Column::Loudness | Column::Peak)
    }

    pub fn named(name: &str) -> Option<Column> {
        Self::NAMES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name.trim()))
            .map(|(_, c)| *c)
    }
}

/// A column to sort by, and which way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortKey {
    pub column: Column,
    pub descending: bool,
}

impl SortKey {
    /// `tempo` or `tempo desc`, as `settings.toml` and `:sort` write it.
    pub fn named(text: &str) -> Option<SortKey> {
        let text = text.trim();
        let (name, descending) = match text.rsplit_once(char::is_whitespace) {
            Some((name, "desc")) => (name, true),
            Some((name, "asc")) => (name, false),
            _ => (text, false),
        };
        Some(SortKey {
            column: Column::named(name)?,
            descending,
        })
    }

    pub fn text(self) -> String {
        match self.descending {
            true => format!("{} desc", self.column.name()),
            false => self.column.name().to_string(),
        }
    }
}

/// What `playr analyze` measured about a track, for the columns that show it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Measures {
    pub loudness: Option<f32>,
    pub peak: Option<f32>,
    pub bpm: Option<f32>,
}

/// One cell: text, a number, or nothing measured or tagged yet.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Text(String),
    Number(f64),
    Missing,
}

impl Cell {
    /// The cell as a list shows it; `-` where there is nothing.
    pub fn text(&self, column: Column) -> String {
        match self {
            Cell::Text(s) => s.clone(),
            Cell::Missing => "-".into(),
            Cell::Number(n) => match column {
                Column::Time => {
                    let d = std::time::Duration::from_secs_f64(n.max(0.0));
                    let (m, s) = (d.as_secs() / 60, d.as_secs() % 60);
                    format!("{m}:{s:02}")
                }
                Column::Loudness => format!("{n:.1}"),
                Column::Peak => format!("{n:.3}"),
                Column::Tempo => format!("{n:.0}"),
                _ => format!("{n:.0}"),
            },
        }
    }
}

/// The value `column` holds for `track`.
pub fn cell(track: &Track, measures: Measures, column: Column) -> Cell {
    let text = |s: Option<&String>| match s.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(s) => Cell::Text(s.to_string()),
        None => Cell::Missing,
    };
    let number = |n: Option<f64>| n.map_or(Cell::Missing, Cell::Number);
    match column {
        Column::Title => match track.title.as_ref() {
            Some(_) => text(track.title.as_ref()),
            // A file with no title tag is listed by its name, as it is
            // everywhere else.
            None => Cell::Text(track.display_title()),
        },
        Column::Artist => text(track.artist.as_ref()),
        Column::AlbumArtist => text(track.album_artist.as_ref().or(track.artist.as_ref())),
        Column::Album => text(track.album.as_ref()),
        Column::Disc => number(track.disc_no.map(f64::from)),
        Column::TrackNo => number(track.track_no.map(f64::from)),
        Column::Year => number(track.year.map(f64::from)),
        Column::Time => number(track.duration_ms.map(|ms| ms.max(0) as f64 / 1000.0)),
        Column::Tempo => number(measures.bpm.map(f64::from)),
        Column::Loudness => number(measures.loudness.map(f64::from)),
        Column::Peak => number(measures.peak.map(f64::from)),
        Column::Path => Cell::Text(track.path.clone()),
    }
}

/// Orders two cells, with nothing measured or tagged last whichever way the
/// column is sorted, so unanalysed tracks never fill the top of the list.
fn compare_cells(a: &Cell, b: &Cell, descending: bool) -> Ordering {
    let order = match (a, b) {
        (Cell::Missing, Cell::Missing) => return Ordering::Equal,
        (Cell::Missing, _) => return Ordering::Greater,
        (_, Cell::Missing) => return Ordering::Less,
        (Cell::Number(a), Cell::Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (Cell::Text(a), Cell::Text(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
        // Only a mixed column, which cannot happen for one key.
        _ => Ordering::Equal,
    };
    match descending {
        true => order.reverse(),
        false => order,
    }
}

/// The order a list holds ties in, so a sort is stable however it is reached:
/// the library order, then the path, which is unique.
const TIE: [Column; 6] = [
    Column::AlbumArtist,
    Column::Album,
    Column::Disc,
    Column::TrackNo,
    Column::Title,
    Column::Path,
];

/// Orders `a` before or after `b` by `keys`, then by the library's own order.
pub fn compare(a: (&Track, Measures), b: (&Track, Measures), keys: &[SortKey]) -> Ordering {
    for key in keys {
        let order = compare_cells(
            &cell(a.0, a.1, key.column),
            &cell(b.0, b.1, key.column),
            key.descending,
        );
        if order != Ordering::Equal {
            return order;
        }
    }
    for column in TIE {
        let order = compare_cells(&cell(a.0, a.1, column), &cell(b.0, b.1, column), false);
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}
