//! Both colour sets stay legible on the backgrounds of the theme they serve.

use eframe::egui::{Color32, Visuals};
use playr_gui::palette::{Palette, DARK, LIGHT};

/// Relative luminance, as WCAG 2 defines it.
fn luminance(c: Color32) -> f32 {
    let channel = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// WCAG 2 contrast ratio, from 1 to 21.
fn contrast(a: Color32, b: Color32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn themes() -> [(&'static str, Visuals); 2] {
    [("dark", Visuals::dark()), ("light", Visuals::light())]
}

#[test]
fn each_theme_gets_its_own_set() {
    assert_eq!(Palette::of(&Visuals::dark()), &DARK);
    assert_eq!(Palette::of(&Visuals::light()), &LIGHT);
}

#[test]
fn text_reaches_4_5_to_1_on_the_panel() {
    for (theme, visuals) in themes() {
        let p = Palette::of(&visuals);
        for (name, colour) in [("yellow", p.yellow), ("red", p.red), ("edge", p.edge)] {
            let ratio = contrast(colour, visuals.panel_fill);
            assert!(ratio >= 4.5, "{theme} {name}: {ratio:.2}");
        }
    }
}

/// Lines and bars need 3:1, WCAG's floor for graphics a reader must see.
#[test]
fn lines_and_bars_reach_3_to_1_on_what_they_cross() {
    for (theme, visuals) in themes() {
        let p = Palette::of(&visuals);
        let ground = visuals.extreme_bg_color;
        let lines = [
            ("mark", p.yellow),
            ("edge", p.edge),
            ("playhead", visuals.strong_text_color()),
        ];
        for (name, colour) in lines {
            for (behind, back) in [("ground", ground), ("region", p.region)] {
                let ratio = contrast(colour, back);
                assert!(ratio >= 3.0, "{theme} {name} on {behind}: {ratio:.2}");
            }
        }
        let bars = [
            ("rms", p.rms, p.region),
            ("outside rms", p.outside_rms, ground),
            ("green", p.green, ground),
            ("yellow", p.yellow, ground),
            ("red", p.red, ground),
        ];
        for (name, colour, back) in bars {
            let ratio = contrast(colour, back);
            assert!(ratio >= 3.0, "{theme} {name}: {ratio:.2}");
        }
    }
}

/// A peak sits between its RMS and the ground in luminance, so RMS stands out
/// in both themes and peak recedes.
#[test]
fn peaks_recede_towards_the_ground() {
    for (theme, visuals) in themes() {
        let p = Palette::of(&visuals);
        let ground = luminance(visuals.extreme_bg_color);
        for (name, rms, peak) in [
            ("region", p.rms, p.peak),
            ("outside", p.outside_rms, p.outside_peak),
        ] {
            let (rms, peak) = (luminance(rms), luminance(peak));
            assert!(
                (rms - ground).abs() > (peak - ground).abs()
                    && (peak - ground) * (rms - ground) > 0.0,
                "{theme} {name}: rms {rms:.3}, peak {peak:.3}, ground {ground:.3}"
            );
        }
    }
}
