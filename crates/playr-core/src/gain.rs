//! ReplayGain: one fixed gain per track, bringing it to a common loudness.
//!
//! Gains come from the library's analysis, else from the file's tags; playr
//! never writes them to the file. `docs/dev/analyze.md` sets out the design.

use std::path::Path;

use lofty::file::TaggedFileExt;
use lofty::prelude::ItemKey;
use lofty::probe::Probe;

use crate::audio::Mode;

/// The loudness ReplayGain 2.0 brings every track to, in LUFS.
pub const REFERENCE_LUFS: f32 = -18.0;

/// The reference `R128_*_GAIN` tags count from, in LUFS (RFC 7845 section
/// 5.2.1). They count from the Opus header's output gain as well, which the
/// decoder applies, so the tag's value stands on its own here.
const R128_LUFS: f32 = -23.0;

/// Which gain applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplayGain {
    #[default]
    Off,
    Track,
    Album,
    /// Album gain where tracks play in list order, track gain where they do not.
    Auto,
}

impl ReplayGain {
    /// Each setting's name in commands and settings.
    pub const NAMES: [(&'static str, ReplayGain); 4] = [
        ("off", ReplayGain::Off),
        ("track", ReplayGain::Track),
        ("album", ReplayGain::Album),
        ("auto", ReplayGain::Auto),
    ];

    pub fn name(self) -> &'static str {
        match self {
            ReplayGain::Off => "off",
            ReplayGain::Track => "track",
            ReplayGain::Album => "album",
            ReplayGain::Auto => "auto",
        }
    }

    /// Whether album gain applies under playback `mode`; `None` when off.
    pub fn album(self, mode: Mode) -> Option<bool> {
        match self {
            ReplayGain::Off => None,
            ReplayGain::Track => Some(false),
            ReplayGain::Album => Some(true),
            ReplayGain::Auto => Some(matches!(mode, Mode::Normal | Mode::Repeat)),
        }
    }
}

/// A gain in dB, with the peak it must not push past full scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gain {
    pub db: f32,
    /// Sample peak, linear; `None` when unknown.
    pub peak: Option<f32>,
}

impl Gain {
    /// The gain that brings `lufs` to [`REFERENCE_LUFS`].
    pub fn for_loudness(lufs: f32, peak: f32) -> Gain {
        Gain {
            db: REFERENCE_LUFS - lufs,
            peak: Some(peak),
        }
    }

    /// The factor to multiply samples by. It is capped so the peak stays at
    /// or below full scale; with no peak known, it never boosts.
    pub fn linear(self) -> f32 {
        let factor = 10f32.powf(self.db / 20.0);
        let cap = match self.peak {
            Some(p) if p > 0.0 => 1.0 / p,
            Some(_) => f32::INFINITY,
            None => 1.0,
        };
        factor.min(cap)
    }
}

/// A track's gains.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Gains {
    pub track: Option<Gain>,
    pub album: Option<Gain>,
}

impl Gains {
    /// The gain to apply: album gain when `album`, else track gain, each
    /// falling back to the other.
    pub fn pick(self, album: bool) -> Option<Gain> {
        match album {
            true => self.album.or(self.track),
            false => self.track.or(self.album),
        }
    }

    /// Gains from the file's tags: `REPLAYGAIN_*`, else Opus's `R128_*_GAIN`.
    /// Empty when the file has neither or cannot be read.
    pub fn from_tags(path: &Path) -> Gains {
        let Some(tagged) = Probe::open(path).ok().and_then(|p| p.read().ok()) else {
            return Gains::default();
        };
        let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
            return Gains::default();
        };
        let get = |k: ItemKey| tag.get_string(k).map(str::to_owned);
        let replaygain = |gain: ItemKey, peak: ItemKey| {
            Some(Gain {
                db: parse_db(&get(gain)?)?,
                peak: get(peak).and_then(|p| p.trim().parse().ok()),
            })
        };
        let r128 = |gain: ItemKey| {
            let q78: i32 = get(gain)?.trim().parse().ok()?;
            Some(Gain {
                db: q78 as f32 / 256.0 + (REFERENCE_LUFS - R128_LUFS),
                peak: None,
            })
        };
        Gains {
            track: replaygain(ItemKey::ReplayGainTrackGain, ItemKey::ReplayGainTrackPeak)
                .or_else(|| r128(ItemKey::R128TrackGain)),
            album: replaygain(ItemKey::ReplayGainAlbumGain, ItemKey::ReplayGainAlbumPeak)
                .or_else(|| r128(ItemKey::R128AlbumGain)),
        }
    }
}

/// A gain as tags write it: `-7.43 dB`, `+2.1 dB` or a bare number.
pub fn parse_db(text: &str) -> Option<f32> {
    let text = text.trim();
    let number = text
        .strip_suffix("dB")
        .or_else(|| text.strip_suffix("db"))
        .or_else(|| text.strip_suffix("DB"))
        .unwrap_or(text)
        .trim();
    number.parse::<f32>().ok().filter(|db| db.is_finite())
}
