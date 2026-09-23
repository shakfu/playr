//! The library, selection and playlists views as tables.
//!
//! Only the rows in view are laid out, so a library of 50,000 tracks costs as
//! much to draw as a screenful. A click puts the cursor on a row, a double
//! click plays from it, as enter does, and a right click opens the row's menu
//! with the cursor on it. Selection rows can be dragged to a new place.

use std::collections::HashSet;

use eframe::egui;
use egui_extras::{Column, TableBuilder, TableRow};
use playr_app::action::Action;
use playr_app::dispatch::Frontend;
use playr_app::model::Model;
use playr_app::View;
use playr_core::columns::{self, SortKey};

use crate::controls::{self, Control};

const ROW: f32 = 20.0;

/// What a view was asked to do: the row to put the cursor on, if any, and
/// the action to perform, if any.
pub type Clicked = Option<(Option<usize>, Option<Action>)>;

/// The payload of a selection row being dragged: its index.
struct Dragged(usize);

/// The library or the selection, as `view` shows it.
pub fn tracks(model: &Model, ui: &mut egui::Ui, view: View, scroll: Option<usize>) -> Clicked {
    let mut result: Clicked = None;
    if view == View::Selection {
        ui.horizontal(|ui| {
            for control in controls::SELECTION_BAR {
                if ui.button(control.label).clicked() {
                    result = Some((None, Some(control.action.clone())));
                }
            }
        });
    }
    let tracks = match view {
        View::Library => model.listed(),
        _ => model.session().selection(),
    };
    if tracks.is_empty() {
        ui.weak(match (view, model.results()) {
            (View::Library, Some(_)) => "No matches. Esc in the search field clears it.",
            (View::Library, None) => "The library is empty. File, Add folder to library adds one.",
            _ => "The selection is empty. Tick tracks in the library to add them.",
        });
        return result;
    }
    let mut sorted: Option<SortKey> = None;
    let cursor = model.cursor(view);
    let playing = model.snapshot().status.current().cloned();
    let selected: HashSet<&str> = match view {
        View::Library => model
            .session()
            .selection()
            .iter()
            .map(|t| t.path.as_str())
            .collect(),
        _ => HashSet::new(),
    };
    let (menu, sense) = match view {
        View::Library => (controls::LIBRARY_ROW, egui::Sense::click()),
        _ => (controls::SELECTION_ROW, egui::Sense::click_and_drag()),
    };

    // Text in a row does not take the click, so it reaches the row.
    ui.style_mut().interaction.selectable_labels = false;
    let shown = model.columns().to_vec();
    let sort = model.session().sort().to_vec();
    let measures = model.measures_map().clone();
    let mut table = TableBuilder::new(ui)
        .id_salt(view.title())
        .striped(true)
        .sense(sense)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
    if view == View::Library {
        table = table.column(Column::exact(24.0));
    }
    if let Some(row) = scroll {
        table = table.scroll_to_row(row, None);
    }
    // A number needs only its own width; text shares what is left.
    for (i, column) in shown.iter().enumerate() {
        table = match (column.numeric(), i + 1 == shown.len()) {
            (true, _) => table.column(Column::exact(64.0)),
            (false, true) => table.column(Column::remainder().at_least(120.0).clip(true)),
            (false, false) => table.column(Column::initial(200.0).clip(true).resizable(true)),
        };
    }
    table
        .header(ROW, |mut header| {
            if view == View::Library {
                header.col(|_| {});
            }
            for column in &shown {
                header.col(|ui| {
                    // The heading sorts by its column, and clicking the one
                    // already sorted by turns it around.
                    let key = sort.first().filter(|k| k.column == *column);
                    let arrow = match key {
                        Some(k) if k.descending => " v",
                        Some(_) => " ^",
                        None => "",
                    };
                    let label = format!("{}{arrow}", column.heading());
                    if ui
                        .add(egui::Button::new(label).frame(false).wrap())
                        .clicked()
                    {
                        let descending = key.is_some_and(|k| !k.descending);
                        sorted = Some(SortKey {
                            column: *column,
                            descending,
                        });
                    }
                });
            }
        })
        .body(|body| {
            body.rows(ROW, tracks.len(), |mut row| {
                let i = row.index();
                let t = &tracks[i];
                row.set_selected(cursor == Some(i));
                if view == View::Library {
                    row.col(|ui| {
                        let mut ticked = selected.contains(t.path.as_str());
                        if ui.checkbox(&mut ticked, "").changed() {
                            result = Some((Some(i), Some(Action::Add)));
                        }
                    });
                }
                let is_playing = playing
                    .as_ref()
                    .is_some_and(|p| p.as_os_str() == t.path.as_str());
                let measured = measures.get(&t.path).copied().unwrap_or_default();
                for column in &shown {
                    row.col(|ui| {
                        let text = columns::cell(t, measured, *column).text(*column);
                        match column {
                            playr_core::columns::Column::Title => {
                                let title = egui::RichText::new(text);
                                ui.label(if is_playing { title.strong() } else { title });
                            }
                            c if c.numeric() => {
                                ui.weak(text);
                            }
                            _ => {
                                ui.label(text);
                            }
                        }
                    });
                }
                if let Some(picked) = respond(&row, menu) {
                    result = Some(picked);
                }
                if view == View::Selection {
                    let response = row.response();
                    response.dnd_set_drag_payload(Dragged(i));
                    if let Some(from) = response.dnd_release_payload::<Dragged>() {
                        if from.0 != i {
                            let by = i as i64 - from.0 as i64;
                            result = Some((Some(from.0), Some(Action::MoveTrack(by))));
                        }
                    }
                }
            });
        });
    // A heading click replaces the sort, keeping the keys under it as
    // tie-breakers, so clicking Album inside an artist's records still
    // groups them.
    if let Some(key) = sorted {
        let mut keys = vec![key];
        keys.extend(sort.iter().filter(|k| k.column != key.column).copied());
        keys.truncate(4);
        result = Some((None, Some(Action::SetSort(keys))));
    }
    result
}

