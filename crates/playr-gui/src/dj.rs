//! The DJ tab: a zoomed waveform and an overview per deck, a strip of
//! controls per deck and the mixer between them. The terminal has the `:dj`
//! commands only; see `docs/dev/dj-engine.md`.

use eframe::egui;
use playr_app::action::Action;
use playr_app::dj::{
    Band, CueOut, Curve, DjAction as D, GridEdit, Loaded, Nudge, Range, Side, EQ_DB, HOT_CUES,
};
use playr_app::model::Model;

use crate::controls::{self, Control};
use crate::palette::Palette;

/// A zoomed waveform's height, and an overview's.
const WAVE: f32 = 64.0;
const OVERVIEW: f32 = 14.0;
/// Seconds of output either side of the playhead a zoomed waveform shows,
/// the same for both decks so beats in phase line up.
const SPAN: f64 = 3.0;
/// The phase meter's size.
const METER: egui::Vec2 = egui::vec2(120.0, 10.0);

/// What the tab keeps between frames: the held buttons, so a release is
/// sent once.
#[derive(Debug, Default)]
pub struct State {
    cue: [bool; 2],
    nudge: [Nudge; 2],
}

fn i(side: Side) -> usize {
    match side {
        Side::A => 0,
        Side::B => 1,
    }
}

fn letter(side: Side) -> &'static str {
    match side {
        Side::A => "A",
        Side::B => "B",
    }
}

/// Draws the tab and returns what its controls ask for.
pub fn show(model: &Model, ui: &mut egui::Ui, tab: &mut State) -> Vec<Action> {
    let mut actions = Vec::new();
    let decks = model.decks();
    if decks.loaded(Side::A).is_none() && decks.loaded(Side::B).is_none() {
        ui.weak("Choose a track in the library, then Load, or a row's Load to deck A or B.");
    }
    for side in [Side::A, Side::B] {
        waveform(ui, model, side);
    }
    ui.add_space(4.0);
    ui.columns(4, |columns| {
        deck(&mut columns[0], model, Side::A, tab, &mut actions);
        channel(&mut columns[1], model, Side::A, &mut actions);
        channel(&mut columns[2], model, Side::B, &mut actions);
        deck(&mut columns[3], model, Side::B, tab, &mut actions);
    });
    mixer(ui, model, &mut actions);
    actions
}

/// The deck's track around the playhead, fixed at the centre, with its beat
/// grid, bars heavier; under it the whole track, with the cue point.
fn waveform(ui: &mut egui::Ui, model: &Model, side: Side) {
    let size = egui::vec2(ui.available_width(), WAVE + OVERVIEW);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let name = format!("Deck {} waveform", letter(side));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &name));
    let visuals = ui.visuals();
    let palette = Palette::of(visuals);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
    let decks = model.decks();
    let (Some(loaded), Some(status), Some(rate)) =
        (decks.loaded(side), decks.status(), decks.rate())
    else {
        return;
    };
    let s = status.deck(side);
    let (pos, speed) = (s.pos(), s.rate().max(0.01));
    let wave = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), WAVE));
    let over = egui::Rect::from_min_size(wave.left_bottom(), egui::vec2(rect.width(), OVERVIEW));
    let rate = f64::from(rate);

    // Track frames either side of the playhead.
    let half = SPAN * rate * speed;
    let x = |frame: f64| wave.center().x + ((frame - pos) / half) as f32 * wave.width() / 2.0;
    let columns = wave.width().max(1.0) as usize;
    let per = 2.0 * half / columns as f64;
    let (mid, h) = (wave.center().y, WAVE / 2.0 - 2.0);
    for c in 0..columns {
        let a = pos - half + c as f64 * per;
        let (lo, hi) = (a.max(0.0) as u64, (a + per).max(0.0) as u64);
        if hi <= lo || lo >= loaded.frames as u64 {
            continue;
        }
        if let Some(e) = loaded.peaks.range(lo, hi) {
            let cx = wave.left() + c as f32 + 0.5;
            painter.line_segment(
                [
                    egui::pos2(cx, mid - e.max.min(1.0) * h),
                    egui::pos2(cx, mid - e.min.max(-1.0) * h),
                ],
                egui::Stroke::new(1.0, palette.rms),
            );
        }
    }
    if let Some(g) = loaded.grid {
        let period = 60.0 / g.bpm * rate;
        let first = ((pos - half) / rate - g.t0) * g.bpm / 60.0;
        let mut n = first.ceil();
        loop {
            let frame = (g.t0 * rate) + n * period;
            if frame > pos + half {
                break;
            }
            let bar = n.rem_euclid(4.0) == 0.0;
            let colour = match bar {
                true => palette.yellow,
                false => visuals.weak_text_color().gamma_multiply(0.6),
            };
            let bx = x(frame);
            painter.line_segment(
                [egui::pos2(bx, wave.top()), egui::pos2(bx, wave.bottom())],
                egui::Stroke::new(if bar { 1.5 } else { 1.0 }, colour),
            );
            n += 1.0;
        }
    }
    painter.line_segment(
        [
            egui::pos2(wave.center().x, wave.top()),
            egui::pos2(wave.center().x, wave.bottom()),
        ],
        egui::Stroke::new(2.0, palette.red),
    );

    overview(&painter, over, loaded, s.pos(), s.cue(), palette, visuals);
}

