//! The Tape tab: a lane per voice, voice 4's on top with the write head, each
//! with its loop, its window and head, and its controls; a lane folds to its
//! header and a strip, and opens a drawer of less-used fields. Above them,
//! the FX lane: the delay and the reverb every voice sends to. The terminal
//! has the `:tape` commands only; see `docs/dev/looper-engine.md`,
//! `docs/dev/tape-lanes.md`, `docs/dev/tape-machine-v2-design.md` and
//! `docs/dev/fx-buses.md`.

use eframe::egui;
use playr_app::action::Action;
use playr_app::dispatch::Frontend;
use playr_app::message::idle_text;
use playr_app::model::Model;
use playr_app::tape::{
    model_name, no_effect, Control as C, Division, Extent, FxSetting as F, Layout, Pos,
    TapeAction as T, TapeState, VoiceSetting as V, DELAY_MODELS, REVERB_MODELS,
};
use playr_looper::{Window, VOICES, WRITER};

use crate::controls;
use crate::palette::Palette;

/// The header's width beside each lane; the write window's strip, a voice's
/// waveform, and the band at its foot the crossfade curves take.
const HEAD: f32 = 150.0;
/// A waveform's height: what the lanes leave, between these.
const WAVE_MIN: f32 = 40.0;
const WAVE_MAX: f32 = 96.0;
/// What a lane takes besides its waveform: its row of fields, and the
/// separator, as measured at 800 x 592; slack for rounding; and the write
/// window's band on voice 4's waveform.
const ROW: f32 = 39.0;
const SEPARATOR: f32 = 12.0;
const SLACK: f32 = 4.0;
const BAND: f32 = 10.0;
const FADES: f32 = 12.0;
/// A folded lane's height: its header's two rows, and the strip beside them;
/// voice 4's header keeps a third, with Write.
const FOLDED: f32 = 46.0;
const BUTTONS: f32 = 24.0;
/// Points along each drawn crossfade curve.
const CURVE: usize = 16;

/// How near an edge, in points, a press grabs it, and a dragged edge or
/// window snaps to the range's edges.
const GRAB: f32 = 6.0;

/// What the tab keeps between frames: a window being dragged, and the
/// lane whose loop list is open, with its search and what it found.
#[derive(Debug, Default)]
pub struct State {
    drag: Option<Drag>,
    picking: Option<usize>,
    query: String,
    found: Vec<playr_core::db::loops::Loop>,
    /// A lane folded or a drawer opened this frame, for the deck to keep.
    layout: Option<Layout>,
}

impl State {
    /// The lane layout changed this frame, once.
    pub fn take_layout(&mut self) -> Option<Layout> {
        self.layout.take()
    }
}

/// A drag in a lane: whose window, which part of it, the window as it was
/// when the drag began, and the frame first pressed.
#[derive(Debug, Clone, Copy)]
struct Drag {
    target: Target,
    part: Part,
    from: Window,
    grab: f64,
}

/// A voice's window, on its waveform, or the write window, in its strip.
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
    let extents: [Extent; VOICES] = std::array::from_fn(|i| deck.voice_extent(i).unwrap_or(extent));

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
        // Beside the buttons, so the lanes fit the smallest window.
        if frames.is_none() {
            ui.weak(match deck.loading() {
                true => "Reading the range.",
                false => "Pick a loop in a lane, or set a range and Load range.",
            });
        }
    });
    // Numbered from the bottom: voice 4, which plays the write loop, on top.
    // An open lane has its main row of fields, and with its drawer open, a
    // second; voice 4's drawer has a third, the write head's.
    let layout = *deck.layout();
    let folded: [bool; VOICES] =
        std::array::from_fn(|i| layout.is_folded(i, deck.voice_name(i).is_none()));
    let open = folded.iter().filter(|f| !**f).count();
    let rows: usize = (0..VOICES)
        .filter(|&i| !folded[i])
        .map(|i| 1 + usize::from(layout.drawer[i]) * (1 + usize::from(i == WRITER)))
        .sum();
    // The FX lane: its header, and open, a row each for the delay and reverb.
    let fx = BUTTONS + if layout.fx { 2.0 * ROW } else { 0.0 };
    let spare = ui.available_height()
        - rows as f32 * ROW
        - (VOICES - open) as f32 * FOLDED
        - if folded[WRITER] { BUTTONS } else { 0.0 }
        - fx
        - VOICES as f32 * SEPARATOR
        - SLACK;
    let wave = (spare / open.max(1) as f32).clamp(WAVE_MIN, WAVE_MAX);
    let lanes = Lanes {
        model,
        state: &state,
        extents,
    };
    let loaded = frames.is_some();
    let mut ctx = (&mut actions, tab);
    egui::ScrollArea::vertical().show(ui, |ui| {
        let (a, tab) = &mut ctx;
        ui.add_enabled_ui(loaded, |ui| fx_lane(ui, &state, extent, layout, tab, a));
        for i in (0..VOICES).rev() {
            ui.separator();
            let e = extents[i];
            let (fold, drawer) = (folded[i], layout.drawer[i]);
            lane(
                ui,
                &mut ctx,
                |ui, (a, tab)| voice_head(ui, model, &state, e, i, fold, layout, tab, a),
                |ui, (a, tab)| {
                    ui.add_enabled_ui(loaded, |ui| {
                        if fold {
                            return lanes.voice_wave(ui, i, FOLDED, tab, a);
                        }
                        lanes.voice_wave(ui, i, wave, tab, a);
                        main_fields(ui, &state, e, i, layout, tab, a);
                        if drawer {
                            drawer_fields(ui, &state, e, i, a);
                            if i == WRITER {
                                write_fields(ui, &state, e, a);
                            }
                        }
                    });
                },
            );
        }
    });
    actions
}

