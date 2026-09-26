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
    harness.get_by_label("Snap to zero").click();
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

    harness.get_by_label("Equal slices").click();
    harness.run_steps(2);
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
    harness.get_by_label("Loop range").click();
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

    // Move end, then Earlier: the end moves back a column.
    harness.get_by_label("Move end").click();
    harness.run_steps(2);
    harness.get_by_label("Earlier").click();
    harness.run_steps(2);
    let moved = model(&harness).sampler().range(current.as_ref());
    assert_eq!(moved, Some((a, end - per_column as u64)));

    // Escape clears the range and ends the loop.
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert_eq!(model(&harness).sampler().range(current.as_ref()), None);
    wait(&mut harness, |m| m.snapshot().status.looping.is_none());

    // The sensitivity starts from the settings, 0.5 by default.
    harness.get_by_label("Slice at onsets").click();
    harness.run_steps(2);
    wait(&mut harness, |m| m.sampler().pending.is_some());
    let cut = model(&harness).sampler().pending.as_ref().unwrap().job.cut;
    assert_eq!(cut, playr_core::samples::Cut::Onsets(0.5));
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
    harness.get_by_label("Fit range").click();
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

/// A window at its minimum size, as `with_min_inner_size` in main.rs, playing a track with a long title, in `view`,
/// with the transport's buttons as words when `text`.
fn smallest(dir: &std::path::Path, view: &str, text: bool) -> Harness<'static, Gui> {
    sized(dir, view, text, 480.0)
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
    typing(&mut harness, "4");
    wait(&mut harness, |m| {
        m.snapshot().status.duration.is_some()
            && matches!(m.sampler().wave, playr_app::sampler::Wave::Ready { .. })
    });
    typing(&mut harness, view);
    harness.run_steps(4);
    harness
}

#[test]
fn every_control_fits_the_smallest_window_without_overlap() {
    for (view, text) in [("1", false), ("4", false), ("1", true)] {
        let dir = tempfile::tempdir().unwrap();
        let harness = smallest(dir.path(), view, text);
        harness.get_by_label("Level");
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 480.0));
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
            assert!(
                window.contains_rect(*rect),
                "view {view}: {name} at {rect:?}"
            );
        }
        for (i, (a, an)) in leaves.iter().enumerate() {
            for (b, bn) in &leaves[i + 1..] {
                assert!(
                    a.intersect(*b).area() <= 0.0,
                    "view {view}: {an} at {a:?} overlaps {bn} at {b:?}"
                );
            }
        }
    }
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
    for height in [480.0, 720.0] {
        let dir = tempfile::tempdir().unwrap();
        let harness = sized(dir.path(), "4", false, height);
        let rect = |label: &str| harness.get_by_label(label).rect();
        let gap = rect("Prev").top() - rect("Discard slices").bottom();
        assert!(
            gap < 30.0,
            "at {height} points, {gap} points below the controls"
        );
    }
}
