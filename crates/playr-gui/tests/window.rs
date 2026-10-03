//! The window, driven headless: clicks and keys as a person would give them,
//! checked against the shared model and against what the window shows.

#[path = "../../playr-core/tests/common/mod.rs"]
mod common;

use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use playr_app::config::Config;
use playr_app::dispatch::Frontend;
use playr_app::message::Message;
use playr_app::model::{Input, Model};
use playr_app::sampler::Edge;
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
    assert_eq!(model(&harness).view(), View::Queue);
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
    typing(&mut harness, "4d");
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
    harness.get_by_label("Playback").click();
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

#[test]
fn the_theme_menu_sets_egui_s_theme() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    let theme = |harness: &Harness<'_, Gui>| harness.ctx.options(|o| o.theme_preference);
    assert_eq!(theme(&harness), egui::ThemePreference::Dark);
    harness.get_by_label("View").click();
    harness.run_steps(2);
    // A submenu's button carries an arrow after its name.
    harness
        .get(egui_kittest::kittest::by().label_contains("Theme"))
        .hover();
    harness.run_steps(2);
    harness.get_by_label("Light").click();
    harness.run_steps(2);
    assert_eq!(model(&harness).theme(), playr_app::Theme::Light);
    assert_eq!(theme(&harness), egui::ThemePreference::Light);
    assert!(!harness.ctx.global_style().visuals.dark_mode);
}

/// Puts tracks A, B and C in the selection and shows it.
fn selecting_all(harness: &mut Harness<'_, Gui>) {
    typing(harness, "aaa3");
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
    harness.get_by_label(":rescan");
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
    typing(&mut harness, "5");
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

/// Chooses `method` in the slice row's drop-down, which plans with it.
fn plan(harness: &mut Harness<'_, Gui>, method: &str) {
    harness.get_by_label("Slice method").click();
    harness.run_steps(2);
    harness.get_by_label(method).click();
    harness.run_steps(2);
}

/// What the slice row's drop-down shows.
fn slice_method(harness: &Harness<'_, Gui>) -> String {
    let node = harness.get_by_label("Slice method");
    node.accesskit_node().value().unwrap_or_default()
}

/// The highest widget whose name contains `word`: a menu, which opens above
/// the tab and the rows that share its word.
fn topmost<'h>(harness: &'h Harness<'_, Gui>, word: &'h str) -> egui_kittest::Node<'h> {
    harness
        .query_all(egui_kittest::kittest::by().label_contains(word))
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap()
}

