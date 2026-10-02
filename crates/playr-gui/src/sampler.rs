//! The sampler view: the playing track's waveform, painted a column a point
//! wide, with its region, marks, playhead, cursor and planned slice edges.
//!
//! A click seeks, a shift-click marks, a drag sets the range to slice, and
//! the mouse wheel zooms around the playhead, or the range with Fit on. A
//! right-click opens a menu for the point under it. The arrow keys nudge the
//! playhead, as in the terminal. The displays match the terminal's: RMS inside peak on a linear
//! scale, the same on a dB scale, and the waveform's extremes around a centre
//! line, which the terminal draws in Braille and `:display braille` names.
//!
//! Above the waveform, the whole track shows with the stretch in view framed;
//! a click or drag there seeks, which the view follows. A time axis runs
//! along the waveform's top. With Scrub on, a drag across the waveform plays
//! a moment wherever the pointer is, then loops the range it set.
//!
//! Two rows of controls sit under the waveform. The Sampler menu holds every
//! action; `docs/dev/ui-refactor.md` says which get a button.

use std::time::Duration;

use eframe::egui;
use playr_app::action::{Action, Slicing, SlotOp, Zoom};
use playr_app::dispatch::Frontend;
use playr_app::model::{Model, Snapshot};
use playr_app::sampler::{self, Edge, Layout};
use playr_app::{Display, View};
use playr_core::samples::{Cut, Edges};

use crate::controls::{self, Control};
use crate::keys;
use crate::palette::Palette;

/// Mouse wheel movement, in points, that makes one zoom step.
const WHEEL_STEP: f32 = 40.0;

/// How near a range's edge or a mark, in points, a drag moves it instead of
/// starting a new range.
const EDGE_REACH: f32 = 8.0;

/// Space, in points, between the two rows under the waveform.
const GROUP_GAP: f32 = 10.0;

/// The height, in points, of the lane of saved loops under the waveform.
const LANE: f32 = 16.0;

/// The height, in points, of the whole track above the waveform.
const OVERVIEW: f32 = 18.0;

/// The fewest points between the time axis's labelled ticks.
const TICK_GAP: f64 = 80.0;

/// The most points a frame takes at the deepest zoom: 1,000 points show
/// about 60 frames, far enough apart to pick one out.
const POINTS_PER_FRAME: u64 = 16;

/// How the slice row cuts.
#[derive(Clone, Copy, PartialEq)]
enum Method {
    Region,
    Marks,
    Equal,
    Onsets,
}

impl Method {
    const ALL: [Method; 4] = [Method::Region, Method::Marks, Method::Equal, Method::Onsets];

    /// The method that makes `cut`.
    fn of(cut: Cut) -> Method {
        match cut {
            Cut::Region => Method::Region,
            Cut::Marks => Method::Marks,
            Cut::Equal(_) => Method::Equal,
            Cut::Onsets(_) => Method::Onsets,
        }
    }

    /// Its name in the drop-down, as `None` when there is no plan; a set
    /// range is sliced in place of the region.
    fn label(method: Option<Method>, ranged: bool) -> &'static str {
        match method {
            None => "None",
            Some(Method::Region) if ranged => "Range",
            Some(Method::Region) => "Region",
            Some(Method::Marks) => "At marks",
            Some(Method::Equal) => "Equal",
            Some(Method::Onsets) => "At onsets",
        }
    }
}

/// What the view keeps between frames that the model does not.
pub struct State {
    /// Wheel movement not yet turned into a zoom step.
    wheel: f32,
    /// Where a drag across the waveform started, and where it is now.
    drag: Option<(Duration, Duration)>,
    /// The end of the range a drag moves, when it started on one.
    edge_drag: Option<Edge>,
    /// The first frame shown when a drag began, held until it ends: a view
    /// moving under the pointer, as fitting a picked end or following the
    /// playhead moves it, would move what is dragged by as much.
    held_start: Option<u64>,
    /// The mark a drag picked up, as the time it sat at when the drag began.
    mark_drag: Option<Duration>,
    /// The time a right-click landed on and the mark within reach of it, for
    /// the menu it opened.
    menu: Option<(Duration, Option<Duration>)>,
    /// The method last chosen in the slice row, shown while its plan is being
    /// made, and the count and sensitivity equal and onset slices take; the
    /// sensitivity starts from the settings.
    method: Option<Method>,
    slices: usize,
    sensitivity: Option<f32>,
    /// The height the view took besides the waveform in the last frame: its
    /// header, the loops, the detail line and the rows of controls.
    around: Option<f32>,
    /// The spectrogram's pixels, replaced each frame it is drawn.
    spectrogram: Option<egui::TextureHandle>,
    /// Whether a drag across the waveform scrubs, then loops its range. The
    /// window's alone: only a pointer drags.
    scrub: bool,
    /// When the last scrub was sent, in egui's seconds, and from where.
    grain: Option<(f64, Duration)>,
    /// Where a click or drag on the overview last sought, in points, so a
    /// pointer held still does not seek again each frame.
    sought: Option<f32>,
}

