//! The Tape tab: the looper's loop, each voice's window and head, the write
//! window and head, and a row of controls each. The terminal has the `:tape`
//! commands only; see `docs/dev/looper-engine.md`.

use eframe::egui;
use playr_app::action::Action;
use playr_app::message::idle_text;
use playr_app::model::Model;
use playr_app::tape::{
    no_effect, Control as C, Extent, Pos, TapeAction as T, TapeState, VoiceSetting as V,
};
use playr_looper::{Window, COLUMNS, VOICES};

use crate::controls;
use crate::palette::Palette;

/// The write window's strip above the waveform, the waveform's height, and
/// the lane under it for each voice's window.
const STRIP: f32 = 12.0;
const WAVE: f32 = 98.0;
const LANE: f32 = 12.0;
/// Points along each drawn crossfade curve.
const CURVE: usize = 16;
/// Narrow enough for every row to fit 800 points.
const SLIDER: f32 = 54.0;

/// How near an edge, in points, a press grabs it, and a dragged edge or
/// window snaps to the range's edges.
const GRAB: f32 = 6.0;

/// What the tab keeps between frames: the voice whose window the waveform
/// shows and edits, and a window being dragged.
#[derive(Debug, Default)]
pub struct State {
    selected: usize,
    drag: Option<Drag>,
}

/// A drag on the waveform: whose window, which part of it, the window as it
/// was when the drag began, and the frame first pressed.
#[derive(Debug, Clone, Copy)]
struct Drag {
    target: Target,
    part: Part,
    from: Window,
    grab: f64,
}

/// A voice's window, in its lane, or the write window, on the waveform.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    Voice(usize),
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Part {
    Start,
    End,
    Body,
}

/// Draws the tab and returns what its controls ask for.
pub fn show(model: &Model, ui: &mut egui::Ui, tab: &mut State) -> Vec<Action> {
    let mut actions = Vec::new();
    let deck = model.deck();
    let frames = deck.loaded().map(|e| e.frames);
    let extent = deck.loaded().unwrap_or(Extent {
        frames: 0,
        range: Window::new(0, 0),
        rate: 1,
    });
    let state = deck
        .state()
        .copied()
        .unwrap_or(TapeState::new(extent.range));

    ui.horizontal(|ui| {
        for c in controls::TAPE_BAR {
            let (enabled, label) = match c.action {
                Action::Tape(T::Load(_)) => (!deck.loading(), c.label),
                Action::Tape(T::Save) => (frames.is_some() && !deck.saving(), c.label),
                Action::Tape(T::Record) if deck.recording() => (true, "Stop recording"),
                _ => (frames.is_some(), c.label),
            };
            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                actions.push(c.action.clone());
            }
        }
    });
    waveform(ui, model, &state, extent, tab, &mut actions);
    if frames.is_none() {
        ui.weak(match deck.loading() {
            true => "Reading the range.",
            false => "Set a range in the sampler while a track plays, then Load range.",
        });
    }
    ui.add_enabled_ui(frames.is_some(), |ui| {
        ui.spacing_mut().slider_width = SLIDER;
        rows(ui, &state, extent, tab, &mut actions);
    });
    actions
}

/// The colour of voice `i`, and of the write head.
fn colours(p: &Palette) -> ([egui::Color32; VOICES], egui::Color32) {
    ([p.green, p.yellow, p.edge], p.red)
}

