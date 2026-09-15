//! playr's desktop window: the terminal interface's features, drawn with egui.
//!
//! [`Gui`] wraps the shared [`Model`]. Each frame it samples the player, turns
//! keys into the same bindings the terminal uses, and draws the model. Every
//! control performs an [`Action`] through the model, so a button does what its
//! key does. `docs/dev/gui.md` sets out the design.

pub mod controls;
pub mod keys;
pub mod palette;
mod sampler;
mod transport;
mod views;

use std::time::Duration;

use controls::Control;
use eframe::egui;
use playr_app::action::Action;
use playr_app::command::{self, view_name};
use playr_app::dispatch::Frontend;
use playr_app::model::{Input, Model};
use playr_app::{Theme, View};
use playr_core::audio::State;

/// Ids of the text fields, so keys go to a field that has focus and to the
/// bindings otherwise.
const SEARCH: &str = "search";
const COMMAND: &str = "command";
const NAME: &str = "name";

/// How often to draw while something moves on screen.
const FRAME: Duration = Duration::from_millis(33);

/// The window's state: the shared model, and what only drawing with egui needs.
pub struct Gui {
    model: Model,
    /// The search field's text.
    search: String,
    /// The command bar's text.
    command: String,
    /// The text of a save or rename dialog.
    name: String,
    /// The input shown last frame, to notice a prompt opening.
    shown: Input,
    /// The view and cursor row shown last frame, to scroll to a row a key moved to.
    cursor: (View, Option<usize>),
    wheel: sampler::Wheel,
    /// The theme last handed to egui.
    theme: Option<Theme>,
}

impl Gui {
    pub fn new(model: Model) -> Gui {
        Gui {
            model,
            search: String::new(),
            command: String::new(),
            name: String::new(),
            shown: Input::None,
            cursor: (View::Library, None),
            wheel: sampler::Wheel::default(),
            theme: None,
        }
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    /// Draws one frame into `ui`, the whole window.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.model.refresh();
        self.model.expire_message();
        self.follow_theme(ui.ctx());
        self.follow_input(ui.ctx());
        self.keys(ui.ctx());

        self.dropped(ui.ctx());
        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::top("tabs").show(ui, |ui| self.tabs(ui));
        egui::Panel::bottom("transport").show(ui, |ui| self.bottom(ui));
        egui::CentralPanel::default().show(ui, |ui| self.view(ui));
        self.dialogs(ui.ctx());

        if self.model.quitting() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.model.snapshot().status.state == State::Playing {
            ui.ctx().request_repaint_after(FRAME);
        } else if self.model.message().is_some() {
            ui.ctx().request_repaint_after(Duration::from_secs(1));
        }
    }

    fn perform(&mut self, action: Action) {
        self.model.perform(action);
    }

    /// Hands egui the model's theme when it changes, from the settings or `:theme`.
    fn follow_theme(&mut self, ctx: &egui::Context) {
        let theme = self.model.theme();
        if self.theme != Some(theme) {
            ctx.set_theme(match theme {
                Theme::System => egui::ThemePreference::System,
                Theme::Light => egui::ThemePreference::Light,
                Theme::Dark => egui::ThemePreference::Dark,
            });
            self.theme = Some(theme);
        }
    }

    /// Readies the text field of a prompt that has just opened.
    fn follow_input(&mut self, ctx: &egui::Context) {
        let input = self.model.input().clone();
        let opened = std::mem::discriminant(&input) != std::mem::discriminant(&self.shown);
        if opened {
            match &input {
                Input::Search(query) => {
                    self.search = query.clone();
                    focus(ctx, SEARCH);
                }
                Input::Command(line) => {
                    self.command = line.text.clone();
                    focus(ctx, COMMAND);
                }
                Input::SavePlaylist(name) | Input::RenamePlaylist { name, .. } => {
                    self.name = name.clone();
                    focus(ctx, NAME);
                }
                _ => {}
            }
        }
        // A search cleared by a key or command empties the field too.
        if self.model.results().is_none() && !matches!(input, Input::Search(_)) {
            self.search.clear();
        }
        self.shown = input;
    }

