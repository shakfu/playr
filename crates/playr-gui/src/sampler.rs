//! The sampler view: the playing track's waveform, painted a column a point
//! wide, with its region, marks, playhead and planned slice edges.
//!
//! A click seeks, a shift-click marks, and the mouse wheel zooms around the
//! playhead. The displays match the terminal's: RMS inside peak on a linear
//! scale, the same on a dB scale, and the waveform's extremes around a centre
//! line, which the terminal draws in Braille and `:display braille` names.

use eframe::egui;
use playr_app::action::{Action, Zoom};
use playr_app::model::Model;
use playr_app::sampler::{self, Layout};
use playr_app::Display;

use crate::controls;
use crate::palette::Palette;

/// Mouse wheel movement, in points, that makes one zoom step.
const WHEEL_STEP: f32 = 40.0;

/// Wheel movement not yet turned into a zoom step, kept between frames.
#[derive(Debug, Default)]
pub struct Wheel(f32);

/// Draws the view and returns what its controls ask for. The zoom the
/// layout settled on goes back to the model.
pub fn show(model: &mut Model, ui: &mut egui::Ui, wheel: &mut Wheel) -> Vec<Action> {
    let mut actions = Vec::new();
    let snapshot = model.snapshot().clone();
    let current = snapshot.status.current().cloned();
    let peaks = match sampler::peaks_of(model.sampler(), current.as_ref()) {
        Ok(peaks) => peaks,
        Err(hint) => {
            ui.weak(hint);
            return actions;
        }
    };
    let display = model.sampler().display;
    let name = current
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Room below the waveform for the detail line and the buttons.
    let height = (ui.available_height() - 64.0).max(80.0);
    let width = ui.available_width();
    let layout = Layout::new(
        peaks,
        width as u64,
        model.sampler().zoom,
        snapshot.position,
        &snapshot.marks,
    );
    model.set_zoom(layout.zoom);

    ui.horizontal(|ui| {
        ui.strong(&name);
        ui.weak(format!("{}  one column {}", layout.shown(), layout.scale()));
    });
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Track waveform"));
    paint(
        ui.painter_at(rect),
        ui.visuals(),
        rect,
        &layout,
        display,
        model,
    );

    if response.hovered() {
        let delta = ui.input(|i| i.smooth_scroll_delta.y);
        wheel.0 += delta;
        while wheel.0.abs() >= WHEEL_STEP {
            actions.push(Action::Zoom(if wheel.0 > 0.0 {
                Zoom::In
            } else {
                Zoom::Out
            }));
            wheel.0 -= WHEEL_STEP.copysign(wheel.0);
        }
        if let Some(pos) = response.hover_pos() {
            let at = layout.time_at(pos.x - rect.left());
            response
                .clone()
                .on_hover_text_at_pointer(playr_app::message::fmt_time(at));
        }
    } else {
        wheel.0 = 0.0;
    }
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let at = layout.time_at(pos.x - rect.left());
            let shift = ui.input(|i| i.modifiers.shift);
            actions.push(if shift {
                Action::MarkAt(at)
            } else {
                Action::SeekTo(at)
            });
        }
    }

    ui.horizontal(|ui| {
        let plan = sampler::plan_text(model.sampler());
        if !plan.is_empty() {
            ui.colored_label(Palette::of(ui.visuals()).edge, plan);
        }
        ui.weak(layout.region_text());
    });
    ui.horizontal(|ui| {
        for control in controls::SAMPLER_BAR {
            let enabled = match control.action {
                Action::WriteSlices | Action::DiscardSlices => model.sampler().pending.is_some(),
                Action::Display(Some(d)) => d != display,
                _ => true,
            };
            if ui
                .add_enabled(enabled, egui::Button::new(control.label))
                .clicked()
            {
                actions.push(control.action.clone());
            }
        }
    });
    actions
}

/// Paints the waveform of `layout` into `rect`, one column a point wide.
fn paint(
    painter: egui::Painter,
    visuals: &egui::Visuals,
    rect: egui::Rect,
    layout: &Layout,
    display: Display,
    model: &Model,
) {
    let colours = Palette::of(visuals);
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
    let playhead = layout.playhead();
    let x = |c: usize| rect.left() + c as f32 + 0.5;
    let columns = (layout.columns as usize).min(rect.width() as usize);

    // The region behind everything.
    let region: Vec<usize> = (0..columns).filter(|&c| layout.in_region(c)).collect();
    if let (Some(&first), Some(&last)) = (region.first(), region.last()) {
        let span = egui::Rect::from_x_y_ranges(x(first) - 0.5..=x(last) + 0.5, rect.y_range());
        painter.rect_filled(span, 0.0, colours.region);
    }

    for c in 0..columns {
        let inside = layout.in_region(c);
        let (rms_colour, peak_colour) = if inside {
            (colours.rms, colours.peak)
        } else {
            (colours.outside_rms, colours.outside_peak)
        };
        let column = |top: f32, bottom: f32, colour| {
            painter.line_segment(
                [egui::pos2(x(c), top), egui::pos2(x(c), bottom)],
                egui::Stroke::new(1.0, colour),
            );
        };
        match display {
            Display::Envelope | Display::Decibels => {
                let (rms, peak) = layout.heights(display, c);
                let y = |h: f32| rect.bottom() - h.clamp(0.0, 1.0) * rect.height();
                if peak > 0.0 {
                    column(y(peak), rect.bottom(), peak_colour);
                }
                if rms > 0.0 {
                    column(y(rms), rect.bottom(), rms_colour);
                }
            }
            Display::Braille => {
                let (a, b) = layout.span_of(c);
                let (lo, hi) = layout.extent(a, b);
                if lo <= hi {
                    let y = |v: f32| rect.center().y - v.clamp(-1.0, 1.0) * rect.height() / 2.0;
                    column(y(hi), y(lo) + 1.0, rms_colour);
                }
            }
        }
    }

    let line = |c: usize, colour, width| {
        painter.line_segment(
            [
                egui::pos2(x(c), rect.top()),
                egui::pos2(x(c), rect.bottom()),
            ],
            egui::Stroke::new(width, colour),
        );
    };
    if let Some(plan) = &model.sampler().pending {
        for c in sampler::edges(plan).filter_map(|e| layout.column_of(e)) {
            line(c, colours.edge, 1.0);
        }
    }
    for c in layout.marks.iter().filter_map(|&m| layout.column_of(m)) {
        line(c, colours.yellow, 1.5);
    }
    if let Some(c) = playhead {
        line(c, visuals.strong_text_color(), 2.0);
    }
}