/// Clicks `item` under `submenu` of the Sampler menu.
fn sampler_menu(harness: &mut Harness<'_, Gui>, submenu: &str, item: &str) {
    topmost(harness, "Sampler").click();
    harness.run_steps(2);
    topmost(harness, submenu).hover();
    harness.run_steps(2);
    harness.get_by_label(item).click();
    harness.run_steps(2);
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

    // The new mark is selected; deselected, a plain click on it selects it.
    let current = model(&harness).snapshot().status.current().cloned();
    let marked = model(&harness).sampler().selected_mark(current.as_ref());
    assert!(marked.is_some(), "the new mark is not selected");
    sampler_menu(&mut harness, "Edit", "Deselect");
    assert_eq!(
        model(&harness).sampler().selected_mark(current.as_ref()),
        None
    );
    harness.hover_at(quarter);
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: quarter,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    harness.run_steps(2);
    assert_eq!(
        model(&harness).sampler().selected_mark(current.as_ref()),
        marked
    );

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
fn the_wheel_zooms_past_the_peaks_and_the_frames_are_read() {
    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    harness.get_by_label("Pause").click();
    harness.run_steps(2);
    let rect = harness.get_by_label("Track waveform").rect();
    for _ in 0..40 {
        harness.hover_at(rect.center());
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 100.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run_steps(1);
    }
    harness.run_steps(2);
    // The deepest zoom: a frame across 16 points.
    let scale = model(&harness).sampler().scale.unwrap();
    assert_eq!((scale.per_column, scale.per_frame), (1, 16));
    wait(&mut harness, |m| {
        let current = m.snapshot().status.current().cloned();
        let (a, b) = m.sampler().scale.unwrap().shown();
        m.sampler()
            .detail(current.as_ref())
            .is_some_and(|d| d.covers(a, b))
    });
    harness.run_steps(2);
    assert!(harness.query_by_label("reading frames").is_none());
    assert!(harness.query_by_label_contains("1/16 frame").is_some());
}

#[test]
fn a_drag_sets_the_range_and_the_bar_slices_it() {
    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    harness.get_by_label("Snap").click();
    harness.run_steps(2);
    assert!(model(&harness).sampler().snap);

    // A quarter to half along the 12 s track, near 3 s and 6 s at 8 kHz.
    let rect = harness.get_by_label("Track waveform").rect();
    let along = |x: f32| egui::pos2(rect.left() + rect.width() * x, rect.center().y);
    harness.hover_at(along(0.25));
    harness.run_steps(1);
    harness.drag_at(along(0.25));
    harness.run_steps(1);
    for x in [0.3, 0.4, 0.5] {
        harness.hover_at(along(x));
        harness.run_steps(1);
    }
    harness.drop_at(along(0.5));
    harness.run_steps(2);
    let current = model(&harness).snapshot().status.current().cloned();
    let (a, b) = model(&harness)
        .sampler()
        .range(current.as_ref())
        .expect("no range");
    // The whole track shows, so a point's frame is its columns times a
    // column's frames. The track has no zero crossing to snap to.
    let per_column = model(&harness).sampler().scale.unwrap().per_column as f32;
    let frame = |x: f32| (rect.width() * x * per_column) as u64;
    assert!(
        a.abs_diff(frame(0.25)) <= 96 && b.abs_diff(frame(0.5)) <= 96,
        "{a}..{b}, not {}..{}",
        frame(0.25),
        frame(0.5)
    );

    plan(&mut harness, "Equal");
    wait(&mut harness, |m| m.sampler().pending.is_some());
    let spans = model(&harness)
        .sampler()
        .pending
        .as_ref()
        .unwrap()
        .spans
        .clone();
    assert_eq!(spans.len(), 8, "the count starts at 8");
    assert_eq!((spans[0].0, spans[7].1), (a, Some(b)));

    harness.get_by_label("Discard slices").click();
    harness.run_steps(2);

    // Loop it, then drag its end back to three eighths along: the loop follows.
    harness.get_by_label("Loop").click();
    harness.run_steps(2);
    wait(&mut harness, |m| {
        m.snapshot().status.looping == Some((a, b))
    });
    let end_x = rect.left() + (b as f32 / per_column).round();
    let cursor = |h: &Harness<'_, Gui>| h.output().platform_output.cursor_icon;
    harness.hover_at(along(0.3));
    harness.run_steps(1);
    assert_eq!(
        cursor(&harness),
        egui::CursorIcon::Default,
        "away from the ends"
    );
    // Within reach of the end, not on it, the cursor offers to move it.
    harness.hover_at(egui::pos2(end_x - 6.0, rect.center().y));
    harness.run_steps(1);
    assert_eq!(cursor(&harness), egui::CursorIcon::ResizeHorizontal);
    assert_eq!(
        model(&harness).sampler().edge,
        Edge::Start,
        "not picked by hovering"
    );
    harness.drag_at(egui::pos2(end_x - 6.0, rect.center().y));
    harness.run_steps(1);
    for x in [0.45, 0.4, 0.375] {
        harness.hover_at(along(x));
        harness.run_steps(1);
    }
    harness.drop_at(along(0.375));
    harness.run_steps(2);
    let (start, end) = model(&harness)
        .sampler()
        .range(current.as_ref())
        .expect("no range");
    assert_eq!(start, a, "the start held");
    assert_eq!(
        model(&harness).sampler().edge,
        Edge::End,
        "the drag picked the end"
    );
    assert!(
        end.abs_diff(frame(0.375)) <= 96,
        "{end}, not {}",
        frame(0.375)
    );
    wait(&mut harness, |m| {
        m.snapshot().status.looping == Some((a, end))
    });

    // Select end, then Earlier: the end moves back a column.
    sampler_menu(&mut harness, "Range", "Select end");
    sampler_menu(&mut harness, "Edit", "Earlier");
    let moved = model(&harness).sampler().range(current.as_ref());
    assert_eq!(moved, Some((a, end - per_column as u64)));

    // Backspace removes the selected end, which clears the range and ends
    // the loop.
    harness.key_press(egui::Key::Backspace);
    harness.run_steps(2);
    assert_eq!(model(&harness).sampler().range(current.as_ref()), None);
    wait(&mut harness, |m| m.snapshot().status.looping.is_none());

    // The sensitivity starts from the settings, 0.5 by default.
    plan(&mut harness, "At onsets");
    wait(&mut harness, |m| m.sampler().pending.is_some());
    let cut = model(&harness).sampler().pending.as_ref().unwrap().job.cut;
    assert_eq!(cut, playr_core::samples::Cut::Onsets(0.5));
}

#[test]
fn slices_planned_in_the_sampler_are_written_with_a_button() {
    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    // With nothing planned there is nothing to write.
    assert!(harness.query_by_label("Write slices").is_none());
    sampler_menu(&mut harness, "Slice", "Slice the region");
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

#[test]
fn the_spectrogram_button_paints_one_texture_the_size_of_the_waveform() {
    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    let spectrograms = |harness: &Harness<'_, Gui>| -> Vec<[usize; 2]> {
        let textures = harness.ctx.tex_manager();
        let textures = textures.read();
        textures
            .allocated()
            .filter(|(_, meta)| meta.name == "spectrogram")
            .map(|(_, meta)| meta.size)
            .collect()
    };
    assert!(spectrograms(&harness).is_empty());
    harness.get_by_label("Display").click();
    harness.run_steps(2);
    harness.get_by_label("Spectrogram").click();
    harness.run_steps(3);
    assert_eq!(
        model(&harness).sampler().display,
        playr_app::Display::Spectrogram
    );
    // Reused across frames, a pixel a column and a row a point.
    let rect = harness.get_by_label("Track waveform").rect();
    let sizes = spectrograms(&harness);
    assert_eq!(sizes.len(), 1, "{sizes:?}");
    assert_eq!(sizes[0][1], rect.height() as usize);
    assert!(sizes[0][0] > 0 && sizes[0][0] <= rect.width() as usize);
}

#[test]
fn with_fit_on_dragging_one_end_then_the_other_moves_only_that_end() {
    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    let rect = harness.get_by_label("Track waveform").rect();
    let along = |x: f32| egui::pos2(rect.left() + rect.width() * x, rect.center().y);
    let drag = |harness: &mut Harness<'_, Gui>, from: egui::Pos2, to: egui::Pos2| {
        harness.hover_at(from);
        harness.run_steps(1);
        harness.drag_at(from);
        harness.run_steps(1);
        for k in 1..=4 {
            harness.hover_at(from + (to - from) * (k as f32 / 4.0));
            harness.run_steps(1);
        }
        harness.drop_at(to);
        harness.run_steps(2);
    };
    let range = |harness: &Harness<'_, Gui>| {
        let m = model(harness);
        let current = m.snapshot().status.current().cloned();
        m.sampler().range(current.as_ref()).expect("no range")
    };
    // Where frame `f` is drawn now.
    let x_of = |harness: &Harness<'_, Gui>, f: u64| {
        let scale = model(harness).sampler().scale.unwrap();
        let x = (f - scale.start) as f32 * scale.per_frame as f32 / scale.per_column as f32;
        egui::pos2(rect.left() + x, rect.center().y)
    };

    drag(&mut harness, along(0.25), along(0.5));
    harness.get_by_label("Fit").click();
    harness.run_steps(3);
    let (a, b) = range(&harness);

    let end = x_of(&harness, b);
    drag(&mut harness, end, end - egui::vec2(30.0, 0.0));
    let (a1, b1) = range(&harness);
    assert!(a1 == a && a < b1 && b1 < b, "{a}..{b} became {a1}..{b1}");

    // Centred on the end now, at the zoom that fits the range: zoomed out,
    // the start is in view to grab.
    harness.get_by_label("Zoom out").click();
    harness.run_steps(3);
    let start = x_of(&harness, a1);
    drag(&mut harness, start, start + egui::vec2(30.0, 0.0));
    let (a2, b2) = range(&harness);
    assert!(
        b2 == b1 && a1 < a2 && a2 < b1,
        "{a1}..{b1} became {a2}..{b2}"
    );
}

