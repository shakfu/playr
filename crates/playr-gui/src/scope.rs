//! The scope view, painted: the trace over the spectrum and stereo image, over
//! the loudness history. The state is `playr_app::scope`.

use eframe::egui::{self, pos2, vec2, Pos2, Rect, Stroke};

use playr_app::action::Action;
use playr_app::model::Model;
use playr_app::scope::{self, Scope};
use playr_core::settings::LOUDNESS_TARGETS;

use crate::palette::Palette;

/// Frequencies the spectrum labels, in Hz.
const LABELS: [(f32, &str); 7] = [
    (50.0, "50"),
    (100.0, "100"),
    (500.0, "500"),
    (1000.0, "1k"),
    (5000.0, "5k"),
    (10_000.0, "10k"),
    (20_000.0, "20k"),
];

/// Space between panes, and around the edge, in points.
const GAP: f32 = 8.0;

/// Paints the view, and returns what its target field asks for.
pub fn show(model: &Model, ui: &mut egui::Ui) -> Option<Action> {
    let s = model.scope();
    let mut target = s.target();
    let changed = ui
        .horizontal(|ui| {
            let field = egui::DragValue::new(&mut target)
                .range(LOUDNESS_TARGETS)
                .speed(0.1)
                .fixed_decimals(1)
                .suffix(" LUFS");
            let changed = ui.add(field).changed();
            ui.label("Loudness target");
            changed
        })
        .inner;
    ui.add_space(GAP / 2.0);
    let area = ui.available_rect_before_wrap();
    ui.allocate_rect(area, egui::Sense::hover());
    let top = area.top() + area.height() * 0.3;
    let bottom = area.bottom() - (area.height() * 0.25).max(80.0);
    // Square, and no wider than 40% of the view.
    let middle = (bottom - top - GAP).clamp(0.0, area.width() * 0.4);
    let trace = Rect::from_min_max(area.min, pos2(area.right(), top - GAP / 2.0));
    let stereo = Rect::from_min_max(
        pos2(area.right() - middle, top + GAP / 2.0),
        pos2(area.right(), bottom - GAP / 2.0),
    );
    let spectrum = Rect::from_min_max(
        pos2(area.left(), top + GAP / 2.0),
        pos2(stereo.left() - GAP, bottom - GAP / 2.0),
    );
    let history = Rect::from_min_max(pos2(area.left(), bottom + GAP / 2.0), area.max);

    let loudness = model.snapshot().loudness;
    paint_trace(ui, pane(ui, trace, "Waveform", String::new()), s);
    paint_spectrum(ui, pane(ui, spectrum, "Spectrum", String::new()), s);
    let correlation = s
        .correlation()
        .map_or("--".into(), |c| format!("correlation {c:+.2}"));
    paint_stereo(ui, pane(ui, stereo, "Stereo", correlation), s);
    let lufs = |l: Option<f32>| l.map_or("--".into(), |l| format!("{l:.1}"));
    let readout = format!(
        "{} LUFS  integrated {}  target {}",
        lufs(loudness),
        lufs(s.integrated()),
        lufs(Some(s.target())),
    );
    paint_history(ui, pane(ui, history, "Loudness, last minute", readout), s);
    changed.then_some(Action::SetLoudnessTarget(target))
}

/// Paints a pane's ground, `title` at its top left and `readout` at its top
/// right, and returns the space under them. Both name the pane to a screen
/// reader.
fn pane(ui: &egui::Ui, rect: Rect, title: &str, readout: String) -> Rect {
    let label = format!("{title} {readout}");
    ui.interact(rect, ui.id().with(title), egui::Sense::hover())
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, label.trim()));
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
    let font = egui::TextStyle::Small.resolve(ui.style());
    let at = rect.min + vec2(4.0, 2.0);
    let title = painter.text(
        at,
        egui::Align2::LEFT_TOP,
        title,
        font.clone(),
        visuals.text_color(),
    );
    painter.text(
        pos2(rect.right() - 4.0, at.y),
        egui::Align2::RIGHT_TOP,
        readout,
        font,
        visuals.weak_text_color(),
    );
    Rect::from_min_max(pos2(rect.left(), title.bottom() + 2.0), rect.max).shrink(4.0)
}

fn paint_trace(ui: &egui::Ui, rect: Rect, s: &Scope) {
    let painter = ui.painter_at(rect);
    painter.hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.4)),
    );
    let trace = s.trace();
    if trace.len() < 2 {
        return;
    }
    let step = rect.width() / (trace.len() - 1) as f32;
    let points: Vec<Pos2> = trace
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let y = rect.center().y - v.clamp(-1.0, 1.0) * rect.height() / 2.0;
            pos2(rect.left() + i as f32 * step, y)
        })
        .collect();
    let colour = Palette::of(ui.visuals()).rms;
    painter.add(egui::Shape::line(points, Stroke::new(1.5, colour)));
}

