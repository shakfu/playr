//! The Mix tab: a strip for the player, the tape, the decks, the headphones
//! and the master, each a fader, its level in dB, a meter and a mute. The
//! player's strip has the EQ; the master's has the recording and the fader
//! law. `docs/dev/mixer.md`, under "Phase 3", has the design.

use eframe::egui;
use playr_app::action::Action;
use playr_app::dispatch::Frontend;
use playr_app::mix::{Law, Mix, MixAction, Strip};
use playr_app::model::Model;
use playr_core::audio::eq::{Band, RANGE_DB};

use crate::controls;
use crate::palette::Palette;

/// The faders' length, in points.
const FADER: f32 = 150.0;
/// The bottom of a meter, in dBFS.
const FLOOR_DB: f32 = -60.0;

/// The meter shown for `s`: [`playr_app::model::Snapshot::meters`] holds
/// the player's, the tape's, the decks' and the master's; the headphones
/// have none.
fn meter_of(s: Strip) -> Option<usize> {
    match s {
        Strip::Player => Some(0),
        Strip::Tape => Some(1),
        Strip::Decks => Some(2),
        Strip::Master => Some(3),
        Strip::Headphones => None,
    }
}

/// Draws the tab and returns what its controls ask for.
pub fn show(model: &Model, ui: &mut egui::Ui) -> Vec<Action> {
    let mix = model.mixer().clone();
    let snapshot = model.snapshot();
    let recording = model.session().master_recording().map(|p| p.to_path_buf());
    let mut actions = Vec::new();
    ui.spacing_mut().slider_width = FADER;
    egui::ScrollArea::horizontal().show(ui, |ui| {
        ui.horizontal_top(|ui| {
            for strip in Strip::ALL {
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        let held = meter_of(strip).map(|i| snapshot.meters[i]);
                        fader(ui, &mix, strip, held, &mut actions);
                    });
                });
                match strip {
                    Strip::Player => {
                        ui.group(|ui| eq(ui, snapshot.eq, &mut actions));
                    }
                    Strip::Master => {
                        ui.group(|ui| master(ui, &mix, recording.as_deref(), &mut actions));
                    }
                    _ => {}
                }
            }
        });
    });
    actions
}

/// A strip's name, fader and meter, its level in dB, and its mute.
fn fader(
    ui: &mut egui::Ui,
    mix: &Mix,
    strip: Strip,
    held: Option<Option<f32>>,
    actions: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        let mut p = (mix.level(strip) * 100.0).round();
        let slider = egui::Slider::new(&mut p, 0.0..=100.0)
            .vertical()
            .show_value(false)
            .text(strip.name());
        if ui.add(slider).changed() {
            actions.push(Action::Mix(MixAction::Set(strip, p / 100.0)));
        }
        if let Some(held) = held {
            meter(ui, held);
        }
    });
    ui.monospace(match mix.law().gain(mix.level(strip)) {
        0.0 => "off".to_string(),
        g => format!("{:.1} dB", 20.0 * g.log10()),
    });
    let mut muted = mix.muted(strip);
    if ui
        .checkbox(&mut muted, format!("mute {}", strip.name()))
        .changed()
    {
        actions.push(Action::Mix(MixAction::Mute(strip, Some(muted))));
    }
}

/// A vertical peak meter of `held` dBFS, from [`FLOOR_DB`] at the bottom to
/// full scale at the top: green, yellow from -6 dB, red over full scale.
fn meter(ui: &mut egui::Ui, held: Option<f32>) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, FADER), egui::Sense::hover());
    let p = Palette::of(ui.visuals());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 1.0, ui.visuals().extreme_bg_color);
    let Some(db) = held else {
        return;
    };
    let fill = ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);
    let top = rect.bottom() - rect.height() * fill;
    let colour = match db {
        d if d > 0.0 => p.red,
        d if d > -6.0 => p.yellow,
        _ => p.green,
    };
    let bar = egui::Rect::from_min_max(egui::pos2(rect.left(), top), rect.max);
    painter.rect_filled(bar, 1.0, colour);
}

/// The player's EQ: a vertical slider a band, and Flat.
fn eq(ui: &mut egui::Ui, gains: [f32; 3], actions: &mut Vec<Action>) {
    ui.vertical(|ui| {
        ui.label("EQ");
        ui.horizontal(|ui| {
            for (band, mut db) in Band::ALL.into_iter().zip(gains) {
                let slider = egui::Slider::new(&mut db, -RANGE_DB..=RANGE_DB)
                    .vertical()
                    .step_by(0.5)
                    .show_value(false)
                    .text(band.name());
                if ui.add(slider).changed() {
                    actions.push(Action::SetEq(band, db));
                }
            }
        });
        for control in controls::EQ {
            let flat = gains == [0.0; 3];
            if ui
                .add_enabled(!flat, egui::Button::new(control.label))
                .clicked()
            {
                actions.push(control.action.clone());
            }
        }
    });
}

/// The master's recording, the file it goes to, and the fader law.
fn master(
    ui: &mut egui::Ui,
    mix: &Mix,
    recording: Option<&std::path::Path>,
    actions: &mut Vec<Action>,
) {
    ui.vertical(|ui| {
        let label = match recording {
            Some(_) => "Stop recording",
            None => "Record",
        };
        if ui.button(label).clicked() {
            actions.push(Action::Mix(MixAction::Record));
        }
        if let Some(path) = recording.and_then(|p| p.file_name()) {
            ui.weak(path.to_string_lossy());
        }
        ui.add_space(8.0);
        ui.label("Law");
        for (name, law) in Law::NAMES {
            if ui.selectable_label(mix.law() == law, name).clicked() && mix.law() != law {
                actions.push(Action::Mix(MixAction::Law(Some(law))));
            }
        }
    });
}
