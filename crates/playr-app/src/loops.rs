//! The Loops view's rows, as both frontends show them.

use playr_core::db::loops::{Loop, Origin, TempoFrom};

/// The Loops view's column headings, in order.
pub const HEADINGS: [&str; 7] = ["Name", "BPM", "Beats", "Key", "Time", "From", "Tags"];

/// Loop `l`'s cells under [`HEADINGS`]. A tempo guessed from the length
/// alone is marked `~`.
pub fn cells(l: &Loop) -> [String; 7] {
    let bpm = match (l.bpm, l.bpm_from) {
        (Some(b), Some(TempoFrom::Length)) => format!("~{b:.1}"),
        (Some(b), _) => format!("{b:.1}"),
        (None, _) => String::new(),
    };
    let from = match (l.origin, l.slot) {
        (Origin::Slot, Some(n)) => format!("slot {n}"),
        (o, _) => o.name().to_string(),
    };
    [
        l.name.clone(),
        bpm,
        l.beats.map(|b| b.to_string()).unwrap_or_default(),
        l.key.clone().unwrap_or_default(),
        format!("{:.2}", l.seconds()),
        from,
        l.tags.clone(),
    ]
}

/// What the Loops view says when it lists nothing.
pub fn empty_text(searching: bool) -> &'static str {
    match searching {
        true => "No loops match. Esc lists them all.",
        false => "No loops. playr loops scan DIR adds a directory of loops; F1 to F8 in the sampler and the tape's Save loop add others.",
    }
}
