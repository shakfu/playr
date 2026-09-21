//! The sampler view: the playing track's waveform, painted a column a point
//! wide, with its region, marks, playhead and planned slice edges.
//!
//! A click seeks, a shift-click marks, a drag sets the range to slice, and
//! the mouse wheel zooms around the playhead. The arrow keys nudge the
//! playhead, as in the terminal. The displays match the terminal's: RMS inside peak on a linear
//! scale, the same on a dB scale, and the waveform's extremes around a centre
//! line, which the terminal draws in Braille and `:display braille` names.

use std::time::Duration;

use eframe::egui;
use playr_app::action::{Action, Slicing, Zoom};
use playr_app::dispatch::Frontend;
use playr_app::model::Model;
use playr_app::sampler::{self, Edge, Layout};
use playr_app::Display;

use crate::controls;
use crate::palette::Palette;

/// Mouse wheel movement, in points, that makes one zoom step.
const WHEEL_STEP: f32 = 40.0;

/// How near a range's edge, in points, a drag moves that edge instead of
/// starting a new range.
const EDGE_REACH: f32 = 5.0;

/// The most points a frame takes at the deepest zoom: 1,000 points show
/// about 60 frames, far enough apart to pick one out.
const POINTS_PER_FRAME: u64 = 16;

/// What the view keeps between frames that the model does not.
pub struct State {
    /// Wheel movement not yet turned into a zoom step.
    wheel: f32,
    /// Where a drag across the waveform started, and where it is now.
    drag: Option<(Duration, Duration)>,
    /// The mark a drag picked up, as the time it sat at when the drag began.
    mark_drag: Option<Duration>,
    /// The count and sensitivity the equal and onset buttons slice with; the
    /// sensitivity starts from the settings.
    slices: usize,
    sensitivity: Option<f32>,
    /// The spectrogram's pixels, replaced each frame it is drawn.
    spectrogram: Option<egui::TextureHandle>,
}

impl Default for State {
    fn default() -> Self {
        State {
            wheel: 0.0,
            drag: None,
            mark_drag: None,
            slices: 8,
            sensitivity: None,
            spectrogram: None,
        }
    }
}