/// The write window in a strip on top; the loop's peaks, with the selected
/// voice's window shaded and its edges marked, and the pre-roll and
/// post-roll dimmed; each voice's window in a lane below; every head as a line.
fn waveform(
    ui: &mut egui::Ui,
    model: &Model,
    state: &TapeState,
    extent: Extent,
    tab: &mut State,
    actions: &mut Vec<Action>,
) {
    let frames = extent.frames;
    let size = egui::vec2(ui.available_width(), STRIP + WAVE + LANE * VOICES as f32);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Tape waveform"));
    let visuals = ui.visuals();
    let palette = Palette::of(visuals);
    let (voice_colour, write_colour) = colours(palette);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
    let Some(status) = model.deck().status().filter(|_| frames > 0) else {
        return;
    };
    let strip = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), STRIP));
    let wave = egui::Rect::from_min_size(strip.left_bottom(), egui::vec2(rect.width(), WAVE));
    let x = |frame: f64| rect.left() + (frame / frames as f64) as f32 * rect.width();
    drag(
        ui,
        &response,
        (strip, wave),
        state,
        extent,
        &x,
        tab,
        actions,
    );
    let span = |w: Window| x(w.start as f64)..=x(w.end as f64);
    let selected = tab.selected;

    // The write window, and the write head, in their strip.
    let bar = egui::Rect::from_x_y_ranges(
        span(state.write_window),
        strip.top() + 2.0..=strip.bottom() - 2.0,
    );
    let bar_colour = match state.write {
        true => write_colour,
        false => visuals.weak_text_color(),
    };
    painter.rect_filled(bar, 1.0, bar_colour.gamma_multiply(0.5));
    painter.text(
        bar.left_center() + egui::vec2(4.0, 0.0),
        egui::Align2::LEFT_CENTER,
        "write",
        egui::FontId::proportional(STRIP - 3.0),
        visuals.strong_text_color(),
    );

    let sw = state.voices[selected].window;
    let shade = egui::Rect::from_x_y_ranges(span(sw), wave.y_range());
    painter.rect_filled(shade, 0.0, palette.region);
    let (mid, half) = (wave.center().y, WAVE / 2.0 - 2.0);
    let step = rect.width() / COLUMNS as f32;
    for (i, peak) in status.columns().iter().enumerate() {
        let cx = rect.left() + (i as f32 + 0.5) * step;
        let h = peak.min(1.0) * half;
        painter.line_segment(
            [egui::pos2(cx, mid - h), egui::pos2(cx, mid + h)],
            egui::Stroke::new(step.max(1.0), palette.rms),
        );
    }
    let dim = visuals.extreme_bg_color.gamma_multiply(0.7);
    for roll in [
        Window::new(0, extent.range.start),
        Window::new(extent.range.end, frames),
    ] {
        if !roll.is_empty() {
            painter.rect_filled(
                egui::Rect::from_x_y_ranges(span(roll), wave.y_range()),
                0.0,
                dim,
            );
        }
    }
    // What the write head cannot change: outside its window, or all of it
    // while writing is off. A hatch marks it too, for those who cannot tell
    // the tint.
    let ww = state.write_window;
    let frozen = match state.write {
        true => vec![Window::new(0, ww.start), Window::new(ww.end, frames)],
        false => vec![Window::new(0, frames)],
    };
    for part in frozen.into_iter().filter(|w| !w.is_empty()) {
        let area = egui::Rect::from_x_y_ranges(span(part), wave.y_range());
        painter.rect_filled(area, 0.0, palette.frozen);
        let hatch = ui.painter_at(area);
        let stroke = egui::Stroke::new(1.0, palette.frozen.gamma_multiply(2.5));
        let mut hx = area.left() - area.height();
        while hx < area.right() {
            hatch.line_segment(
                [
                    egui::pos2(hx, area.bottom()),
                    egui::pos2(hx + area.height(), area.top()),
                ],
                stroke,
            );
            hx += 10.0;
        }
        let label = egui::Rect::from_x_y_ranges(span(part), strip.y_range());
        if label.width() >= 50.0 {
            painter.text(
                label.center(),
                egui::Align2::CENTER_CENTER,
                "frozen",
                egui::FontId::proportional(STRIP - 3.0),
                visuals.weak_text_color(),
            );
        }
    }
    // The selected window's edges, which a drag on the waveform moves.
    for edge in [sw.start, sw.end] {
        let ex = x(edge as f64);
        painter.line_segment(
            [egui::pos2(ex, wave.top()), egui::pos2(ex, wave.bottom())],
            egui::Stroke::new(2.0, voice_colour[selected]),
        );
    }
    for (i, v) in state.voices.iter().enumerate() {
        let top = wave.bottom() + LANE * i as f32;
        let lane = egui::Rect::from_x_y_ranges(span(v.window), top + 1.0..=top + LANE - 1.0);
        let colour = match v.on {
            true => voice_colour[i],
            false => visuals.weak_text_color(),
        };
        painter.rect_filled(lane, 1.0, colour.gamma_multiply(0.35));
        crossfades(&painter, v, extent, lane, &x, colour);
        if i == selected {
            painter.rect_stroke(
                lane,
                1.0,
                egui::Stroke::new(1.0, voice_colour[i]),
                egui::StrokeKind::Inside,
            );
        }
        if v.on {
            let hx = x(status.voice(i));
            painter.line_segment(
                [egui::pos2(hx, wave.top()), egui::pos2(hx, top + LANE)],
                egui::Stroke::new(1.5, voice_colour[i]),
            );
        }
    }
    if state.write {
        let hx = x(status.write_head() as f64);
        painter.line_segment(
            [egui::pos2(hx, strip.top()), egui::pos2(hx, wave.bottom())],
            egui::Stroke::new(1.5, write_colour),
        );
    }
}