impl Default for State {
    fn default() -> Self {
        State {
            wheel: 0.0,
            drag: None,
            edge_drag: None,
            held_start: None,
            mark_drag: None,
            menu: None,
            method: None,
            slices: 8,
            sensitivity: None,
            around: None,
            spectrogram: None,
            scrub: false,
            grain: None,
            sought: None,
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

    // The waveform takes the height the rest of the view left last frame;
    // 87 points is a first guess, which the frame measures and corrects.
    let top = ui.cursor().top();
    let guess = 87.0 + GROUP_GAP + OVERVIEW;
    let height = (ui.available_height() - state.around.unwrap_or(guess)).max(80.0);
    let width = ui.available_width();
    let mut layout = Layout::new(
        peaks,
        width as u64,
        model.sampler().zoom,
        snapshot.position,
        &snapshot.marks,
        POINTS_PER_FRAME,
    )
    .with_centre(model.sampler().centre(current.as_ref()))
    .with_range(model.sampler().range_ends(current.as_ref()))
    .with_detail(model.sampler().detail(current.as_ref()));
    if state.drag.is_none() && state.mark_drag.is_none() {
        state.held_start = None;
    }
    if let Some(start) = state.held_start {
        layout.start = start;
    }
    model.set_zoom(layout.zoom);
    model.set_scale(layout.columns());

    // The deepest zoom this track and width allow, for the zoom slider.
    let deepest = sampler::window(
        layout.peaks.frames,
        layout.columns,
        u32::MAX,
        0,
        POINTS_PER_FRAME,
    )
    .3;
    let mut zoom = layout.zoom;
    // The buttons go first, so the name gets what is left and truncates.
    egui::Sides::new().shrink_left().truncate().show(
        ui,
        |ui| {
            ui.strong(&name);
            ui.weak(format!("{}  one column {}", layout.shown(), layout.scale()));
            let read = layout
                .detail
                .as_ref()
                .is_some_and(|d| d.covers(layout.start, layout.end()));
            if layout.columns().needs_detail() && !read {
                ui.weak("reading frames");
            }
        },
        // Right to left.
        |ui| {
            let shown = Action::Display(Some(display));
            let label = controls::DISPLAYS
                .iter()
                .find(|c| c.action == shown)
                .map_or("", |c| c.label);
            let combo = egui::ComboBox::from_id_salt("display")
                .width(90.0)
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for control in controls::DISPLAYS {
                        let chosen = control.action == shown;
                        if ui.selectable_label(chosen, control.label).clicked() && !chosen {
                            actions.push(control.action.clone());
                        }
                    }
                });
            name_combo(&combo.response, "Display", label);
            for control in controls::SAMPLER_HEADER.iter().rev() {
                let text = match control.action {
                    Action::Zoom(Zoom::In) => "+",
                    Action::Zoom(Zoom::Out) => "-",
                    Action::Zoom(Zoom::All) => "\u{2194}",
                    _ => "info",
                };
                // Right to left, so between `-` and `+`.
                if control.action == Action::Zoom(Zoom::Out) {
                    ui.spacing_mut().slider_width = 80.0;
                    let slider = egui::Slider::new(&mut zoom, 0..=deepest).show_value(false);
                    ui.add(slider)
                        .widget_info(|| egui::WidgetInfo::slider(true, f64::from(zoom), "Zoom"));
                }
                if named(ui, model, text, control) {
                    actions.push(control.action.clone());
                }
            }
        },
    );
    // Takes effect next frame, as the zoom buttons' does.
    if zoom != layout.zoom {
        model.set_zoom(zoom);
    }
    overview(ui, &layout, &mut state.sought, &mut actions);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click_and_drag());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Track waveform"));
    let pointer = response.interact_pointer_pos().or(response.hover_pos());
    let at = |pos: egui::Pos2| layout.time_at(pos.x - rect.left());
    // The end of a set range within reach of `p`, the nearer if both are.
    let edge_at = |p: egui::Pos2| {
        let (Some(a), Some(b)) = layout.range else {
            return None;
        };
        let reach = |frame: u64| {
            layout
                .column_of(frame)
                .map(|c| ((rect.left() + c as f32) - p.x).abs())
                .filter(|&d| d <= EDGE_REACH)
        };
        match (reach(a), reach(b)) {
            (Some(da), Some(db)) if db < da => Some(Edge::End),
            (Some(_), _) => Some(Edge::Start),
            (None, Some(_)) => Some(Edge::End),
            (None, None) => None,
        }
    };
    // The mark within reach of `p`, the nearest if several are, as its time.
    let mark_at = |p: egui::Pos2| {
        let reach = |mark: u64| {
            layout
                .column_of(mark)
                .map(|c| ((rect.left() + c as f32) - p.x).abs())
                .filter(|&d| d <= EDGE_REACH)
        };
        layout
            .marks
            .iter()
            .filter_map(|&mark| reach(mark).map(|d| (mark, d)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(mark, _)| sampler::time_of(mark, layout.rate))
    };
    if response.drag_started() {
        state.held_start = Some(layout.start);
        // A drag starts once the pointer has moved a little; it runs from the press.
        let origin = ui.input(|i| i.pointer.press_origin()).or(pointer);
        // A mark under the press is dragged rather than a range drawn. Checked
        // after the range's edges, so an edge sitting on a mark still wins and
        // the drag that was there before this behaves as it did.
        state.edge_drag = origin.and_then(edge_at);
        if let Some(edge) = state.edge_drag {
            // As `[` or `]`: the keys that move an end go on with this one.
            actions.push(Action::PickEdge(edge));
        }
        state.mark_drag = origin
            .filter(|_| state.edge_drag.is_none())
            .and_then(mark_at);
        state.drag = origin.filter(|_| state.mark_drag.is_none()).map(|p| {
            // From an edge it moves that edge, holding the other one.
            let time = |frame: u64| sampler::time_of(frame, layout.rate);
            match (state.edge_drag, layout.range) {
                (Some(Edge::Start), (_, Some(b))) => (time(b), at(p)),
                (Some(Edge::End), (Some(a), _)) => (time(a), at(p)),
                _ => (at(p), at(p)),
            }
        });
    }
    // Kept from the last frame that had a pointer: a release can come
    // outside the view, or with the pointer gone.
    if let (Some(drag), Some(p)) = (state.drag.as_mut(), pointer) {
        drag.1 = at(p);
    }
    // A scrub a grain apart, from wherever the pointer moved to since the last.
    if let Some((_, to)) = state.drag.filter(|_| state.scrub) {
        let now = ui.input(|i| i.time);
        let grain = sampler::SCRUB.as_secs_f64();
        match state.grain {
            Some((_, from)) if from == to => {}
            // A frame when this grain ends, so the pointer's last move is
            // heard even if it then holds still.
            Some((sent, _)) if now - sent < grain => ui
                .ctx()
                .request_repaint_after(Duration::from_secs_f64(grain - (now - sent))),
            _ => {
                actions.push(Action::Scrub(to));
                state.grain = Some((now, to));
            }
        }
    }
    // An edge that can be dragged shows it, before and while it is.
    let over_edge = response
        .hover_pos()
        .filter(|_| state.drag.is_none())
        .and_then(edge_at);
    if state.edge_drag.is_some() || over_edge.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    let dragged = state.drag;
    let dragging_mark = state.mark_drag.zip(pointer.map(at));
    if response.drag_stopped() {
        state.edge_drag = None;
        state.grain = None;
        if let Some((from, to)) = state.drag.take().filter(|(from, to)| from != to) {
            actions.push(Action::SetRange(Some((from.min(to), from.max(to)))));
            // A scrub leaves no loop running, so the release starts one.
            if state.scrub {
                actions.push(Action::Loop(Some(true)));
            }
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
    axis(&ui.painter_at(rect), ui.visuals(), rect, &layout);
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

    if response.secondary_clicked() {
        state.menu = response.interact_pointer_pos().map(|p| (at(p), mark_at(p)));
    }
    response.context_menu(|ui| {
        let Some((time, mark)) = state.menu else {
            return;
        };
        let ends = |frame: Option<u64>| frame.map(|f| sampler::time_of(f, layout.rate));
        let (start, end) = (ends(layout.range.0), ends(layout.range.1));
        // The other end holds while it is on the right side of this one.
        let end = end
            .filter(|e| *e > time)
            .or(snapshot.status.duration)
            .unwrap_or_default();
        let start = start.filter(|s| *s < time).unwrap_or_default();
        let mut chosen = Vec::new();
        if ui.button("Mark here").clicked() {
            chosen.push(Action::MarkAt(time));
        }
        if ui
            .add_enabled(time < end, egui::Button::new("Range starts here"))
            .clicked()
        {
            chosen.push(Action::SetRange(Some((time, end))));
        }
        if ui
            .add_enabled(start < time, egui::Button::new("Range ends here"))
            .clicked()
        {
            chosen.push(Action::SetRange(Some((start, time))));
        }
        let clear = egui::Button::new("Clear range");
        if ui
            .add_enabled(layout.range != (None, None), clear)
            .clicked()
        {
            chosen.push(Action::SetRange(None));
        }
        if let Some(mark) = mark {
            ui.separator();
            for control in controls::MARK_ROW {
                if ui.button(control.label).clicked() {
                    // The cursor picks the mark, then the action edits it.
                    chosen.push(Action::SetCursor(Some(mark)));
                    chosen.push(control.action.clone());
                }
            }
        }
        if !chosen.is_empty() {
            actions.append(&mut chosen);
            ui.close();
        }
    });

    let sampler = model.sampler();
    let range = sampler.range(current.as_ref());
    let ranged = range.is_some();
    if snapshot.loops.iter().any(Option::is_some) {
        loop_lane(ui, &layout, &snapshot, range, &mut actions);
    }
    ui.horizontal(|ui| {
        let plan = sampler::plan_text(sampler);
        if !plan.is_empty() {
            ui.colored_label(Palette::of(ui.visuals()).edge, plan);
        }
        ui.weak(layout.region_text());
    });
    ui.horizontal(|ui| {
        for control in controls::SAMPLER_BAR {
            let enabled = match control.action {
                Action::SetRange(None) => layout.range != (None, None),
                _ => true,
            };
            let button = ui.add_enabled(enabled, egui::Button::new(control.label));
            if button.on_hover_text(tip(model, control)).clicked() {
                actions.push(control.action.clone());
            }
        }
        // With no range, Loop loops the region around the playhead.
        let mut looping = snapshot.status.looping.is_some();
        if toggle(
            ui,
            model,
            &mut looping,
            "Loop",
            "Loop range",
            Action::Loop(None),
        ) {
            actions.push(Action::Loop(Some(looping)));
        }
        let scrub = egui::Button::new("Scrub").selected(state.scrub);
        if ui
            .add(scrub)
            .on_hover_text("Drag across the waveform to hear it, then loop the range")
            .clicked()
        {
            state.scrub ^= true;
        }
        let empty = snapshot.loops.iter().position(Option::is_none);
        let save = ui
            .add_enabled(ranged && empty.is_some(), egui::Button::new("Save loop"))
            .on_hover_text("Save the range as a loop, in the first empty slot")
            .on_disabled_hover_text(match ranged {
                true => "Every loop is in use. Shift-click one to save the range over it.",
                false => "Set a range to save it as a loop.",
            });
        if let (true, Some(slot)) = (save.clicked(), empty) {
            actions.push(Action::LoopSlot(slot as u8 + 1, SlotOp::Save));
        }
        ui.separator();
        let mut snap = sampler.snap;
        if toggle(
            ui,
            model,
            &mut snap,
            "Snap",
            "Snap to zero",
            Action::Snap(None),
        ) {
            actions.push(Action::Snap(Some(snap)));
        }
        let mut fit = sampler.fit;
        if toggle(ui, model, &mut fit, "Fit", "Fit range", Action::Fit(None)) {
            actions.push(Action::Fit(Some(fit)));
        }
    });
    ui.add_space(GROUP_GAP);
    let sensitivity = state.sensitivity.get_or_insert(model.onset_sensitivity());
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = 90.0;
        ui.label("Slice");
        // The drop-down shows the plan that waits, however it was made, and
        // takes its count or sensitivity once the plan has landed.
        let planning = sampler.planning.is_some();
        let cut = sampler.pending.as_ref().map(|p| p.job.cut);
        match cut.filter(|_| !planning) {
            Some(Cut::Equal(count)) => state.slices = count,
            Some(Cut::Onsets(s)) => *sensitivity = s,
            _ => {}
        }
        let method = cut.map(Method::of).or(state.method.filter(|_| planning));
        // Choosing a method plans with it, again if it is the one shown;
        // choosing None discards the plan.
        let mut chosen = None;
        let combo = egui::ComboBox::from_id_salt("slice method")
            .width(80.0)
            .selected_text(Method::label(method, ranged))
            .show_ui(ui, |ui| {
                for choice in std::iter::once(None).chain(Method::ALL.map(Some)) {
                    let label = Method::label(choice, ranged);
                    if ui.selectable_label(method == choice, label).clicked() {
                        chosen = Some(choice);
                    }
                }
            });
        name_combo(
            &combo.response,
            "Slice method",
            Method::label(method, ranged),
        );
        combo.response.on_hover_text(
            "Draws the cuts on the waveform; no file is written yet. \
             Choose the method again to plan again.",
        );
        // A changed count or sensitivity plans again, so the edges follow it.
        let changed = match method {
            Some(Method::Equal) => {
                let count = egui::DragValue::new(&mut state.slices)
                    .range(2..=playr_core::samples::MAX_SLICES);
                ui.add(count).on_hover_text("Slices").changed()
            }
            Some(Method::Onsets) => ui
                .add(egui::Slider::new(sensitivity, 0.0..=1.0).text("Sensitivity"))
                .changed(),
            _ => false,
        };
        match chosen.unwrap_or(method.filter(|_| changed)) {
            Some(method) => {
                state.method = Some(method);
                actions.push(Action::Slice(match method {
                    Method::Region => Slicing::Region,
                    Method::Marks => Slicing::Marks,
                    Method::Equal => Slicing::Equal(state.slices),
                    Method::Onsets => Slicing::Onsets(Some(*sensitivity)),
                }));
            }
            None if chosen.is_some() => {
                state.method = None;
                if cut.is_some() {
                    actions.push(Action::DiscardSlices);
                }
            }
            None => {}
        }
        // A plan waits: hear its slices, choose their edges, write or discard.
        if sampler.pending.is_none() {
            return;
        }
        ui.separator();
        let (audition, write) = controls::PLAN_BAR.split_at(2);
        for (control, text) in audition.iter().zip(["<", ">"]) {
            if named(ui, model, text, control) {
                actions.push(control.action.clone());
            }
        }
        let edges = model.session().slice_edges();
        let combo = egui::ComboBox::from_label("Edges")
            .width(60.0)
            .selected_text(edges.name())
            .show_ui(ui, |ui| {
                for (_, choice) in Edges::NAMES {
                    if ui
                        .selectable_label(edges == choice, choice.name())
                        .clicked()
                        && edges != choice
                    {
                        actions.push(Action::SetSliceEdges(choice));
                    }
                }
            });
        if edges == Edges::Fade {
            let fades = model.session().fades();
            let ms = |d: Duration| d.as_secs_f64() * 1000.0;
            combo.response.on_hover_text(format!(
                "{} ms in, {} ms out",
                ms(fades.fade_in),
                ms(fades.fade_out)
            ));
        }
        for (control, text) in write.iter().zip(["Write", "Discard"]) {
            if named(ui, model, text, control) {
                actions.push(control.action.clone());
            }
        }
    });
    let around = ui.cursor().top() - top - height;
    if state.around.is_none_or(|a| (a - around).abs() > 0.5) {
        state.around = Some(around);
        ui.ctx().request_discard("sampler controls height");
    }
    actions
}