/// A lane: `head` in a column of [`HEAD`] points, `body` beside it, each
/// given `ctx`.
fn lane<T>(
    ui: &mut egui::Ui,
    ctx: &mut T,
    head: impl FnOnce(&mut egui::Ui, &mut T),
    body: impl FnOnce(&mut egui::Ui, &mut T),
) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(HEAD);
            ui.spacing_mut().item_spacing.y = 2.0;
            head(ui, ctx);
        });
        ui.vertical(|ui| body(ui, ctx));
    });
}

/// The colour of voice `i`, and of the write head.
fn colours(p: &Palette) -> ([egui::Color32; VOICES], egui::Color32) {
    ([p.green, p.yellow, p.edge, p.bar], p.red)
}

/// What the lanes draw from.
struct Lanes<'a> {
    model: &'a Model,
    state: &'a TapeState,
    extents: [Extent; VOICES],
}

impl Lanes<'_> {
    /// Whether the write head is writing.
    fn writing(&self) -> bool {
        self.state.write
    }

    /// Voice `i`'s loop's peaks, with its window shaded and its edges marked,
    /// the pre-roll and post-roll dimmed, its crossfade curves at the foot,
    /// and its head. A lane reading the write loop shows the write head and
    /// what it cannot change.
    fn voice_wave(
        &self,
        ui: &mut egui::Ui,
        i: usize,
        height: f32,
        tab: &mut State,
        actions: &mut Vec<Action>,
    ) {
        let (state, e) = (self.state, self.extents[i]);
        let size = egui::vec2(ui.available_width(), height);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let name = format!("Voice {} waveform", i + 1);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &name));
        let visuals = ui.visuals();
        let palette = Palette::of(visuals);
        let (voice_colour, write_colour) = colours(palette);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
        let Some(status) = self.model.deck().status().filter(|_| e.frames > 0) else {
            return;
        };
        let v = &state.voices[i];
        drag(
            ui,
            &response,
            rect,
            Target::Voice(i),
            v.window,
            e,
            tab,
            actions,
        );
        let x = scale(rect, e);
        let span = |w: Window| x(w.start as f64)..=x(w.end as f64);
        let colour = match v.on {
            true => voice_colour[i],
            false => visuals.weak_text_color(),
        };

        painter.rect_filled(
            egui::Rect::from_x_y_ranges(span(v.window), rect.y_range()),
            0.0,
            palette.region,
        );
        // A muted lane's peaks dim; its head and edges keep their colour.
        let rms = match v.mute {
            true => palette.rms.gamma_multiply(0.35),
            false => palette.rms,
        };
        peaks(&painter, rect, &status.voice_columns(i), rms);
        dim_rolls(&painter, rect, e, &x, visuals);
        // The write loop is the only one the write head writes.
        let shared = !state.own[i];
        if shared {
            for part in frozen(state, e, self.writing()) {
                hatch(
                    ui,
                    egui::Rect::from_x_y_ranges(span(part), rect.y_range()),
                    palette,
                );
            }
        }
        for edge in [v.window.start, v.window.end] {
            let ex = x(edge as f64);
            painter.line_segment(
                [egui::pos2(ex, rect.top()), egui::pos2(ex, rect.bottom())],
                egui::Stroke::new(2.0, colour),
            );
        }
        // A head under Ping turns rather than wraps, so it never crossfades;
        // a folded strip has no room for the curves.
        if !v.ping && height > FOLDED {
            let foot =
                egui::Rect::from_x_y_ranges(span(v.window), rect.bottom() - FADES..=rect.bottom());
            crossfades(&painter, v, e, foot, &x, colour);
        }
        if v.on {
            let hx = x(status.voice(i));
            painter.line_segment(
                [egui::pos2(hx, rect.top()), egui::pos2(hx, rect.bottom())],
                egui::Stroke::new(1.5, colour),
            );
        }
        if shared && self.writing() {
            let hx = x(status.write_head() as f64);
            painter.line_segment(
                [egui::pos2(hx, rect.top()), egui::pos2(hx, rect.bottom())],
                egui::Stroke::new(1.5, write_colour),
            );
        }
        if i == WRITER {
            self.write_band(ui, rect, tab, actions);
        }
    }

    /// The write window as a band along the top of voice 4's waveform, which
    /// drags as a voice's window does.
    fn write_band(
        &self,
        ui: &mut egui::Ui,
        wave: egui::Rect,
        tab: &mut State,
        actions: &mut Vec<Action>,
    ) {
        let rect = egui::Rect::from_min_size(wave.min, egui::vec2(wave.width(), BAND));
        let response = ui.interact(
            rect,
            ui.id().with("write band"),
            egui::Sense::click_and_drag(),
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Write band"));
        let e = self.extents[WRITER];
        let w = self.state.write_window;
        drag(ui, &response, rect, Target::Write, w, e, tab, actions);
        let visuals = ui.visuals();
        let (_, write_colour) = colours(Palette::of(visuals));
        let colour = match self.writing() {
            true => write_colour,
            false => visuals.weak_text_color(),
        };
        let x = scale(rect, e);
        let bar = egui::Rect::from_x_y_ranges(
            x(w.start as f64)..=x(w.end as f64),
            rect.top() + 1.0..=rect.bottom() - 1.0,
        );
        ui.painter_at(wave)
            .rect_filled(bar, 1.0, colour.gamma_multiply(0.6));
    }
}

/// A peak grid across `rect`, a column each, about its middle.
fn peaks(painter: &egui::Painter, rect: egui::Rect, columns: &[f32], colour: egui::Color32) {
    let (mid, half) = (rect.center().y, rect.height() / 2.0 - 2.0);
    let step = rect.width() / columns.len().max(1) as f32;
    for (k, peak) in columns.iter().enumerate() {
        let cx = rect.left() + (k as f32 + 0.5) * step;
        let h = peak.min(1.0) * half;
        painter.line_segment(
            [egui::pos2(cx, mid - h), egui::pos2(cx, mid + h)],
            egui::Stroke::new(step.max(1.0), colour),
        );
    }
}