/// The whole track's peaks, the cue point and the playhead.
fn overview(
    painter: &egui::Painter,
    over: egui::Rect,
    loaded: &Loaded,
    pos: f64,
    cue: f64,
    palette: &Palette,
    visuals: &egui::Visuals,
) {
    let frames = loaded.frames.max(1) as f64;
    let columns = over.width().max(1.0) as usize;
    let per = frames / columns as f64;
    let (mid, h) = (over.center().y, OVERVIEW / 2.0 - 1.0);
    for c in 0..columns {
        let (lo, hi) = ((c as f64 * per) as u64, ((c + 1) as f64 * per) as u64);
        if let Some(e) = loaded.peaks.range(lo, hi.max(lo + 1)) {
            let cx = over.left() + c as f32 + 0.5;
            let p = e.max.max(-e.min).min(1.0) * h;
            painter.line_segment(
                [egui::pos2(cx, mid - p), egui::pos2(cx, mid + p)],
                egui::Stroke::new(1.0, palette.outside_rms),
            );
        }
    }
    let x = |f: f64| over.left() + (f / frames) as f32 * over.width();
    painter.line_segment(
        [
            egui::pos2(x(cue), over.top()),
            egui::pos2(x(cue), over.bottom()),
        ],
        egui::Stroke::new(2.0, palette.yellow),
    );
    painter.line_segment(
        [
            egui::pos2(x(pos), over.top()),
            egui::pos2(x(pos), over.bottom()),
        ],
        egui::Stroke::new(1.5, visuals.strong_text_color()),
    );
}

fn table(side: Side) -> &'static [Control] {
    match side {
        Side::A => controls::DECK_A,
        Side::B => controls::DECK_B,
    }
}

/// The button in `side`'s table labelled `label`.
fn button(ui: &mut egui::Ui, side: Side, label: &str, enabled: bool) -> Option<Action> {
    let c = table(side).iter().find(|c| c.label == label)?;
    let name = format!("Deck {} {}", letter(side), c.label);
    let r = ui.add_enabled(enabled, egui::Button::new(c.label));
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &name));
    r.clicked().then(|| c.action.clone())
}

/// A button that sends one action when pressed and another when released.
fn held(
    ui: &mut egui::Ui,
    text: &str,
    name: String,
    was: bool,
    enabled: bool,
) -> (bool, Option<bool>) {
    let r = ui.add_enabled(
        enabled,
        egui::Button::new(text).sense(egui::Sense::click_and_drag()),
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &name));
    let down = r.is_pointer_button_down_on();
    (down, (down != was).then_some(down))
}