/// `label`, with the key that performs `action` in the sampler view.
fn with_key(model: &Model, label: &str, action: &Action) -> String {
    match keys::key_for(model, action, View::Sampler) {
        Some(key) => format!("{label} ({key})"),
        None => label.to_string(),
    }
}

/// A control's hover text: its name and its key.
fn tip(model: &Model, control: &Control) -> String {
    with_key(model, control.label, &control.action)
}

/// A button showing `text` that keeps the control's name, for a symbol or a
/// shorter word. Returns whether it was clicked.
fn named(ui: &mut egui::Ui, model: &Model, text: &str, control: &Control) -> bool {
    let response = ui.button(text);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, control.label));
    response.on_hover_text(tip(model, control)).clicked()
}

/// A button that stays pressed while `on`; `name` and the key of `action`,
/// which toggles it, are its hover text. Returns whether it changed.
fn toggle(
    ui: &mut egui::Ui,
    model: &Model,
    on: &mut bool,
    label: &str,
    name: &str,
    action: Action,
) -> bool {
    // Framed, unlike `toggle_value`, so it reads as a button while off.
    let clicked = ui
        .add(egui::Button::new(label).selected(*on))
        .on_hover_text(with_key(model, name, &action))
        .clicked();
    *on ^= clicked;
    clicked
}

