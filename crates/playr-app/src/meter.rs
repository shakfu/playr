//! The level meter's scale: where a level sits on it, and its colour zones.
//!
//! LUFS and dBFS are both relative to full scale, so one bar shows both the
//! momentary loudness and the held peak.

/// Lowest level the meter shows, in dB relative to full scale.
pub const FLOOR_DB: f32 = -40.0;
/// Where the meter turns yellow: the EBU R68 digital alignment level.
pub const YELLOW_FROM_DB: f32 = -18.0;
/// Where the meter turns red: a conventional headroom mark.
pub const RED_FROM_DB: f32 = -6.0;

/// A colour zone of the meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Green,
    Yellow,
    Red,
}

/// How far up the meter a level of `db` sits, from 0 at [`FLOOR_DB`] to 1 at
/// full scale.
pub fn fraction(db: f32) -> f32 {
    ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
}

/// The level at `fraction` of the way up the meter, in dB.
pub fn level_at(fraction: f32) -> f32 {
    FLOOR_DB * (1.0 - fraction)
}

/// The zone a level of `db` falls in.
pub fn zone(db: f32) -> Zone {
    if db >= RED_FROM_DB {
        Zone::Red
    } else if db >= YELLOW_FROM_DB {
        Zone::Yellow
    } else {
        Zone::Green
    }
}

/// Whether a peak of `db` is at full scale, where the source itself clips;
/// playr's gain never exceeds 1.
pub fn clipping(db: f32) -> bool {
    db >= -0.1
}