#[test]
fn a_click_on_the_overview_seeks_and_the_slider_shows_the_zoom() {
    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    harness.get_by_label("Track overview");
    let rect = harness.get_by_label("Track waveform").rect();
    harness.hover_at(rect.center());
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 100.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(3);
    let zoom = model(&harness).sampler().zoom;
    assert!(zoom > 0, "the wheel did not zoom");
    let slider = harness
        .get_by_label("Zoom")
        .accesskit_node()
        .numeric_value();
    assert_eq!(slider, Some(f64::from(zoom)));

    // Three quarters along the 12 s track seeks near 9 s.
    let overview = harness.get_by_label("Track overview").rect();
    let at = egui::pos2(
        overview.left() + overview.width() * 0.75,
        overview.center().y,
    );
    harness.hover_at(at);
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    harness.run_steps(2);
    wait(&mut harness, |m| {
        (8.5..10.0).contains(&m.snapshot().position.as_secs_f64())
    });
}

#[test]
fn with_scrub_on_a_drag_plays_under_the_pointer_then_loops_the_range() {
    use playr_core::audio::State;

    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    harness.get_by_label("Scrub").click();
    harness.run_steps(2);
    let rect = harness.get_by_label("Track waveform").rect();
    let along = |x: f32| egui::pos2(rect.left() + rect.width() * x, rect.center().y);
    harness.hover_at(along(0.25));
    harness.run_steps(1);
    harness.drag_at(along(0.25));
    harness.run_steps(1);
    harness.hover_at(along(0.4));
    harness.run_steps(1);
    // Held still, a moment from under the pointer plays, then pauses at its
    // end. The whole track shows, so a point is a column of 8 kHz frames.
    let per_column = model(&harness).sampler().scale.unwrap().per_column as f64;
    let pointer = f64::from(rect.width() * 0.4) * per_column / 8000.0;
    let end = pointer + playr_app::sampler::SCRUB.as_secs_f64();
    wait(&mut harness, |m| {
        let status = m.session().player().status();
        let secs = m.session().player().position().as_secs_f64();
        status.state == State::Paused && (secs - end).abs() < 0.01
    });
    harness.drop_at(along(0.5));
    harness.run_steps(2);
    let current = model(&harness).snapshot().status.current().cloned();
    let range = model(&harness).sampler().range(current.as_ref());
    assert!(range.is_some(), "no range");
    wait(&mut harness, |m| {
        let status = m.session().player().status();
        status.looping == range && status.state == State::Playing
    });
}

