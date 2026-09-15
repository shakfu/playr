//! The terminal's light colours stay legible on a white background. The ANSI
//! set's shades belong to the terminal's theme, so only its roles are checked.

use playr::ui::palette::{self, Palette, ANSI, LIGHT};
use playr::ui::Theme;
use ratatui::style::Color;

/// The RGB value xterm and most terminals give a 256-colour index from 16 on.
fn rgb(colour: Color) -> [u8; 3] {
    let Color::Indexed(i @ 16..) = colour else {
        panic!("{colour:?} is not in the 256-colour table");
    };
    if i >= 232 {
        return [8 + 10 * (i - 232); 3];
    }
    let level = |n: u8| [0, 95, 135, 175, 215, 255][n as usize];
    let i = i - 16;
    [level(i / 36), level(i / 6 % 6), level(i % 6)]
}

/// WCAG 2 relative luminance.
fn luminance([r, g, b]: [u8; 3]) -> f32 {
    let channel = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

const WHITE: [u8; 3] = [255; 3];
/// Text in the terminal's default colour, assumed near black on a light theme.
const BLACK: [u8; 3] = [0; 3];

fn every_role(p: &Palette) -> [Color; 18] {
    [
        p.accent,
        p.accent_peak,
        p.progress,
        p.progress_text,
        p.dim,
        p.selected_bg,
        p.dim_selected,
        p.name,
        p.notice,
        p.mark,
        p.mark_peak,
        p.edge,
        p.clip,
        p.outside,
        p.outside_peak,
        p.green,
        p.yellow,
        p.red,
    ]
}

#[test]
fn themes_choose_their_sets() {
    assert_eq!(palette::of(Theme::System), &ANSI);
    assert_eq!(palette::of(Theme::Dark), &ANSI);
    assert_eq!(palette::of(Theme::Light), &LIGHT);
    // Every light role is a fixed colour; `rgb` panics on any other.
    for colour in every_role(&LIGHT) {
        rgb(colour);
    }
}

#[test]
fn light_text_reaches_4_5_to_1() {
    let p = &LIGHT;
    let on_white = [
        ("accent", p.accent),
        ("dim", p.dim),
        ("name", p.name),
        ("notice", p.notice),
        ("edge", p.edge),
        ("clip", p.clip),
    ];
    for (role, colour) in on_white {
        let ratio = contrast(rgb(colour), WHITE);
        assert!(ratio >= 4.5, "{role} on white: {ratio:.2}");
    }
    // A cursor row shows these over its background.
    let selected = rgb(p.selected_bg);
    let on_selected = [
        ("default text", BLACK),
        ("accent", rgb(p.accent)),
        ("name", rgb(p.name)),
        ("mark", rgb(p.mark)),
        ("dim_selected", rgb(p.dim_selected)),
    ];
    for (role, colour) in on_selected {
        let ratio = contrast(colour, selected);
        assert!(ratio >= 4.5, "{role} on the cursor row: {ratio:.2}");
    }
}

/// Bars and marks need 3:1, WCAG's floor for graphics a reader must see.
#[test]
fn light_bars_and_marks_reach_3_to_1() {
    let p = &LIGHT;
    for (role, colour) in [
        ("accent", p.accent),
        ("mark", p.mark),
        ("outside", p.outside),
        ("progress", p.progress),
        ("green", p.green),
        ("yellow", p.yellow),
        ("red", p.red),
    ] {
        let ratio = contrast(rgb(colour), WHITE);
        assert!(ratio >= 3.0, "{role} on white: {ratio:.2}");
    }
    let over = contrast(rgb(p.progress_text), rgb(p.progress));
    assert!(over >= 4.5, "the time over the progress bar: {over:.2}");
}

/// A peak is lighter than its RMS and darker than the background.
#[test]
fn light_peaks_recede_towards_white() {
    let p = &LIGHT;
    for (role, rms, peak) in [
        ("accent", p.accent, p.accent_peak),
        ("mark", p.mark, p.mark_peak),
        ("outside", p.outside, p.outside_peak),
    ] {
        let (rms, peak) = (luminance(rgb(rms)), luminance(rgb(peak)));
        assert!(
            rms < peak && peak < 1.0,
            "{role}: rms {rms:.3}, peak {peak:.3}"
        );
    }
}