/// Names a drop-down that shows no label, for a screen reader and the tests.
fn name_combo(response: &egui::Response, name: &str, chosen: &str) {
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, name);
        info.current_text_value = Some(chosen.to_string());
        info
    });
}

/// The saved loops, each a band under the stretch of waveform it spans: a
/// click loops it, a shift-click saves the range over it, and its menu saves
/// or clears it. A loop outside the frames shown is not drawn.
fn loop_lane(
    ui: &mut egui::Ui,
    layout: &Layout,
    snapshot: &Snapshot,
    range: Option<(u64, u64)>,
    actions: &mut Vec<Action>,
) {
    let (lane, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), LANE), egui::Sense::hover());
    let x_of = |frame: u64| {
        let x = frame.saturating_sub(layout.start) as f32 * layout.per_frame as f32
            / layout.per_column as f32;
        lane.left() + x.min(lane.width())
    };
    let rate = snapshot.status.source.map_or(1, |s| s.rate);
    for (i, saved) in snapshot.loops.iter().enumerate() {
        let Some((a, b)) = *saved else {
            continue;
        };
        if b < layout.start || a > layout.end() {
            continue;
        }
        let slot = i as u8 + 1;
        // Wide enough for its number, however short the loop.
        let left = x_of(a).min(lane.right() - LANE);
        let band = egui::Rect::from_x_y_ranges(left..=x_of(b).max(left + LANE), lane.y_range());
        let playing = *saved == range;
        let response = ui.interact(band, ui.id().with(("loop", slot)), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                true,
                playing,
                format!("Loop {slot}"),
            )
        });
        let visuals = ui.style().interact_selectable(&response, playing);
        ui.painter()
            .rect_filled(band.shrink(1.0), 2.0, visuals.weak_bg_fill);
        ui.painter().text(
            band.left_center() + egui::vec2(4.0, 0.0),
            egui::Align2::LEFT_CENTER,
            slot,
            egui::TextStyle::Small.resolve(ui.style()),
            visuals.text_color(),
        );
        let response = response.on_hover_text(format!(
            "Loop {slot}: {}-{}. Click to loop it; shift-click to save the range over it.",
            sampler::fmt_frames(a, rate),
            sampler::fmt_frames(b, rate)
        ));
        if response.clicked() {
            let shift = ui.input(|i| i.modifiers.shift);
            actions.push(Action::LoopSlot(
                slot,
                if shift { SlotOp::Save } else { SlotOp::Use },
            ));
        }
        response.context_menu(|ui| {
            if ui.button("Save the range here").clicked() {
                actions.push(Action::LoopSlot(slot, SlotOp::Save));
                ui.close();
            }
            if ui.button("Clear").clicked() {
                actions.push(Action::LoopSlot(slot, SlotOp::Clear));
                ui.close();
            }
        });
    }
}