/// Drags across the waveform from `from` to `to`, each a fraction of its width.
fn drag_range(harness: &mut Harness<'_, Gui>, from: f32, to: f32) {
    let rect = harness.get_by_label("Track waveform").rect();
    let along = |x: f32| egui::pos2(rect.left() + rect.width() * x, rect.center().y);
    harness.hover_at(along(from));
    harness.run_steps(1);
    harness.drag_at(along(from));
    harness.run_steps(1);
    for k in 1..=4 {
        harness.hover_at(along(from + (to - from) * k as f32 / 4.0));
        harness.run_steps(1);
    }
    harness.drop_at(along(to));
    harness.run_steps(2);
}

#[test]
fn a_saved_loop_shows_under_the_stretch_it_spans_and_a_click_loops_it() {
    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    assert!(harness.query_by_label("Loop 1").is_none(), "no loop yet");
    let before = harness.get_by_label("Track waveform").rect();

    drag_range(&mut harness, 0.25, 0.5);
    let current = model(&harness).snapshot().status.current().cloned();
    let range = model(&harness).sampler().range(current.as_ref());
    assert!(range.is_some(), "no range");
    // Not looping, the button names what it saves.
    assert!(harness.query_by_label("Save loop").is_none());
    harness.get_by_label("Save range").click();
    harness.run_steps(3);
    assert_eq!(model(&harness).snapshot().loops[0], range);

    // The band sits under the range, and the waveform gave it the height.
    let wave = harness.get_by_label("Track waveform").rect();
    let band = harness.get_by_label("Loop 1").rect();
    let along = |x: f32| wave.left() + wave.width() * x;
    assert!(
        (band.left() - along(0.25)).abs() <= 2.0 && (band.right() - along(0.5)).abs() <= 2.0,
        "{band:?} under {wave:?}"
    );
    assert!(band.top() >= wave.bottom() && wave.height() < before.height());

    // Backspace clears the range; the loop brings it back and plays it.
    harness.key_press(egui::Key::Backspace);
    harness.run_steps(2);
    assert_eq!(model(&harness).sampler().range(current.as_ref()), None);
    harness.get_by_label("Loop 1").click();
    harness.run_steps(2);
    wait(&mut harness, |m| m.snapshot().status.looping == range);
    assert_eq!(model(&harness).sampler().range(current.as_ref()), range);
    harness.run_steps(2);
    assert!(harness.query_by_label("Save range").is_none());
    harness.get_by_label("Save loop");
}

#[test]
fn the_slice_drop_down_shows_the_plan_and_plans_or_discards_when_chosen() {
    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    let spans = |harness: &Harness<'_, Gui>| {
        let pending = model(harness).sampler().pending.as_ref();
        pending.map(|p| p.spans.clone()).unwrap_or_default()
    };
    let landed = |m: &Model| m.sampler().planning.is_none() && m.sampler().pending.is_some();
    assert_eq!(slice_method(&harness), "None");

    plan(&mut harness, "Equal");
    wait(&mut harness, landed);
    harness.run_steps(2);
    assert_eq!(slice_method(&harness), "Equal");
    assert_eq!(spans(&harness).len(), 8);
    let whole = spans(&harness)[0].0;

    // Chosen again, it plans again: now the range, not the region.
    drag_range(&mut harness, 0.25, 0.5);
    assert_eq!(spans(&harness)[0].0, whole, "the plan held");
    plan(&mut harness, "Equal");
    wait(&mut harness, |m| {
        let current = m.snapshot().status.current().cloned();
        let range = m.sampler().range(current.as_ref());
        let first = m.sampler().pending.as_ref().map(|p| p.spans[0].0);
        landed(m) && first == range.map(|r| r.0)
    });

    // A plan made elsewhere shows too, with its sensitivity.
    typing(&mut harness, ":");
    harness.run_steps(2);
    let bar = harness.get(
        egui_kittest::kittest::by()
            .role(egui::accesskit::Role::TextInput)
            .label(":"),
    );
    bar.type_text("slice onsets 0.3");
    harness.run_steps(1);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    wait(&mut harness, |m| {
        let cut = m.sampler().pending.as_ref().map(|p| p.job.cut);
        landed(m) && cut == Some(playr_core::samples::Cut::Onsets(0.3))
    });
    harness.run_steps(2);
    assert_eq!(slice_method(&harness), "At onsets");
    let slider = harness
        .query_all_by_label("Sensitivity")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Slider)
        .unwrap();
    assert_eq!(slider.accesskit_node().numeric_value(), Some(0.3f32 as f64));

    plan(&mut harness, "None");
    assert!(model(&harness).sampler().pending.is_none());
    assert_eq!(slice_method(&harness), "None");
    assert!(harness.query_by_label("Discard slices").is_none());
}