/// Dims loop `e`'s pre-roll and post-roll across `rect`.
fn dim_rolls(
    painter: &egui::Painter,
    rect: egui::Rect,
    e: Extent,
    x: &dyn Fn(f64) -> f32,
    visuals: &egui::Visuals,
) {
    let dim = visuals.extreme_bg_color.gamma_multiply(0.7);
    for roll in [
        Window::new(0, e.range.start),
        Window::new(e.range.end, e.frames),
    ] {
        if !roll.is_empty() {
            let xs = x(roll.start as f64)..=x(roll.end as f64);
            painter.rect_filled(egui::Rect::from_x_y_ranges(xs, rect.y_range()), 0.0, dim);
        }
    }
}

/// A frame's position across `rect`, on loop `e`'s scale.
fn scale(rect: egui::Rect, e: Extent) -> impl Fn(f64) -> f32 {
    move |frame| rect.left() + (frame / e.frames.max(1) as f64) as f32 * rect.width()
}

/// What the write head cannot change of the write loop, `e`: outside its
/// window, or all of it while it is not writing. While its window is the
/// range it writes the rolls too, up to a range's length out.
fn frozen(state: &TapeState, e: Extent, writing: bool) -> Vec<Window> {
    let ww = state.write_window;
    let reach = match ww == e.range {
        true => ww.len(),
        false => 0,
    };
    let parts = match writing {
        true => vec![
            Window::new(0, ww.start.saturating_sub(reach)),
            Window::new((ww.end + reach).min(e.frames), e.frames),
        ],
        false => vec![Window::new(0, e.frames)],
    };
    parts.into_iter().filter(|w| !w.is_empty()).collect()
}

/// Tints and hatches `area`, for those who cannot tell the tint.
fn hatch(ui: &egui::Ui, area: egui::Rect, palette: &Palette) {
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 0.0, palette.frozen);
    let stroke = egui::Stroke::new(1.0, palette.frozen.gamma_multiply(2.5));
    let mut hx = area.left() - area.height();
    while hx < area.right() {
        painter.line_segment(
            [
                egui::pos2(hx, area.bottom()),
                egui::pos2(hx + area.height(), area.top()),
            ],
            stroke,
        );
        hx += 10.0;
    }
}

/// The part of window `w` under `pos`, drawn across `rect` on loop `e`'s scale.
fn hit(pos: egui::Pos2, rect: egui::Rect, w: Window, e: Extent) -> Option<Part> {
    let x = scale(rect, e);
    let (a, b) = (x(w.start as f64), x(w.end as f64));
    match ((pos.x - a).abs(), (pos.x - b).abs()) {
        (da, db) if da <= GRAB && da <= db => Some(Part::Start),
        (_, db) if db <= GRAB => Some(Part::End),
        _ if a < pos.x && pos.x < b => Some(Part::Body),
        _ => None,
    }
}

/// Drags window `w`'s edges or the whole of it across `rect`, sending the
/// window as each frame moves it. Edges pull to the range's edges.
#[allow(clippy::too_many_arguments)]
fn drag(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: egui::Rect,
    target: Target,
    w: Window,
    e: Extent,
    tab: &mut State,
    actions: &mut Vec<Action>,
) {
    let x = scale(rect, e);
    let frame_at =
        |px: f32| f64::from((px - rect.left()) / rect.width()).clamp(0.0, 1.0) * e.frames as f64;
    if response.drag_started() {
        // Where the press was: a drag starts only once the pointer has moved.
        tab.drag = ui.input(|i| i.pointer.press_origin()).and_then(|pos| {
            Some(Drag {
                target,
                part: hit(pos, rect, w, e)?,
                from: w,
                grab: frame_at(pos.x),
            })
        });
    }
    let mine = tab.drag.filter(|d| d.target == target);
    if response.drag_stopped() && mine.is_some() {
        tab.drag = None;
    }
    let cursor = match (mine, response.hover_pos()) {
        (Some(d), _) => Some(d.part),
        (None, Some(pos)) => hit(pos, rect, w, e),
        _ => None,
    };
    match cursor {
        Some(Part::Body) if mine.is_some() => ui.set_cursor_icon(egui::CursorIcon::Grabbing),
        Some(Part::Body) => ui.set_cursor_icon(egui::CursorIcon::Grab),
        Some(_) => ui.set_cursor_icon(egui::CursorIcon::ResizeHorizontal),
        None => {}
    }
    let (Some(d), Some(pos)) = (mine, response.interact_pointer_pos()) else {
        return;
    };
    // An edge within reach of the range's edge lands on it.
    let snap = |f: usize| {
        [e.range.start, e.range.end]
            .into_iter()
            .find(|&r| (x(r as f64) - x(f as f64)).abs() <= GRAB)
            .unwrap_or(f)
    };
    let at = frame_at(pos.x).round() as usize;
    let from = d.from;
    let to = match d.part {
        Part::Start => Window::new(snap(at).min(from.end - 1), from.end),
        Part::End => Window::new(from.start, snap(at).max(from.start + 1)),
        Part::Body => {
            let shift = frame_at(pos.x) - d.grab;
            let last = e.frames - from.len();
            let start = (from.start as f64 + shift).round().clamp(0.0, last as f64) as usize;
            let start = match (snap(start), snap(start + from.len())) {
                (s, _) if s != start => s.min(last),
                (_, end) if end != start + from.len() => end.saturating_sub(from.len()).min(last),
                _ => start,
            };
            Window::new(start, start + from.len())
        }
    };
    if to == w || e.range.is_empty() {
        return;
    }
    let p = |f: usize| {
        Pos::Percent(((f as f64 - e.range.start as f64) * 100.0 / e.range.len() as f64) as f32)
    };
    actions.push(Action::Tape(match target {
        Target::Voice(i) => T::Voice(i as u8 + 1, V::Window(p(to.start), p(to.end))),
        Target::Write => T::WriteWindow(p(to.start), p(to.end)),
    }));
}