/// The window `target` names.
fn window_of(state: &TapeState, target: Target) -> Window {
    match target {
        Target::Voice(i) => state.voices[i].window,
        Target::Write => state.write_window,
    }
}

/// Whose window is under `pos`: the write window's in its strip, the
/// selected voice's on the waveform, or a voice's in its lane.
fn target(
    pos: egui::Pos2,
    (strip, wave): (egui::Rect, egui::Rect),
    selected: usize,
) -> Option<Target> {
    let lane = ((pos.y - wave.bottom()) / LANE).floor();
    match () {
        _ if pos.y < strip.bottom() => Some(Target::Write),
        _ if pos.y < wave.bottom() => Some(Target::Voice(selected)),
        _ if (0.0..VOICES as f32).contains(&lane) => Some(Target::Voice(lane as usize)),
        _ => None,
    }
}

/// The window and part of it under `pos`.
fn hit(
    pos: egui::Pos2,
    areas: (egui::Rect, egui::Rect),
    state: &TapeState,
    selected: usize,
    x: &dyn Fn(f64) -> f32,
) -> Option<(Target, Part)> {
    let target = target(pos, areas, selected)?;
    let w = window_of(state, target);
    let (a, b) = (x(w.start as f64), x(w.end as f64));
    let part = match ((pos.x - a).abs(), (pos.x - b).abs()) {
        (da, db) if da <= GRAB && da <= db => Part::Start,
        (_, db) if db <= GRAB => Part::End,
        _ if a < pos.x && pos.x < b => Part::Body,
        _ => return None,
    };
    Some((target, part))
}