#[test]
fn the_waveform_s_menu_acts_on_the_point_right_clicked() {
    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    let menu = |harness: &mut Harness<'_, Gui>, item: &str| {
        harness.get_by_label("Track waveform").click_secondary();
        harness.run_steps(2);
        harness.get_by_label(item).click();
        harness.run_steps(2);
    };
    // No mark is under the pointer yet, so the menu offers none to edit.
    harness.get_by_label("Track waveform").click_secondary();
    harness.run_steps(2);
    assert!(harness.query_by_label("Delete mark").is_none());
    harness.get_by_label("Mark here").click();
    harness.run_steps(2);
    // The frame under the waveform's middle, at 8 kHz.
    let wave = harness.get_by_label("Track waveform").rect();
    let per_column = model(&harness).sampler().scale.unwrap().per_column;
    let middle = (wave.width() / 2.0) as u64 * per_column;
    let marks = model(&harness).snapshot().marks.clone();
    let marked = (marks[0].as_secs_f64() * 8000.0) as u64;
    assert!(
        marks.len() == 1 && marked.abs_diff(middle) <= 96,
        "{marks:?}"
    );

    menu(&mut harness, "Range starts here");
    let current = model(&harness).snapshot().status.current().cloned();
    let (a, b) = model(&harness)
        .sampler()
        .range(current.as_ref())
        .expect("no range");
    // From there to the track's end.
    assert!(a.abs_diff(middle) <= 96 && b == 96_000, "{a}..{b}");

    menu(&mut harness, "Delete mark");
    wait(&mut harness, |m| m.snapshot().marks.is_empty());
}

#[test]
fn convert_to_shows_once_enabled_and_is_disabled_until_convertwithmoss_is_installed() {
    // Off as shipped: the Slice menu has no Convert to.
    let (mut off, _dir) = window();
    off.run_steps(2);
    topmost(&off, "Sampler").click();
    off.run_steps(2);
    topmost(&off, "Slice").hover();
    off.run_steps(2);
    off.get_by_label("Slice the region");
    assert!(off
        .query(egui_kittest::kittest::by().label_contains("Convert to"))
        .is_none());
    assert!(off
        .query(egui_kittest::kittest::by().label_contains("Convert an export to"))
        .is_none());

    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("ConvertWithMoss");
    let config = Config::parse(&format!(
        "[extensions]\nconvert-with-moss.enable = true\nconvert-with-moss.path = '{}'",
        program.display()
    ))
    .unwrap();
    let model = Model::new(
        db::open_memory().unwrap(),
        common::fake_player().0,
        Vec::new(),
        config,
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 592.0))
        .build_ui_state(|ui, gui: &mut Gui| gui.show(ui), Gui::new(model));
    harness.run_steps(2);
    let open = |harness: &mut Harness<'_, Gui>| {
        topmost(harness, "Sampler").click();
        harness.run_steps(2);
        topmost(harness, "Slice").hover();
        harness.run_steps(2);
    };
    open(&mut harness);
    let entry = harness.get(egui_kittest::kittest::by().label_contains("Convert to"));
    assert!(entry.accesskit_node().is_disabled());
    let export = harness.get(egui_kittest::kittest::by().label_contains("Convert an export to"));
    assert!(export.accesskit_node().is_disabled());

    std::fs::write(&program, "").unwrap();
    harness.run_steps(2);
    let export = harness.get(egui_kittest::kittest::by().label_contains("Convert an export to"));
    assert!(!export.accesskit_node().is_disabled());
    let entry = harness.get(egui_kittest::kittest::by().label_contains("Convert to"));
    assert!(!entry.accesskit_node().is_disabled());
    entry.hover();
    harness.run_steps(2);
    harness.get_by_label("sf2").click();
    harness.run_steps(2);
    // Installed, so it gets as far as finding nothing to convert.
    assert_eq!(
        harness.state().model().message(),
        Some(&Message::Core(Notice::Refused(Refusal::NothingExported)))
    );
}

#[test]
fn the_sampler_menu_holds_every_table_no_button_draws() {
    for (submenu, table) in [
        ("Range", playr_gui::controls::RANGE_MENU),
        ("Edit", playr_gui::controls::EDIT_MENU),
        ("Loops", playr_gui::controls::LOOP_MENU),
    ] {
        let samples = tempfile::tempdir().unwrap();
        let (mut harness, _dir) = sampling(samples.path());
        topmost(&harness, "Sampler").click();
        harness.run_steps(2);
        topmost(&harness, submenu).hover();
        harness.run_steps(2);
        for control in table {
            harness.get_by_label(control.label);
        }
    }
}

/// A window at its minimum size, as `with_min_inner_size` in main.rs, playing a track with a long title, in `view`,
/// with the transport's buttons as words when `text`.
fn smallest(dir: &std::path::Path, view: &str, text: bool) -> Harness<'static, Gui> {
    sized(dir, view, text, 592.0)
}

/// The same, 800 points wide and `height` high.
fn sized(dir: &std::path::Path, view: &str, text: bool, height: f32) -> Harness<'static, Gui> {
    let file = dir.join("Boards of Canada - Inferno - 10 The Word Becomes Flesh.wav");
    common::levels(&file, 8000, &[(1.0, -3.0), (11.0, -40.0)]);
    let track = Track {
        path: file.to_string_lossy().into_owned(),
        title: Some("The Word Becomes Flesh (Live at the Inferno, Extended Version)".into()),
        artist: Some("Boards of Canada".into()),
        ..Default::default()
    };
    let model = Model::new(
        db::open_memory().unwrap(),
        common::fake_player().0,
        vec![track],
        Config {
            transport_text_buttons: text,
            ..Config::default()
        },
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, height))
        .build_ui_state(|ui, gui: &mut Gui| gui.show(ui), Gui::new(model));
    harness.run_steps(2);
    // The sampler reads the waveform only while shown.
    typing(&mut harness, "5");
    wait(&mut harness, |m| {
        m.snapshot().status.duration.is_some()
            && matches!(m.sampler().wave, playr_app::sampler::Wave::Ready { .. })
    });
    typing(&mut harness, view);
    harness.run_steps(4);
    harness
}

