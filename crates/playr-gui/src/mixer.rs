//! The Mix tab: a strip for the player, the tape, the decks, the headphones
//! and the master, each a fader, its level in dB, a meter and a mute. The
//! master's strip has the recording and the fader law. Each source's own
//! controls stay in its view. `docs/dev/mixer.md`, under "Phase 3", has the
//! design.

use eframe::egui;
use playr_app::action::Action;
use playr_app::dispatch::Frontend;
use playr_app::mix::{Law, Mix, MixAction, Strip};
use playr_app::model::Model;

use crate::palette::Palette;

/// The faders' length, in points.
const FADER: f32 = 150.0;
/// The bottom of a meter, in dBFS.
const FLOOR_DB: f32 = -60.0;

/// The meter shown for `s`: its index in [`playr_app::model::Snapshot::meters`].
fn meter_of(s: Strip) -> usize {
    match s {
        Strip::Player => 0,
        Strip::Tape => 1,
        Strip::Decks => 2,
        Strip::Master => 3,
        Strip::Headphones => 4,
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
                        let held = snapshot.meters[meter_of(strip)];
                        fader(ui, &mix, strip, held, &mut actions);
                    });
                });
                if strip == Strip::Master {
                    ui.group(|ui| master(ui, &mix, recording.as_deref(), &mut actions));
                }
            }
        });
    });
    actions
}

/// A strip's name, fader and meter, its level in dB, and its mute.
fn fader(ui: &mut egui::Ui, mix: &Mix, strip: Strip, held: Option<f32>, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        let mut p = (mix.level(strip) * 100.0).round();
        let slider = egui::Slider::new(&mut p, 0.0..=100.0)
            .vertical()
            .show_value(false)
            .text(strip.name());
        if ui.add(slider).changed() {
            actions.push(Action::Mix(MixAction::Set(strip, p / 100.0)));
        }
        meter(ui, held);
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

/// The master's recording, the file it goes to, the fader law, and how the
/// decks' and the tape's heads read, for an A/B by ear.
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
        ui.add_space(8.0);
        ui.label("Heads").on_hover_text(
            "How the decks and the tape read between samples: sinc filters \
             what a fast read would fold back; hermite is cheaper and brighter",
        );
        for (name, interp) in playr_dsp::Interp::NAMES {
            let on = mix.interp() == interp;
            if ui.selectable_label(on, name).clicked() && !on {
                actions.push(Action::Interp(Some(interp)));
            }
        }
    });
}