/// A labelled slider with a value field; the new value on a change.
fn slider(
    ui: &mut egui::Ui,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    name: String,
    enabled: bool,
) -> Option<f32> {
    let mut v = value;
    let changed = ui
        .add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.spacing_mut().slider_width = (ui.available_width() - 56.0).max(40.0);
                let rail = ui.add(egui::Slider::new(&mut v, range.clone()).show_value(false));
                rail.widget_info(|| egui::WidgetInfo::slider(enabled, f64::from(v), &name));
                let field = ui.add(
                    egui::DragValue::new(&mut v)
                        .range(range)
                        .speed(0.05)
                        .fixed_decimals(2),
                );
                let label = format!("{name} value");
                field.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::DragValue, enabled, &label)
                });
                rail.changed() || field.changed()
            })
            .inner
        })
        .inner;
    changed.then_some(v)
}

/// A toggle named `name` for assistive technology; whether it was clicked.
fn toggle(ui: &mut egui::Ui, on: bool, text: &str, name: String, enabled: bool) -> bool {
    let r = ui.add_enabled(enabled, egui::Button::selectable(on, text));
    r.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, enabled, on, &name)
    });
    r.clicked()
}

/// A deck's strip: its track and tempo, the transport, the rate, the phase
/// against the other deck, and the grid's edits.
fn deck(ui: &mut egui::Ui, model: &Model, side: Side, tab: &mut State, actions: &mut Vec<Action>) {
    let decks = model.decks();
    let loaded = decks.loaded(side);
    let on = loaded.is_some();
    let status = decks.status().map(|s| s.deck(side));
    let k = letter(side);

    ui.horizontal(|ui| {
        ui.strong(format!("Deck {k}"));
        let title = match (loaded, decks.loading(side)) {
            (_, true) => "reading".to_string(),
            (Some(l), false) => l.title.clone(),
            (None, false) => "empty".to_string(),
        };
        ui.add(egui::Label::new(title).truncate());
    });
    ui.horizontal(|ui| {
        let bpm = status.and_then(|s| s.bpm());
        ui.monospace(match bpm {
            Some(b) => format!("{b:6.2} BPM"),
            None => "  --   BPM".into(),
        });
        phase(ui, model, side);
    });
    ui.horizontal(|ui| {
        actions.extend(button(ui, side, "Load", !decks.loading(side)));
        let playing = status.is_some_and(|s| s.playing());
        let label = if playing { "Pause" } else { "Play" };
        actions.extend(button(ui, side, label, on));
        let (down, change) = held(ui, "Cue", format!("Deck {k} Cue"), tab.cue[i(side)], on);
        tab.cue[i(side)] = down;
        if let Some(d) = change {
            actions.push(Action::Dj(D::CueHold(side, d)));
        }
        let synced = status.is_some_and(|s| s.synced());
        if toggle(ui, synced, "Sync", format!("Deck {k} Sync"), on) {
            actions.push(Action::Dj(D::Sync(side, !synced)));
        }
    });
    let range = status.map_or(Range::Narrow, |s| s.range());
    let p = range.percent() as f32;
    let pct = status.map_or(0.0, |s| s.pct() as f32);
    if let Some(v) = slider(ui, pct, -p..=p, format!("Deck {k} rate"), on) {
        actions.push(Action::Dj(D::Rate(side, v)));
    }
    ui.horizontal(|ui| {
        for r in [Range::Narrow, Range::Medium, Range::Wide] {
            let text = format!("{}%", r.percent());
            if toggle(ui, r == range, &text, format!("Deck {k} range {text}"), on) {
                actions.push(Action::Dj(D::Range(side, r)));
            }
        }
        let mut nudge = Nudge::Off;
        for (n, text) in [(Nudge::Behind, "-"), (Nudge::Ahead, "+")] {
            let was = tab.nudge[i(side)] == n;
            let (down, _) = held(ui, text, format!("Deck {k} nudge {text}"), was, on);
            if down {
                nudge = n;
            }
        }
        if nudge != tab.nudge[i(side)] {
            tab.nudge[i(side)] = nudge;
            actions.push(Action::Dj(D::Nudge(side, nudge)));
        }
    });
    let hot = status.map_or([None; HOT_CUES], |s| s.hot_cues());
    ui.horizontal(|ui| {
        ui.weak("Hot").on_hover_text(
            "a click sets a hot cue at the head, or jumps to it and plays; a right-click clears it",
        );
        for (n, at) in hot.iter().enumerate() {
            let text = (n + 1).to_string();
            let name = format!("Deck {k} hot cue {text}");
            let r = ui.add_enabled(on, egui::Button::selectable(at.is_some(), &text));
            r.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Button, on, at.is_some(), &name)
            });
            if r.clicked() {
                actions.push(Action::Dj(D::HotCue(side, n as u8 + 1)));
            }
            if r.secondary_clicked() {
                actions.push(Action::Dj(D::HotClear(side, n as u8 + 1)));
            }
        }
    });
    let looping = status.and_then(|s| s.looping());
    let beat = loaded
        .and_then(|l| l.grid)
        .zip(decks.rate())
        .map(|(g, r)| 60.0 / g.bpm * f64::from(r));
    let timed = beat.is_some();
    ui.horizontal(|ui| {
        ui.weak("Loop")
            .on_hover_text("loop this many beats; the lit one again ends it");
        for beats in [1.0f32, 2.0, 4.0, 8.0] {
            let lit = looping
                .zip(beat)
                .is_some_and(|((_, len), b)| (len / b - f64::from(beats)).abs() < 1e-6);
            let text = format!("{beats}");
            if toggle(ui, lit, &text, format!("Deck {k} loop {text}"), timed) {
                actions.push(Action::Dj(D::Loop(side, (!lit).then_some(beats))));
            }
        }
    });
    ui.horizontal(|ui| {
        ui.weak("Jump")
            .on_hover_text("move the head this many beats");
        for beats in [-4.0f32, -1.0, 1.0, 4.0] {
            let text = format!("{beats:+}");
            let name = format!("Deck {k} jump {text}");
            let r = ui.add_enabled(timed, egui::Button::new(&text));
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, timed, &name));
            if r.clicked() {
                actions.push(Action::Dj(D::Jump(side, beats)));
            }
        }
    });
    ui.horizontal(|ui| {
        ui.weak("Grid");
        for label in ["x2", "/2", "<", ">", "Tap"] {
            actions.extend(button(ui, side, label, on));
        }
    });
    ui.horizontal(|ui| {
        for (text, ms) in [("-1 ms", -1.0), ("+1 ms", 1.0)] {
            let name = format!("Deck {k} grid {text}");
            let r = ui.add_enabled(on, egui::Button::new(text));
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, on, &name));
            if r.clicked() {
                actions.push(Action::Dj(D::Grid(side, GridEdit::Offset(ms))));
            }
        }
        actions.extend(button(ui, side, "Reset", on));
    });
}