/// Voice `v`'s crossfade curves in `foot`, where the engine reads them:
/// the leaving head fades out from the edge it leaves, past it into the
/// post-roll, or before it when the new head fades in from the pre-roll before
/// the other edge instead. Equal power, as the engine fades.
fn crossfades(
    painter: &egui::Painter,
    v: &playr_app::tape::VoiceState,
    e: Extent,
    foot: egui::Rect,
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
        let mut points = vec![egui::pos2(at(0.0), foot.bottom())];
        points.extend((0..=CURVE).map(|k| {
            let u = k as f64 / CURVE as f64;
            let g = gain(std::f32::consts::FRAC_PI_2 * u as f32);
            egui::pos2(at(u), foot.bottom() - g * foot.height())
        }));
        points.push(egui::pos2(at(1.0), foot.bottom()));
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

/// Two drag values for a window's start and end, in percent, side by side,
/// with `tip` on hover; the action on a change.
fn window(
    ui: &mut egui::Ui,
    w: Window,
    e: Extent,
    name: &str,
    tip: &str,
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
        let response = match idle {
            Some(_) => response,
            None => response.on_hover_text(tip),
        };
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, format!("{name} {end}"))
        });
        response.changed()
    };
    let changed = ui
        .horizontal(|ui| drag(ui, &mut a, "start") | drag(ui, &mut b, "end"))
        .inner;
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

/// A field over `range`, dimmed when `idle` says why it has no effect; the
/// new value on a change.
fn field(
    ui: &mut egui::Ui,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    name: String,
    idle: Option<&str>,
) -> Option<f32> {
    let mut v = value;
    let speed = (range.end() - range.start()) / 200.0;
    let response = dimmed(ui, idle, |ui| {
        ui.add(
            egui::DragValue::new(&mut v)
                .range(range)
                .speed(speed)
                .fixed_decimals(2),
        )
    });
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, &name));
    response.changed().then_some(v)
}

/// A field in ms from 0 to `max`, dimmed when `idle` says why it has no
/// effect; the new value on a change.
fn ms(ui: &mut egui::Ui, value: f32, max: f32, name: String, idle: Option<&str>) -> Option<f32> {
    let mut v = value;
    let response = dimmed(ui, idle, |ui| {
        ui.add(egui::DragValue::new(&mut v).range(0.0..=max))
    });
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, &name));
    response.changed().then_some(v)
}

/// A voice's main controls, with what each does. On, Ping, Mute and Solo
/// are in the lane's header.
const MAIN_ROWS: [(&str, &str); 5] = [
    (
        "Window",
        "where the voice's window starts and ends, in % of the loop",
    ),
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
];

/// A voice's controls in its drawer.
const DRAWER_ROWS: [(&str, &str); 11] = [
    (
        "Slew",
        "how long a rate change takes, in ms; long slews bend the pitch as a tape speeding up or slowing down",
    ),
    (
        "Wear",
        "a low-pass on what the voice sends; it darkens what the voice prints each pass",
    ),
    (
        "Drive",
        "saturation on what the voice reads, heard and sent: quiet material up to 12 dB louder, loud material held down",
    ),
    (
        "HP",
        "a high-pass, in Hz: cuts below it; off at 20 Hz",
    ),
    (
        "LP",
        "a low-pass, in Hz: cuts above it; off at 20 kHz",
    ),
    ("Bell", "the bell's centre, in Hz"),
    (
        "dB",
        "the bell's gain, -18 to 18 dB: a boost or a cut around its centre; 0 is flat",
    ),
    (
        "Q",
        "the bell's width: 0.1 is many octaves wide, 10 a narrow notch; 1 is about an octave and a half",
    ),
    (
        "Xfade",
        "the crossfade at each wrap, in ms: the head leaving fades out into the audio past the window as the one starting fades in; the loop keeps its length",
    ),
    (
        "Delay",
        "what the voice sends to the delay, after its level: muting the voice silences its echo",
    ),
    (
        "Reverb",
        "what the voice sends to the reverb, after its level: muting the voice silences its reverb",
    ),
];

/// The write head's controls.
const WRITE_ROWS: [(&str, &str); 5] = [
    (
        "Write window",
        "where the part of the loop that is rewritten starts and ends",
    ),
    (
        "Feedback",
        "how much of what the loop held survives each pass of the write head; 0 replaces it with the sends",
    ),
    (
        "Print wear",
        "a low-pass on everything the write head records; it darkens the loop each pass",
    ),
    (
        "Print thin",
        "a high-pass on everything the write head records, 20 Hz to 2 kHz; it thins the loop each pass",
    ),
    (
        "Print FX",
        "how much of the delay and reverb returns the write head records with the sends",
    ),
];

/// A field over `range` on a log scale, as a frequency or a Q is, shown to
/// `decimals`; the new value on a change.
fn log_field(
    ui: &mut egui::Ui,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    decimals: usize,
    name: String,
    idle: Option<&str>,
) -> Option<f32> {
    // Dragged as its log2, so each point moves it by the same ratio.
    let (lo, hi) = (range.start().log2(), range.end().log2());
    let mut l = value.log2();
    let response = dimmed(ui, idle, |ui| {
        ui.add(
            egui::DragValue::new(&mut l)
                .range(lo..=hi)
                .speed((hi - lo) / 200.0)
                .custom_formatter(|l, _| format!("{:.*}", decimals, l.exp2()))
                .custom_parser(|text| {
                    text.trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|v| *v > 0.0)
                        .map(f64::log2)
                }),
        )
    });
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, &name));
    response
        .changed()
        .then(|| l.exp2().clamp(*range.start(), *range.end()))
}