/// Drags a window's edges or the whole of it along the waveform, sending the
/// window as each frame moves it. Edges pull to the range's edges.
#[allow(clippy::too_many_arguments)]
fn drag(
    ui: &egui::Ui,
    response: &egui::Response,
    areas: (egui::Rect, egui::Rect),
    state: &TapeState,
    e: Extent,
    x: &dyn Fn(f64) -> f32,
    tab: &mut State,
    actions: &mut Vec<Action>,
) {
    let wave = areas.1;
    let frame_at =
        |px: f32| f64::from((px - wave.left()) / wave.width()).clamp(0.0, 1.0) * e.frames as f64;
    // A click or a drag in a lane selects its voice.
    let pressed = ui.input(|i| i.pointer.press_origin());
    if response.clicked() || response.drag_started() {
        if let Some(Target::Voice(i)) = pressed.and_then(|pos| target(pos, areas, tab.selected)) {
            tab.selected = i;
        }
    }
    // An edge within reach of the range's edge lands on it.
    let snap = |f: usize| {
        [e.range.start, e.range.end]
            .into_iter()
            .find(|&r| (x(r as f64) - x(f as f64)).abs() <= GRAB)
            .unwrap_or(f)
    };
    if response.drag_started() {
        // Where the press was: a drag starts only once the pointer has moved.
        tab.drag = pressed.and_then(|pos| {
            let (target, part) = hit(pos, areas, state, tab.selected, x)?;
            Some(Drag {
                target,
                part,
                from: window_of(state, target),
                grab: frame_at(pos.x),
            })
        });
    }
    if response.drag_stopped() {
        tab.drag = None;
    }
    let cursor = match (tab.drag, response.hover_pos()) {
        (Some(d), _) => Some(d.part),
        (None, Some(pos)) => hit(pos, areas, state, tab.selected, x).map(|(_, part)| part),
        _ => None,
    };
    match cursor {
        Some(Part::Body) if tab.drag.is_some() => ui.set_cursor_icon(egui::CursorIcon::Grabbing),
        Some(Part::Body) => ui.set_cursor_icon(egui::CursorIcon::Grab),
        Some(_) => ui.set_cursor_icon(egui::CursorIcon::ResizeHorizontal),
        None => {}
    }
    let (Some(d), Some(pos)) = (tab.drag, response.interact_pointer_pos()) else {
        return;
    };
    let at = frame_at(pos.x).round() as usize;
    let w = d.from;
    let to = match d.part {
        Part::Start => Window::new(snap(at).min(w.end - 1), w.end),
        Part::End => Window::new(w.start, snap(at).max(w.start + 1)),
        Part::Body => {
            let shift = frame_at(pos.x) - d.grab;
            let start = (w.start as f64 + shift)
                .round()
                .clamp(0.0, (e.frames - w.len()) as f64) as usize;
            let last = e.frames - w.len();
            let start = match (snap(start), snap(start + w.len())) {
                (s, _) if s != start => s.min(last),
                (_, end) if end != start + w.len() => end.saturating_sub(w.len()).min(last),
                _ => start,
            };
            Window::new(start, start + w.len())
        }
    };
    if to == window_of(state, d.target) || e.range.is_empty() {
        return;
    }
    let p = |f: usize| {
        Pos::Percent(((f as f64 - e.range.start as f64) * 100.0 / e.range.len() as f64) as f32)
    };
    actions.push(Action::Tape(match d.target {
        Target::Voice(i) => T::Voice(i as u8 + 1, V::Window(p(to.start), p(to.end))),
        Target::Write => T::WriteWindow(p(to.start), p(to.end)),
    }));
}

/// Voice `v`'s crossfade curves in its `lane`, where the engine reads them:
/// the leaving head fades out from the edge it leaves, past it into the
/// post-roll, or before it when the new head fades in from the pre-roll before
/// the other edge instead. Equal power, as the engine fades.
fn crossfades(
    painter: &egui::Painter,
    v: &playr_app::tape::VoiceState,
    e: Extent,
    lane: egui::Rect,
    x: &dyn Fn(f64) -> f32,
    colour: egui::Color32,
) {
    let w = v.window;
    let xf = playr_looper::crossfade(w, w, e.frames, v.rate, v.fade, e.rate);
    if xf.frames == 0 || (x(xf.span) - x(0.0)).abs() < 1.0 {
        return;
    }
    let (start, end) = (w.start as f64, w.end as f64);
    let (dir, fade_in, fade_out) = match v.rate > 0.0 {
        true => (1.0, start - xf.lead, end - xf.lead),
        false => (-1.0, end + xf.lead, start + xf.lead),
    };
    let curve = |anchor: f64, gain: fn(f32) -> f32| {
        let at = |u: f64| x(anchor + dir * u * xf.span);
        let mut points = vec![egui::pos2(at(0.0), lane.bottom())];
        points.extend((0..=CURVE).map(|k| {
            let u = k as f64 / CURVE as f64;
            let g = gain(std::f32::consts::FRAC_PI_2 * u as f32);
            egui::pos2(at(u), lane.bottom() - g * lane.height())
        }));
        points.push(egui::pos2(at(1.0), lane.bottom()));
        egui::Shape::convex_polygon(points, colour.gamma_multiply(0.9), egui::Stroke::NONE)
    };
    painter.add(curve(fade_in, f32::sin));
    painter.add(curve(fade_out, f32::cos));
}

/// A window's ends as percentages of the range, below 0 or past 100 in the
/// pre-roll or post-roll; the whole range before a loop is loaded.
fn percent(w: Window, e: Extent) -> (f32, f32) {
    match e.range.len() {
        0 => (0.0, 100.0),
        n => {
            let p = |f: usize| ((f as f64 - e.range.start as f64) * 100.0 / n as f64) as f32;
            (p(w.start), p(w.end))
        }
    }
}

