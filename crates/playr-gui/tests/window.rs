//! The window, driven headless: clicks and keys as a person would give them,
//! checked against the shared model and against what the window shows.

#[path = "../../playr-core/tests/common/mod.rs"]
mod common;

use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use playr_app::config::Config;
use playr_app::dispatch::Frontend;
use playr_app::message::Message;
use playr_app::model::{Input, Model};
use playr_app::View;
use playr_core::db::{self, query, Track};
use playr_core::notice::{Notice, Outcome, Refusal};
use playr_gui::Gui;

/// A window over a library file of tracks A, B and C and playlists "early"
/// and "late", with its directory.
fn window() -> (Harness<'static, Gui>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = db::open(&dir.path().join("library.db")).unwrap();
    let ids: Vec<i64> = ["A", "B", "C"]
        .iter()
        .map(|title| {
            let track = Track {
                path: format!("/m/{title}.flac"),
                title: Some(title.to_string()),
                mtime: 1,
                size: 1,
                ..Default::default()
            };
            db::upsert(&conn, &track).unwrap()
        })
        .collect();
    query::save_playlist(&mut conn, "late", &ids[..2]).unwrap();
    query::save_playlist(&mut conn, "early", &ids[2..]).unwrap();
    let model = Model::new(conn, common::fake_player().0, Vec::new(), Config::default());
    let harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .build_ui_state(|ui, gui: &mut Gui| gui.show(ui), Gui::new(model));
    (harness, dir)
}

/// Types `text` as key presses outside any text field.
fn typing(harness: &mut Harness<'_, Gui>, text: &str) {
    for c in text.chars() {
        harness.event(egui::Event::Text(c.to_string()));
        harness.run_steps(2);
    }
}

fn model<'a>(harness: &'a Harness<'_, Gui>) -> &'a Model {
    harness.state().model()
}

#[test]
fn the_tabs_count_their_rows_and_switch_the_view() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    harness.get_by_label("Library 3");
    harness.get_by_label("Selection 0");
    harness.get_by_label("Playlists 2").click();
    harness.run_steps(2);
    assert_eq!(model(&harness).view(), View::Playlists);
    harness.get_by_label("early");
}

#[test]
fn keys_run_the_terminal_s_bindings() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    typing(&mut harness, "j");
    assert_eq!(model(&harness).cursor(View::Library), Some(1));
    harness.key_press(egui::Key::ArrowDown);
    harness.run_steps(2);
    assert_eq!(model(&harness).cursor(View::Library), Some(2));
    typing(&mut harness, "a");
    assert_eq!(model(&harness).session().selection().len(), 1);
    harness.key_press(egui::Key::Tab);
    harness.run_steps(2);
    assert_eq!(model(&harness).view(), View::Selection);
}

#[test]
fn a_click_moves_the_cursor_and_a_tick_selects() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    harness.get_by_label("C").click();
    harness.run_steps(2);
    assert_eq!(model(&harness).cursor(View::Library), Some(2));
    harness
        .get_all_by_role(egui::accesskit::Role::CheckBox)
        .next()
        .unwrap()
        .click();
    harness.run_steps(2);
    let selected: Vec<&str> = model(&harness)
        .session()
        .selection()
        .iter()
        .map(|t| t.path.as_str())
        .collect();
    assert_eq!(selected, ["/m/A.flac"]);
}

#[test]
fn a_question_waits_in_a_dialog() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    typing(&mut harness, "3d");
    assert!(matches!(model(&harness).input(), Input::Confirm(_)));
    harness.get_by_label("delete playlist \"early\"?");
    harness.get_by_label("No").click();
    harness.run_steps(2);
    assert_eq!(model(&harness).message(), Some(&Message::Cancelled));
    assert_eq!(model(&harness).session().playlists().len(), 2);

    typing(&mut harness, "dy");
    assert_eq!(model(&harness).session().playlists().len(), 1);
}

#[test]
fn the_search_field_filters_as_typed_and_esc_clears_it() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    typing(&mut harness, "/");
    harness.run_steps(2);
    harness.get_by_label("Search").type_text("b");
    harness.run_steps(2);
    // The `/` that opened the field is not part of the query.
    assert_eq!(model(&harness).input(), &Input::Search("b".into()));
    assert_eq!(model(&harness).results().map(<[Track]>::len), Some(1));
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert_eq!(model(&harness).results(), None);
    assert_eq!(model(&harness).input(), &Input::None);
}

#[test]
fn buttons_do_what_their_keys_do_and_the_message_shows() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    harness.get_by_label("Mark").click();
    harness.run_steps(2);
    assert_eq!(
        model(&harness).message(),
        Some(&Message::Core(Notice::Refused(Refusal::NothingPlaying)))
    );
    harness.get_by_label("nothing is playing");

    typing(&mut harness, "m");
    assert!(matches!(
        model(&harness).message(),
        Some(Message::Core(Notice::Done(Outcome::Mode(_))))
    ));
}

