//! The transport: what is playing, the buttons, the progress bar with its
//! marks, volume, speed, mode and the level meter.

use std::time::Duration;

use eframe::egui;
use playr_app::action::Action;

use crate::controls;
use crate::palette::Palette;
use playr_app::message::{self, fmt_time};
use playr_app::meter::{self, Zone};
use playr_app::model::{self, Model};
use playr_core::audio::{speed_for, Mode, State};
use playr_core::gain::ReplayGain;

/// Draws the transport into `ui` and performs what its controls ask for.
pub fn show(model: &mut Model, ui: &mut egui::Ui) {
    let mut actions = Vec::new();
    let snapshot = model.snapshot().clone();
    let status = &snapshot.status;

    ui.horizontal(|ui| {
        for control in controls::TRANSPORT {
            let label = match control.action {
                Action::TogglePause if status.state == State::Playing => "Pause",
                _ => control.label,
            };
            if ui.button(label).clicked() {
                actions.push(control.action.clone());
            }
        }
        ui.separator();
        let now = model::now_playing(model.playing(), status);
        ui.strong(now.as_deref().unwrap_or("nothing playing"));
        if let Some(src) = status.source {
            let mut format = format!("{:.1} kHz {} ch", src.rate as f64 / 1000.0, src.channels);
            // As in the terminal: only a real rate conversion is worth showing.
            if status.output_rate != 0 && status.output_rate != src.rate {
                format.push_str(&format!(
                    " -> {:.1} kHz",
                    status.output_rate as f64 / 1000.0
                ));
            }
            ui.weak(format);
        }
        // As it sounds: varispeed moves it with the music.
        if let Some(bpm) = model.bpm() {
            ui.weak(format!("{bpm:.0} BPM"));
        }
        if status.state == State::Playing {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                level_meter(ui, snapshot.loudness, snapshot.peak);
            });
        }
    });

    ui.horizontal(|ui| {
        let total = status.duration.unwrap_or_default();
        ui.monospace(fmt_time(snapshot.position));
        let width = (ui.available_width() - 60.0).max(80.0);
        match progress(ui, width, snapshot.position, total, &snapshot.marks) {
            Some((at, false)) => actions.push(Action::SeekTo(at)),
            Some((at, true)) => actions.push(Action::MarkAt(at)),
            None => {}
        }
        ui.monospace(fmt_time(total));
    });

    // Wraps: the row is wider than the default window since ReplayGain.
    ui.horizontal_wrapped(|ui| {
        for control in controls::MARKS {
            if ui.button(control.label).clicked() {
                actions.push(control.action.clone());
            }
        }
        ui.separator();
        ui.spacing_mut().slider_width = 90.0;

        let mut volume = (snapshot.volume * 100.0).round();
        let slider = egui::Slider::new(&mut volume, 0.0..=100.0)
            .text("Volume")
            .fixed_decimals(0)
            .suffix("%");
        if ui.add(slider).changed() {
            actions.push(Action::SetVolume(volume / 100.0));
        }

        let mut semitones = status.semitones;
        let label = format!("{:.2}x", speed_for(semitones));
        let slider = egui::Slider::new(&mut semitones, -12..=12)
            .text(format!("Speed {label}"))
            .suffix(" st");
        if ui.add(slider).changed() {
            actions.push(Action::SetSpeed(semitones));
        }
        if ui
            .add_enabled(status.semitones != 0, egui::Button::new("Normal speed"))
            .clicked()
        {
            actions.push(Action::SetSpeed(0));
        }

        egui::ComboBox::from_label("Mode")
            .selected_text(status.mode.name())
            .show_ui(ui, |ui| {
                for (_, mode) in Mode::NAMES {
                    if ui
                        .selectable_label(status.mode == mode, mode.name())
                        .clicked()
                        && status.mode != mode
                    {
                        actions.push(Action::SetMode(mode));
                    }
                }
            });

        let replaygain = model.replaygain();
        let selected = match status.gain_db {
            Some(db) => format!("{} ({})", replaygain.name(), message::replaygain(db)),
            None => replaygain.name().to_string(),
        };
        egui::ComboBox::from_label("ReplayGain")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for (_, choice) in ReplayGain::NAMES {
                    if ui
                        .selectable_label(replaygain == choice, choice.name())
                        .clicked()
                        && replaygain != choice
                    {
                        actions.push(Action::SetReplayGain(choice));
                    }
                }
            });
    });

    for action in actions {
        model.perform(action);
    }
}