/// An entry of the Sampler menu, with its key. Returns whether it was chosen.
fn item(ui: &mut egui::Ui, model: &Model, control: &Control, enabled: bool, on: bool) -> bool {
    let mut button = egui::Button::selectable(on, control.label);
    if let Some(key) = keys::key_for(model, &control.action, View::Sampler) {
        button = button.shortcut_text(key.to_string());
    }
    let response = ui.add_enabled(enabled, button);
    // Without the key, which the button's own name would include.
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, control.label)
    });
    response.clicked()
}

/// The Sampler menu: every action of the view, with the key that performs it
/// there. Slicing works in every view; the rest needs the sampler showing.
pub fn menu(model: &Model, ui: &mut egui::Ui) -> Option<Action> {
    let snapshot = model.snapshot();
    let sampler = model.sampler();
    let here = model.view() == View::Sampler;
    let pending = sampler.pending.is_some();
    let ranged = sampler.range_ends(snapshot.status.current()) != (None, None);
    let mut chosen = None;
    let mut items = |ui: &mut egui::Ui, table: &[Control]| {
        for control in table {
            let enabled = match control.action {
                Action::Slice(_) => true,
                Action::AuditionSlice(_) | Action::WriteSlices | Action::DiscardSlices => {
                    here && pending
                }
                Action::SetRange(None) => here && ranged,
                Action::ClearLoops => here && snapshot.loops.iter().any(Option::is_some),
                _ => here,
            };
            let on = match control.action {
                Action::Display(Some(display)) => display == sampler.display,
                Action::PickEdge(edge) => edge == sampler.edge,
                _ => false,
            };
            if item(ui, model, control, enabled, on) {
                chosen = Some(control.action.clone());
                ui.close();
            }
        }
    };
    items(ui, controls::SAMPLER_HEADER);
    ui.menu_button("Display", |ui| items(ui, controls::DISPLAYS));
    ui.separator();
    items(ui, controls::SAMPLER_BAR);
    ui.menu_button("Range", |ui| items(ui, controls::RANGE_MENU));
    ui.menu_button("Marks", |ui| items(ui, controls::MARK_MENU));
    // Chosen outside `items`: the slices' edges, a loop's entry, or a toggle.
    let mut other = None;
    ui.menu_button("Slice", |ui| {
        items(ui, controls::SLICE_MENU);
        // Set here too: outside the sampler view a slice is written at once.
        let edges = model.session().slice_edges();
        ui.menu_button("Edges", |ui| {
            for (_, choice) in Edges::NAMES {
                if ui.radio(edges == choice, choice.name()).clicked() {
                    other = Some(Action::SetSliceEdges(choice));
                    ui.close();
                }
            }
        });
        ui.separator();
        items(ui, controls::PLAN_BAR);
        // An extension, shown once enabled in the settings: the slices last
        // written, or an export chosen in a dialog, by ConvertWithMoss, into
        // a directory inside the export named after the format.
        let Some(program) = model.session().convertwithmoss() else {
            return;
        };
        ui.separator();
        if program.is_file() {
            ui.menu_button("Convert to", |ui| {
                for format in playr_core::convertwithmoss::FORMATS {
                    if ui.button(format).clicked() {
                        other = Some(Action::Convert(format.to_string(), None));
                        ui.close();
                    }
                }
            });
            ui.menu_button("Convert an export to", |ui| {
                for format in playr_core::convertwithmoss::FORMATS {
                    if ui.button(format).clicked() {
                        ui.close();
                        other = rfd::FileDialog::new()
                            .set_title("Choose an export")
                            .set_directory(model.session().samples_dir())
                            .pick_folder()
                            .map(|dir| Action::Convert(format.to_string(), Some(dir)));
                    }
                }
            });
        } else {
            for label in ["Convert to", "Convert an export to"] {
                ui.add_enabled(false, egui::Button::new(label))
                    .on_disabled_hover_text(playr_core::convertwithmoss::not_installed(program));
            }
        }
    });
    ui.add_enabled_ui(here, |ui| {
        ui.menu_button("Loops", |ui| {
            let rate = snapshot.status.source.map_or(1, |s| s.rate);
            for (i, saved) in snapshot.loops.iter().enumerate() {
                let slot = i as u8 + 1;
                let title = match saved {
                    Some((a, b)) => format!(
                        "Loop {slot}  {}-{}",
                        sampler::fmt_frames(*a, rate),
                        sampler::fmt_frames(*b, rate)
                    ),
                    None => format!("Loop {slot}  empty"),
                };
                ui.menu_button(title, |ui| {
                    let ops = [
                        ("Loop it", SlotOp::Use, saved.is_some()),
                        ("Save the range here", SlotOp::Save, true),
                        ("Clear", SlotOp::Clear, saved.is_some()),
                    ];
                    for (label, op, enabled) in ops {
                        let action = Action::LoopSlot(slot, op);
                        if item(ui, model, &Control { label, action }, enabled, false) {
                            other = Some(Action::LoopSlot(slot, op));
                            ui.close();
                        }
                    }
                });
            }
            ui.separator();
            items(ui, controls::LOOP_MENU);
        });
    });
    ui.separator();
    let toggles = [
        ("Snap to zero", Action::Snap(None), sampler.snap),
        ("Fit range", Action::Fit(None), sampler.fit),
        (
            "Loop range",
            Action::Loop(None),
            snapshot.status.looping.is_some(),
        ),
    ];
    for (label, action, on) in toggles {
        let control = Control { label, action };
        if item(ui, model, &control, here, on) {
            other = Some(control.action);
            ui.close();
        }
    }
    chosen.or(other)
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
    // A playhead out of view, as while fitting a range, points the way to it.
    if let Some(side) = layout.playhead_off() {
        let (x, dx) = match side {
            std::cmp::Ordering::Less => (rect.left() + 2.0, 10.0),
            _ => (rect.right() - 2.0, -10.0),
        };
        let y = rect.center().y;
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(x, y),
                egui::pos2(x + dx, y - 8.0),
                egui::pos2(x + dx, y + 8.0),
            ],
            visuals.strong_text_color(),
            egui::Stroke::NONE,
        ));
    }
    // Last, as in the terminal: the mark keys act on it. Dashed, so a mark or
    // range end under it still shows.
    if let Some(c) = model.sampler().cursor.and_then(|f| layout.column_of(f)) {
        painter.extend(egui::Shape::dashed_line(
            &[
                egui::pos2(x(c), rect.top()),
                egui::pos2(x(c), rect.bottom()),
            ],
            egui::Stroke::new(2.0, visuals.selection.stroke.color),
            6.0,
            4.0,
        ));
    }
}