#[test]
fn a_command_typed_in_the_bar_runs() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    typing(&mut harness, ":");
    harness.run_steps(2);
    assert!(matches!(model(&harness).input(), Input::Command(_)));
    let bar = harness.get(
        egui_kittest::kittest::by()
            .role(egui::accesskit::Role::TextInput)
            .label(":"),
    );
    bar.type_text("view playlists");
    harness.run_steps(1);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    assert_eq!(model(&harness).view(), View::Playlists);
    assert_eq!(model(&harness).input(), &Input::None);
}

/// Puts tracks A, B and C in the selection and shows it.
fn selecting_all(harness: &mut Harness<'_, Gui>) {
    typing(harness, "aaa2");
}

fn selection(harness: &Harness<'_, Gui>) -> Vec<String> {
    model(harness)
        .session()
        .selection()
        .iter()
        .map(|t| t.title.clone().unwrap_or_default())
        .collect()
}

#[test]
fn a_row_menu_acts_on_the_row_right_clicked() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    selecting_all(&mut harness);
    assert_eq!(selection(&harness), ["A", "B", "C"]);
    // Escape closes a row's menu: no binding takes it in this view.
    harness.get_by_label("C").click_secondary();
    harness.run_steps(2);
    harness.get_by_label("Remove");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(
        harness.query_by_label("Remove").is_none(),
        "the menu stayed open"
    );

    harness.get_by_label("B").click_secondary();
    harness.run_steps(2);
    harness.get_by_label("Remove").click();
    harness.run_steps(2);
    assert_eq!(selection(&harness), ["A", "C"]);
    assert_eq!(
        model(&harness).message(),
        Some(&Message::Core(Notice::Done(Outcome::RemovedTrack {
            title: "B".into()
        })))
    );
}

#[test]
fn a_selection_row_dragged_moves_the_track() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    selecting_all(&mut harness);
    let from = harness.get_by_label("A").rect().center();
    let to = harness.get_by_label("C").rect().center();
    harness.drag_at(from);
    harness.run_steps(2);
    for step in 1..=4 {
        harness.hover_at(from + (to - from) * (step as f32 / 4.0));
        harness.run_steps(1);
    }
    harness.drop_at(to);
    harness.run_steps(3);
    assert_eq!(selection(&harness), ["B", "C", "A"]);
    assert_eq!(model(&harness).cursor(View::Selection), Some(2));
}

#[test]
fn the_menu_bar_performs_its_items() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    harness.get_by_label("View").click();
    harness.run_steps(2);
    harness.get_by_label("Playlists").click();
    harness.run_steps(2);
    assert_eq!(model(&harness).view(), View::Playlists);

    harness.get_by_label("Help").click();
    harness.run_steps(2);
    harness.get_by_label("Commands").click();
    harness.run_steps(2);
    assert_eq!(model(&harness).input(), &Input::CommandHelp);
    harness.get_by_label(":scan DIR");
}

/// The command bar's text field.
fn bar<'h>(harness: &'h Harness<'_, Gui>) -> egui_kittest::Node<'h> {
    harness.get(
        egui_kittest::kittest::by()
            .role(egui::accesskit::Role::TextInput)
            .label(":"),
    )
}

fn bar_text(harness: &Harness<'_, Gui>) -> String {
    match model(harness).input() {
        Input::Command(line) => line.text.clone(),
        other => panic!("the command bar is not open: {other:?}"),
    }
}

#[test]
fn the_command_bar_completes_with_tab_and_recalls_with_up() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    typing(&mut harness, ":");
    bar(&harness).type_text("vo");
    harness.run_steps(2);
    harness.key_press(egui::Key::Tab);
    harness.run_steps(2);
    assert_eq!(bar_text(&harness), "volume");
    bar(&harness).type_text(" 40");
    harness.run_steps(2);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    assert!((model(&harness).session().player().volume() - 0.4).abs() < 1e-3);

    typing(&mut harness, ":");
    harness.run_steps(2);
    harness.key_press(egui::Key::ArrowUp);
    harness.run_steps(2);
    assert_eq!(bar_text(&harness), "volume 40");
}