/// A field's value, its control for `no_effect`, the setting it sends, and
/// its name.
type Field<S> = (f32, C, fn(f32) -> S, &'static str);

/// A grid with `headings` in a row, with what each does on hover, and the
/// controls `add` puts under them.
fn fields(
    ui: &mut egui::Ui,
    id: impl egui::AsIdSalt,
    headings: &[(&str, &str)],
    add: impl FnOnce(&mut egui::Ui),
) {
    egui::Grid::new(id).spacing([6.0, 0.0]).show(ui, |ui| {
        for (heading, tip) in headings {
            ui.weak(*heading).on_hover_text(*tip);
        }
        ui.end_row();
        add(ui);
    });
}

/// A toggle named `name` for assistive technology, showing `text`; whether
/// it was clicked.
fn toggle(ui: &mut egui::Ui, on: bool, text: &str, name: String, tip: &str) -> bool {
    let response = ui.selectable_label(on, text).on_hover_text(tip);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, &name)
    });
    response.clicked()
}

/// Adds a clickable control with `add`, dimmed when `idle` says it has no
/// effect; whether it was clicked.
fn dimmed_click(
    ui: &mut egui::Ui,
    idle: Option<&str>,
    add: impl FnOnce(&mut egui::Ui) -> bool,
) -> bool {
    ui.scope(|ui| {
        if idle.is_some() {
            ui.multiply_opacity(0.4);
        }
        add(ui)
    })
    .inner
}

/// Voice `i`'s header: the loop it reads; where it lies in its file; fold,
/// on, its name, Ping, Mute and Solo; and unless folded, its buttons.
#[allow(clippy::too_many_arguments)]
fn voice_head(
    ui: &mut egui::Ui,
    model: &Model,
    state: &TapeState,
    e: Extent,
    i: usize,
    folded: bool,
    layout: Layout,
    tab: &mut State,
    actions: &mut Vec<Action>,
) {
    let loaded = model.deck().loaded().is_some();
    picker(ui, model, state, i, folded, tab, actions);
    let fold = ui
        .add_enabled_ui(loaded, |ui| {
            voice_controls(ui, state, e, i, folded, actions)
        })
        .inner;
    if fold {
        let mut layout = layout;
        layout.folded[i] = Some(!folded);
        tab.layout = Some(layout);
    }
}

/// The chevron that folds a lane named `label`, `>` folded and `v` open;
/// whether it was clicked. Two strokes, over egui's filled triangle, which
/// read as too sharp.
fn fold_toggle(ui: &mut egui::Ui, folded: bool, label: &str, tip: &str) -> bool {
    let size = egui::vec2(10.0, ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_text(tip);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, !folded, label));
    let c = rect.center();
    let (a, b) = (2.0, 4.0);
    let points = match folded {
        true => [
            c + egui::vec2(-a, -b),
            c + egui::vec2(a, 0.0),
            c + egui::vec2(-a, b),
        ],
        false => [
            c + egui::vec2(-b, -a),
            c + egui::vec2(0.0, a),
            c + egui::vec2(b, -a),
        ],
    };
    let mut stroke = ui.style().interact(&response).fg_stroke;
    stroke.width = 1.5;
    ui.painter().add(egui::Shape::line(points.to_vec(), stroke));
    response.clicked()
}

/// Voice `i`'s loop, as a list that loads another from the loop library,
/// searched as typed, or for voices 1 to 3, voice 4's loop, the write loop.
fn picker(
    ui: &mut egui::Ui,
    model: &Model,
    state: &TapeState,
    i: usize,
    folded: bool,
    tab: &mut State,
    actions: &mut Vec<Action>,
) {
    let n = i as u8 + 1;
    let shown = match model.deck().voice_name(i) {
        None => "empty".to_string(),
        Some(_) if i != WRITER && !state.own[i] => "voice 4's loop".to_string(),
        Some(name) => name.to_string(),
    };
    let combo = egui::ComboBox::from_id_salt(("tape loop", i))
        .selected_text(&shown)
        .truncate()
        .width(HEAD - 8.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            // Wide enough for a name and where it lies in its file.
            ui.set_min_width(320.0);
            let opened = tab.picking != Some(i);
            if opened {
                tab.picking = Some(i);
                tab.query.clear();
                tab.found = model.session().find_loops("");
            }
            let edit = ui.add(
                egui::TextEdit::singleline(&mut tab.query)
                    .hint_text("search: name, bpm:120, key:Am"),
            );
            let label = format!("Voice {n} loop search");
            edit.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, &label)
            });
            if opened {
                edit.request_focus();
            }
            if edit.changed() {
                tab.found = model.session().find_loops(&tab.query);
            }
            if i != WRITER && ui.selectable_label(false, "voice 4's loop").clicked() {
                actions.push(Action::Tape(T::Share(n)));
                ui.close();
            }
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for l in &tab.found {
                        let about = playr_app::loops::cells(l)[1..].join("  ");
                        // The name, and where it lies in its file.
                        let clicked = ui
                            .horizontal(|ui| {
                                let row = ui.selectable_label(false, &l.name).on_hover_text(about);
                                ui.weak(playr_app::loops::span_of(l));
                                row.clicked()
                            })
                            .inner;
                        if clicked {
                            actions.push(Action::Tape(T::Loop(n, l.id)));
                            ui.close();
                        }
                    }
                });
            if tab.found.is_empty() {
                ui.weak(playr_app::loops::empty_text(!tab.query.trim().is_empty()));
            }
        });
    if combo.inner.is_none() && tab.picking == Some(i) {
        tab.picking = None;
    }
    let label = format!("Voice {n} loop");
    combo
        .response
        .on_hover_text(&shown)
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, &label));
    // Where the loop lies in its file, which tells apart loops of one track.
    if folded {
        return;
    }
    let span = model.deck().voice_span(i).unwrap_or_default();
    ui.weak(span)
        .on_hover_text("where the loop lies in its source file");
}