/// Two drag values for a window's start and end, in percent, each in its own
/// cell of a grid row; the action on a change.
fn window(
    ui: &mut egui::Ui,
    w: Window,
    e: Extent,
    name: &str,
    idle: Option<&str>,
) -> Option<(Pos, Pos)> {
    let (mut a, mut b) = percent(w, e);
    // As far as the pre-roll and post-roll reach, within what `:tape` takes.
    let (lo, hi) = match e.range.len() {
        0 => (0.0, 100.0),
        _ => (
            percent(Window::new(0, 0), e).0.max(-100.0),
            percent(Window::new(0, e.frames), e).1.min(200.0),
        ),
    };
    let drag = |ui: &mut egui::Ui, v: &mut f32, end: &str| {
        let field = egui::DragValue::new(v)
            .range(lo..=hi)
            .speed(0.5)
            .fixed_decimals(1)
            .suffix("%");
        let response = dimmed(ui, idle, |ui| ui.add(field));
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, format!("{name} {end}"))
        });
        response.changed()
    };
    let changed = drag(ui, &mut a, "start") | drag(ui, &mut b, "end");
    changed.then_some((Pos::Percent(a), Pos::Percent(b)))
}

/// Adds a control with `add`, dimmed, and with the reason on hover, when
/// `idle` says it has no effect. It stays usable, so it can be set ahead.
fn dimmed(
    ui: &mut egui::Ui,
    idle: Option<&str>,
    add: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    let response = ui
        .scope(|ui| {
            if idle.is_some() {
                ui.multiply_opacity(0.4);
            }
            add(ui)
        })
        .inner;
    match idle {
        Some(why) => response.on_hover_text(why),
        None => response,
    }
}

/// A slider over `range` and a field with its value, dimmed when `idle`
/// says why it has no effect; the new value on a change of either.
fn slider(
    ui: &mut egui::Ui,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    name: String,
    idle: Option<&str>,
) -> Option<f32> {
    let mut v = value;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if idle.is_some() {
            ui.multiply_opacity(0.4);
        }
        let rail = ui
            .add(egui::Slider::new(&mut v, range.clone()).show_value(false))
            .on_hover_text(idle.unwrap_or(&name));
        rail.widget_info(|| egui::WidgetInfo::slider(true, f64::from(v), &name));
        let field = ui.add(
            egui::DragValue::new(&mut v)
                .range(range)
                .speed(0.01)
                .fixed_decimals(2),
        );
        let label = format!("{name} value");
        field.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, &label));
        let field = match idle {
            Some(why) => field.on_hover_text(why),
            None => field,
        };
        rail.changed() || field.changed()
    })
    .inner
    .then_some(v)
}

/// The voice grid's headings, with what each column does.
const HEADINGS: [(&str, &str); 10] = [
    ("", ""),
    ("On", "plays the voice"),
    ("Start", "where the voice's window starts, in % of the loop"),
    ("End", "where the voice's window ends"),
    ("Rate", "frames a frame; negative plays in reverse"),
    ("Level", "what is heard of the voice"),
    (
        "Pan",
        "left to right; on a stereo loop, the far channel folds into the near one",
    ),
    (
        "Send",
        "what the write head records of the voice into the loop, with Write on; apart from Level",
    ),
    (
        "Wear",
        "a low-pass on what the voice sends; it darkens what the voice prints each pass",
    ),
    (
        "Xfade",
        "the crossfade at each wrap, in ms: the head leaving fades out into the audio past the window as the one starting fades in; the loop keeps its length",
    ),
];

/// The voices' rows, then the write head's, in one grid so its columns line
/// up with theirs.
/// The write row's headings, in the voices' columns.
const WRITE_HEADINGS: [(&str, &str); 9] = [
    ("", ""),
    ("On", "records the voices' sends into the loop"),
    ("Start", "where the part of the loop that is rewritten starts"),
    ("End", "where it ends"),
    (
        "Feedback",
        "how much of what the loop held survives each pass of the write head; 0 replaces it with the sends",
    ),
    ("", ""),
    ("", ""),
    ("", ""),
    (
        "Wear",
        "a low-pass on everything the write head records; it darkens the loop each pass",
    ),
];