#[test]
fn shift_clicking_the_progress_bar_marks_there() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let conn = db::open(&dir.path().join("library.db")).unwrap();
    let track = Track {
        path: file.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let playing = Model::new(
        conn,
        common::fake_player().0,
        vec![track],
        Config::default(),
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .build_ui_state(|ui, gui: &mut Gui| gui.show(ui), Gui::new(playing));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while model(&harness).snapshot().status.duration.is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "never started playing"
        );
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run_steps(2);
    harness
        .get_by_label("Position")
        .click_modifiers(egui::Modifiers::SHIFT);
    harness.run_steps(2);
    match model(&harness).message() {
        Some(Message::Core(Notice::Done(Outcome::Marked { at, .. }))) => {
            assert!((4..=6).contains(&at.as_secs()), "marked at {at:?}")
        }
        other => panic!("no mark: {other:?}"),
    }
}

/// A window playing 12 s of loud and quiet stretches, showing the sampler once
/// its waveform is read, with slices written under `samples`.
fn sampling(samples: &std::path::Path) -> (Harness<'static, Gui>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hits.wav");
    common::levels(
        &file,
        8000,
        &[(1.0, -3.0), (2.0, -40.0), (1.0, -3.0), (8.0, -40.0)],
    );
    let track = Track {
        path: file.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let mut model = Model::new(
        db::open_memory().unwrap(),
        common::fake_player().0,
        vec![track],
        Config::default(),
    );
    model.session_mut().set_samples_dir(samples.to_path_buf());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .build_ui_state(|ui, gui: &mut Gui| gui.show(ui), Gui::new(model));
    harness.run_steps(2);
    typing(&mut harness, "4");
    wait(&mut harness, |m| {
        matches!(m.sampler().wave, playr_app::sampler::Wave::Ready { .. })
    });
    harness.run_steps(2);
    (harness, dir)
}

/// Steps until `done` holds for the model, or five seconds pass.
fn wait(harness: &mut Harness<'_, Gui>, done: impl Fn(&Model) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !done(model(harness)) {
        assert!(
            std::time::Instant::now() < deadline,
            "gave up waiting: {:?}",
            model(harness).message()
        );
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn the_waveform_seeks_marks_and_zooms_under_the_mouse() {
    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    let rect = harness.get_by_label("Track waveform").rect();

    // A shift-click a quarter of the way along marks near 3 s of 12.
    let quarter = egui::pos2(rect.left() + rect.width() / 4.0, rect.center().y);
    harness.hover_at(quarter);
    harness.run_steps(1);
    harness.event_modifiers(
        egui::Event::PointerButton {
            pos: quarter,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::SHIFT,
        },
        egui::Modifiers::SHIFT,
    );
    harness.event_modifiers(
        egui::Event::PointerButton {
            pos: quarter,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::SHIFT,
        },
        egui::Modifiers::SHIFT,
    );
    harness.run_steps(2);
    match model(&harness).message() {
        Some(Message::Core(Notice::Done(Outcome::Marked { at, .. }))) => {
            assert!((2..=3).contains(&at.as_secs()), "marked at {at:?}")
        }
        other => panic!("no mark: {other:?}"),
    }

    // A plain click three quarters along seeks near 9 s.
    let three_quarters = egui::pos2(rect.left() + rect.width() * 0.75, rect.center().y);
    harness.hover_at(three_quarters);
    harness.run_steps(1);
    harness.event(egui::Event::PointerButton {
        pos: three_quarters,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.event(egui::Event::PointerButton {
        pos: three_quarters,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);
    wait(&mut harness, |m| {
        (8.0..10.5).contains(&m.snapshot().position.as_secs_f64())
    });

    harness.hover_at(rect.center());
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 100.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(3);
    assert!(model(&harness).sampler().zoom > 0, "the wheel did not zoom");
    harness.get_by_label("Whole track").click();
    harness.run_steps(2);
    assert_eq!(model(&harness).sampler().zoom, 0);
}

#[test]
fn slices_planned_in_the_sampler_are_written_with_a_button() {
    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    // With nothing planned the button is disabled, so a click does nothing.
    harness.get_by_label("Write slices").click();
    harness.run_steps(2);
    assert_ne!(model(&harness).message(), Some(&Message::NoSlicesPlanned));
    harness.get_by_label("Slice").click();
    harness.run_steps(2);
    harness.get_by_label("Slice the region").click();
    harness.run_steps(2);
    wait(&mut harness, |m| m.sampler().pending.is_some());
    harness.run_steps(2);
    let planned = model(&harness)
        .sampler()
        .pending
        .as_ref()
        .unwrap()
        .spans
        .len();
    // No marks: the region is the whole track, one slice.
    assert_eq!(planned, 1);

    harness.get_by_label("Write slices").click();
    harness.run_steps(2);
    wait(&mut harness, |m| {
        matches!(
            m.message(),
            Some(Message::Core(Notice::Done(Outcome::Exported { .. })))
        )
    });
    let written = std::fs::read_dir(samples.path()).unwrap().count();
    assert_eq!(written, 1, "one export directory");
    assert!(model(&harness).sampler().pending.is_none());
}