/// Voice `i`'s on, name, Ping, Mute and Solo; Clear, on the write loop, and
/// Eject; for voice 4, Write.
fn voice_controls(
    ui: &mut egui::Ui,
    state: &TapeState,
    e: Extent,
    i: usize,
    folded: bool,
    actions: &mut Vec<Action>,
) -> bool {
    let v = &state.voices[i];
    let n = i as u8 + 1;
    let name = format!("Voice {n}");
    let why = |c| no_effect(state, e, c).map(idle_text);
    let mut set = |s| actions.push(Action::Tape(T::Voice(n, s)));
    let mut fold = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let tip = match folded {
            true => "show the lane's waveform and fields",
            false => "fold the lane to its header and a strip",
        };
        fold = fold_toggle(ui, folded, &format!("{name} lane"), tip);
        let mut on = v.on;
        let tick = ui.checkbox(&mut on, "").on_hover_text("plays the voice");
        let label = format!("{name} on");
        tick.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, &label)
        });
        if tick.changed() {
            set(V::On(on));
        }
        ui.strong(&name);
        let ping = "turn at the window's edges and play back, instead of wrapping";
        let ping = why(C::Ping(i)).unwrap_or(ping);
        let clicked = dimmed_click(ui, why(C::Ping(i)), |ui| {
            toggle(ui, v.ping, "Ping", format!("{name} ping"), ping)
        });
        if clicked {
            set(V::Ping(!v.ping));
        }
        let mute = "silence the voice; its head and send go on";
        let mute = why(C::Mute(i)).unwrap_or(mute);
        let clicked = dimmed_click(ui, why(C::Mute(i)), |ui| {
            toggle(ui, v.mute, "M", format!("{name} mute"), mute)
        });
        if clicked {
            set(V::Mute(!v.mute));
        }
        let solo = "hear only the soloed voices; what each sends is unchanged";
        if toggle(ui, v.solo, "S", format!("{name} solo"), solo) {
            set(V::Solo(!v.solo));
        }
    });
    // Voice 4 keeps Write, which makes the write loop of an empty lane.
    if folded && i != WRITER {
        return fold;
    }
    let has = !state.empty[i];
    let shared = !state.own[i];
    ui.horizontal(|ui| {
        // Only the write loop is cleared; a loop of the voice's own would only be lost.
        let clears = i == WRITER || shared;
        let clear = match (has, i == WRITER) {
            (false, _) => "the voice is empty",
            (true, true) => "silence the write loop, keeping its length; Reset restores it",
            (true, false) => {
                "silence voice 4's loop, the write loop, keeping its length; Reset restores it"
            }
        };
        let eject = match (has, i == WRITER, shared) {
            (false, ..) => "the voice is empty",
            (true, true, _) => "drop the write loop; the next Write makes a silent one",
            (true, false, false) => "drop this voice's loop; the voice is empty",
            (true, false, true) => "stop reading voice 4's loop; the voice is empty",
        };
        let buttons = [("Clear", clear, T::Clear(n)), ("Eject", eject, T::Eject(n))];
        for (text, tip, action) in buttons.into_iter().skip(usize::from(!clears)) {
            let button = ui
                .add_enabled(has, egui::Button::new(text))
                .on_hover_text(tip)
                .on_disabled_hover_text(tip);
            let label = format!("{name} {}", text.to_lowercase());
            button.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, has, &label));
            if button.clicked() {
                actions.push(Action::Tape(action));
            }
        }
        if i == WRITER {
            write_toggle(ui, state, e, actions);
        }
    });
    fold
}

/// Voice `i`'s main controls, in a row under its waveform, and the toggle
/// that opens its drawer.
fn main_fields(
    ui: &mut egui::Ui,
    state: &TapeState,
    e: Extent,
    i: usize,
    layout: Layout,
    tab: &mut State,
    actions: &mut Vec<Action>,
) {
    let v = &state.voices[i];
    let n = i as u8 + 1;
    let name = format!("Voice {n}");
    let why = |c| no_effect(state, e, c).map(idle_text);
    let mut set = |s| actions.push(Action::Tape(T::Voice(n, s)));
    let label = |heading: &str| format!("{name} {}", heading.to_lowercase());
    ui.horizontal_top(|ui| {
        fields(ui, ("tape voice", i), &MAIN_ROWS, |ui| {
            let tip = MAIN_ROWS[0].1;
            if let Some((a, b)) = window(ui, v.window, e, &name, tip, why(C::Window(i))) {
                set(V::Window(a, b));
            }
            if let Some(x) = field(ui, v.rate, -4.0..=4.0, label("rate"), why(C::Rate(i))) {
                set(V::Rate(x));
            }
            let unit: [Field<V>; 3] = [
                (v.level, C::Level(i), V::Level, "level"),
                (v.pan, C::Pan(i), V::Pan, "pan"),
                (v.send, C::Send(i), V::Send, "send"),
            ];
            for (value, control, setting, heading) in unit {
                let range = match heading {
                    "pan" => -1.0..=1.0,
                    _ => 0.0..=1.0,
                };
                if let Some(x) = field(ui, value, range, label(heading), why(control)) {
                    set(setting(x));
                }
            }
            ui.end_row();
        });
        // Level with the fields, under the headings.
        ui.vertical(|ui| {
            ui.weak("");
            let open = layout.drawer[i];
            let tip = match (open, i == WRITER) {
                (true, _) => "hide the less-used fields",
                (false, true) => {
                    "show slew, wear, drive, filter and crossfade, and the write head's fields"
                }
                (false, false) => "show slew, wear, drive, filter and crossfade",
            };
            if toggle(ui, open, "More", label("drawer"), tip) {
                let mut layout = layout;
                layout.drawer[i] = !open;
                tab.layout = Some(layout);
            }
        });
    });
}

