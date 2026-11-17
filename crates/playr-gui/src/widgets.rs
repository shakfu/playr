//! Controls drawn the same way in every view.

use eframe::egui;

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
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, enabled, on, name)
    });
    response
}
