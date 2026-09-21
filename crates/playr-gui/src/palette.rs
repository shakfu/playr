//! Colours the window paints itself, one set for each egui theme. Widgets,
//! backgrounds and the playhead take theirs from egui's `Visuals`.

use eframe::egui::{Color32, Visuals};

/// Colours of the waveform, marks, messages and level meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Behind the waveform between the marks around the playhead.
    pub region: Color32,
    /// RMS in the region, inside [`Palette::peak`].
    pub rms: Color32,
    pub peak: Color32,
    pub outside_rms: Color32,
    pub outside_peak: Color32,
    /// Planned slices.
    pub edge: Color32,
    /// The meter's zones. Yellow also draws marks and messages; red, a
    /// clipping peak.
    pub green: Color32,
    pub yellow: Color32,
    pub red: Color32,
}

pub const DARK: Palette = Palette {
    region: Color32::from_rgb(40, 60, 80),
    rms: Color32::from_rgb(90, 170, 220),
    peak: Color32::from_rgb(40, 90, 130),
    outside_rms: Color32::from_gray(150),
    outside_peak: Color32::from_gray(80),
    edge: Color32::from_rgb(210, 90, 210),
    green: Color32::from_rgb(80, 200, 120),
    yellow: Color32::from_rgb(230, 200, 60),
    red: Color32::from_rgb(230, 80, 70),
};

/// As [`DARK`], with peaks lighter than RMS so they recede on a light ground.
pub const LIGHT: Palette = Palette {
    region: Color32::from_rgb(214, 228, 242),
    rms: Color32::from_rgb(25, 105, 165),
    peak: Color32::from_rgb(125, 170, 210),
    outside_rms: Color32::from_gray(95),
    outside_peak: Color32::from_gray(175),
    edge: Color32::from_rgb(160, 40, 160),
    green: Color32::from_rgb(30, 140, 60),
    yellow: Color32::from_rgb(150, 100, 0),
    red: Color32::from_rgb(200, 40, 40),
};

impl Palette {
    /// The set for the theme `visuals` draws.
    pub fn of(visuals: &Visuals) -> &'static Palette {
        if visuals.dark_mode {
            &DARK
        } else {
            &LIGHT
        }
    }
}

/// matplotlib's magma at nine even steps, from quiet to loud. Lightness rises
/// throughout, so the order survives colour blindness and greyscale.
const MAGMA: [Color32; 9] = [
    Color32::from_rgb(0x00, 0x00, 0x04),
    Color32::from_rgb(0x1c, 0x10, 0x44),
    Color32::from_rgb(0x4f, 0x12, 0x7b),
    Color32::from_rgb(0x81, 0x25, 0x81),
    Color32::from_rgb(0xb5, 0x36, 0x7a),
    Color32::from_rgb(0xe5, 0x50, 0x64),
    Color32::from_rgb(0xfb, 0x87, 0x61),
    Color32::from_rgb(0xfe, 0xc2, 0x87),
    Color32::from_rgb(0xfc, 0xfd, 0xbf),
];

/// A spectrogram level from 0 to 1 as a magma colour, linear between steps.
pub fn magma(level: f32) -> Color32 {
    let at = level.clamp(0.0, 1.0) * (MAGMA.len() - 1) as f32;
    let i = (at as usize).min(MAGMA.len() - 2);
    MAGMA[i].lerp_to_gamma(MAGMA[i + 1], at - i as f32)
}