/// A deck's channel: its trim, EQ with kills, filter and fader.
fn channel(ui: &mut egui::Ui, model: &Model, side: Side, actions: &mut Vec<Action>) {
    let state = *model.decks().state();
    let k = letter(side);
    let s = i(side);
    ui.strong(format!("Channel {k}"));
    egui::Grid::new(format!("dj channel {k}"))
        .num_columns(2)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            ui.weak("Gain").on_hover_text("the deck's trim, in dB");
            if let Some(v) = slider(
                ui,
                state.gain[s],
                -12.0..=12.0,
                format!("Deck {k} gain"),
                true,
            ) {
                actions.push(Action::Dj(D::Gain(side, v)));
            }
            ui.end_row();
            let (lo, hi) = (EQ_DB.0 as f32, EQ_DB.1 as f32);
            for (band, name, tip) in [
                (Band::High, "High", "above 2.5 kHz, in dB; K kills it"),
                (Band::Mid, "Mid", "246 Hz to 2.5 kHz, in dB; K kills it"),
                (Band::Low, "Low", "below 246 Hz, in dB; K kills it"),
            ] {
                let b = band as usize;
                ui.weak(name).on_hover_text(tip);
                ui.horizontal(|ui| {
                    let killed = state.kill[s][b];
                    let label = format!("Deck {k} {} kill", name.to_lowercase());
                    if toggle(ui, killed, "K", label, true) {
                        actions.push(Action::Dj(D::Kill(side, band, !killed)));
                    }
                    let label = format!("Deck {k} {}", name.to_lowercase());
                    if let Some(v) = slider(ui, state.eq[s][b], lo..=hi, label, true) {
                        actions.push(Action::Dj(D::Eq(side, band, v)));
                    }
                });
                ui.end_row();
            }
            ui.weak("Filter")
                .on_hover_text("left of centre a low-pass, right a high-pass");
            if let Some(v) = slider(
                ui,
                state.filter[s],
                -1.0..=1.0,
                format!("Deck {k} filter"),
                true,
            ) {
                actions.push(Action::Dj(D::Filter(side, v)));
            }
            ui.end_row();
            ui.weak("Level").on_hover_text("the deck's channel fader");
            if let Some(v) = slider(
                ui,
                state.level[s],
                0.0..=1.0,
                format!("Deck {k} level"),
                true,
            ) {
                actions.push(Action::Dj(D::Level(side, v)));
            }
            ui.end_row();
        });
}