/// Voice `i`'s less-used controls, in its drawer.
fn drawer_fields(
    ui: &mut egui::Ui,
    state: &TapeState,
    e: Extent,
    i: usize,
    actions: &mut Vec<Action>,
) {
    let v = &state.voices[i];
    let n = i as u8 + 1;
    let name = format!("Voice {n}");
    let why = |c| no_effect(state, e, c).map(idle_text);
    let mut set = |s| actions.push(Action::Tape(T::Voice(n, s)));
    let label = |heading: &str| format!("{name} {}", heading.to_lowercase());
    fields(ui, ("tape drawer", i), &DRAWER_ROWS, |ui| {
        let max = playr_looper::MAX_SLEW_MS;
        if let Some(x) = ms(ui, v.slew, max, label("slew"), why(C::Slew(i))) {
            set(V::Slew(x));
        }
        let unit: [Field<V>; 2] = [
            (v.wear, C::Wear(i), V::Wear, "wear"),
            (v.drive, C::Drive(i), V::Drive, "drive"),
        ];
        for (value, control, setting, heading) in unit {
            if let Some(x) = field(ui, value, 0.0..=1.0, label(heading), why(control)) {
                set(setting(x));
            }
        }
        let (lo, hi) = playr_looper::FILTER_HZ;
        let hz: [Field<V>; 3] = [
            (v.high_pass, C::HighPass(i), V::HighPass, "high-pass"),
            (v.low_pass, C::LowPass(i), V::LowPass, "low-pass"),
            (v.bell_hz, C::BellHz(i), V::BellHz, "bell"),
        ];
        for (value, control, setting, heading) in hz {
            if let Some(x) = log_field(ui, value, lo..=hi, 0, label(heading), why(control)) {
                set(setting(x));
            }
        }
        let db = playr_looper::BELL_DB;
        let gain = field(
            ui,
            v.bell_gain,
            -db..=db,
            label("bell gain"),
            why(C::BellGain(i)),
        );
        if let Some(x) = gain {
            set(V::BellGain(x));
        }
        let (q0, q1) = playr_looper::BELL_Q;
        if let Some(x) = log_field(ui, v.bell_q, q0..=q1, 2, label("bell q"), why(C::BellQ(i))) {
            set(V::BellQ(x));
        }
        if let Some(x) = ms(ui, v.fade, 1000.0, label("fade"), why(C::Fade(i))) {
            set(V::Fade(x));
        }
        let sends: [Field<V>; 2] = [
            (v.delay, C::DelaySend(i), V::Delay, "delay send"),
            (v.reverb, C::ReverbSend(i), V::Reverb, "reverb send"),
        ];
        for (value, control, setting, heading) in sends {
            if let Some(x) = field(ui, value, 0.0..=1.0, label(heading), why(control)) {
                set(setting(x));
            }
        }
        ui.end_row();
    });
}

/// Write, which records every voice's send into the write loop, voice 4's.
fn write_toggle(ui: &mut egui::Ui, state: &TapeState, e: Extent, actions: &mut Vec<Action>) {
    let why = no_effect(state, e, C::Write).map(idle_text);
    let mut write = state.write;
    let tick = dimmed(ui, why, |ui| ui.checkbox(&mut write, "Write"));
    let tick = match why {
        Some(_) => tick,
        None => tick.on_hover_text(
            "records every voice's send into voice 4's loop; with voice 4 empty, the first Write makes it, silent, the length of voice 1's loop",
        ),
    };
    tick.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, write, "Write on")
    });
    if tick.changed() {
        actions.push(Action::Tape(T::Write(write)));
    }
}

/// The write head's window and controls, in a second row of voice 4's lane.
fn write_fields(ui: &mut egui::Ui, state: &TapeState, e: Extent, actions: &mut Vec<Action>) {
    let why = |c| no_effect(state, e, c).map(idle_text);
    fields(ui, "tape write", &WRITE_ROWS, |ui| {
        let tip = WRITE_ROWS[0].1;
        if let Some((a, b)) = window(ui, state.write_window, e, "Write", tip, why(C::WriteWindow)) {
            actions.push(Action::Tape(T::WriteWindow(a, b)));
        }
        let controls: [Field<T>; 4] = [
            (state.feedback, C::Feedback, T::Feedback, "feedback"),
            (state.wear, C::WriteWear, T::Wear, "wear"),
            (state.thin, C::Thin, T::Thin, "thin"),
            (
                state.fx.print,
                C::PrintFx,
                |v| T::Fx(F::Print(v)),
                "print fx",
            ),
        ];
        for (value, control, action, heading) in controls {
            let name = format!("Write {heading}");
            if let Some(x) = field(ui, value, 0.0..=1.0, name, why(control)) {
                actions.push(Action::Tape(action(x)));
            }
        }
        ui.end_row();
    });
}

/// The delay's controls.
const DELAY_ROWS: [(&str, &str); 7] = [
    (
        "Delay",
        "the delay's model: clean repeats; ping-pong, each repeat crossing sides; or tape, with wow, flutter and saturation",
    ),
    ("Time", "the delay, in ms; setting it turns Sync off"),
    (
        "Sync",
        "sets Time to a part of voice 1's window, and follows it",
    ),
    ("Feedback", "how much of each echo repeats"),
    (
        "Tone",
        "a low-pass on the repeats, in Hz: each repeat darker; the first echo is not filtered",
    ),
    ("Wow", "the tape model's wow and flutter: the repeats drift in pitch"),
    ("Return", "the delay's level in the mix"),
];