/// Fails unless every widget of `harness` is inside `window` and none
/// overlaps another.
fn assert_fits(harness: &Harness<'_, Gui>, window: egui::Rect, what: &str) {
    let leaves: Vec<(egui::Rect, String)> = harness
        .query_all(egui_kittest::kittest::by())
        .filter(|n| n.accesskit_node().children().next().is_none())
        .map(|n| {
            let a = n.accesskit_node();
            let name = a.label().or_else(|| a.value()).unwrap_or_default();
            (n.rect(), format!("{:?} {name:?}", a.role()))
        })
        .filter(|(r, _)| r.area() > 0.0)
        .collect();
    for (rect, name) in &leaves {
        assert!(window.contains_rect(*rect), "{what}: {name} at {rect:?}");
    }
    for (i, (a, an)) in leaves.iter().enumerate() {
        for (b, bn) in &leaves[i + 1..] {
            assert!(
                a.intersect(*b).area() <= 0.0,
                "{what}: {an} at {a:?} overlaps {bn} at {b:?}"
            );
        }
    }
}

#[test]
fn every_control_fits_the_smallest_window_without_overlap() {
    let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 592.0));
    for (view, text) in [("1", false), ("5", false), ("1", true)] {
        let dir = tempfile::tempdir().unwrap();
        let harness = smallest(dir.path(), view, text);
        harness.get_by_label("Level");
        assert_fits(&harness, window, &format!("view {view}"));
    }
}

#[test]
fn the_sampler_s_controls_leave_the_waveform_200_points_of_the_smallest_window() {
    let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 592.0));
    let dir = tempfile::tempdir().unwrap();
    let mut harness = smallest(dir.path(), "5", false);
    // The slice row at its widest: a slider, and a plan to review.
    plan(&mut harness, "At onsets");
    wait(&mut harness, |m| m.sampler().pending.is_some());
    harness.run_steps(4);
    harness.get_by_label("Discard slices");
    assert_fits(&harness, window, "a plan waiting");
    let height = harness.get_by_label("Track waveform").rect().height();
    assert!(height >= 200.0, "the waveform is {height} points high");
}

#[test]
fn replaygain_is_chosen_in_the_playback_menu() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    harness.get_by_label("Playback").click();
    harness.run_steps(2);
    harness
        .get(egui_kittest::kittest::by().label_contains("ReplayGain"))
        .hover();
    harness.run_steps(2);
    harness.get_by_label("album").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().model().replaygain(),
        playr_core::gain::ReplayGain::Album
    );
}

#[test]
fn transport_buttons_show_symbols_but_keep_their_names() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    for name in ["Prev", "Play", "From start", "Stop", "Next"] {
        harness.get_by_label(name);
    }
    assert!(harness.query_by_label("\u{23EE}").is_none());
}

#[test]
fn transport_text_buttons_show_words() {
    let width = |text: bool| {
        let dir = tempfile::tempdir().unwrap();
        let harness = smallest(dir.path(), "1", text);
        let rect = |label: &str| harness.get_by_label(label).rect();
        (rect("From start").width(), rect("Stop").width())
    };
    // Symbols share one size; words take their own.
    let (from_start, stop) = width(false);
    assert!(
        (from_start - stop).abs() < 0.5,
        "{from_start} against {stop}"
    );
    let (from_start, stop) = width(true);
    assert!(from_start > stop + 20.0, "{from_start} against {stop}");
}

#[test]
fn the_waveform_takes_the_height_the_controls_leave() {
    for height in [592.0, 720.0] {
        let dir = tempfile::tempdir().unwrap();
        let harness = sized(dir.path(), "5", false, height);
        let rect = |label: &str| harness.get_by_label(label).rect();
        let gap = rect("Prev").top() - rect("Slice method").bottom();
        assert!(
            gap < 30.0,
            "at {height} points, {gap} points below the controls"
        );
    }
}

#[test]
fn shift_tab_moves_to_the_previous_view() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    harness.run_steps(2);
    assert_eq!(model(&harness).view(), View::Sampler);
    // The binding took the key, so egui did not move focus with it.
    assert_eq!(harness.ctx.memory(|m| m.focused()), None);
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    harness.run_steps(2);
    assert_eq!(model(&harness).view(), View::Playlists);
    harness.key_press(egui::Key::Tab);
    harness.run_steps(2);
    assert_eq!(model(&harness).view(), View::Sampler);
}

