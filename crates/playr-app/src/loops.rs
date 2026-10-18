//! The Loops view's rows, as both frontends show them.

use playr_core::db::loops::{Loop, TempoFrom};

/// The Loops view's column headings, in order.
pub const HEADINGS: [&str; 9] = [
    "Name", "BPM", "Beats", "Key", "From", "To", "Time", "Tags", "ID",
];

/// Loop `l`'s cells under [`HEADINGS`]: From and To are where it lies in
/// its file. A tempo guessed from the length alone is marked `~`. The ID is
/// what `:tape N loop ID` takes.
pub fn cells(l: &Loop) -> [String; 9] {
    let bpm = match (l.bpm, l.bpm_from) {
        (Some(b), Some(TempoFrom::Length)) => format!("~{b:.1}"),
        (Some(b), _) => format!("{b:.1}"),
        (None, _) => String::new(),
    };
    [
        l.name.clone(),
        bpm,
        l.beats.map(|b| b.to_string()).unwrap_or_default(),
        l.key.clone().unwrap_or_default(),
        clock(l.start, l.rate),
        clock(l.end, l.rate),
        format!("{:.2}", l.seconds()),
        l.tags.clone(),
        l.id.to_string(),
    ]
}

/// Frames `start` to `end` of a source at `rate`, as `1:02.3-1:06.1`: where
/// in its file a loop lies, which tells apart loops cut from one track.
pub fn span_text(start: u64, end: u64, rate: u32) -> String {
    format!("{}-{}", clock(start, rate), clock(end, rate))
}

/// Frame `frame` of a source at `rate`, as `1:02.3`.
pub fn clock(frame: u64, rate: u32) -> String {
    let tenths = (frame as f64 * 10.0 / f64::from(rate.max(1))).round() as u64;
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

/// Where loop `l` lies in its file, as [`span_text`] gives it.
pub fn span_of(l: &Loop) -> String {
    span_text(l.start, l.end, l.rate)
}

/// What the Loops view says when it lists nothing.
pub fn empty_text(searching: bool) -> &'static str {
    match searching {
        true => "No loops match. Esc lists them all.",
        false => "No loops. playr loops scan DIR adds a directory of loops; F1 to F8 in the sampler and the tape's Save loop add others.",
    }
}
