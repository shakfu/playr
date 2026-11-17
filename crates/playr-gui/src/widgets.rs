//! Controls drawn the same way in every view.

use eframe::egui;

use crate::palette::Palette;

/// The bottom of a peak meter, in dBFS.
const FLOOR_DB: f32 = -60.0;

/// A toggle: plain text while off, filled while `on`, so off reads apart
/// from a framed action button. `name` is its name for assistive technology.
pub fn toggle(
    ui: &mut egui::Ui,
    enabled: bool,
    on: bool,
    text: impl Into<egui::WidgetText>,
    name: &str,
) -> egui::Response {
    let response = ui.add_enabled(enabled, egui::Button::selectable(on, text));
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, name));
    response
}

/// A peak meter of `held` dBFS, `size` big, named `name`: from [`FLOOR_DB`]
/// to full scale, upward when taller than wide, else rightward. Green,
/// yellow from -6 dB, red over full scale.
pub fn peak_meter(
    ui: &mut egui::Ui,
    held: Option<f32>,
    size: egui::Vec2,
    name: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ProgressIndicator, true, name));
    let p = Palette::of(ui.visuals());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 1.0, ui.visuals().extreme_bg_color);
    let Some(db) = held else {
        return response;
    };
    let fill = ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);
    let colour = match db {
        d if d > 0.0 => p.red,
        d if d > -6.0 => p.yellow,
        _ => p.green,
    };
    let bar = match size.y > size.x {
        true => egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - rect.height() * fill),
            rect.max,
        ),
        false => egui::Rect::from_min_max(
            rect.min,
            egui::pos2(rect.left() + rect.width() * fill, rect.bottom()),
        ),
    };
    painter.rect_filled(bar, 1.0, colour);
    response
}
