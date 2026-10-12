//! The delay's and the reverb's fields, as the Tape tab's FX lane and the
//! DJ tab's FX window show them. See `docs/dev/fx-buses.md`.

use eframe::egui;
use playr_app::fx::{model_name, Division, FxField, FxSetting as F, FxState};
use playr_app::fx::{DELAY_MODELS, REVERB_MODELS};
use playr_app::message::idle_text;
use playr_app::tape::Idle;

use crate::tape::{dimmed, field, fields, log_field, ms};

/// Why a field does nothing, as its tooltip says, for [`rows`].
pub(crate) fn why(idle: Option<Idle>) -> Option<&'static str> {
    idle.map(idle_text)
}

/// The delay's row and the reverb's, set as `fx`; each field dimmed when
/// `why` says it does nothing, and `set` given each change. `id` keeps the
/// grids of two tabs apart.
pub(crate) fn rows(
    ui: &mut egui::Ui,
    id: &str,
    fx: &FxState,
    why: &dyn Fn(FxField) -> Option<&'static str>,
    set: &mut dyn FnMut(F),
) {
    use FxField as X;
    fields(ui, (id, "delay"), &DELAY_ROWS, |ui| {
        if let Some(m) = model_pick(
            ui,
            &DELAY_MODELS,
            fx.delay_model,
            "Delay model",
            why(X::DelayModel),
        ) {
            set(F::DelayModel(m));
        }
        let max = playr_looper::DELAY_SECONDS * 1000.0;
        if let Some(x) = ms(ui, fx.time, max, "Delay time".into(), why(X::Time)) {
            set(F::Time(x.max(playr_looper::DELAY_MIN * 1000.0)));
        }
        if let Some(d) = sync_pick(ui, fx.sync, why(X::Sync)) {
            set(F::Sync(d));
        }
        let fb = playr_looper::FEEDBACK_MAX;
        if let Some(x) = field(
            ui,
            fx.feedback,
            0.0..=fb,
            "Delay feedback".into(),
            why(X::Feedback),
        ) {
            set(F::Feedback(x));
        }
        let (lo, hi) = playr_looper::TONE_HZ;
        if let Some(x) = log_field(ui, fx.tone, lo..=hi, 0, "Delay tone".into(), why(X::Tone)) {
            set(F::Tone(x));
        }
        if let Some(x) = field(ui, fx.wow, 0.0..=1.0, "Delay wow".into(), why(X::Wow)) {
            set(F::Wow(x));
        }
        let ret = field(
            ui,
            fx.delay_return,
            0.0..=1.0,
            "Delay return".into(),
            why(X::DelayReturn),
        );
        if let Some(x) = ret {
            set(F::DelayReturn(x));
        }
        ui.end_row();
    });
    fields(ui, (id, "reverb"), &REVERB_ROWS, |ui| {
        let idle = why(X::ReverbModel);
        if let Some(m) = model_pick(ui, &REVERB_MODELS, fx.reverb_model, "Reverb model", idle) {
            set(F::ReverbModel(m));
        }
        if let Some(x) = field(ui, fx.size, 0.0..=1.0, "Reverb size".into(), why(X::Size)) {
            set(F::Size(x));
        }
        let (lo, hi) = playr_looper::DECAY_SECONDS;
        if let Some(x) = log_field(
            ui,
            fx.decay,
            lo..=hi,
            1,
            "Reverb decay".into(),
            why(X::Decay),
        ) {
            set(F::Decay(x));
        }
        if let Some(x) = field(ui, fx.damp, 0.0..=1.0, "Reverb damp".into(), why(X::Damp)) {
            set(F::Damp(x));
        }
        let ret = field(
            ui,
            fx.reverb_return,
            0.0..=1.0,
            "Reverb return".into(),
            why(X::ReverbReturn),
        );
        if let Some(x) = ret {
            set(F::ReverbReturn(x));
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
        "sets Time to a part of a bar, and follows it: on the tape, voice 1's window; on the decks, 4 beats of the deck heard most",
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

/// Sync's list: off, or a part of a bar; the one picked.
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