/// The progress bar, `width` wide, with a tick at each mark. Returns the
/// position clicked or dragged to, once the mouse is released, and whether
/// Shift was held, which marks there instead of seeking.
fn progress(
    ui: &mut egui::Ui,
    width: f32,
    position: Duration,
    total: Duration,
    marks: &[Duration],
) -> Option<(Duration, bool)> {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, 14.0), egui::Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ProgressIndicator, true, "Position")
    });
    let visuals = ui.visuals();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, visuals.extreme_bg_color);
    if total.is_zero() {
        return None;
    }
    let fraction = |d: Duration| (d.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0) as f32;
    let played = egui::Rect::from_min_size(
        rect.min,
        egui::vec2(rect.width() * fraction(position), rect.height()),
    );
    painter.rect_filled(played, 3.0, visuals.selection.bg_fill);
    for mark in marks {
        let x = rect.left() + rect.width() * fraction(*mark);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(2.0, Palette::of(visuals).yellow),
        );
    }

    let at = |x: f32| total.mul_f32(((x - rect.left()) / rect.width()).clamp(0.0, 1.0));
    let response = match response.hover_pos() {
        Some(pos) => response.on_hover_text_at_pointer(fmt_time(at(pos.x))),
        None => response,
    };
    if response.clicked() || response.drag_stopped() {
        let shift = ui.input(|i| i.modifiers.shift);
        return response
            .interact_pointer_pos()
            .map(|pos| (at(pos.x), shift));
    }
    None
}

/// Momentary loudness filled in its zone colours, the held peak as a line,
/// and both as numbers for anyone who cannot tell the colours apart.
///
/// Drawn right to left, so the readouts come first.
fn level_meter(ui: &mut egui::Ui, loudness: Option<f32>, peak: Option<f32>) {
    let Palette {
        green, yellow, red, ..
    } = *Palette::of(ui.visuals());
    let number = |v: Option<f32>| v.map_or("--".to_string(), |v| format!("{v:.1}"));
    let peak_text = egui::RichText::new(format!("pk {}", number(peak))).monospace();
    ui.label(if peak.is_some_and(meter::clipping) {
        peak_text.color(red)
    } else {
        peak_text
    });
    ui.monospace(format!("{} LUFS", number(loudness)));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(160.0, 12.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    let x = |db: f32| rect.left() + rect.width() * meter::fraction(db);
    if let Some(level) = loudness {
        // One rectangle per zone the level reaches.
        let zones = [
            (meter::FLOOR_DB, meter::YELLOW_FROM_DB, green),
            (meter::YELLOW_FROM_DB, meter::RED_FROM_DB, yellow),
            (meter::RED_FROM_DB, 0.0, red),
        ];
        for (from, to, colour) in zones {
            if level > from {
                let span = egui::Rect::from_x_y_ranges(x(from)..=x(level.min(to)), rect.y_range());
                painter.rect_filled(span, 0.0, colour);
            }
        }
    }
    if let Some(level) = peak {
        let colour = match meter::zone(level) {
            Zone::Green => green,
            Zone::Yellow => yellow,
            Zone::Red => red,
        };
        painter.line_segment(
            [
                egui::pos2(x(level), rect.top()),
                egui::pos2(x(level), rect.bottom()),
            ],
            egui::Stroke::new(2.0, colour),
        );
    }
}