/// The saved playlists with their track counts.
pub fn playlists(model: &Model, ui: &mut egui::Ui, scroll: Option<usize>) -> Clicked {
    let playlists = model.session().playlists();
    if playlists.is_empty() {
        ui.weak("No playlists. Build a selection, then save it.");
        return None;
    }
    let cursor = model.cursor(View::Playlists);
    let mut result: Clicked = None;
    ui.style_mut().interaction.selectable_labels = false;
    let mut table = TableBuilder::new(ui)
        .id_salt("playlists")
        .striped(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
    if let Some(row) = scroll {
        table = table.scroll_to_row(row, None);
    }
    table
        .column(Column::remainder().at_least(160.0).clip(true))
        .column(Column::exact(80.0))
        .header(ROW, |mut header| {
            for name in ["Playlist", "Tracks"] {
                header.col(|ui| {
                    ui.strong(name);
                });
            }
        })
        .body(|body| {
            body.rows(ROW, playlists.len(), |mut row| {
                let i = row.index();
                row.set_selected(cursor == Some(i));
                row.col(|ui| {
                    ui.label(&playlists[i].name);
                });
                row.col(|ui| {
                    ui.weak(playlists[i].len.to_string());
                });
                if let Some(picked) = respond(&row, controls::PLAYLIST_ROW) {
                    result = Some(picked);
                }
            });
        });
    result
}

/// What a click, double click or the row's menu, from `menu`, asks of a row.
fn respond(row: &TableRow<'_, '_>, menu: &[Control]) -> Option<(Option<usize>, Option<Action>)> {
    let i = row.index();
    let response = row.response();
    let mut picked = None;
    response.context_menu(|ui| {
        for control in menu {
            if ui.button(control.label).clicked() {
                picked = Some((Some(i), Some(control.action.clone())));
                ui.close();
            }
        }
    });
    if picked.is_some() {
        return picked;
    }
    if response.double_clicked() {
        Some((Some(i), Some(Action::Activate)))
    } else if response.clicked() || response.secondary_clicked() {
        Some((Some(i), None))
    } else {
        None
    }
}