    /// Turns key presses into actions, unless a text field has them.
    fn keys(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|m| m.focused()).is_some_and(|id| {
            [SEARCH, COMMAND, NAME]
                .iter()
                .any(|name| id == egui::Id::new(name))
        });
        let events = ctx.input(|i| i.events.clone());
        // Indices of the events a binding, an answer or a closed list took.
        let mut used = Vec::new();
        for (index, event) in events.iter().enumerate() {
            // A prompt's field takes the keys from the one that opened it on.
            if typing || prompting(self.model.input()) {
                break;
            }
            let Some(key) = keys::key_of(event) else {
                continue;
            };
            match self.model.input() {
                // Anything but `y` cancels, so a stray key cannot confirm.
                Input::Confirm(_) => self
                    .model
                    .answer(key == playr_app::action::Key::parse("y").expect("a key")),
                // Any key closes a list.
                Input::Help | Input::CommandHelp => self.model.set_input(Input::None),
                _ => match self.model.keymap().lookup(key, self.model.view()).cloned() {
                    Some(action) => self.perform(action),
                    // An unbound key stays for egui, as Escape closing a menu.
                    None => continue,
                },
            }
            used.push(index);
        }
        // A key taken here is not also seen by a widget this frame, as the `:`
        // that opens the command bar would be typed into it.
        ctx.input_mut(|i| {
            let mut index = 0;
            i.events.retain(|_| {
                index += 1;
                !used.contains(&(index - 1))
            });
        });
        self.follow_input(ctx);
    }

    /// Files dropped on the window play, as `:open`.
    fn dropped(&mut self, ctx: &egui::Context) {
        let paths: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !paths.is_empty() {
            self.perform(Action::Open(paths));
        }
    }

    /// The menu bar. File dialogs choose what to open or scan.
    fn menu(&mut self, ui: &mut egui::Ui) {
        let mut chosen = None;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open files...").clicked() {
                    ui.close();
                    chosen = rfd::FileDialog::new().pick_files().map(Action::Open);
                }
                if ui.button("Open folder...").clicked() {
                    ui.close();
                    chosen = rfd::FileDialog::new()
                        .pick_folder()
                        .map(|dir| Action::Open(vec![dir]));
                }
                if ui.button("Add folder to library...").clicked() {
                    ui.close();
                    chosen = rfd::FileDialog::new().pick_folder().map(Action::Scan);
                }
                if ui.button("Remove missing files...").clicked() {
                    ui.close();
                    chosen = rfd::FileDialog::new().pick_folder().map(Action::Prune);
                }
                ui.separator();
                items(ui, controls::FILE_MENU, &mut chosen);
            });
            let theme = Action::Theme(self.model.theme());
            ui.menu_button("View", |ui| {
                items(ui, controls::VIEW_MENU, &mut chosen);
                ui.separator();
                ui.menu_button("Theme", |ui| {
                    for control in controls::THEME_MENU {
                        if ui.radio(control.action == theme, control.label).clicked() {
                            chosen = Some(control.action.clone());
                            ui.close();
                        }
                    }
                });
            });
            ui.menu_button("Playback", |ui| {
                items(ui, controls::PLAYBACK_MENU, &mut chosen);
                ui.separator();
                items(ui, controls::MARKS, &mut chosen);
            });
            ui.menu_button("Slice", |ui| items(ui, controls::SLICE_MENU, &mut chosen));
            ui.menu_button("Help", |ui| items(ui, controls::HELP_MENU, &mut chosen));
        });
        if let Some(action) = chosen {
            self.perform(action);
        }
    }

    /// The view tabs, with their row counts, and the search field.
    fn tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for view in View::ALL {
                let title = match view {
                    View::Library => format!("{} {}", view.title(), self.model.listed().len()),
                    View::Selection => format!(
                        "{} {}",
                        view.title(),
                        self.model.session().selection().len()
                    ),
                    View::Playlists => format!(
                        "{} {}",
                        view.title(),
                        self.model.session().playlists().len()
                    ),
                    View::Sampler => view.title().to_string(),
                };
                if ui
                    .selectable_label(self.model.view() == view, title)
                    .clicked()
                {
                    self.perform(Action::ShowView(view));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.search_field(ui)
            });
        });
    }

    fn search_field(&mut self, ui: &mut egui::Ui) {
        // Read before the field, which takes the key.
        let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
        let field = egui::TextEdit::singleline(&mut self.search)
            .id(egui::Id::new(SEARCH))
            .desired_width(240.0);
        let response = ui.add(field);
        let label = ui.label("Search");
        let response = response.labelled_by(label.id);
        if response.changed() {
            self.model.search_as_typed(self.search.clone());
        }
        if response.lost_focus() && matches!(self.model.input(), Input::Search(_)) {
            self.model.end_search(!cancelled);
            if cancelled {
                self.search.clear();
            }
        }
    }

    /// The command bar when it is open, the message, and the transport.
    fn bottom(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        transport::show(&mut self.model, ui);
        if matches!(self.model.input(), Input::Command(_)) {
            self.command_bar(ui);
        } else {
            let text = self.model.message_text().unwrap_or_default().to_string();
            ui.colored_label(palette::Palette::of(ui.visuals()).yellow, text);
        }
        ui.add_space(4.0);
    }

    /// The `:` command line: Tab and Shift-Tab cycle through completions, the
    /// arrows recall earlier lines, enter runs it, and escape closes it.
    fn command_bar(&mut self, ui: &mut egui::Ui) {
        let Input::Command(mut line) = self.model.input().clone() else {
            return;
        };
        let id = egui::Id::new(COMMAND);
        let view = self.model.view();
        let names: Vec<String> = self
            .model
            .session()
            .playlists()
            .iter()
            .map(|p| p.name.clone())
            .collect();
        // Taken before the field sees them, or Tab would move focus away.
        let keys = if ui.memory(|m| m.has_focus(id)) {
            ui.input_mut(|i| {
                use egui::{Key, Modifiers};
                [
                    i.consume_key(Modifiers::NONE, Key::Tab),
                    i.consume_key(Modifiers::SHIFT, Key::Tab),
                    i.consume_key(Modifiers::NONE, Key::ArrowUp),
                    i.consume_key(Modifiers::NONE, Key::ArrowDown),
                ]
            })
        } else {
            [false; 4]
        };
        match keys {
            [true, ..] => line.complete(true, view, &names),
            [_, true, ..] => line.complete(false, view, &names),
            [_, _, true, _] => line.recall(true, self.model.history()),
            [.., true] => line.recall(false, self.model.history()),
            _ => {}
        }
        if keys.contains(&true) {
            self.command = line.text.clone();
            cursor_to_end(ui.ctx(), id, self.command.chars().count());
        }

        let matches = command::completions(&self.command, view, &names);
        if !self.command.is_empty() && matches.len() > 1 {
            ui.weak(
                matches
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("  "),
            );
        }
        let enter = ui.horizontal(|ui| {
            let label = ui.label(":");
            // Locked, so Tab completes rather than moving focus.
            let field = egui::TextEdit::singleline(&mut self.command)
                .id(id)
                .lock_focus(true)
                .desired_width(f32::INFINITY);
            let response = ui.add(field).labelled_by(label.id);
            if response.changed() {
                line.replace(&self.command);
            }
            response
                .lost_focus()
                .then(|| ui.input(|i| i.key_pressed(egui::Key::Enter)))
        });
        self.model.set_input(Input::Command(line));
        match enter.inner {
            Some(true) => {
                let text = std::mem::take(&mut self.command);
                self.model.run_command(&text);
            }
            Some(false) => {
                self.command.clear();
                self.model.set_input(Input::None);
            }
            None => {}
        }
    }

    fn view(&mut self, ui: &mut egui::Ui) {
        let view = self.model.view();
        let cursor = self.model.cursor(view);
        // A row the cursor moved to by a key, not a click, is scrolled into view.
        let scroll = (self.cursor != (view, cursor)).then_some(cursor).flatten();
        let clicked = match view {
            View::Library | View::Selection => views::tracks(&self.model, ui, view, scroll),
            View::Playlists => views::playlists(&self.model, ui, scroll),
            View::Sampler => {
                for action in sampler::show(&mut self.model, ui, &mut self.wheel) {
                    self.perform(action);
                }
                None
            }
        };
        if let Some((row, action)) = clicked {
            if let Some(row) = row {
                self.model.set_cursor(view, Some(row));
            }
            if let Some(action) = action {
                self.perform(action);
            }
        }
        self.cursor = (self.model.view(), self.model.cursor(self.model.view()));
    }

    /// The question, name dialog or list the model has open.
    fn dialogs(&mut self, ctx: &egui::Context) {
        match self.model.input().clone() {
            Input::Confirm(question) => {
                egui::Modal::new(egui::Id::new("confirm")).show(ctx, |ui| {
                    ui.label(question.question());
                    ui.horizontal(|ui| {
                        if ui.button("Yes").clicked() {
                            self.model.answer(true);
                        }
                        if ui.button("No").clicked() {
                            self.model.answer(false);
                        }
                    });
                });
            }
            Input::SavePlaylist(_) => self.name_dialog(ctx, "Save the selection as"),
            Input::RenamePlaylist { from, .. } => {
                self.name_dialog(ctx, &format!("Rename \"{}\" to", from.name))
            }
            Input::Help => {
                let rows = command::key_rows(self.model.keymap(), self.model.view());
                let title = format!("Keys in the {} view", view_name(self.model.view()));
                self.list(ctx, &title, &rows);
            }
            Input::CommandHelp => self.list(ctx, "Commands", &command::command_rows()),
            _ => {}
        }
    }

    /// A dialog collecting a playlist name, which saves or renames on enter.
    fn name_dialog(&mut self, ctx: &egui::Context, heading: &str) {
        egui::Modal::new(egui::Id::new("name dialog")).show(ctx, |ui| {
            ui.label(heading);
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let field = egui::TextEdit::singleline(&mut self.name).id(egui::Id::new(NAME));
            let response = ui.add(field);
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let (ok, cancel) = ui
                .horizontal(|ui| (ui.button("OK").clicked(), ui.button("Cancel").clicked()))
                .inner;
            if ok || enter {
                let name = std::mem::take(&mut self.name);
                match self.model.input().clone() {
                    Input::SavePlaylist(_) => self.model.save_as(&name),
                    Input::RenamePlaylist { from, .. } => self.model.rename_to(&from, &name),
                    _ => {}
                }
            } else if cancel || escape {
                self.model.set_input(Input::None);
            }
        });
    }

    /// A window of keys or commands and what they do; a row with no
    /// description is a heading.
    fn list(&mut self, ctx: &egui::Context, title: &str, rows: &[(String, String)]) {
        let mut open = true;
        egui::Window::new(title)
            .collapsible(false)
            .open(&mut open)
            .default_height(480.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("rows").striped(true).show(ui, |ui| {
                        for (keys, what) in rows {
                            if what.is_empty() {
                                ui.strong(keys);
                            } else {
                                ui.monospace(keys);
                                ui.label(what);
                            }
                            ui.end_row();
                        }
                    });
                });
            });
        if !open {
            self.model.set_input(Input::None);
        }
    }
}

impl eframe::App for Gui {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

/// Buttons for `controls`, in a menu or a row. The one clicked sets `chosen`.
fn items(ui: &mut egui::Ui, controls: &[Control], chosen: &mut Option<Action>) {
    for control in controls {
        if ui.button(control.label).clicked() {
            *chosen = Some(control.action.clone());
            ui.close();
        }
    }
}

/// Puts the text cursor of field `id` after its last character.
fn cursor_to_end(ctx: &egui::Context, id: egui::Id, chars: usize) {
    if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, id) {
        let end = egui::text::CCursor::new(chars);
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(ctx, id);
    }
}

fn focus(ctx: &egui::Context, name: &str) {
    ctx.memory_mut(|m| m.request_focus(egui::Id::new(name)));
}

/// A window that lists why playr could not start.
pub struct Errors(pub Vec<String>);

impl eframe::App for Errors {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("playr could not start");
            for error in &self.0 {
                ui.label(error);
            }
            if ui.button("Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}

/// Whether `input` is a prompt whose text field takes the keys.
fn prompting(input: &Input) -> bool {
    matches!(
        input,
        Input::Search(_)
            | Input::Command(_)
            | Input::SavePlaylist(_)
            | Input::RenamePlaylist { .. }
    )
}