fn paint_spectrum(ui: &egui::Ui, rect: Rect, s: &Scope) {
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    let font = egui::TextStyle::Small.resolve(ui.style());
    let axis = font.size + 2.0;
    let bars = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - axis));
    for (hz, label) in LABELS {
        let Some(at) = s.place(hz) else {
            continue;
        };
        let x = bars.left() + at * bars.width();
        painter.vline(
            x,
            bars.y_range(),
            Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.3)),
        );
        painter.text(
            pos2(x, rect.bottom()),
            egui::Align2::CENTER_BOTTOM,
            label,
            font.clone(),
            visuals.weak_text_color(),
        );
    }
    let bands = s.bands();
    if bands.is_empty() {
        return;
    }
    let floor = scope::SPECTRUM_FLOOR_DB;
    let width = bars.width() / bands.len() as f32;
    let colour = Palette::of(visuals).rms;
    for (i, &db) in bands.iter().enumerate() {
        let height = ((db - floor) / -floor).clamp(0.0, 1.0) * bars.height();
        let x = bars.left() + i as f32 * width;
        let bar = Rect::from_min_max(
            pos2(x, bars.bottom() - height),
            pos2(x + (width - 1.0).max(1.0), bars.bottom()),
        );
        painter.rect_filled(bar, 0.0, colour);
    }
}

fn paint_stereo(ui: &egui::Ui, rect: Rect, s: &Scope) {
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    // Square, so the diamond a full-scale signal can reach is not stretched.
    let side = rect.width().min(rect.height());
    let square = Rect::from_center_size(rect.center(), vec2(side, side));
    let at = |(x, y): (f32, f32)| {
        pos2(
            square.center().x + x * side / 2.0,
            square.center().y - y * side / 2.0,
        )
    };
    let guide = Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.4));
    let diamond = [(0.0, 1.0), (1.0, 0.0), (0.0, -1.0), (-1.0, 0.0)].map(at);
    painter.add(egui::Shape::closed_line(diamond.to_vec(), guide));
    painter.line_segment([at((0.0, -1.0)), at((0.0, 1.0))], guide);
    painter.line_segment([at((-0.5, 0.5)), at((0.5, -0.5))], guide);
    painter.line_segment([at((-0.5, -0.5)), at((0.5, 0.5))], guide);
    let colour = Palette::of(visuals).rms.gamma_multiply(0.6);
    for point in s.stereo() {
        painter.rect_filled(
            Rect::from_center_size(at(point), vec2(1.5, 1.5)),
            0.0,
            colour,
        );
    }
    painter.text(
        square.left_top(),
        egui::Align2::LEFT_TOP,
        "L",
        egui::TextStyle::Small.resolve(ui.style()),
        visuals.weak_text_color(),
    );
    painter.text(
        square.right_top(),
        egui::Align2::RIGHT_TOP,
        "R",
        egui::TextStyle::Small.resolve(ui.style()),
        visuals.weak_text_color(),
    );
}

/// Momentary loudness over the last minute, on a scale fixed to the target,
/// with the target dashed and the integrated loudness drawn over the bars.
fn paint_history(ui: &egui::Ui, rect: Rect, s: &Scope) {
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    let y = |lufs: f32| rect.bottom() - s.height_of(lufs) * rect.height();
    // Newest at the right edge, a reading every tenth of a second.
    let width = rect.width() / scope::HISTORY as f32;
    let history = s.history();
    let first = scope::HISTORY - history.len();
    let colour = Palette::of(visuals).rms;
    for (i, level) in history.iter().enumerate() {
        let Some(level) = *level else {
            continue;
        };
        let x = rect.left() + (first + i) as f32 * width;
        let bar = Rect::from_min_max(pos2(x, y(level)), pos2(x + width, rect.bottom()));
        painter.rect_filled(bar, 0.0, colour);
    }
    let font = egui::TextStyle::Small.resolve(ui.style());
    let ground = visuals.extreme_bg_color.gamma_multiply(0.85);
    // A label over the line at `at`, or under it near the top, on the pane's
    // ground so the bars behind do not hide it.
    let label = |x: f32, align: egui::Align2, at: f32, text: String, colour| {
        let galley = painter.layout_no_wrap(text, font.clone(), colour);
        let size = galley.size() + vec2(4.0, 0.0);
        let top = match at - size.y - 1.0 < rect.top() {
            true => at + 1.0,
            false => at - size.y - 1.0,
        };
        let left = match align.x() {
            egui::Align::Max => x - size.x,
            _ => x,
        };
        let panel = Rect::from_min_size(pos2(left, top), size);
        painter.rect_filled(panel, 2.0, ground);
        painter.galley(panel.min + vec2(2.0, 0.0), galley, colour);
    };
    // Yellow, as marks are, so it shows over the bars as well as above them.
    let target_colour = Palette::of(visuals).yellow;
    let target = y(s.target());
    painter.extend(egui::Shape::dashed_line(
        &[pos2(rect.left(), target), pos2(rect.right(), target)],
        Stroke::new(1.5, target_colour),
        6.0,
        4.0,
    ));
    // The target's label at the right and the integrated one at the left, so
    // they do not cover each other when the two are close.
    label(
        rect.right() - 2.0,
        egui::Align2::RIGHT_BOTTOM,
        target,
        format!("target {:.1}", s.target()),
        target_colour,
    );
    if let Some(level) = s.integrated() {
        let at = y(level);
        painter.hline(
            rect.x_range(),
            at,
            Stroke::new(1.5, visuals.strong_text_color()),
        );
        label(
            rect.left() + 2.0,
            egui::Align2::LEFT_BOTTOM,
            at,
            format!("integrated {level:.1}"),
            visuals.strong_text_color(),
        );
    }
}