/// Where this deck's beat falls against the other's, a beat wide: centred
/// when in phase, right when ahead.
fn phase(ui: &mut egui::Ui, model: &Model, side: Side) {
    let size = egui::vec2(ui.available_width().min(METER.x), METER.y);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let name = format!("Deck {} phase", letter(side));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &name));
    let visuals = ui.visuals();
    let palette = Palette::of(visuals);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
    painter.line_segment(
        [rect.center_top(), rect.center_bottom()],
        egui::Stroke::new(1.0, visuals.weak_text_color()),
    );
    let Some(status) = model.decks().status() else {
        return;
    };
    let (Some(a), Some(b)) = (status.deck(side).phase(), status.deck(side.other()).phase()) else {
        return;
    };
    let e = (a - b + 0.5).rem_euclid(1.0) - 0.5;
    let mx = rect.center().x + e as f32 * rect.width();
    let colour = if e.abs() < 0.02 {
        palette.green
    } else {
        palette.yellow
    };
    painter.rect_filled(
        egui::Rect::from_center_size(egui::pos2(mx, rect.center().y), egui::vec2(4.0, METER.y)),
        1.0,
        colour,
    );
}

/// The crossfader, and what applies to both decks: quantize, the cue, where
/// the cue goes, and the crossfader's curve.
fn mixer(ui: &mut egui::Ui, model: &Model, actions: &mut Vec<Action>) {
    let state = *model.decks().state();
    ui.horizontal(|ui| {
        ui.weak("A - B").on_hover_text("the crossfader");
        ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(40.0);
        let mut x = state.xfade;
        let r = ui.add(egui::Slider::new(&mut x, 0.0..=1.0).show_value(false));
        r.widget_info(|| egui::WidgetInfo::slider(true, f64::from(x), "Crossfader"));
        if r.changed() {
            actions.push(Action::Dj(D::Xfade(x)));
        }
    });
    ui.horizontal(|ui| {
        if toggle(ui, state.quantize, "Quantize", "Quantize".into(), true) {
            actions.push(Action::Dj(D::Quantize(!state.quantize)));
        }
        ui.separator();
        ui.weak("Cue")
            .on_hover_text("the deck heard in the headphones");
        for (c, text) in [(None, "Off"), (Some(Side::A), "A"), (Some(Side::B), "B")] {
            if toggle(ui, state.cue_bus == c, text, format!("Cue {text}"), true) {
                actions.push(Action::Dj(D::CueBus(c)));
            }
        }
        ui.separator();
        ui.weak("Out").on_hover_text(
            "Split: the main mix in the left ear, the cue in the right; \
             3-4: the cue in stereo on channels 3 and 4 of a 4-channel device",
        );
        for (c, text) in [(CueOut::Split, "Split"), (CueOut::Channels, "3-4")] {
            if toggle(
                ui,
                state.cue_out == c,
                text,
                format!("Cue out {text}"),
                true,
            ) {
                actions.push(Action::Dj(D::CueOut(c)));
            }
        }
        ui.separator();
        ui.weak("Curve");
        for (c, text) in [(Curve::Smooth, "Smooth"), (Curve::Sharp, "Sharp")] {
            if toggle(ui, state.curve == c, text, format!("Curve {text}"), true) {
                actions.push(Action::Dj(D::Curve(c)));
            }
        }
    });
}