/// The reverb's controls.
const REVERB_ROWS: [(&str, &str); 5] = [
    (
        "Reverb",
        "the reverb's model: Freeverb; Zita, an 8-line FDN; JP, a modulated hall; or Plate, Dattorro's",
    ),
    ("Size", "the room's size; Freeverb and JP"),
    ("Decay", "seconds for the tail to fall 60 dB; not Freeverb"),
    ("Damp", "how much faster the highs decay"),
    ("Return", "the reverb's level in the mix"),
];

/// A list of `table`'s models named `name`, showing `current`; the one
/// picked.
fn model_pick<M: Copy + PartialEq>(
    ui: &mut egui::Ui,
    table: &[(&'static str, M)],
    current: M,
    name: &str,
    idle: Option<&str>,
) -> Option<M> {
    let mut picked = None;
    let response = dimmed(ui, idle, |ui| {
        egui::ComboBox::from_id_salt(name)
            .selected_text(model_name(table, current))
            .show_ui(ui, |ui| {
                for (label, m) in table {
                    if ui.selectable_label(*m == current, *label).clicked() {
                        picked = Some(*m);
                    }
                }
            })
            .response
    });
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, name));
    picked
}

/// The FX lane: its header, and unfolded, the delay's and the reverb's rows.
fn fx_lane(
    ui: &mut egui::Ui,
    state: &TapeState,
    e: Extent,
    layout: Layout,
    tab: &mut State,
    actions: &mut Vec<Action>,
) {
    let fx = &state.fx;
    let why = |c| no_effect(state, e, c).map(idle_text);
    let mut set = |f| actions.push(Action::Tape(T::Fx(f)));
    let folded = !layout.fx;
    lane(
        ui,
        &mut (),
        |ui, _| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let tip = match folded {
                    true => "show the delay and reverb every voice sends to",
                    false => "fold the FX lane to its header",
                };
                if fold_toggle(ui, folded, "FX lane", tip) {
                    let mut layout = layout;
                    layout.fx = folded;
                    tab.layout = Some(layout);
                }
                ui.strong("FX");
            });
        },
        |ui, _| {
            if folded {
                let delay = model_name(&DELAY_MODELS, fx.delay_model);
                let reverb = model_name(&REVERB_MODELS, fx.reverb_model);
                ui.weak(format!(
                    "delay {delay}, reverb {reverb}; sends in each voice's drawer"
                ));
                return;
            }
            fields(ui, "tape delay", &DELAY_ROWS, |ui| {
                let idle = why(C::DelayModel);
                if let Some(m) = model_pick(ui, &DELAY_MODELS, fx.delay_model, "Delay model", idle)
                {
                    set(F::DelayModel(m));
                }
                let max = playr_looper::DELAY_SECONDS * 1000.0;
                if let Some(x) = ms(ui, fx.time, max, "Delay time".into(), why(C::Time)) {
                    set(F::Time(x.max(playr_looper::DELAY_MIN * 1000.0)));
                }
                if let Some(d) = sync_pick(ui, fx.sync, why(C::Sync)) {
                    set(F::Sync(d));
                }
                let fb = playr_looper::FEEDBACK_MAX;
                let feedback = field(
                    ui,
                    fx.feedback,
                    0.0..=fb,
                    "Delay feedback".into(),
                    why(C::DelayFeedback),
                );
                if let Some(x) = feedback {
                    set(F::Feedback(x));
                }
                let (lo, hi) = playr_looper::TONE_HZ;
                if let Some(x) =
                    log_field(ui, fx.tone, lo..=hi, 0, "Delay tone".into(), why(C::Tone))
                {
                    set(F::Tone(x));
                }
                if let Some(x) = field(ui, fx.wow, 0.0..=1.0, "Delay wow".into(), why(C::Wow)) {
                    set(F::Wow(x));
                }
                let ret = field(
                    ui,
                    fx.delay_return,
                    0.0..=1.0,
                    "Delay return".into(),
                    why(C::DelayReturn),
                );
                if let Some(x) = ret {
                    set(F::DelayReturn(x));
                }
                ui.end_row();
            });
            fields(ui, "tape reverb", &REVERB_ROWS, |ui| {
                let idle = why(C::ReverbModel);
                if let Some(m) =
                    model_pick(ui, &REVERB_MODELS, fx.reverb_model, "Reverb model", idle)
                {
                    set(F::ReverbModel(m));
                }
                if let Some(x) = field(ui, fx.size, 0.0..=1.0, "Reverb size".into(), why(C::Size)) {
                    set(F::Size(x));
                }
                let (lo, hi) = playr_looper::DECAY_SECONDS;
                if let Some(x) = log_field(
                    ui,
                    fx.decay,
                    lo..=hi,
                    1,
                    "Reverb decay".into(),
                    why(C::Decay),
                ) {
                    set(F::Decay(x));
                }
                if let Some(x) = field(ui, fx.damp, 0.0..=1.0, "Reverb damp".into(), why(C::Damp)) {
                    set(F::Damp(x));
                }
                let ret = field(
                    ui,
                    fx.reverb_return,
                    0.0..=1.0,
                    "Reverb return".into(),
                    why(C::ReverbReturn),
                );
                if let Some(x) = ret {
                    set(F::ReverbReturn(x));
                }
                ui.end_row();
            });
        },
    );
}

/// Sync's list: off, or a part of voice 1's window; the one picked.
fn sync_pick(
    ui: &mut egui::Ui,
    current: Option<Division>,
    idle: Option<&str>,
) -> Option<Option<Division>> {
    let mut picked = None;
    let response = dimmed(ui, idle, |ui| {
        egui::ComboBox::from_id_salt("Delay sync")
            .selected_text(current.map_or("off", Division::name))
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), "off").clicked() {
                    picked = Some(None);
                }
                for d in Division::ALL {
                    if ui.selectable_label(current == Some(d), d.name()).clicked() {
                        picked = Some(Some(d));
                    }
                }
            })
            .response
    });
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Delay sync"));
    picked
}