/// Draws the view and returns what its controls ask for. The zoom and
/// columns the layout settled on go back to the model.
pub fn show(model: &mut Model, ui: &mut egui::Ui, state: &mut State) -> Vec<Action> {
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

    // Room below the waveform for the detail line and three rows of controls.
    let height = (ui.available_height() - 124.0).max(80.0);
    let width = ui.available_width();
    let layout = Layout::new(
        peaks,
        width as u64,
        model.sampler().zoom,
        snapshot.position,
        &snapshot.marks,
        POINTS_PER_FRAME,
    )
    .with_range(model.sampler().range_ends(current.as_ref()))
    .with_detail(model.sampler().detail(current.as_ref()));
    model.set_zoom(layout.zoom);
    model.set_scale(layout.columns());

    ui.horizontal(|ui| {
        ui.strong(&name);
        ui.weak(format!("{}  one column {}", layout.shown(), layout.scale()));
        let read = layout
            .detail
            .as_ref()
            .is_some_and(|d| d.covers(layout.start, layout.end()));
        if layout.columns().needs_detail() && !read {
            ui.weak("reading frames");
        }
    });
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click_and_drag());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Track waveform"));
    let pointer = response.interact_pointer_pos().or(response.hover_pos());
    let at = |pos: egui::Pos2| layout.time_at(pos.x - rect.left());
    if response.drag_started() {
        // A drag starts once the pointer has moved a little; it runs from the press.
        let origin = ui.input(|i| i.pointer.press_origin()).or(pointer);
        // A mark under the press is dragged rather than a range drawn. Checked
        // after the range's edges, so an edge sitting on a mark still wins and
        // the drag that was there before this behaves as it did.
        let on_edge = |p: egui::Pos2| {
            let (a, b) = layout.range;
            [a, b].into_iter().flatten().any(|f| {
                layout
                    .column_of(f)
                    .is_some_and(|c| ((rect.left() + c as f32) - p.x).abs() <= EDGE_REACH)
            })
        };
        state.mark_drag = origin.filter(|p| !on_edge(*p)).and_then(|p| {
            layout
                .marks
                .iter()
                .copied()
                .filter(|&m| {
                    layout
                        .column_of(m)
                        .is_some_and(|c| ((rect.left() + c as f32) - p.x).abs() <= EDGE_REACH)
                })
                .min_by_key(|&m| {
                    let c = layout.column_of(m).unwrap_or(0);
                    (((rect.left() + c as f32) - p.x).abs() * 100.0) as i64
                })
                .map(|m| sampler::time_of(m, layout.rate))
        });
        state.drag = origin.filter(|_| state.mark_drag.is_none()).map(|p| {
            // From near an edge it moves that edge, holding the other one.
            let x = |frame: u64| layout.column_of(frame).map(|c| rect.left() + c as f32);
            let near = |frame: Option<u64>| {
                frame.filter(|&f| x(f).is_some_and(|x| (x - p.x).abs() <= EDGE_REACH))
            };
            let time = |frame: u64| sampler::time_of(frame, layout.rate);
            match layout.range {
                (Some(a), Some(b)) if near(Some(a)).is_some() => (time(b), at(p)),
                (Some(a), Some(b)) if near(Some(b)).is_some() => (time(a), at(p)),
                _ => (at(p), at(p)),
            }
        });
    }
    // Kept from the last frame that had a pointer: a release can come
    // outside the view, or with the pointer gone.
    if let (Some(drag), Some(p)) = (state.drag.as_mut(), pointer) {
        drag.1 = at(p);
    }
    let dragged = state.drag;
    let dragging_mark = state.mark_drag.zip(pointer.map(at));
    if response.drag_stopped() {
        if let Some((from, to)) = state.drag.take().filter(|(from, to)| from != to) {
            actions.push(Action::SetRange(Some((from.min(to), from.max(to)))));
        }
        // The cursor picks the mark up, then it moves: `MoveMarkTo` acts on
        // whatever the cursor is on, as the keys do.
        if let Some((from, to)) = state.mark_drag.take().zip(pointer.map(at)) {
            if from != to {
                actions.push(Action::SetCursor(Some(from)));
                actions.push(Action::MoveMarkTo(to));
            }
        }
    }
    paint(
        ui.painter_at(rect),
        ui.visuals(),
        rect,
        &layout,
        display,
        model,
        &mut state.spectrogram,
    );
    // The mark follows the pointer while it is held, so the drop is not a guess.
    if let Some((_, to)) = dragging_mark.filter(|_| response.dragged()) {
        let frame = sampler::frame_of(to, layout.rate);
        let x = rect.left()
            + frame.saturating_sub(layout.start) as f32 * layout.per_frame as f32
                / layout.per_column as f32;
        ui.painter_at(rect).vline(
            x,
            rect.y_range(),
            egui::Stroke::new(1.0, ui.visuals().selection.stroke.color),
        );
    }
    if let Some((from, to)) = dragged.filter(|_| response.dragged()) {
        let x = |t: Duration| {
            let frame = sampler::frame_of(t, layout.rate);
            rect.left()
                + frame.saturating_sub(layout.start) as f32 * layout.per_frame as f32
                    / layout.per_column as f32
        };
        let span = egui::Rect::from_x_y_ranges(x(from.min(to))..=x(from.max(to)), rect.y_range());
        ui.painter_at(rect).rect_filled(
            span,
            0.0,
            ui.visuals().selection.bg_fill.gamma_multiply(0.4),
        );
    }

    if response.hovered() {
        let delta = ui.input(|i| i.smooth_scroll_delta.y);
        state.wheel += delta;
        while state.wheel.abs() >= WHEEL_STEP {
            actions.push(Action::Zoom(if state.wheel > 0.0 {
                Zoom::In
            } else {
                Zoom::Out
            }));
            state.wheel -= WHEEL_STEP.copysign(state.wheel);
        }
        if let Some(pos) = response.hover_pos() {
            let at = layout.time_at(pos.x - rect.left());
            response
                .clone()
                .on_hover_text_at_pointer(playr_app::message::fmt_time(at));
        }
    } else {
        state.wheel = 0.0;
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
    let sampler = model.sampler();
    let ranged = sampler.range(current.as_ref()).is_some();
    let range_ends = sampler.range_ends(current.as_ref());
    let enabled = |action: &Action| match action {
        Action::WriteSlices | Action::DiscardSlices => sampler.pending.is_some(),
        Action::Display(Some(d)) => *d != display,
        Action::SetRange(None) => range_ends != (None, None),
        Action::MoveEdge(_) => match sampler.edge {
            Edge::Start => range_ends.0.is_some(),
            Edge::End => range_ends.1.is_some(),
        },
        _ => true,
    };
    let buttons = |ui: &mut egui::Ui, table: &[controls::Control], actions: &mut Vec<Action>| {
        for control in table {
            let label = match control.action {
                Action::Slice(Slicing::Region) if ranged => "Slice range",
                _ => control.label,
            };
            // A chosen end shows as chosen.
            let clicked = match control.action {
                Action::PickEdge(edge) => {
                    ui.selectable_label(sampler.edge == edge, label).clicked()
                }
                _ => ui
                    .add_enabled(enabled(&control.action), egui::Button::new(label))
                    .clicked(),
            };
            if clicked {
                actions.push(control.action.clone());
            }
        }
    };
    ui.horizontal(|ui| {
        buttons(ui, controls::SAMPLER_BAR, &mut actions);
        ui.separator();
        let mut snap = sampler.snap;
        if ui.checkbox(&mut snap, "Snap to zero").changed() {
            actions.push(Action::Snap(Some(snap)));
        }
    });
    ui.horizontal(|ui| {
        buttons(ui, controls::RANGE_BAR, &mut actions);
        ui.separator();
        let looping = snapshot.status.looping.is_some();
        let mut on = looping;
        let loop_box = egui::Checkbox::new(&mut on, "Loop range");
        if ui.add_enabled(ranged || looping, loop_box).changed() {
            actions.push(Action::Loop(Some(on)));
        }
        ui.separator();
        buttons(ui, controls::EDGE_BAR, &mut actions);
    });
    let sensitivity = state.sensitivity.get_or_insert(model.onset_sensitivity());
    ui.horizontal(|ui| {
        buttons(ui, controls::SLICE_BAR, &mut actions);
        ui.separator();
        ui.add(egui::DragValue::new(&mut state.slices).range(2..=playr_core::samples::MAX_SLICES));
        if ui.button("Equal slices").clicked() {
            actions.push(Action::Slice(Slicing::Equal(state.slices)));
        }
        ui.separator();
        ui.add(egui::Slider::new(sensitivity, 0.0..=1.0).text("Sensitivity"));
        if ui.button("Slice at onsets").clicked() {
            actions.push(Action::Slice(Slicing::Onsets(Some(*sensitivity))));
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
    texture: &mut Option<egui::TextureHandle>,
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

    // A frame a column or finer, once read: a line through each frame's
    // channels' mean, the signal snaps find crossings in, around a zero line.
    let traced = match &layout.detail {
        Some(detail) if display == Display::Braille && layout.per_column == 1 => {
            detail.covers(layout.start, layout.end())
        }
        _ => false,
    };
    if traced {
        let detail = layout.detail.as_ref().expect("traced");
        let centre = rect.center().y;
        painter.hline(
            rect.x_range(),
            centre,
            egui::Stroke::new(1.0, visuals.weak_text_color()),
        );
        let y = |v: f32| centre - (v / layout.loudest).clamp(-1.0, 1.0) * rect.height() / 2.0;
        let points: Vec<(egui::Pos2, egui::Color32)> = (layout.start..layout.end())
            .filter_map(|f| {
                let c = layout.column_of(f)?;
                let inside = f >= layout.region.0 && f < layout.region.1;
                let colour = if inside {
                    colours.rms
                } else {
                    colours.outside_rms
                };
                Some((egui::pos2(x(c), y(detail.mean(f)?)), colour))
            })
            .collect();
        for pair in points.windows(2) {
            painter.line_segment([pair[0].0, pair[1].0], egui::Stroke::new(1.5, pair[1].1));
        }
        if layout.per_frame >= 4 {
            for &(at, colour) in &points {
                painter.circle_filled(at, 2.5, colour);
            }
        }
    }

    if display == Display::Spectrogram {
        spectrogram(&painter, visuals, rect, layout, columns, texture);
    }
    for c in (0..columns).filter(|_| !traced && display != Display::Spectrogram) {
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
            Display::Spectrogram => {}
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
    // The end edge moves shift is drawn thicker.
    let (start, end) = layout.range;
    let chosen = model.sampler().edge;
    for (frame, edge) in [(start, Edge::Start), (end, Edge::End)] {
        if let Some(c) = frame.and_then(|e| layout.column_of(e)) {
            let width = if edge == chosen { 3.0 } else { 1.5 };
            line(c, visuals.selection.stroke.color, width);
        }
    }
    for c in layout.marks.iter().filter_map(|&m| layout.column_of(m)) {
        line(c, colours.yellow, 1.5);
    }
    if let Some(c) = playhead {
        line(c, visuals.strong_text_color(), 2.0);
    }
}

/// Frequencies marked beside the spectrogram.
const GRID_HZ: [(f32, &str); 3] = [(100.0, "100 Hz"), (1000.0, "1 kHz"), (10_000.0, "10 kHz")];

/// Paints the spectrogram of `columns` columns of `layout` into `rect` as one
/// texture, a pixel a column and a row a point, in magma; outside the region,
/// halfway to the ground colour.
fn spectrogram(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: egui::Rect,
    layout: &Layout,
    columns: usize,
    texture: &mut Option<egui::TextureHandle>,
) {
    let ground = visuals.extreme_bg_color;
    let rows = rect.height().max(1.0) as usize;
    let mut pixels = vec![ground; columns * rows];
    for c in 0..columns {
        let dim = if layout.in_region(c) { 0.0 } else { 0.5 };
        // Lowest frequency first, so the bottom row.
        for (r, level) in layout.spectrum(c, rows).into_iter().enumerate() {
            pixels[(rows - 1 - r) * columns + c] =
                crate::palette::magma(level).lerp_to_gamma(ground, dim);
        }
    }
    if columns > 0 {
        let image = egui::ColorImage::new([columns, rows], pixels);
        let options = egui::TextureOptions::NEAREST;
        let texture = match texture {
            Some(t) => {
                t.set(image, options);
                t
            }
            None => texture.insert(painter.ctx().load_texture("spectrogram", image, options)),
        };
        let shown = egui::Rect::from_min_size(rect.min, egui::vec2(columns as f32, rect.height()));
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(texture.id(), shown, uv, egui::Color32::WHITE);
    }
    let spectrum = &layout.peaks.spectrum;
    for (hz, label) in GRID_HZ {
        let Some(h) = spectrum.height_of(hz) else {
            continue;
        };
        let y = rect.bottom() - h * rect.height();
        painter.hline(
            rect.x_range(),
            y,
            egui::Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.4)),
        );
        // On a panel of the ground colour, so it reads over any level.
        let galley = painter.layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(11.0),
            visuals.text_color(),
        );
        let at = egui::pos2(rect.left() + 6.0, y - 2.0 - galley.size().y);
        let panel = egui::Rect::from_min_size(at, galley.size()).expand2(egui::vec2(3.0, 1.0));
        painter.rect_filled(panel, 2.0, ground.gamma_multiply(0.8));
        painter.galley(at, galley, visuals.text_color());
    }
}