#[test]
fn the_eq_button_opens_a_dialog_whose_sliders_set_bands() {
    let (mut harness, _dir) = window();
    harness.run_steps(2);
    let slider = |harness: &Harness<'_, Gui>, band: &str| {
        harness
            .query_all_by_label(band)
            .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Slider)
            .map(|n| n.rect())
    };
    assert_eq!(slider(&harness, "bass"), None, "the dialog starts closed");
    // The button, not the dialog's title, which shares its name.
    let button = |harness: &Harness<'_, Gui>| {
        let lowest = harness
            .query_all_by_label("EQ")
            .max_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
            .unwrap();
        lowest.click();
    };
    button(&harness);
    harness.run_steps(2);
    let (bass, treble) = (
        slider(&harness, "bass").unwrap(),
        slider(&harness, "treble").unwrap(),
    );

    // A press at a slider's right end boosts fully; Flat returns every band.
    let press = |harness: &mut Harness<'_, Gui>, at: egui::Pos2| {
        harness.event(egui::Event::PointerMoved(at));
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            });
        }
        harness.run_steps(2);
    };
    let end = |r: egui::Rect| r.right_center() - egui::vec2(1.0, 0.0);
    press(&mut harness, end(bass));
    press(&mut harness, end(treble));
    assert_eq!(model(&harness).snapshot().eq, [12.0, 0.0, 12.0]);
    harness.get_by_label("Flat").click();
    harness.run_steps(2);
    assert_eq!(model(&harness).snapshot().eq, [0.0; 3]);

    // The button closes it again.
    button(&harness);
    harness.run_steps(2);
    assert_eq!(slider(&harness, "bass"), None);
}

#[test]
fn a_planned_slice_start_is_clicked_and_dragged_over_a_mark() {
    let samples = tempfile::tempdir().unwrap();
    let (mut harness, _dir) = sampling(samples.path());
    plan(&mut harness, "Equal");
    wait(&mut harness, |m| m.sampler().pending.is_some());
    harness.run_steps(2);
    let starts = |harness: &Harness<'_, Gui>| -> Vec<u64> {
        let plan = model(harness).sampler().pending.as_ref().unwrap();
        plan.spans.iter().map(|s| s.0).collect()
    };
    // Eight slices of the 12 s track, 1.5 s apart at 8 kHz.
    assert_eq!(starts(&harness)[..3], [0, 12_000, 24_000]);
    let rect = harness.get_by_label("Track waveform").rect();
    let scale = model(&harness).sampler().scale.unwrap();
    let x = |frame: u64| {
        let column = (frame - scale.start) as f32 / scale.per_column as f32;
        egui::pos2(rect.left() + column + 0.5, rect.center().y)
    };
    let click = |harness: &mut Harness<'_, Gui>, pos, modifiers| {
        harness.hover_at(pos);
        harness.run_steps(1);
        for pressed in [true, false] {
            harness.event_modifiers(
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                },
                modifiers,
            );
        }
        harness.run_steps(2);
    };
    let current = model(&harness).snapshot().status.current().cloned();
    let selected =
        |harness: &Harness<'_, Gui>| model(harness).sampler().selected_slice(current.as_ref());

    // A click on a slice's start selects the slice.
    click(&mut harness, x(12_000), egui::Modifiers::NONE);
    assert_eq!(selected(&harness), Some(12_000));

    // A mark on the third slice's start: the slice still takes the click.
    click(&mut harness, x(24_000), egui::Modifiers::SHIFT);
    let marks = model(&harness).snapshot().marks.clone();
    assert_eq!(marks.len(), 1, "no mark");
    click(&mut harness, x(24_000), egui::Modifiers::NONE);
    assert_eq!(selected(&harness), Some(24_000));

    // The pointer turns to a left-right arrow over a start, not between.
    let cursor = |h: &Harness<'_, Gui>| h.output().platform_output.cursor_icon;
    harness.hover_at(x(18_000));
    harness.run_steps(1);
    assert_eq!(cursor(&harness), egui::CursorIcon::Default);
    harness.hover_at(x(24_000));
    harness.run_steps(1);
    assert_eq!(cursor(&harness), egui::CursorIcon::ResizeHorizontal);

    // And the drag, with the arrow throughout: the slice's start moves, the
    // mark stays.
    harness.drag_at(x(24_000));
    harness.run_steps(1);
    for k in 1..=4 {
        harness.hover_at(x(24_000 + 1_200 * k));
        harness.run_steps(1);
        assert_eq!(cursor(&harness), egui::CursorIcon::ResizeHorizontal);
    }
    harness.drop_at(x(28_800));
    harness.run_steps(2);
    let moved = starts(&harness)[2];
    assert!(
        moved.abs_diff(28_800) <= 2 * scale.per_column,
        "slice 3 starts at {moved}"
    );
    assert_eq!(selected(&harness), Some(moved));
    assert_eq!(model(&harness).snapshot().marks, marks, "the mark moved");
}