/// The whole track in a strip, the stretch the waveform shows framed, with
/// the range, marks and playhead. A click or drag seeks.
fn overview(
    ui: &mut egui::Ui,
    layout: &Layout,
    sought: &mut Option<f32>,
    actions: &mut Vec<Action>,
) {
    let size = egui::vec2(ui.available_width(), OVERVIEW);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Track overview"));
    let frames = layout.peaks.frames.max(1);
    let x_of = |f: u64| rect.left() + f.min(frames) as f32 / frames as f32 * rect.width();
    match response.interact_pointer_pos() {
        Some(p) if (response.clicked() || response.dragged()) && *sought != Some(p.x) => {
            *sought = Some(p.x);
            let along = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let frame = (along as f64 * frames as f64) as u64;
            actions.push(Action::SeekTo(sampler::time_of(frame, layout.rate)));
        }
        Some(_) => {}
        None => *sought = None,
    }

    let visuals = ui.visuals();
    let colours = Palette::of(visuals);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
    let shown = egui::Rect::from_x_y_ranges(
        x_of(layout.start)..=x_of(layout.end()).max(x_of(layout.start) + 2.0),
        rect.y_range(),
    );
    painter.rect_filled(shown, 0.0, colours.region);
    if let (Some(a), Some(b)) = layout.range {
        let span = egui::Rect::from_x_y_ranges(x_of(a)..=x_of(b), rect.y_range());
        painter.rect_filled(span, 0.0, visuals.selection.bg_fill.gamma_multiply(0.4));
    }
    // A peak a point, mirrored about the middle.
    let whole = Layout::new(
        layout.peaks.clone(),
        rect.width() as u64,
        0,
        Duration::ZERO,
        &[],
        1,
    );
    for c in 0..(whole.columns as usize).min(rect.width() as usize) {
        let (_, peak) = whole.heights(Display::Envelope, c);
        let half = peak.clamp(0.0, 1.0) * rect.height() / 2.0;
        if half > 0.0 {
            let x = rect.left() + c as f32 + 0.5;
            painter.vline(
                x,
                rect.center().y - half..=rect.center().y + half,
                egui::Stroke::new(1.0, colours.outside_rms),
            );
        }
    }
    let line = |frame: u64, colour, width| {
        painter.vline(
            x_of(frame),
            rect.y_range(),
            egui::Stroke::new(width, colour),
        );
    };
    for &mark in &layout.marks {
        line(mark, colours.yellow, 1.0);
    }
    line(layout.at, visuals.strong_text_color(), 1.5);
    painter.rect_stroke(
        shown,
        2.0,
        egui::Stroke::new(1.5, visuals.selection.stroke.color),
        egui::StrokeKind::Inside,
    );
}