fn rows(
    ui: &mut egui::Ui,
    state: &TapeState,
    e: Extent,
    tab: &mut State,
    actions: &mut Vec<Action>,
) {
    egui::Grid::new("tape rows")
        .num_columns(10)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for (heading, tip) in HEADINGS {
                ui.weak(heading).on_hover_text(tip);
            }
            ui.end_row();
            for (i, v) in state.voices.iter().enumerate() {
                let n = i as u8 + 1;
                let name = format!("Voice {n}");
                let why = |c| no_effect(state, e, c).map(idle_text);
                let mut set = |s| actions.push(Action::Tape(T::Voice(n, s)));
                if ui
                    .selectable_label(tab.selected == i, &name)
                    .on_hover_text("show and edit this voice's window on the waveform")
                    .clicked()
                {
                    tab.selected = i;
                }
                let mut on = v.on;
                if ui.checkbox(&mut on, "").on_hover_text(&name).changed() {
                    set(V::On(on));
                }
                if let Some((a, b)) = window(ui, v.window, e, &name, why(C::Window(i))) {
                    set(V::Window(a, b));
                }
                let rate = format!("{name} rate");
                if let Some(x) = slider(ui, v.rate, -4.0..=4.0, rate, why(C::Rate(i))) {
                    set(V::Rate(x));
                }
                let level = format!("{name} level");
                if let Some(x) = slider(ui, v.level, 0.0..=1.0, level, why(C::Level(i))) {
                    set(V::Level(x));
                }
                let pan = format!("{name} pan");
                if let Some(x) = slider(ui, v.pan, -1.0..=1.0, pan, why(C::Pan(i))) {
                    set(V::Pan(x));
                }
                let send = format!("{name} send");
                if let Some(x) = slider(ui, v.send, 0.0..=1.0, send, why(C::Send(i))) {
                    set(V::Send(x));
                }
                let wear = format!("{name} wear");
                if let Some(x) = slider(ui, v.wear, 0.0..=1.0, wear, why(C::Wear(i))) {
                    set(V::Wear(x));
                }
                let mut fade = v.fade;
                let response = dimmed(ui, why(C::Fade(i)), |ui| {
                    ui.add(egui::DragValue::new(&mut fade).range(0.0..=1000.0))
                });
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::DragValue,
                        true,
                        format!("{name} fade"),
                    )
                });
                if response.changed() {
                    set(V::Fade(fade));
                }
                ui.end_row();
            }
            ui.end_row();
            for (heading, tip) in WRITE_HEADINGS {
                ui.weak(heading).on_hover_text(tip);
            }
            ui.end_row();
            write_row(ui, state, e, actions);
            ui.end_row();
        });
}

/// The write head's row: its window, then Feedback under Rate and Wear
/// under the voices' Wear.
fn write_row(ui: &mut egui::Ui, state: &TapeState, e: Extent, actions: &mut Vec<Action>) {
    let why = |c| no_effect(state, e, c).map(idle_text);
    ui.label("Write");
    let mut write = state.write;
    let tick = dimmed(ui, why(C::Write), |ui| ui.checkbox(&mut write, ""));
    if tick.on_hover_text("Write").changed() {
        actions.push(Action::Tape(T::Write(write)));
    }
    if let Some((a, b)) = window(ui, state.write_window, e, "Write", why(C::WriteWindow)) {
        actions.push(Action::Tape(T::WriteWindow(a, b)));
    }
    let feedback = "Write feedback".to_string();
    if let Some(x) = slider(ui, state.feedback, 0.0..=1.0, feedback, why(C::Feedback)) {
        actions.push(Action::Tape(T::Feedback(x)));
    }
    for _ in 0..3 {
        ui.label("");
    }
    let wear = "Write wear".to_string();
    if let Some(x) = slider(ui, state.wear, 0.0..=1.0, wear, why(C::WriteWear)) {
        actions.push(Action::Tape(T::Wear(x)));
    }
}