#[test]
fn the_tape_tab_fits_the_smallest_window_and_a_view_key_leaves_it() {
    let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 592.0));
    let dir = tempfile::tempdir().unwrap();
    let mut harness = smallest(dir.path(), "1", false);
    harness.get_by_label("Tape").click();
    harness.run_steps(2);
    for label in [
        "Tape waveform",
        "Voice 1 rate",
        "Voice 3 end",
        "Voice 3 wear",
        "Write wear",
        "Write end",
        "Voice 2 on",
        "Voice 3 fade",
        "Write on",
        "Voice 2 ping",
        "Voice 3 solo",
        "Voice 1 slew",
        "Voice 3 drive",
        "Voice 3 filter",
        "Voice 3 band-pass",
        "Write thin",
    ] {
        harness.get_by_label(label);
    }
    assert_fits(&harness, window, "the Tape tab");

    // With no range set, Load range is refused as :tape load is.
    harness.get_by_label("Load range").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().model().message(),
        Some(&Message::Tape(playr_app::tape::TapeMessage::NoRange))
    );

    harness.key_press(egui::Key::Tab);
    harness.run_steps(2);
    assert_eq!(harness.state().model().view(), View::Queue);
    assert!(harness.query_by_label("Tape waveform").is_none());
}

#[test]
fn a_window_s_edge_and_body_drag_along_the_tape_waveform() {
    use playr_app::tape::{Deck, Extent};
    use playr_looper::Window;
    let dir = tempfile::tempdir().unwrap();
    let mut harness = smallest(dir.path(), "1", false);
    harness.state_mut().model_mut().set_deck(Deck::manual());
    // 2 s to 6 s of the 12 s track at 8 kHz, with a second either side.
    harness
        .state_mut()
        .model_mut()
        .perform(playr_app::action::Action::SetRange(Some((
            std::time::Duration::from_secs(2),
            std::time::Duration::from_secs(6),
        ))));
    harness.get_by_label("Tape").click();
    harness.run_steps(2);
    harness.get_by_label("Load range").click();
    wait(&mut harness, |m| m.deck().loaded().is_some());
    harness.run_steps(2);
    let e = harness.state().model().deck().loaded().unwrap();
    assert_eq!(
        e,
        Extent {
            frames: 48_000,
            range: Window::new(8000, 40_000),
            rate: 8000,
        }
    );
    let window =
        |h: &Harness<'_, Gui>, i: usize| h.state().model().deck().state().unwrap().voices[i].window;
    let rect = harness.get_by_label("Tape waveform").rect();
    // The 12-point write strip, the 98-point waveform, then a 12-point lane a voice.
    let strip = rect.top() + 6.0;
    let wave = rect.top() + 12.0 + 49.0;
    let lane = |i: usize| rect.top() + 12.0 + 98.0 + 12.0 * i as f32 + 6.0;
    let at = |frame: usize| rect.left() + rect.width() * frame as f32 / 48_000.0;
    let drag = |h: &mut Harness<'_, Gui>, y: f32, from: f32, to: f32| {
        h.hover_at(egui::pos2(from, y));
        h.run_steps(1);
        h.drag_at(egui::pos2(from, y));
        h.run_steps(1);
        for k in 1..=4 {
            h.hover_at(egui::pos2(from + (to - from) * k as f32 / 4.0, y));
            h.run_steps(1);
        }
        h.drop_at(egui::pos2(to, y));
        h.run_steps(2);
    };

    // On the waveform, voice 1's end, from the range's end to a quarter in.
    drag(&mut harness, wave, at(40_000), at(16_000));
    let w = window(&harness, 0);
    assert_eq!(w.start, 8000);
    assert!(w.end.abs_diff(16_000) <= 100, "{w:?}");

    // The whole window, later, by its middle.
    let len = w.len();
    drag(&mut harness, wave, at(12_000), at(28_000));
    let w = window(&harness, 0);
    assert_eq!(w.len(), len, "a moved window keeps its length");
    assert!(w.start.abs_diff(24_000) <= 100, "{w:?}");

    // An end dropped within reach of the range's end lands on it exactly.
    drag(&mut harness, wave, at(w.end), at(40_150));
    assert_eq!(window(&harness, 0).end, 40_000);

    // Voice 2's lane selects it, and the waveform then edits its window.
    drag(&mut harness, lane(1), at(8000), at(20_000));
    assert!(window(&harness, 1).start.abs_diff(20_000) <= 100);
    drag(&mut harness, wave, at(40_000), at(36_000));
    assert!(window(&harness, 1).end.abs_diff(36_000) <= 100);
    assert_eq!(window(&harness, 0).end, 40_000, "voice 1 stays");
    // Its name in the grid selects a voice too.
    harness.get_by_label("Voice 1").click();
    harness.run_steps(2);
    drag(&mut harness, wave, at(40_000), at(32_000));
    assert!(window(&harness, 0).end.abs_diff(32_000) <= 100);

    // The write window's start, in its strip; the voices' windows stay.
    drag(&mut harness, strip, at(8000), at(24_000));
    let ww = harness.state().model().deck().state().unwrap().write_window;
    assert!(
        ww.start.abs_diff(24_000) <= 100 && ww.end == 40_000,
        "{ww:?}"
    );
    assert!(window(&harness, 1).start.abs_diff(20_000) <= 100);

    // The toggles by each name, and the filter's type.
    for label in ["Voice 2 ping", "Voice 3 solo", "Voice 1 high-pass"] {
        harness.get_by_label(label).click();
        harness.run_steps(2);
    }
    let s = *harness.state().model().deck().state().unwrap();
    assert!(s.voices[1].ping && s.voices[2].solo);
    assert_eq!(s.voices[0].filter, playr_app::tape::Filter::High);
}