/// Ticks down from the top of `rect` at times a round step apart, labelled.
fn axis(painter: &egui::Painter, visuals: &egui::Visuals, rect: egui::Rect, layout: &Layout) {
    let rate = f64::from(layout.rate);
    let per_point = layout.per_column as f64 / layout.per_frame as f64 / rate;
    let step = sampler::tick_step(per_point, TICK_GAP);
    let start = layout.start as f64 / rate;
    let x = |t: f64| rect.left() + ((t - start) / per_point) as f32;
    let stroke = egui::Stroke::new(1.0, visuals.weak_text_color());
    // Halves between the labelled ticks, shorter.
    let first = (start / step * 2.0).ceil() as u64;
    for k in first.. {
        let t = k as f64 * step / 2.0;
        let at = x(t);
        if at > rect.right() {
            break;
        }
        if k % 2 == 1 {
            painter.vline(at, rect.top()..=rect.top() + 3.0, stroke);
            continue;
        }
        painter.vline(at, rect.top()..=rect.top() + 6.0, stroke);
        let galley = painter.layout_no_wrap(
            sampler::fmt_tick(t, step),
            egui::FontId::proportional(10.0),
            visuals.text_color(),
        );
        let pos = egui::pos2(at + 3.0, rect.top() + 1.0);
        if pos.x + galley.size().x > rect.right() {
            continue;
        }
        // On a panel of the ground colour, so it reads over the spectrogram.
        let panel = egui::Rect::from_min_size(pos, galley.size()).expand2(egui::vec2(2.0, 0.0));
        painter.rect_filled(panel, 2.0, visuals.extreme_bg_color.gamma_multiply(0.8));
        painter.galley(pos, galley, visuals.text_color());
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
