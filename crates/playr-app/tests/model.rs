//! The shared model, driven with no drawing and no key events: what the
//! terminal does with a key, and a GUI with a control, arrives as these calls.

#[path = "../../playr-core/tests/common/mod.rs"]
mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use playr_app::action::Action;
use playr_app::command::CommandLine;
use playr_app::config::Config;
use playr_app::dispatch::{Confirm, Frontend};
use playr_app::message::Message;
use playr_app::model::{hold_peak, Input, Model, PEAK_HOLD};
use playr_app::View;
use playr_core::audio::Mode;
use playr_core::db::{self, query, Track};
use playr_core::notice::{Notice, Outcome, Refusal, Task};

fn track(path: &str) -> Track {
    Track {
        path: path.into(),
        mtime: 1,
        size: 1,
        ..Default::default()
    }
}

/// A model over a library file of tracks a, b and c and playlists "early"
/// and "late", with its directory.
fn model() -> (Model, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = db::open(&dir.path().join("library.db")).unwrap();
    let ids: Vec<i64> = ["/m/a.flac", "/m/b.flac", "/m/c.flac"]
        .iter()
        .map(|p| db::upsert(&conn, &track(p)).unwrap())
        .collect();
    query::save_playlist(&mut conn, "late", &ids[..2]).unwrap();
    query::save_playlist(&mut conn, "early", &ids[2..]).unwrap();
    let player = common::fake_player().0;
    (Model::new(conn, player, Vec::new(), Config::default()), dir)
}

#[test]
fn tracks_handed_over_are_selected_played_and_shown() {
    let tracks = vec![track("/x/one.wav"), track("/x/two.wav")];
    let conn = db::open_memory().unwrap();
    let model = Model::new(
        conn,
        common::fake_player().0,
        tracks.clone(),
        Config::default(),
    );
    assert_eq!(model.view(), View::Selection);
    assert_eq!(model.session().selection(), &tracks[..]);
    assert_eq!(model.playing(), &tracks[..]);
    assert_eq!(model.cursors().selection, Some(0));
}

#[test]
fn a_question_waits_for_its_answer() {
    let (mut model, _dir) = model();
    model.perform(Action::ShowView(View::Playlists));
    model.perform(Action::DeletePlaylist);
    assert!(matches!(
        model.input(),
        Input::Confirm(Confirm::DeletePlaylist(p)) if p.name == "early"
    ));

    model.answer(false);
    assert_eq!(model.input(), &Input::None);
    assert_eq!(model.message(), Some(&Message::Cancelled));
    assert_eq!(model.session().playlists().len(), 2);

    model.perform(Action::DeletePlaylist);
    model.answer(true);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::Deleted {
            name: "early".into()
        })))
    );
    assert_eq!(model.session().playlists().len(), 1);

    // With no question open, an answer does nothing.
    model.answer(true);
    assert_eq!(model.session().playlists().len(), 1);
}

#[test]
fn the_theme_starts_from_the_settings_and_changes_by_command() {
    use playr_app::Theme;
    let config = Config::parse("theme = \"light\"").unwrap();
    let conn = db::open_memory().unwrap();
    let mut model = Model::new(conn, common::fake_player().0, Vec::new(), config);
    assert_eq!(model.theme(), Theme::Light);
    model.run_command("theme dark");
    assert_eq!(model.theme(), Theme::Dark);
    assert_eq!(model.message(), Some(&Message::Theme(Theme::Dark)));
}

#[test]
fn the_sampler_snaps_ranges_marks_and_nudges_and_slices_the_range() {
    use playr_app::action::{Nudge, Slicing};
    use playr_app::sampler::{frame_of, Scale, Wave};

    // Four seconds at 8 kHz changing sign every half second: crossings at
    // frames 4,000, 8,000 ... 28,000. A snap reaches 80 frames either side.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("steps.wav");
    let parts: Vec<(f32, f32)> = (0..8)
        .map(|i| (0.5, if i % 2 == 0 { 0.25 } else { -0.25 }))
        .collect();
    common::levels(&file, 8000, &parts);
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    let ms = Duration::from_millis;
    let current = Some(file.clone());

    model.perform(Action::Nudge(Nudge::Columns(1)));
    assert_eq!(
        model.message(),
        Some(&Message::NoWaveform),
        "no columns drawn yet"
    );

    model.perform(Action::Snap(None));
    assert_eq!(model.message(), Some(&Message::Snap(true)));
    model.perform(Action::SetRange(Some((ms(1_005), ms(2_995)))));
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((8_000, 24_000))
    );

    // 1.508 s snaps to 1.5 s; 1.6 s has no crossing near. They are 100 ms
    // apart, which only the sampler view allows.
    model.perform(Action::MarkAt(ms(1_508)));
    model.perform(Action::MarkAt(ms(1_600)));
    let marks: Vec<u64> = model
        .session_mut()
        .marks_for(current.as_ref())
        .iter()
        .map(|m| m.frame)
        .collect();
    assert_eq!(marks, [12_000, 12_800]);

    model.perform(Action::Slice(Slicing::Region));
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.sampler().pending.is_none() {
        assert!(
            Instant::now() < deadline,
            "nothing planned: {:?}",
            model.message()
        );
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    let plan = model.sampler().pending.as_ref().unwrap();
    assert_eq!(plan.job.range, Some((8_000, 24_000)));
    assert_eq!(plan.spans, [(8_000, Some(24_000))]);

    // Paused, so the position is where each seek put it.
    model.perform(Action::TogglePause);
    model.set_scale(Scale {
        start: 0,
        per_column: 64,
        per_frame: 1,
        columns: 100,
    });
    let at = |model: &Model, frame: u64| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while frame_of(model.session().player().position(), 8000) != frame {
            assert!(
                Instant::now() < deadline,
                "at {:?}, not frame {frame}",
                model.session().player().position()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    model.perform(Action::SeekTo(ms(990)));
    at(&model, 8_000);
    model.perform(Action::Nudge(Nudge::Columns(1)));
    at(&model, 8_064);
    model.perform(Action::Nudge(Nudge::Columns(-1)));
    at(&model, 8_000);
    model.perform(Action::Nudge(Nudge::Percent(-10)));
    at(&model, 7_360);
}

#[test]
fn snap_on_snaps_the_range_and_fit_zooms_to_it() {
    use playr_app::sampler::{Scale, Wave};

    // Crossings at frames 4,000, 8,000 ... 28,000, as above.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("steps.wav");
    let parts: Vec<(f32, f32)> = (0..8)
        .map(|i| (0.5, if i % 2 == 0 { 0.25 } else { -0.25 }))
        .collect();
    common::levels(&file, 8000, &parts);
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    let ms = Duration::from_millis;
    let current = Some(file.clone());

    model.perform(Action::SetRange(Some((ms(1_005), ms(1_495)))));
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((8_040, 11_960))
    );
    model.perform(Action::Snap(Some(true)));
    assert_eq!(model.message(), Some(&Message::Snap(true)));
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((8_000, 12_000))
    );

    // Only a start: it snaps alone.
    model.perform(Action::Snap(Some(false)));
    model.sampler_mut().range = None;
    model.sampler_mut().set_range_start(&file, 16_050);
    model.perform(Action::Snap(Some(true)));
    assert_eq!(
        model.sampler().range_ends(current.as_ref()),
        (Some(16_000), None)
    );

    // Fitting with no columns drawn yet keeps the zoom.
    model.perform(Action::SetRange(Some((ms(1_000), ms(1_500)))));
    model.perform(Action::Fit(None));
    assert_eq!(model.message(), Some(&Message::Fit(true)));
    assert_eq!(model.sampler().zoom, 0);
    assert_eq!(model.sampler().centre(current.as_ref()), Some(10_000));
    model.set_scale(Scale {
        start: 0,
        per_column: 320,
        per_frame: 1,
        columns: 100,
    });
    model.perform(Action::Fit(Some(true)));
    assert_eq!(model.sampler().zoom, 2);
    // An end picked centres on it, at the zoom set; fitting again returns
    // to the whole range.
    use playr_app::sampler::Edge;
    model.perform(Action::PickEdge(Edge::End));
    assert_eq!(model.sampler().centre(current.as_ref()), Some(12_000));
    model.perform(Action::Zoom(playr_app::action::Zoom::In));
    model.perform(Action::PickEdge(Edge::Start));
    assert_eq!(model.sampler().centre(current.as_ref()), Some(8_000));
    assert_eq!(model.sampler().zoom, 3);
    model.perform(Action::Fit(Some(true)));
    assert_eq!(model.sampler().centre(current.as_ref()), Some(10_000));
    assert_eq!(model.sampler().zoom, 2);

    model.perform(Action::Fit(None));
    assert_eq!(model.message(), Some(&Message::Fit(false)));
    assert_eq!(model.sampler().centre(current.as_ref()), None);
    assert_eq!(model.sampler().zoom, 2, "turning it off keeps the zoom");
}

#[test]
fn a_looped_range_sliced_whole_is_planned_to_loop_and_takes_the_edge_setting() {
    use playr_app::action::Slicing;
    use playr_app::sampler::Wave;
    use playr_core::samples::Edges;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let config = Config::parse("slice_edges = \"fade\"").unwrap();
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        config,
    );
    assert_eq!(
        model.session().slice_edges(),
        Edges::Fade,
        "from the settings"
    );
    model.perform(Action::SetSliceEdges(Edges::Zero));
    assert_eq!(model.session().slice_edges(), Edges::Zero);
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    let ms = Duration::from_millis;
    model.perform(Action::SetRange(Some((ms(2_000), ms(3_000)))));

    let planned = |model: &mut Model, slicing| {
        model.perform(Action::Slice(slicing));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            model.refresh();
            if let Some(plan) = model.sampler().pending.clone() {
                model.perform(Action::DiscardSlices);
                return plan.job;
            }
            assert!(Instant::now() < deadline, "nothing planned");
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let job = planned(&mut model, Slicing::Region);
    assert_eq!((job.loops, job.edges), (false, Edges::Zero), "not looping");
    model.perform(Action::Loop(Some(true)));
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.session().player().status().looping.is_none() {
        assert!(Instant::now() < deadline, "never looped");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(planned(&mut model, Slicing::Region).loops);
    assert!(
        !planned(&mut model, Slicing::Equal(4)).loops,
        "more than one slice"
    );
}

#[test]
fn restart_plays_from_the_range_start_or_the_track_start() {
    use playr_app::sampler::Wave;
    use playr_core::audio::State;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    let playing_near = |model: &Model, from: Duration| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = model.session().player().status();
            let at = model.session().player().position();
            if status.state == State::Playing && at >= from && at < from + Duration::from_secs(1) {
                return;
            }
            assert!(Instant::now() < deadline, "{:?} at {at:?}", status.state);
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let ms = Duration::from_millis;

    // Paused past a range, it plays from the range's start.
    model.perform(Action::SetRange(Some((ms(4_000), ms(6_000)))));
    // Each waits for the engine: `Restart` reads the state it published.
    let settle = |model: &Model, state: State| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while model.session().player().status().state != state {
            assert!(Instant::now() < deadline, "never {state:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    model.perform(Action::TogglePause);
    settle(&model, State::Paused);
    model.perform(Action::SeekTo(ms(8_000)));
    model.perform(Action::Restart);
    playing_near(&model, ms(4_000));

    // Stopped, with no range, it plays from the track's start.
    model.perform(Action::SetRange(None));
    model.perform(Action::Stop);
    settle(&model, State::Stopped);
    model.perform(Action::Restart);
    playing_near(&model, Duration::ZERO);
}

#[test]
fn the_range_loops_follows_its_changes_and_escape_clears_it() {
    use playr_app::sampler::Wave;
    use playr_core::audio::State;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    let status = |model: &Model| model.session().player().status();
    let settle = |model: &Model, done: &dyn Fn(&playr_core::audio::Status) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(&status(model)) {
            assert!(Instant::now() < deadline, "{:?}", status(model).looping);
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let ms = Duration::from_millis;

    // With no range, the region between the marks around the playhead loops.
    model.perform(Action::MarkAt(ms(9_000)));
    model.perform(Action::Loop(None));
    assert_eq!(model.message(), Some(&Message::Loop(true)));
    let current = model.snapshot().status.current().cloned();
    assert_eq!(model.sampler().range(current.as_ref()), Some((0, 72_000)));
    settle(&model, &|s| s.looping == Some((0, 72_000)));
    model.perform(Action::Loop(Some(false)));
    model.perform(Action::SetRange(None));
    settle(&model, &|s| s.looping.is_none());

    // A range with one end does not loop.
    model.perform(Action::RangeIn);
    model.perform(Action::Loop(None));
    assert_eq!(model.message(), Some(&Message::NoRangeToLoop));
    model.perform(Action::SetRange(None));

    // A paused track plays once it loops.
    model.perform(Action::TogglePause);
    settle(&model, &|s| s.state == State::Paused);
    model.perform(Action::SetRange(Some((ms(2_000), ms(3_000)))));
    model.perform(Action::Loop(None));
    assert_eq!(model.message(), Some(&Message::Loop(true)));
    settle(&model, &|s| {
        s.looping == Some((16_000, 24_000)) && s.state == State::Playing
    });

    // A new end moves the loop with it.
    model.perform(Action::SetRange(Some((ms(2_000), ms(2_500)))));
    settle(&model, &|s| s.looping == Some((16_000, 20_000)));

    // So does moving an end, which stops a frame short of the other.
    use playr_app::action::Nudge;
    use playr_app::sampler::{Edge, Scale};
    model.set_scale(Scale {
        start: 0,
        per_column: 64,
        per_frame: 1,
        columns: 100,
    });
    model.perform(Action::PickEdge(Edge::End));
    model.perform(Action::MoveSelected(Nudge::Columns(-2)));
    let current = model.snapshot().status.current().cloned();
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((16_000, 19_872))
    );
    settle(&model, &|s| s.looping == Some((16_000, 19_872)));
    model.perform(Action::PickEdge(Edge::Start));
    model.perform(Action::MoveSelected(Nudge::Percent(100)));
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((19_871, 19_872))
    );
    settle(&model, &|s| s.looping == Some((19_871, 19_872)));
    model.perform(Action::SetRange(Some((ms(2_000), ms(2_500)))));

    // Escape, with no slices planned, leaves the range.
    model.perform(Action::DiscardSlices);
    assert_eq!(model.message(), Some(&Message::NoSlicesPlanned));
    assert!(model.sampler().range.is_some());

    // Removing with nothing selected clears the range, which ends the loop.
    model.perform(Action::Deselect);
    model.perform(Action::RemoveSelected);
    assert_eq!(model.sampler().range, None);
    settle(&model, &|s| s.looping.is_none());
    model.perform(Action::RemoveSelected);
    assert_eq!(model.message(), Some(&Message::NothingSelected));
}

#[test]
fn a_close_view_reads_its_frames_and_again_once_it_leaves_them() {
    use playr_app::sampler::{DetailRead, Scale, Wave};

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("steps.wav");
    let parts: Vec<(f32, f32)> = (0..8)
        .map(|i| (0.5, if i % 2 == 0 { 0.25 } else { -0.25 }))
        .collect();
    common::levels(&file, 8000, &parts);
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let until = |model: &mut Model, done: &dyn Fn(&Model) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(model) {
            assert!(Instant::now() < deadline, "{:?}", model.sampler().detail);
            model.refresh();
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    until(&mut model, &|m| {
        matches!(m.sampler().wave, Wave::Ready { .. })
    });
    let current = Some(file.clone());

    // Columns of 64 frames draw from the peaks: nothing is read.
    let coarse = Scale {
        start: 8_000,
        per_column: 64,
        per_frame: 1,
        columns: 1_000,
    };
    model.set_scale(coarse);
    model.refresh();
    assert!(matches!(model.sampler().detail, DetailRead::None));

    // 16 columns a frame: 63 frames in view, and 2 s, 16,000 frames, either side.
    let close = Scale {
        per_column: 1,
        per_frame: 16,
        ..coarse
    };
    model.set_scale(close);
    model.refresh();
    assert!(matches!(
        model.sampler().detail,
        DetailRead::Reading {
            start: 0,
            end: 24_063,
            ..
        }
    ));
    until(&mut model, &|m| m.sampler().detail(Some(&file)).is_some());
    let detail = model.sampler().detail(current.as_ref()).unwrap();
    assert!(detail.covers(8_000, 8_063));
    assert!(detail.mean(8_000).unwrap() > 0.0 && detail.mean(7_999).unwrap() < 0.0);

    // Inside what was read, no new read; past it, another.
    model.set_scale(Scale {
        start: 20_000,
        ..close
    });
    model.refresh();
    assert!(matches!(model.sampler().detail, DetailRead::Ready { .. }));
    model.set_scale(Scale {
        start: 30_000,
        ..close
    });
    model.refresh();
    assert!(matches!(
        model.sampler().detail,
        DetailRead::Reading {
            start: 14_000,
            end: 32_000,
            ..
        }
    ));
}

#[test]
fn command_lines_run_and_are_remembered_even_when_they_fail() {
    let (mut model, _dir) = model();
    model.set_input(Input::Command(CommandLine::default()));
    model.run_command("mode shuffle");
    assert_eq!(model.input(), &Input::None);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::Mode(Mode::Shuffle))))
    );
    model.run_command("frob");
    assert!(matches!(model.message(), Some(Message::Command(_))));
    model.run_command("   ");

    let mut line = CommandLine::default();
    line.recall(true, model.history());
    assert_eq!(line.text, "frob");
    line.recall(true, model.history());
    assert_eq!(line.text, "mode shuffle", "a blank line was remembered");
}

#[test]
fn a_search_shows_results_as_typed_and_ends_kept_or_cleared() {
    let (mut model, _dir) = model();
    model.search_as_typed("b".into());
    assert_eq!(model.input(), &Input::Search("b".into()));
    assert_eq!(model.results().map(<[Track]>::len), Some(1));

    model.end_search(true);
    assert_eq!(model.input(), &Input::None);
    assert_eq!(model.results().map(<[Track]>::len), Some(1));
    assert_eq!(model.message(), None);

    model.search_as_typed("zzz".into());
    model.end_search(true);
    assert_eq!(model.message(), Some(&Message::NoMatches));

    model.end_search(false);
    assert_eq!(model.results(), None);
    assert_eq!(model.listed().len(), 3);
}

/// Refreshes until the message showing matches `done`, or five seconds pass.
fn refresh_until(model: &mut Model, done: impl Fn(&Message) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        model.refresh();
        if model.message().is_some_and(&done) {
            return;
        }
        assert!(Instant::now() < deadline, "saw {:?}", model.message());
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Three short WAV files in `dir`.
fn music(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    for name in ["a.wav", "b.wav", "c.wav"] {
        common::silence(&dir.join(name), 8000, 0.05);
    }
}

#[test]
fn a_scan_fills_a_library_that_started_empty() {
    let dir = tempfile::tempdir().unwrap();
    let songs = dir.path().join("music");
    music(&songs);
    let conn = db::open_memory().unwrap();
    let mut model = Model::new(conn, common::fake_player().0, Vec::new(), Config::default());
    model
        .session_mut()
        .set_library_path(dir.path().join("library.db"));

    model.perform(Action::Scan(songs.clone()));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::ScanStarted {
            dir: Some(songs.clone())
        })))
    );
    refresh_until(&mut model, |m| {
        matches!(m, Message::Core(Notice::Done(Outcome::Scanned { .. })))
    });
    assert_eq!(model.session().tracks().len(), 3);
    assert!(model.session().has_library_file());
    assert_eq!(
        model.cursors().library,
        Some(0),
        "no cursor on the new rows"
    );
}

#[test]
fn opened_files_are_added_to_the_selection_and_played() {
    let dir = tempfile::tempdir().unwrap();
    let songs = dir.path().join("music");
    music(&songs);
    let (mut model, _library) = model();
    model.perform(Action::Add);
    assert_eq!(model.session().selection().len(), 1);

    model.perform(Action::Open(vec![songs.clone()]));
    refresh_until(&mut model, |m| {
        matches!(m, Message::Core(Notice::Done(Outcome::Opened { .. })))
    });
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::Opened {
            tracks: 3,
            skipped: 0
        })))
    );
    assert_eq!(model.view(), View::Selection);
    assert_eq!(model.session().selection().len(), 4);
    assert_eq!(
        model.cursors().selection,
        Some(1),
        "not on the first opened"
    );
    let playing: Vec<&str> = model.playing().iter().map(|t| t.path.as_str()).collect();
    assert_eq!(playing.len(), 3);
    assert!(playing[0].ends_with("a.wav"), "{playing:?}");

    model.perform(Action::Open(vec![dir.path().join("gone")]));
    refresh_until(&mut model, |m| {
        matches!(m, Message::Core(Notice::Failed { .. }))
    });
    assert!(matches!(
        model.message(),
        Some(Message::Core(Notice::Failed {
            task: Task::Open,
            ..
        }))
    ));
    assert_eq!(
        model.session().selection().len(),
        4,
        "a failed open changed the selection"
    );
}

#[test]
fn a_peak_is_held_then_released() {
    let start = Instant::now();
    let held = hold_peak(None, 0.9, start);
    assert_eq!(held, Some((0.9, start)));
    // A lower reading inside the hold keeps the peak.
    let soon = start + PEAK_HOLD / 2;
    assert_eq!(hold_peak(held, 0.2, soon), held);
    // A higher one replaces it at once.
    assert_eq!(hold_peak(held, 0.95, soon), Some((0.95, soon)));
    // Once the hold has passed, the current reading shows.
    let later = start + PEAK_HOLD;
    assert_eq!(hold_peak(held, 0.2, later), Some((0.2, later)));
    assert_eq!(hold_peak(held, 0.0, later), None);
}

/// Waits until `count` passes `seen`, or five seconds pass.
fn wait_past(count: &std::sync::atomic::AtomicUsize, seen: usize) -> usize {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let now = count.load(std::sync::atomic::Ordering::SeqCst);
        if now > seen {
            return now;
        }
        assert!(Instant::now() < deadline, "no event arrived");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
#[test]
fn only_the_latest_slicing_is_shown() {
    use playr_app::action::Slicing;
    use playr_app::sampler::{plan_text, Wave};
    use playr_core::samples::Cut;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 60.0);
    let events = Arc::new(AtomicUsize::new(0));
    let counted = events.clone();
    let mut model = Model::waking(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
        move || {
            counted.fetch_add(1, Ordering::SeqCst);
        },
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }

    // The engine keeps its open file. Planning opens the path again, and
    // finds a pipe that blocks until the test writes the track into it.
    let moved = dir.path().join("moved.wav");
    std::fs::rename(&file, &moved).unwrap();
    let made = std::process::Command::new("mkfifo")
        .arg(&file)
        .status()
        .unwrap();
    assert!(made.success());

    let seen = events.load(Ordering::SeqCst);
    model.perform(Action::Slice(Slicing::Region));
    let seen = wait_past(&events, seen);
    // The region's plan waits in the channel while a newer slicing starts.
    model.perform(Action::Slice(Slicing::Onsets(None)));
    model.refresh();
    assert!(model.sampler().pending.is_none(), "an older plan was shown");
    assert_eq!(plan_text(model.sampler()), "planning slices");

    let mut pipe = std::fs::OpenOptions::new().write(true).open(&file).unwrap();
    std::io::copy(&mut std::fs::File::open(&moved).unwrap(), &mut pipe).unwrap();
    drop(pipe);
    wait_past(&events, seen);
    model.refresh();
    let pending = model.sampler().pending.as_ref().map(|p| p.job.cut);
    assert!(
        matches!(pending, Some(Cut::Onsets(_))),
        "{pending:?}, {:?}",
        model.message()
    );
    assert!(!plan_text(model.sampler()).contains("planning"));
}

/// A model on a library file in `dir`, with `auto_prune` as given.
fn model_in(dir: &Path, auto_prune: bool) -> Model {
    let library = dir.join("library.db");
    let conn = db::open(&library).unwrap();
    let mut config = Config::default();
    config.settings.auto_prune = auto_prune;
    let mut model = Model::new(conn, common::fake_player().0, Vec::new(), config);
    model.session_mut().set_library_path(library);
    model
}

/// Scans `songs` and refreshes until the scan reports.
fn scan_and_wait(model: &mut Model, songs: &Path) {
    model.perform(Action::Scan(songs.to_path_buf()));
    refresh_until(model, |m| {
        matches!(m, Message::Core(Notice::Done(Outcome::Scanned { .. })))
    });
}

#[test]
fn a_scan_that_finds_missing_files_asks_to_prune() {
    let dir = tempfile::tempdir().unwrap();
    let songs = dir.path().join("music");
    music(&songs);
    let mut model = model_in(dir.path(), false);
    scan_and_wait(&mut model, &songs);
    assert_eq!(model.input(), &Input::None, "asked with nothing missing");
    assert_eq!(model.session().tracks().len(), 3);

    std::fs::remove_file(songs.join("a.wav")).unwrap();
    scan_and_wait(&mut model, &songs);
    assert_eq!(
        model.input(),
        &Input::Confirm(Confirm::Prune(Some(songs.clone())))
    );
    assert_eq!(model.session().tracks().len(), 3, "pruned without a yes");
}

#[test]
fn a_finished_scan_does_not_ask_over_something_being_typed() {
    let dir = tempfile::tempdir().unwrap();
    let songs = dir.path().join("music");
    music(&songs);
    let mut model = model_in(dir.path(), false);
    scan_and_wait(&mut model, &songs);
    std::fs::remove_file(songs.join("a.wav")).unwrap();

    // The scan finishes on its own thread, so the question would land on
    // whatever is open and take the next keystroke as the answer.
    model.perform(Action::Scan(songs.clone()));
    let typed = Input::Command(CommandLine::default());
    model.set_input(typed.clone());
    refresh_until(&mut model, |m| {
        matches!(m, Message::Core(Notice::Done(Outcome::Scanned { .. })))
    });
    assert_eq!(model.input(), &typed, "asked over the command line");
}

#[test]
fn auto_prune_removes_missing_files_without_asking() {
    let dir = tempfile::tempdir().unwrap();
    let songs = dir.path().join("music");
    music(&songs);
    let mut model = model_in(dir.path(), true);
    scan_and_wait(&mut model, &songs);

    std::fs::remove_file(songs.join("a.wav")).unwrap();
    model.perform(Action::Scan(songs.clone()));
    refresh_until(&mut model, |m| {
        matches!(m, Message::Core(Notice::Done(Outcome::Pruned { .. })))
    });
    assert_eq!(
        model.input(),
        &Input::None,
        "asked although auto_prune is set"
    );
    assert_eq!(model.session().tracks().len(), 2);
}

#[test]
fn auto_prune_asks_instead_when_the_directory_could_not_be_read() {
    let dir = tempfile::tempdir().unwrap();
    let songs = dir.path().join("music");
    music(&songs);
    let mut model = model_in(dir.path(), true);
    scan_and_wait(&mut model, &songs);

    // Every file gone and the directory still there: an unmounted drive reads
    // the same way, so auto_prune steps aside and asks.
    for name in ["a.wav", "b.wav", "c.wav"] {
        std::fs::remove_file(songs.join(name)).unwrap();
    }
    scan_and_wait(&mut model, &songs);
    assert_eq!(
        model.input(),
        &Input::Confirm(Confirm::Prune(Some(songs.clone()))),
        "pruned a directory it could not read"
    );
    assert_eq!(model.session().tracks().len(), 3);
}

#[test]
fn forgetting_a_root_asks_first_then_removes_its_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let songs = dir.path().join("music");
    music(&songs);
    let mut model = model_in(dir.path(), false);
    scan_and_wait(&mut model, &songs);
    assert_eq!(model.session().tracks().len(), 3);

    // A directory that is not a root is refused without a question.
    let elsewhere = dir.path().join("elsewhere");
    model.perform(Action::ForgetRoot(elsewhere.clone()));
    assert_eq!(
        model.input(),
        &Input::None,
        "asked about a directory that is not a root"
    );
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::NotARoot(
            elsewhere
        ))))
    );

    model.perform(Action::ForgetRoot(songs.clone()));
    assert_eq!(
        model.input(),
        &Input::Confirm(Confirm::ForgetRoot(songs.clone()))
    );
    assert_eq!(model.session().tracks().len(), 3, "forgot before a yes");

    model.answer(true);
    assert_eq!(model.session().tracks().len(), 0);
    assert!(model.session().roots().is_empty());
}

#[test]
fn the_root_list_shows_what_the_library_covers() {
    let dir = tempfile::tempdir().unwrap();
    let songs = dir.path().join("music");
    music(&songs);
    let mut model = model_in(dir.path(), false);
    model.perform(Action::ShowRoots);
    assert_eq!(model.input(), &Input::Roots(Vec::new()));

    model.set_input(Input::None);
    scan_and_wait(&mut model, &songs);
    model.perform(Action::ShowRoots);
    assert_eq!(
        model.input(),
        &Input::Roots(vec![songs.canonicalize().unwrap()])
    );
}

#[test]
fn a_remembered_track_is_offered_at_the_next_start_and_plays_on_a_yes() {
    let dir = tempfile::tempdir().unwrap();
    let songs = dir.path().join("music");
    music(&songs);
    let first = songs.join("a.wav");
    let library = dir.path().join("library.db");

    // Nothing stored: no question.
    let model = model_in(dir.path(), false);
    assert_eq!(model.input(), &Input::None);
    model.session().remember(&first, Duration::from_secs(42));
    drop(model);

    // The queue is stored beside it: b had played, c waits.
    let queue = db::SavedQueue {
        played: vec![songs.join("b.wav")],
        playing: false,
        waiting: vec![songs.join("c.wav")],
    };
    db::set_resume_queue(&db::open(&library).unwrap(), &queue, &first).unwrap();

    let mut model = model_in(dir.path(), false);
    assert_eq!(
        model.input(),
        &Input::Confirm(Confirm::Resume {
            path: first.clone(),
            at: Duration::from_secs(42),
            queue,
        })
    );
    model.answer(true);
    model.refresh();
    assert_eq!(
        model.session().player().status().queue.as_ref(),
        [first.clone(), songs.join("c.wav")],
        "not playing the track taken up, then the queue"
    );
    let rows: Vec<&str> = model.queue().iter().map(|t| t.path.as_str()).collect();
    assert_eq!(
        rows,
        [songs.join("b.wav"), songs.join("c.wav")].map(|p| p.to_string_lossy().into_owned())
    );
    assert_eq!(model.queue_played(), 1);

    // Tracks handed over say what to play, so nothing is offered.
    let conn = db::open(&library).unwrap();
    let handed = Model::new(
        conn,
        common::fake_player().0,
        vec![track("/x/one.wav")],
        Config::default(),
    );
    assert_eq!(
        handed.input(),
        &Input::None,
        "asked over handed-over tracks"
    );
}

#[test]
fn audition_takes_the_range_then_the_slice_then_the_region_under_the_playhead() {
    use playr_app::sampler::Wave;
    use playr_core::audio::State;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    model.perform(Action::TogglePause);
    let settle = |model: &Model, done: &dyn Fn(&playr_core::audio::Status) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(&model.session().player().status()) {
            assert!(Instant::now() < deadline, "never settled");
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    settle(&model, &|s| s.state == State::Paused);
    let ms = Duration::from_millis;

    // No range and no marks: the region is the whole track, so it plays.
    model.perform(Action::Audition);
    assert_eq!(model.message(), Some(&Message::Auditioning));
    settle(&model, &|s| s.state == State::Playing);
    // A one-shot range is not left behind as a loop.
    assert_eq!(model.session().player().status().looping, None);

    // A range wins over the region, and auditioning does not leave it looping.
    model.perform(Action::SetRange(Some((ms(2_000), ms(3_000)))));
    model.perform(Action::Audition);
    assert_eq!(model.message(), Some(&Message::Auditioning));
    settle(&model, &|s| s.state == State::Paused);
    let at = model.session().player().position().as_secs_f64();
    assert!(
        (2.9..3.2).contains(&at),
        "paused at {at}s, not the range end"
    );
    assert_eq!(model.session().player().status().looping, None);
}

/// A model on a track with a waveform read, in the sampler view.
fn sampler_model(dir: &Path, file: &Path) -> Model {
    use playr_app::sampler::Wave;
    let mut model = Model::new(
        db::open(&dir.join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    model
}

/// A model in the sampler, playing `file`, which is in the library file at
/// `dir/library.db` as it is on disk, so an analysis of it is current.
fn analysable(dir: &Path, file: &Path, tagged: Option<f32>) -> Model {
    use playr_app::sampler::Wave;
    let library = dir.join("library.db");
    let conn = db::open(&library).unwrap();
    let mut t = track(file.to_str().unwrap());
    let meta = std::fs::metadata(file).unwrap();
    t.size = meta.len() as i64;
    t.mtime = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64;
    db::upsert(&conn, &t).unwrap();
    if let Some(bpm) = tagged {
        let a = playr_core::analysis::Analysis {
            bpm_tag: Some(bpm),
            ..Default::default()
        };
        db::analysis::put(&conn, &t, a).unwrap();
    }
    let mut model = Model::new(conn, common::fake_player().0, vec![t], Config::default());
    model.session_mut().set_library_path(library);
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    model
}

/// Refreshes `model` until `done` holds, for at most 30 s: an analysis
/// decodes the whole file.
fn until(model: &mut Model, done: impl Fn(&Model) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done(model) {
        assert!(Instant::now() < deadline, "{:?}", model.message());
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn slicing_at_beats_takes_the_track_s_tempo() {
    use playr_app::action::Slicing;
    use playr_core::samples::Cut;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("t.wav");
    common::silence(&file, 8000, 10.0);
    // Tagged at 120 BPM: 4 beats is 2 s, 16,000 frames at 8 kHz.
    let mut model = analysable(dir.path(), &file, Some(120.0));
    model.perform(Action::Slice(Slicing::Beats(4)));
    until(&mut model, |m| m.sampler().pending.is_some());
    let plan = model.sampler().pending.as_ref().unwrap();
    assert_eq!(plan.job.cut, Cut::Beats(4, 120.0));
    assert_eq!(
        plan.spans.iter().map(|s| s.0).collect::<Vec<_>>(),
        [0, 16_000, 32_000, 48_000, 64_000]
    );
}

#[test]
fn slicing_at_beats_analyses_a_track_with_no_tempo_first() {
    use playr_app::action::Slicing;
    use playr_core::samples::Cut;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("clicks.wav");
    common::clicks(&file, 22_050, 120.0, 12.0);
    let mut model = analysable(dir.path(), &file, None);
    model.perform(Action::Slice(Slicing::Beats(4)));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::FindingTempo)))
    );
    until(&mut model, |m| m.sampler().pending.is_some());
    let Cut::Beats(4, bpm) = model.sampler().pending.as_ref().unwrap().job.cut else {
        panic!("not cut at beats");
    };
    assert!((bpm - 120.0).abs() < 1.0, "{bpm}");
    assert_eq!(model.session().bpm(&file), Some(bpm), "the tempo is kept");
}

#[test]
fn slicing_at_beats_refuses_a_track_analysed_with_no_pulse() {
    use playr_app::action::Slicing;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("t.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = analysable(dir.path(), &file, None);
    model.perform(Action::Slice(Slicing::Beats(4)));
    let refused = Message::Core(Notice::Refused(Refusal::NoTempo));
    until(&mut model, |m| m.message() == Some(&refused));
    assert!(model.sampler().pending.is_none() && model.sampler().tempo_for.is_none());
}

/// Seeks to `at`, and waits for the player to get there. Playing, the
/// playhead moves on at once, so a poll can miss `at` itself.
fn seek(model: &mut Model, at: Duration) {
    model.perform(Action::SeekTo(at));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !(at..at + Duration::from_millis(500)).contains(&model.session().player().position()) {
        assert!(Instant::now() < deadline, "never sought to {at:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The selected mark's frame, on the playing track.
fn selected_mark(model: &Model) -> Option<u64> {
    let current = model.snapshot().status.current().cloned();
    model.sampler().selected_mark(current.as_ref())
}

#[test]
fn the_mark_keys_select_a_mark_and_step_from_the_selection() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = sampler_model(dir.path(), &file);
    let secs = |n| Duration::from_secs(n);

    // A new mark is selected.
    model.perform(Action::MarkAt(secs(2)));
    assert_eq!(selected_mark(&model), Some(16_000));
    model.perform(Action::MarkAt(secs(6)));
    assert_eq!(selected_mark(&model), Some(48_000));
    model.perform(Action::Deselect);
    assert_eq!(selected_mark(&model), None);

    // From the playhead with nothing selected, then from the selection, so
    // the playhead an audition leaves behind does not skip a mark.
    seek(&mut model, secs(1));
    model.perform(Action::NextMark);
    assert_eq!(selected_mark(&model), Some(16_000));
    assert_eq!(model.message(), Some(&Message::Auditioning));
    seek(&mut model, secs(9));
    model.perform(Action::PrevMark);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::NoEarlierMark)))
    );
    model.perform(Action::NextMark);
    assert_eq!(selected_mark(&model), Some(48_000));
    model.perform(Action::NextMark);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::NoLaterMark)))
    );
    assert_eq!(selected_mark(&model), Some(48_000));

    // Outside the sampler they seek, and select nothing.
    model.perform(Action::Deselect);
    model.perform(Action::ShowView(View::Library));
    seek(&mut model, secs(1));
    model.perform(Action::NextMark);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::AtMark {
            at: secs(2)
        })))
    );
    assert_eq!(selected_mark(&model), None);

    // The selection goes with its mark, by any command.
    model.perform(Action::ShowView(View::Sampler));
    model.perform(Action::SelectMarkAt(secs(6)));
    assert_eq!(selected_mark(&model), Some(48_000));
    model.perform(Action::UndoMark);
    assert_eq!(selected_mark(&model), None);
}

#[test]
fn a_selected_mark_moves_and_is_removed() {
    use playr_app::action::Nudge;
    use playr_app::sampler::Scale;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = sampler_model(dir.path(), &file);
    let secs = |n| Duration::from_secs(n);
    model.set_scale(Scale {
        start: 0,
        per_column: 64,
        per_frame: 1,
        columns: 100,
    });
    model.perform(Action::MarkAt(secs(2)));
    model.perform(Action::MarkAt(secs(6)));

    // Nothing selected: refused rather than acting on the nearest.
    model.perform(Action::Deselect);
    for action in [
        Action::MoveSelected(Nudge::Columns(1)),
        Action::SnapSelected,
        Action::RemoveSelected,
    ] {
        model.perform(action);
        assert_eq!(model.message(), Some(&Message::NothingSelected));
    }

    // A move carries the selection along, wherever the playhead is.
    model.perform(Action::SelectMarkAt(secs(2)));
    seek(&mut model, secs(9));
    model.perform(Action::MoveSelected(Nudge::Columns(1)));
    assert_eq!(selected_mark(&model), Some(16_064));
    model.perform(Action::MoveSelectedTo(secs(3)));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::MarkMoved {
            from: Duration::from_millis(2008),
            to: secs(3)
        })))
    );
    assert_eq!(selected_mark(&model), Some(24_000));

    model.perform(Action::RemoveSelected);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::MarkRemoved {
            at: secs(3)
        })))
    );
    assert_eq!(selected_mark(&model), None);
    let current = model.snapshot().status.current().cloned();
    let marks: Vec<u64> = (model.session_mut().marks_for(current.as_ref()).iter())
        .map(|m| m.frame)
        .collect();
    assert_eq!(marks, vec![48_000]);
}

#[test]
fn a_mark_will_not_move_onto_another() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = sampler_model(dir.path(), &file);
    let secs = |n| Duration::from_secs(n);
    model.perform(Action::MarkAt(secs(2)));
    model.perform(Action::MarkAt(secs(6)));

    model.perform(Action::SelectMarkAt(secs(2)));
    model.perform(Action::MoveSelectedTo(secs(6)));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::MarkInTheWay {
            at: secs(6)
        })))
    );
    // Neither moved, and the selection stayed.
    assert_eq!(selected_mark(&model), Some(16_000));
    model.perform(Action::NextMark);
    assert_eq!(selected_mark(&model), Some(48_000));
}

#[test]
fn a_range_end_is_selected_moved_and_removed() {
    use playr_app::action::Nudge;
    use playr_app::sampler::{Edge, Scale};
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = sampler_model(dir.path(), &file);
    let secs = |n| Duration::from_secs(n);
    model.set_scale(Scale {
        start: 0,
        per_column: 64,
        per_frame: 1,
        columns: 100,
    });
    let current = model.snapshot().status.current().cloned();

    // An end not yet set cannot be selected.
    model.perform(Action::PickEdge(Edge::End));
    assert_eq!(model.message(), Some(&Message::NoEdge(Edge::End)));
    // Set directly: RangeIn/RangeOut take the moving playhead, not an exact frame.
    model.perform(Action::SetRange(Some((secs(2), secs(4)))));
    model.perform(Action::PickEdge(Edge::End));
    assert_eq!(
        model.sampler().selected_edge(current.as_ref()),
        Some(Edge::End)
    );

    // Selecting a mark replaces it: one thing is selected at a time.
    model.perform(Action::MarkAt(secs(6)));
    assert_eq!(model.sampler().selected_edge(current.as_ref()), None);
    model.perform(Action::PickEdge(Edge::Start));
    model.perform(Action::MoveSelected(Nudge::Columns(-1)));
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((16_000 - 64, 32_000))
    );

    // Removing an end clears the range, and the selection with it.
    model.perform(Action::RemoveSelected);
    assert_eq!(model.sampler().range(current.as_ref()), None);
    assert_eq!(model.sampler().selected_edge(current.as_ref()), None);
}

#[test]
fn undo_puts_back_marks_and_the_range() {
    use playr_app::action::Nudge;
    use playr_app::sampler::Scale;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = sampler_model(dir.path(), &file);
    let secs = |n| Duration::from_secs(n);
    model.set_scale(Scale {
        start: 0,
        per_column: 64,
        per_frame: 1,
        columns: 100,
    });
    let current = model.snapshot().status.current().cloned();
    let marks = |model: &mut Model| -> Vec<u64> {
        (model.session_mut().marks_for(current.as_ref()).iter())
            .map(|m| m.frame)
            .collect()
    };

    model.perform(Action::Undo);
    assert_eq!(model.message(), Some(&Message::NothingToUndo));

    // Added, moved, removed and cleared, then undone in reverse.
    model.perform(Action::MarkAt(secs(2)));
    model.perform(Action::MarkAt(secs(6)));
    model.perform(Action::MoveSelected(Nudge::Columns(1)));
    model.perform(Action::SelectMarkAt(secs(2)));
    model.perform(Action::RemoveSelected);
    assert_eq!(marks(&mut model), vec![48_064]);
    model.perform(Action::ClearMarks);
    model.answer(true);
    assert!(marks(&mut model).is_empty());

    model.perform(Action::Undo);
    assert_eq!(model.message(), Some(&Message::Undone));
    assert_eq!(marks(&mut model), vec![48_064]);
    model.perform(Action::Undo);
    assert_eq!(marks(&mut model), vec![16_000, 48_064]);
    // The mark removed comes back selected, as it was.
    assert_eq!(selected_mark(&model), Some(16_000));
    model.perform(Action::Undo);
    assert_eq!(marks(&mut model), vec![16_000, 48_000]);
    model.perform(Action::Undo);
    assert_eq!(marks(&mut model), vec![16_000]);

    // The range, too.
    model.perform(Action::SetRange(Some((secs(1), secs(3)))));
    model.perform(Action::SetRange(Some((secs(4), secs(5)))));
    model.perform(Action::Undo);
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((8_000, 24_000))
    );
    model.perform(Action::Undo);
    assert_eq!(model.sampler().range(current.as_ref()), None);

    // What changes neither is not an edit.
    model.perform(Action::SeekTo(secs(3)));
    model.perform(Action::Undo);
    assert!(marks(&mut model).is_empty());
    model.perform(Action::Undo);
    assert_eq!(model.message(), Some(&Message::NothingToUndo));
}

#[test]
fn redo_puts_back_what_undo_took_until_a_new_edit() {
    use playr_app::action::Nudge;
    use playr_app::sampler::Scale;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = sampler_model(dir.path(), &file);
    let secs = |n| Duration::from_secs(n);
    model.set_scale(Scale {
        start: 0,
        per_column: 64,
        per_frame: 1,
        columns: 100,
    });
    let current = model.snapshot().status.current().cloned();
    let marks = |model: &mut Model| -> Vec<u64> {
        (model.session_mut().marks_for(current.as_ref()).iter())
            .map(|m| m.frame)
            .collect()
    };

    model.perform(Action::Redo);
    assert_eq!(model.message(), Some(&Message::NothingToRedo));

    // A mark nudged three columns, and a range, undone one step too many.
    model.perform(Action::MarkAt(secs(2)));
    for _ in 0..3 {
        model.perform(Action::MoveSelected(Nudge::Columns(1)));
    }
    model.perform(Action::SetRange(Some((secs(1), secs(3)))));
    for _ in 0..3 {
        model.perform(Action::Undo);
    }
    assert_eq!(marks(&mut model), vec![16_064]);
    assert_eq!(model.sampler().range(current.as_ref()), None);

    // Redo walks forward again, in order.
    model.perform(Action::Redo);
    assert_eq!(model.message(), Some(&Message::Redone));
    assert_eq!(marks(&mut model), vec![16_128]);
    model.perform(Action::Redo);
    assert_eq!(marks(&mut model), vec![16_192]);
    model.perform(Action::Redo);
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((8_000, 24_000))
    );
    model.perform(Action::Redo);
    assert_eq!(model.message(), Some(&Message::NothingToRedo));

    // Undo after a redo still works; a new edit then ends the redo.
    model.perform(Action::Undo);
    assert_eq!(model.sampler().range(current.as_ref()), None);
    model.perform(Action::MarkAt(secs(6)));
    model.perform(Action::Redo);
    assert_eq!(model.message(), Some(&Message::NothingToRedo));
    assert_eq!(marks(&mut model), vec![16_192, 48_000]);
}

/// A sampler model on `file` with `n` equal slices of the track planned.
fn planned(dir: &Path, file: &Path, n: usize) -> Model {
    let mut model = sampler_model(dir, file);
    model.perform(Action::Slice(playr_app::action::Slicing::Equal(n)));
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.sampler().pending.is_none() {
        assert!(Instant::now() < deadline, "no plan");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    model
}

/// The planned slices' starts.
fn starts(model: &Model) -> Vec<u64> {
    let plan = model.sampler().pending.as_ref().expect("a plan");
    plan.spans.iter().map(|s| s.0).collect()
}

#[test]
fn a_planned_slice_is_selected_moved_and_joined_to_the_one_before() {
    use playr_app::action::Nudge;
    use playr_app::sampler::{plan_text, Scale};
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = planned(dir.path(), &file, 4);
    let file = model.snapshot().status.current().cloned().unwrap();
    let current = Some(file.clone());
    let selected = |model: &Model| model.sampler().selected_slice(current.as_ref());
    let scale = |per_column| Scale {
        start: 0,
        per_column,
        per_frame: 1,
        columns: 100,
    };
    model.set_scale(scale(64));
    assert_eq!(starts(&model), [0, 20_000, 40_000, 60_000]);

    // Stepping selects the slice it plays, and steps from the selection.
    model.perform(Action::AuditionSlice(true));
    assert_eq!(selected(&model), Some(20_000));
    assert_eq!(model.message(), Some(&Message::Auditioning));

    // A move takes the slice before's end with it, and marks the plan edited.
    model.perform(Action::MoveSelected(Nudge::Columns(1)));
    assert_eq!(starts(&model), [0, 20_064, 40_000, 60_000]);
    let plan = model.sampler().pending.clone().unwrap();
    assert_eq!(plan.spans[0], (0, Some(20_064)));
    assert_eq!(plan.job.cuts, Some(vec![0, 20_064, 40_000, 60_000]));
    assert!(plan_text(model.sampler()).contains("edited"));
    assert_eq!(
        model.message(),
        Some(&Message::SliceMoved {
            slice: 2,
            at: 20_064,
            rate: 8000
        })
    );
    assert_eq!(selected(&model), Some(20_064));

    // It stays a frame inside its neighbours.
    model.set_scale(scale(20_000));
    model.perform(Action::MoveSelected(Nudge::Columns(-5)));
    assert_eq!(starts(&model), [0, 1, 40_000, 60_000]);
    model.perform(Action::MoveSelected(Nudge::Columns(5)));
    assert_eq!(starts(&model), [0, 39_999, 40_000, 60_000]);
    model.set_scale(scale(64));

    // The first slice keeps to the region, and has nothing before to join.
    model.perform(Action::AuditionSlice(false));
    assert_eq!(selected(&model), Some(0));
    model.perform(Action::MoveSelected(Nudge::Columns(-1)));
    assert_eq!(starts(&model), [0, 39_999, 40_000, 60_000]);
    model.perform(Action::RemoveSelected);
    assert_eq!(model.message(), Some(&Message::FirstSlice));

    // Removing a start joins its slice to the one before.
    model.perform(Action::AuditionSlice(true));
    model.perform(Action::RemoveSelected);
    assert_eq!(model.message(), Some(&Message::SlicesJoined { slice: 1 }));
    assert_eq!(starts(&model), [0, 40_000, 60_000]);
    assert_eq!(selected(&model), None);

    // Undo puts the slice back, selected; redo joins it again.
    model.perform(Action::Undo);
    assert_eq!(starts(&model), [0, 39_999, 40_000, 60_000]);
    assert_eq!(selected(&model), Some(39_999));
    model.perform(Action::Redo);
    assert_eq!(starts(&model), [0, 40_000, 60_000]);

    // `a` hears the selected slice.
    model.perform(Action::AuditionSlice(true));
    assert_eq!(selected(&model), Some(40_000));
    model.perform(Action::Audition);
    assert_eq!(
        model.sampler().auditioned,
        Some((file.clone(), 40_000, 60_000))
    );

    // Planned again for other edges, the plan keeps the starts set by hand.
    model.perform(Action::SetSliceEdges(playr_core::samples::Edges::Zero));
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.sampler().pending.is_none() {
        assert!(Instant::now() < deadline, "no plan");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(starts(&model), [0, 40_000, 60_000]);
    assert_eq!(selected(&model), Some(40_000));

    // Discarding an edited plan is a step: undo brings it back, selected,
    // and redo discards it again.
    model.perform(Action::DiscardSlices);
    assert_eq!(selected(&model), None);
    model.perform(Action::Undo);
    assert_eq!(starts(&model), [0, 40_000, 60_000]);
    assert_eq!(selected(&model), Some(40_000));
    model.perform(Action::Redo);
    assert!(model.sampler().pending.is_none());
}

/// Refreshes `model` until no plan is being made.
fn wait_for_plan(model: &mut Model) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.sampler().planning.is_some() {
        assert!(Instant::now() < deadline, "no plan");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn undo_brings_back_an_edited_plan_a_new_cut_replaced() {
    use playr_app::action::{Nudge, Slicing};
    use playr_app::sampler::Scale;
    use playr_core::samples::Edges;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = planned(dir.path(), &file, 4);
    model.set_scale(Scale {
        start: 0,
        per_column: 64,
        per_frame: 1,
        columns: 100,
    });

    // An unedited plan replaced is not a step.
    model.perform(Action::Slice(Slicing::Equal(2)));
    wait_for_plan(&mut model);
    assert_eq!(starts(&model), [0, 40_000]);
    model.perform(Action::Undo);
    assert_eq!(model.message(), Some(&Message::NothingToUndo));
    assert_eq!(starts(&model), [0, 40_000]);

    // An edited one is: undo puts it back over the new cut, and redo the cut.
    model.perform(Action::AuditionSlice(true));
    model.perform(Action::MoveSelected(Nudge::Columns(1)));
    assert_eq!(starts(&model), [0, 40_064]);
    model.perform(Action::Slice(Slicing::Equal(4)));
    wait_for_plan(&mut model);
    assert_eq!(starts(&model), [0, 20_000, 40_000, 60_000]);
    model.perform(Action::Undo);
    assert_eq!(model.message(), Some(&Message::Undone));
    assert_eq!(starts(&model), [0, 40_064]);
    model.perform(Action::Redo);
    assert_eq!(starts(&model), [0, 20_000, 40_000, 60_000]);

    // Undo while a cut is being made undoes the last edit, and drops the cut.
    model.perform(Action::Undo);
    model.perform(Action::Slice(Slicing::Equal(3)));
    model.perform(Action::Undo);
    assert!(model.sampler().planning.is_none());
    model.refresh();
    std::thread::sleep(Duration::from_millis(200));
    model.refresh();
    assert_eq!(starts(&model), [0, 40_000]);

    // An edited plan put back after the edges changed is planned their way.
    model.perform(Action::Redo);
    assert_eq!(starts(&model), [0, 40_064]);
    model.perform(Action::DiscardSlices);
    model.perform(Action::SetSliceEdges(Edges::Fade));
    model.perform(Action::Undo);
    assert!(model.sampler().planning.is_some());
    wait_for_plan(&mut model);
    assert_eq!(starts(&model), [0, 40_064]);
    let plan = model.sampler().pending.as_ref().unwrap();
    assert_eq!(plan.job.edges, Edges::Fade);
}

#[test]
fn a_slice_start_or_range_end_is_moved_to_a_time() {
    use playr_app::sampler::{Edge, Scale};
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = planned(dir.path(), &file, 4);
    let current = model.snapshot().status.current().cloned();
    let ms = Duration::from_millis;
    model.set_scale(Scale {
        start: 0,
        per_column: 64,
        per_frame: 1,
        columns: 100,
    });

    // A click picks the slice starting within a column; none there is said.
    model.perform(Action::SelectSliceAt(ms(3_000)));
    assert_eq!(model.message(), Some(&Message::NoSliceHere));
    model.perform(Action::SelectSliceAt(ms(2_505)));
    assert_eq!(
        model.sampler().selected_slice(current.as_ref()),
        Some(20_000)
    );
    // A drag moves it, kept a frame inside its neighbours.
    model.perform(Action::MoveSelectedTo(ms(3_000)));
    assert_eq!(starts(&model), [0, 24_000, 40_000, 60_000]);
    model.perform(Action::MoveSelectedTo(ms(9_000)));
    assert_eq!(starts(&model), [0, 39_999, 40_000, 60_000]);

    // A range end moves to a time too.
    model.perform(Action::SetRange(Some((ms(1_000), ms(2_000)))));
    model.perform(Action::PickEdge(Edge::End));
    model.perform(Action::MoveSelectedTo(ms(2_500)));
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((8_000, 20_000))
    );
    model.perform(Action::Deselect);
    model.perform(Action::MoveSelectedTo(ms(2_500)));
    assert_eq!(model.message(), Some(&Message::NothingSelected));
}

#[test]
fn undo_keeps_the_latest_edits_up_to_its_depth() {
    use playr_app::sampler::UNDO_DEPTH;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = sampler_model(dir.path(), &file);
    let current = model.snapshot().status.current().cloned();
    let ms = Duration::from_millis;

    // Five more ranges than undo keeps, each 0 to n ms.
    let edits = UNDO_DEPTH as u64 + 5;
    for n in 1..=edits {
        model.perform(Action::SetRange(Some((ms(0), ms(n)))));
    }
    assert_eq!(model.sampler().history.len(), UNDO_DEPTH);
    for _ in 0..UNDO_DEPTH {
        model.perform(Action::Undo);
        assert_eq!(model.message(), Some(&Message::Undone));
    }
    // The five oldest were dropped: back to the fifth range, 5 ms, no further.
    assert_eq!(model.sampler().range(current.as_ref()), Some((0, 40)));
    model.perform(Action::Undo);
    assert_eq!(model.message(), Some(&Message::NothingToUndo));
}

/// Writes 4 s of silence at 8 kHz with a hit at 1 s, as `hit.wav` in `dir`.
fn hit(dir: &Path) -> std::path::PathBuf {
    let file = dir.join("hit.wav");
    let rate = 8000;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&file, spec).unwrap();
    for i in 0..rate as usize * 4 {
        let k = i as f32 - rate as f32;
        let v = match k {
            k if (0.0..2000.0).contains(&k) => (1.0 - k / 2000.0) * (k * 0.3).sin(),
            _ => 0.0,
        };
        w.write_sample((v * 32_000.0) as i16).unwrap();
    }
    w.finalize().unwrap();
    file
}

/// Waits for an onset snap to land.
fn snapped(model: &mut Model) {
    assert_eq!(model.message(), Some(&Message::Snapping));
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.message() == Some(&Message::Snapping) {
        assert!(Instant::now() < deadline, "no snap");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_selected_range_end_snaps_to_the_nearest_rise() {
    use playr_app::sampler::Edge;
    let dir = tempfile::tempdir().unwrap();
    let file = hit(dir.path());
    let rate = 8000;
    let mut model = sampler_model(dir.path(), &file);
    let current = model.snapshot().status.current().cloned();

    model.perform(Action::SetRange(Some((
        Duration::from_millis(1100),
        Duration::from_secs(3),
    ))));
    model.perform(Action::PickEdge(Edge::Start));
    model.perform(Action::SnapSelected);
    snapped(&mut model);
    let (start, end) = model.sampler().range(current.as_ref()).unwrap();
    assert!(start.abs_diff(rate as u64) <= 100, "start at {start}");
    assert_eq!(end, 3 * rate as u64);
    // One undo puts the end back.
    model.perform(Action::Undo);
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((8_800, 24_000))
    );
}

#[test]
fn the_tempo_shown_follows_varispeed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.wav");
    common::tone(&path, 44100, 0.5, -12.0);
    let conn = db::open(&dir.path().join("library.db")).unwrap();
    let mut t = track(path.to_str().unwrap());
    let meta = std::fs::metadata(&path).unwrap();
    t.size = meta.len() as i64;
    t.mtime = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64;
    db::upsert(&conn, &t).unwrap();
    db::analysis::put(
        &conn,
        &t,
        playr_core::analysis::Analysis {
            bpm_tag: Some(120.0),
            ..Default::default()
        },
    )
    .unwrap();
    let mut model = Model::new(conn, common::fake_player().0, vec![t], Config::default());

    model.refresh();
    assert_eq!(model.bpm(), Some(120.0));
    // Twelve semitones is twice the speed, so the music is twice as fast.
    // The engine publishes the speed on its own pass, so wait for it.
    model.perform(Action::SetSpeed(12));
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.bpm() != Some(240.0) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        model.refresh();
    }
    assert_eq!(model.bpm().map(f32::round), Some(240.0));
}

#[test]
fn a_scan_analyses_what_it_added_when_the_setting_asks() {
    let dir = tempfile::tempdir().unwrap();
    let music = dir.path().join("music");
    std::fs::create_dir(&music).unwrap();
    common::tone(&music.join("t.wav"), 44100, 0.5, -12.0);
    let library = dir.path().join("library.db");
    let conn = db::open(&library).unwrap();
    let mut config = Config::default();
    config.settings.analyze_on_scan = true;
    let mut model = Model::new(conn, common::fake_player().0, Vec::new(), config);

    model.perform(Action::Scan(music.clone()));
    // The analysis writes through its own connection, so read the file.
    let reader = db::open(&library).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut rows = 0;
    while rows == 0 && Instant::now() < deadline {
        model.refresh();
        std::thread::sleep(Duration::from_millis(20));
        rows = db::analysis::stats(&reader).map(|s| s.len()).unwrap_or(0);
    }
    assert_eq!(rows, 1, "the scan did not analyse what it added");
}

/// `:info` on a track the library has measured, and on one it has not.
#[test]
fn info_shows_what_analysis_measured() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.wav");
    common::tone(&path, 44100, 0.5, -12.0);
    let library = dir.path().join("library.db");
    let conn = db::open(&library).unwrap();
    let mut t = track(path.to_str().unwrap());
    t.title = Some("Peace Piece".into());
    t.artist = Some("Bill Evans".into());
    t.sample_rate = Some(44100);
    t.channels = Some(2);
    t.bit_depth = Some(16);
    t.duration_ms = Some(500);
    db::upsert(&conn, &t).unwrap();
    let mut model = Model::new(conn, common::fake_player().0, Vec::new(), Config::default());

    // Nothing measured yet: the dialog says so rather than showing blanks.
    model.perform(Action::ShowInfo);
    let rows = match model.input() {
        Input::Info(info) => {
            assert!(info.title.contains("Peace Piece"), "{}", info.title);
            info.rows.clone()
        }
        other => panic!("{other:?}"),
    };
    let value = |label: &str| {
        rows.iter()
            .find(|(l, _)| l == label)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(
        value("format").as_deref(),
        Some("44.1 kHz, 2 ch, 16 bit, 0:00")
    );
    assert!(value("measured").is_some_and(|v| v.contains(":analyze")));
    assert_eq!(value("loudness"), None);

    // Analysed: the measurements and the gain that would apply.
    model.perform(Action::Analyze(None));
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut rows = Vec::new();
    while Instant::now() < deadline {
        model.refresh();
        model.perform(Action::ShowInfo);
        if let Input::Info(info) = model.input() {
            rows = info.rows.clone();
        }
        if rows.iter().any(|(l, _)| l == "loudness") {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let value = |label: &str| {
        rows.iter()
            .find(|(l, _)| l == label)
            .map(|(_, v)| v.clone())
    };
    let loudness = value("loudness").expect("no loudness row");
    assert!(loudness.contains("LUFS"), "{loudness}");
    assert!(
        value("peak").is_some_and(|v| v.contains("dBFS")),
        "{rows:?}"
    );
    assert!(
        value("track gain").is_some_and(|v| v.contains("dB")),
        "{rows:?}"
    );
    // Half a second is too short for a tempo, and it says which.
    assert!(
        value("tempo").is_some_and(|v| v.contains("too short")),
        "{rows:?}"
    );
}

#[test]
fn info_refuses_when_there_is_no_track_to_describe() {
    let (mut model, _dir) = model();
    model.perform(Action::ShowView(View::Playlists));
    model.perform(Action::ShowInfo);
    assert!(matches!(model.input(), Input::None), "{:?}", model.input());
    assert_eq!(
        model.message_text(),
        Some(playr_app::message::text(&Message::Core(Notice::Refused(
            Refusal::NothingPlaying
        ))))
        .as_deref()
    );
}

#[test]
fn scrub_plays_a_moment_from_the_time_then_pauses() {
    use playr_app::sampler::{Wave, SCRUB};
    use playr_core::audio::State;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    let state = |model: &Model| model.session().player().status().state;
    let at = Duration::from_secs(4);
    model.perform(Action::Scrub(at));
    // Paused at the moment's end, having played it: from a stop, a pause
    // could only come from the scrub.
    while state(&model) != State::Paused || model.session().player().position() < at {
        assert!(Instant::now() < deadline, "never paused after it");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(model.session().player().position(), at + SCRUB);
    assert_eq!(model.session().player().status().looping, None);
}

/// Loop started right after a command that plays, before the engine has
/// published that it plays: it must not read the state it left behind and
/// pause what that command started.
#[test]
fn loop_right_after_audition_plays_the_loop() {
    use playr_app::sampler::Wave;
    use playr_core::audio::State;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    let status = |model: &Model| model.session().player().status();
    model.perform(Action::TogglePause);
    while status(&model).state != State::Paused {
        assert!(Instant::now() < deadline, "never paused");
        std::thread::sleep(Duration::from_millis(5));
    }
    let ms = Duration::from_millis;
    model.perform(Action::SetRange(Some((ms(2_000), ms(3_000)))));
    let range = model.sampler().range(status(&model).current());

    model.perform(Action::Audition);
    model.perform(Action::Loop(Some(true)));
    // Settled: the engine has taken both, and a loop of 1 s has had time to
    // pass its end, which would show a one-shot's pause.
    std::thread::sleep(ms(1_500));
    let settled = status(&model);
    assert_eq!(
        (settled.state, settled.looping),
        (State::Playing, range),
        "the loop was paused"
    );
}

#[test]
fn audition_hears_the_same_span_each_time_it_is_pressed() {
    use playr_app::sampler::Wave;
    use playr_core::audio::State;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    let ms = Duration::from_millis;
    let state = |model: &Model| model.session().player().status().state;
    // Each press waits to see playing, then the pause at the span's end.
    let audition = |model: &mut Model| {
        model.perform(Action::Audition);
        let deadline = Instant::now() + Duration::from_secs(5);
        while state(model) != State::Playing {
            assert!(Instant::now() < deadline, "never played");
            std::thread::sleep(ms(5));
        }
        while state(model) != State::Paused {
            assert!(Instant::now() < deadline, "never paused");
            std::thread::sleep(ms(5));
        }
        model.session().player().position()
    };

    // The region between the marks, not the one after it the second time.
    for at in [2_000, 2_500, 3_000] {
        model.perform(Action::MarkAt(ms(at)));
    }
    // A new mark is selected, which `a` would hear instead.
    model.perform(Action::Deselect);
    model.perform(Action::TogglePause);
    let deadline = Instant::now() + Duration::from_secs(5);
    while state(&model) != State::Paused {
        assert!(Instant::now() < deadline, "never paused");
        std::thread::sleep(ms(5));
    }
    model.perform(Action::SeekTo(ms(2_100)));
    while model.session().player().position() != ms(2_100) {
        assert!(Instant::now() < deadline, "never sought");
        std::thread::sleep(ms(5));
    }
    assert_eq!(audition(&mut model), ms(2_500));
    assert_eq!(audition(&mut model), ms(2_500));
    // A selected mark is heard up to the next.
    model.perform(Action::SelectMarkAt(ms(2_000)));
    assert_eq!(audition(&mut model), ms(2_500));
    model.perform(Action::Deselect);

    // A range to the track's end pauses a frame short of it, and plays again
    // rather than ending the track.
    model.perform(Action::SetRange(Some((ms(9_700), ms(10_000)))));
    let end = ms(10_000) - Duration::from_nanos(125_000);
    assert_eq!(audition(&mut model), end);
    assert_eq!(audition(&mut model), end);

    // Stopped, it cues the track and plays the range.
    model.perform(Action::Stop);
    let deadline = Instant::now() + Duration::from_secs(5);
    while state(&model) != State::Stopped {
        assert!(Instant::now() < deadline, "never stopped");
        std::thread::sleep(ms(5));
    }
    assert_eq!(audition(&mut model), end);
}

#[test]
fn convert_is_refused_at_once_while_off_or_convertwithmoss_is_not_installed() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("ConvertWithMoss");
    // Off as shipped: an extension runs a program that is not playr's.
    let mut off = Model::new(
        db::open_memory().unwrap(),
        common::fake_player().0,
        Vec::new(),
        Config::default(),
    );
    assert!(!off.session().convert_enabled());
    off.perform(Action::Convert("sf2".into(), None));
    assert_eq!(
        off.message(),
        Some(&Message::Core(Notice::Refused(Refusal::ConvertOff)))
    );
    assert_eq!(
        off.message_text(),
        Some(":convert is off; set convert-with-moss.enable = true under [extensions] in settings.toml")
    );
    // A prefix does not find the command while it is off.
    off.run_command("conv sf2");
    assert_eq!(off.message_text(), Some("unknown command: conv"));
    off.run_command("convert sf2");
    assert_eq!(
        off.message(),
        Some(&Message::Core(Notice::Refused(Refusal::ConvertOff)))
    );

    let config = Config::parse(&format!(
        "[extensions]\nconvert-with-moss.enable = true\nconvert-with-moss.path = '{}'",
        program.display()
    ))
    .unwrap();
    let mut model = Model::new(
        db::open_memory().unwrap(),
        common::fake_player().0,
        Vec::new(),
        config,
    );
    assert!(model.session().convert_enabled());
    assert!(!model.session().can_convert());
    model.perform(Action::Convert("sf2".into(), None));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::NoConvertWithMoss(
            program.clone()
        ))))
    );
    let text = model.message_text().unwrap().to_string();
    assert!(
        text.starts_with("ConvertWithMoss is not at ")
            && text.contains("set convert-with-moss.path"),
        "{text}"
    );
    // Looked up each time: installed since, it is found without a restart.
    std::fs::write(&program, "").unwrap();
    assert!(model.session().can_convert());
    model.perform(Action::Convert("sf2".into(), None));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::NothingExported)))
    );
}

/// ConvertWithMoss is stood in for by a script that writes one file.
#[cfg(unix)]
#[test]
fn the_slices_last_written_are_converted_into_a_directory_beside_them() {
    use playr_app::action::Slicing;
    use playr_app::message::Message;
    use playr_core::notice::{Notice, Outcome, Refusal};
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("amen.wav");
    common::levels(&file, 8000, &[(1.0, 0.25), (1.0, -0.25)]);
    let out = dir.path().join("out");
    let program = dir.path().join("ConvertWithMoss");
    std::fs::write(
        &program,
        "#!/bin/sh\nfor last; do :; done\necho \"$@\" > \"$last/args\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = Config::parse(&format!(
        "samples = '{}'\n[extensions]\nconvert-with-moss.enable = true\nconvert-with-moss.path = '{}'",
        out.display(),
        program.display()
    ))
    .unwrap();
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        config,
    );
    // Steps until a message other than `while_` shows.
    let after = |model: &mut Model, while_: Outcome| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            model.refresh();
            match model.message() {
                Some(Message::Core(Notice::Done(o))) if *o == while_ => {}
                Some(message) => return message.clone(),
                None => {}
            }
            assert!(Instant::now() < deadline, "still {while_:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    };

    model.perform(Action::Convert("sf2".into(), None));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::NothingExported)))
    );

    // The player takes a moment to report the track as playing. Paused, so
    // the 2 s track cannot end before the slicing, as it could on a loaded
    // machine.
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.snapshot().status.current().is_none() {
        assert!(Instant::now() < deadline, "nothing playing");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    model.perform(Action::TogglePause);
    while model.session().player().status().state != playr_core::audio::State::Paused {
        assert!(Instant::now() < deadline, "never paused");
        std::thread::sleep(Duration::from_millis(5));
    }
    // Outside the sampler view a slicing is written at once.
    model.perform(Action::Slice(Slicing::Equal(2)));
    let exported = after(&mut model, Outcome::ExportStarted);
    let export = out.join("amen");
    assert_eq!(
        exported,
        Message::Core(Notice::Done(Outcome::Exported {
            dir: export.clone(),
            slices: 2
        }))
    );

    model.perform(Action::Convert("sf2".into(), None));
    let started = Outcome::ConvertStarted {
        format: "sf2".into(),
    };
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(started.clone())))
    );
    let converted = after(&mut model, started);
    let dest = export.join("sf2");
    assert_eq!(
        converted,
        Message::Core(Notice::Done(Outcome::Converted {
            dir: dest.clone(),
            warnings: Vec::new()
        }))
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("args")).unwrap().trim(),
        format!(
            "-s sfz -d sf2 {} {}",
            export.join("amen.sfz").display(),
            dest.display()
        )
    );
}

/// An export from an earlier run, named or by path; a directory with no kit
/// is refused before anything runs.
#[cfg(unix)]
#[test]
fn an_earlier_export_is_converted_by_name_or_path() {
    use playr_app::message::Message;
    use playr_core::notice::{Notice, Outcome, Refusal};
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let samples = dir.path().join("samples");
    let export = samples.join("amen");
    std::fs::create_dir_all(&export).unwrap();
    std::fs::write(export.join("amen.sfz"), "<region> sample=a.wav key=36\n").unwrap();
    let old = samples.join("old");
    std::fs::create_dir_all(&old).unwrap();
    let program = dir.path().join("ConvertWithMoss");
    std::fs::write(
        &program,
        "#!/bin/sh\nfor last; do :; done\necho \"$@\" > \"$last/args\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = Config::parse(&format!(
        "samples = '{}'\n[extensions]\nconvert-with-moss.enable = true\nconvert-with-moss.path = '{}'",
        samples.display(),
        program.display()
    ))
    .unwrap();
    let mut model = Model::new(
        db::open_memory().unwrap(),
        common::fake_player().0,
        Vec::new(),
        config,
    );
    let converted = |model: &mut Model, format: &str| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            model.refresh();
            if let Some(Message::Core(Notice::Done(Outcome::Converted { dir, .. }))) =
                model.message()
            {
                return dir.clone();
            }
            assert!(Instant::now() < deadline, "not converted to {format}");
            std::thread::sleep(Duration::from_millis(10));
        }
    };

    // Listed for Tab: only a directory holding a kit is an export.
    assert_eq!(model.session().exports(), ["amen"]);

    // Nothing written in this run, yet an export by name converts.
    model.perform(Action::Convert("sf2".into(), Some("amen".into())));
    assert_eq!(converted(&mut model, "sf2"), export.join("sf2"));
    assert_eq!(
        std::fs::read_to_string(export.join("sf2/args"))
            .unwrap()
            .trim(),
        format!(
            "-s sfz -d sf2 {} {}",
            export.join("amen.sfz").display(),
            export.join("sf2").display()
        )
    );
    // By its full path, the same.
    model.perform(Action::Convert("mpc".into(), Some(export.clone())));
    assert_eq!(converted(&mut model, "mpc"), export.join("mpc"));

    // An export from before kits, and a name that is not there.
    for name in ["old", "missing"] {
        model.perform(Action::Convert("sf2".into(), Some(name.into())));
        assert_eq!(
            model.message(),
            Some(&Message::Core(Notice::Refused(Refusal::NoKit(
                samples.join(name)
            ))))
        );
    }
    assert!(!old.join("sf2").exists());
    assert_eq!(
        model.message_text().map(|t| t.starts_with("no kit in ")),
        Some(true)
    );
}

#[test]
fn slice_edges_chosen_after_planning_replan_what_is_written() {
    use playr_app::action::Slicing;
    use playr_app::sampler::Wave;
    use playr_core::samples::Edges;

    // Crossings at frames 4,000, 8,000 ... 28,000.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("steps.wav");
    let parts: Vec<(f32, f32)> = (0..8)
        .map(|i| (0.5, if i % 2 == 0 { 0.25 } else { -0.25 }))
        .collect();
    common::levels(&file, 8000, &parts);
    let out = dir.path().join("out");
    let config = Config::parse(&format!("samples = '{}'", out.display())).unwrap();
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        config,
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    let ms = Duration::from_millis;
    let planned = |model: &mut Model| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            model.refresh();
            if let Some(plan) = &model.sampler().pending {
                return plan.spans.clone();
            }
            assert!(Instant::now() < deadline, "nothing planned");
            std::thread::sleep(ms(10));
        }
    };
    model.perform(Action::SetRange(Some((ms(1_005), ms(2_995)))));
    let exact = vec![
        (8_040, Some(12_020)),
        (12_020, Some(16_000)),
        (16_000, Some(19_980)),
        (19_980, Some(23_960)),
    ];
    let zero = vec![
        (8_000, Some(12_000)),
        (12_000, Some(16_000)),
        (16_000, Some(20_000)),
        (20_000, Some(24_000)),
    ];

    // Planned, then changed.
    model.perform(Action::Slice(Slicing::Equal(4)));
    assert_eq!(planned(&mut model), exact);
    model.perform(Action::SetSliceEdges(Edges::Zero));
    assert_eq!(planned(&mut model), zero);

    // Changed while planning.
    model.perform(Action::SetSliceEdges(Edges::Exact));
    planned(&mut model);
    model.perform(Action::DiscardSlices);
    model.perform(Action::Slice(Slicing::Equal(4)));
    model.perform(Action::SetSliceEdges(Edges::Zero));
    assert_eq!(planned(&mut model), zero);
    assert_eq!(
        model.sampler().pending.as_ref().unwrap().job.edges,
        Edges::Zero
    );

    // What is written is what was planned last.
    model.perform(Action::WriteSlices);
    let deadline = Instant::now() + Duration::from_secs(5);
    let json = loop {
        let written = std::fs::read_dir(&out)
            .into_iter()
            .flatten()
            .map(|e| e.unwrap().path().join("samples.json"))
            .find(|j| j.exists());
        if let Some(json) = written.and_then(|j| std::fs::read_to_string(j).ok()) {
            if json.ends_with("}\n") {
                break json;
            }
        }
        assert!(Instant::now() < deadline, "never written");
        std::thread::sleep(ms(10));
    };
    assert!(
        json.contains(r#""start_frame": 8000, "end_frame": 12000"#),
        "{json}"
    );
}

#[test]
fn n_and_p_step_through_planned_slices_inside_the_range() {
    use playr_app::action::Slicing;
    use playr_app::sampler::{frame_of, Wave};
    use playr_core::audio::State;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    let ms = Duration::from_millis;
    let state = |model: &Model| model.session().player().status().state;
    let settle = |model: &Model, want: State| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while state(model) != want {
            assert!(Instant::now() < deadline, "never {want:?}");
            std::thread::sleep(ms(5));
        }
    };
    // Where each press pauses, in frames.
    let hear = |model: &mut Model, action: Action| {
        model.perform(action);
        settle(model, State::Playing);
        settle(model, State::Paused);
        frame_of(model.session().player().position(), 8000)
    };

    model.perform(Action::SetRange(Some((ms(1_000), ms(3_000)))));
    model.perform(Action::AuditionSlice(true));
    assert_eq!(model.message(), Some(&Message::NoSlicesPlanned));
    model.perform(Action::Slice(Slicing::Equal(4)));
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.sampler().pending.is_none() {
        assert!(Instant::now() < deadline, "nothing planned");
        model.refresh();
        std::thread::sleep(ms(10));
    }
    model.perform(Action::TogglePause);
    settle(&model, State::Paused);
    model.perform(Action::SeekTo(ms(1_100)));
    while model.session().player().position() != ms(1_100) {
        std::thread::sleep(ms(5));
    }

    // The slice under the playhead, not the range around it.
    assert_eq!(hear(&mut model, Action::Audition), 12_000);
    assert_eq!(hear(&mut model, Action::AuditionSlice(true)), 16_000);
    assert_eq!(hear(&mut model, Action::AuditionSlice(true)), 20_000);
    assert_eq!(hear(&mut model, Action::AuditionSlice(true)), 24_000);
    // Past the last slice, back to the first, and the other way round.
    assert_eq!(hear(&mut model, Action::AuditionSlice(true)), 12_000);
    assert_eq!(hear(&mut model, Action::AuditionSlice(false)), 24_000);
    assert_eq!(hear(&mut model, Action::AuditionSlice(false)), 20_000);
    assert_eq!(hear(&mut model, Action::Audition), 20_000, "again");
    assert_eq!(hear(&mut model, Action::AuditionSlice(false)), 16_000);
    assert_eq!(hear(&mut model, Action::AuditionSlice(false)), 12_000);
}

#[test]
fn sensitivities_asked_for_while_planning_plan_only_the_last() {
    use playr_app::action::Slicing;
    use playr_app::sampler::Wave;
    use playr_core::samples::Cut;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(model.sampler().wave, Wave::Ready { .. }) {
        assert!(Instant::now() < deadline, "no waveform");
        model.refresh();
        std::thread::sleep(Duration::from_millis(10));
    }
    model.perform(Action::SetRange(Some((
        Duration::from_secs(1),
        Duration::from_secs(3),
    ))));

    // A slider dragged: three values before the first plan lands.
    for s in [0.1, 0.9, 0.5] {
        model.perform(Action::Slice(Slicing::Onsets(Some(s))));
    }
    assert_eq!(model.sampler().onsets_wanted, Some(0.5));
    let deadline = Instant::now() + Duration::from_secs(5);
    let cut = loop {
        model.refresh();
        if let Some(plan) = &model.sampler().pending {
            break plan.job.cut;
        }
        assert!(Instant::now() < deadline, "nothing planned");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(cut, Cut::Onsets(0.5));
    assert_eq!(model.sampler().onsets_wanted, None);
    assert_eq!(model.sampler().planning, None, "0.9 was never planned");
}

#[test]
fn loop_slots_save_the_range_recall_it_looping_and_are_kept_in_the_library() {
    use playr_app::action::SlotOp::{Clear, Save, Use};
    use playr_core::notice::Outcome;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let library = dir.path().join("library.db");
    let mut model = Model::new(
        db::open(&library).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    model.perform(Action::ShowView(View::Sampler));
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.session().playing_track().is_err() {
        assert!(Instant::now() < deadline, "never played");
        std::thread::sleep(Duration::from_millis(10));
    }
    let s = Duration::from_secs;
    let looping = |model: &Model, want: Option<(u64, u64)>| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while model.session().player().status().looping != want {
            assert!(Instant::now() < deadline, "never looped {want:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let slots = |model: &mut Model| {
        model.refresh();
        model.snapshot().loops
    };

    model.perform(Action::LoopSlot(1, Use));
    assert_eq!(model.message(), Some(&Message::NoRangeToSave(1)));

    // An empty slot takes the range.
    model.perform(Action::SetRange(Some((s(1), s(2)))));
    model.perform(Action::LoopSlot(1, Use));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::LoopSaved {
            slot: 1,
            kept: true
        })))
    );
    model.perform(Action::SetRange(Some((s(3), s(4)))));
    model.perform(Action::LoopSlot(2, Use));
    assert_eq!(
        slots(&mut model)[..3],
        [Some((8_000, 16_000)), Some((24_000, 32_000)), None]
    );

    // A full one recalls it into the range, looping, and moves a loop playing.
    model.perform(Action::SetRange(None));
    model.perform(Action::LoopSlot(1, Use));
    let current = Some(file.clone());
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((8_000, 16_000))
    );
    assert!(matches!(
        model.message(),
        Some(Message::LoopRecalled { slot: 1, .. })
    ));
    looping(&model, Some((8_000, 16_000)));
    model.perform(Action::LoopSlot(2, Use));
    looping(&model, Some((24_000, 32_000)));

    // Saving over one, and clearing another.
    model.perform(Action::SetRange(Some((s(5), s(6)))));
    model.perform(Action::LoopSlot(1, Save));
    model.perform(Action::LoopSlot(2, Clear));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::LoopCleared {
            slot: 2
        })))
    );
    assert_eq!(slots(&mut model)[..2], [Some((40_000, 48_000)), None]);

    // Clearing them all asks first.
    model.perform(Action::LoopSlot(4, Save));
    model.perform(Action::ClearLoops);
    assert!(matches!(
        model.input(),
        Input::Confirm(Confirm::ClearLoops { count: 2, .. })
    ));
    model.answer(false);
    assert_eq!(slots(&mut model).iter().flatten().count(), 2);
    model.perform(Action::ClearLoops);
    model.answer(true);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::LoopsCleared)))
    );
    assert_eq!(slots(&mut model), [None; 8]);
    model.perform(Action::ClearLoops);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::NoLoops)))
    );
    model.perform(Action::LoopSlot(1, Save));
    drop(model);

    // Kept with the track in the library file.
    let mut again = Model::new(
        db::open(&library).unwrap(),
        common::fake_player().0,
        Vec::new(),
        Config::default(),
    );
    assert_eq!(
        again.session_mut().loops_for(Some(&file))[..2],
        [Some((40_000, 48_000)), None]
    );
}

#[test]
fn eq_moves_from_where_the_player_is_stays_in_range_and_says_so() {
    use playr_core::audio::eq::Band;
    let mut model = Model::new(
        db::open_memory().unwrap(),
        common::fake_player().0,
        Vec::new(),
        Config::default(),
    );
    model.perform(Action::EqBy(Band::Bass, 3.0));
    model.run_command("eq bass +3");
    model.run_command("eq treble =-2");
    assert_eq!(model.message_text(), Some("eq bass +6 treble -2"));
    model.refresh();
    assert_eq!(model.snapshot().eq, [6.0, 0.0, -2.0]);
    model.perform(Action::EqBy(Band::Bass, 20.0));
    assert_eq!(model.message_text(), Some("eq bass +12 treble -2"));
    model.perform(Action::FlatEq);
    assert_eq!(model.message_text(), Some("eq flat"));
    model.refresh();
    assert_eq!(model.snapshot().eq, [0.0; 3]);
}

#[test]
fn the_queue_lists_what_plays_opens_on_it_and_jumps_within_it() {
    let dir = tempfile::tempdir().unwrap();
    let tracks: Vec<Track> = ["one", "two", "three"]
        .iter()
        .map(|name| {
            let file = dir.path().join(format!("{name}.wav"));
            common::silence(&file, 8000, 5.0);
            track(&file.to_string_lossy())
        })
        .collect();
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        tracks.clone(),
        Config::default(),
    );
    let index = |model: &Model| model.session().player().status().index;
    // The last selected track, queued to play next, is in the queue twice.
    model.set_cursor(View::Selection, Some(2));
    model.perform(Action::Enqueue(true));
    model.refresh();
    let paths: Vec<&str> = model.playing().iter().map(|t| t.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            &tracks[0].path,
            &tracks[2].path,
            &tracks[1].path,
            &tracks[2].path
        ]
    );

    model.perform(Action::ShowView(View::Queue));
    assert_eq!(model.cursors().queue, Some(index(&model)));
    model.perform(Action::Cursor(1));
    model.perform(Action::Activate);
    let deadline = Instant::now() + Duration::from_secs(5);
    while index(&model) != 1 {
        assert!(Instant::now() < deadline, "did not jump");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(model.playing().len(), 4, "the queue was replaced");
}

#[test]
fn replacing_the_queue_says_how_many_tracks_were_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let tracks: Vec<Track> = ["one", "two", "three"]
        .iter()
        .map(|name| {
            let file = dir.path().join(format!("{name}.wav"));
            common::silence(&file, 8000, 5.0);
            track(&file.to_string_lossy())
        })
        .collect();
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        tracks,
        Config::default(),
    );
    let replaced = |model: &Model| match model.message() {
        Some(Message::Core(Notice::Done(Outcome::QueueReplaced { tracks }))) => Some(*tracks),
        _ => None,
    };
    // Opened files play as a list, so nothing waits and nothing is said.
    // Enter on the selection fills the queue: one plays, two and three wait.
    model.perform(Action::Activate);
    assert_eq!(replaced(&model), None);
    // Two more queued wait behind them.
    model.set_cursor(View::Selection, Some(1));
    model.perform(Action::Enqueue(false));
    model.perform(Action::Enqueue(false));
    assert_eq!(model.session().queue_rows().len(), 5);
    model.set_cursor(View::Selection, Some(0));
    model.perform(Action::Activate);
    assert_eq!(replaced(&model), Some(4));
}

#[test]
fn the_sleep_timer_counts_down_and_stops_playback() {
    use playr_core::audio::State;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        vec![track(&file.to_string_lossy())],
        Config::default(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.session().player().status().state != State::Playing {
        assert!(Instant::now() < deadline, "not playing");
        model.refresh();
    }

    model.perform(Action::StopIn(Some(Duration::from_secs(3600))));
    model.refresh();
    let left = model.snapshot().sleep.unwrap();
    assert!(left > Duration::from_secs(3590), "{left:?}");
    model.perform(Action::StopIn(None));
    model.refresh();
    assert_eq!(model.snapshot().sleep, None);

    model.perform(Action::StopIn(Some(Duration::ZERO)));
    model.refresh();
    assert_eq!(model.message(), Some(&Message::Slept));
    assert_eq!(model.snapshot().sleep, None, "set once run out");
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.session().player().status().state != State::Stopped {
        assert!(Instant::now() < deadline, "not stopped");
        model.refresh();
    }
    // Nothing plays, so there is no track to stop after.
    model.perform(Action::StopAfter);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::NothingPlaying)))
    );
}

#[test]
fn an_old_draft_is_asked_about_and_can_be_saved_under_a_name() {
    use playr_app::model::DraftAnswer;
    use playr_core::session::DRAFT;

    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library.db");
    {
        let conn = db::open(&library).unwrap();
        for p in ["/m/a.flac", "/m/b.flac"] {
            db::upsert(&conn, &track(p)).unwrap();
        }
    }
    let open = || {
        let config = Config::default();
        let conn = db::open(&library).unwrap();
        Model::new(conn, common::fake_player().0, Vec::new(), config)
    };
    let names = |m: &Model| -> Vec<String> {
        m.session()
            .playlists()
            .iter()
            .map(|p| p.name.clone())
            .collect()
    };

    let mut first = open();
    first.set_cursor(View::Library, Some(0));
    first.perform(Action::Add);
    first.refresh();
    assert_eq!(first.input(), &Input::None, "asked with no old draft");
    assert_eq!(names(&first), [DRAFT]);
    drop(first);

    let mut second = open();
    assert!(second.session().selection().is_empty());
    second.set_cursor(View::Library, Some(1));
    second.perform(Action::Add);
    second.refresh();
    assert_eq!(second.input(), &Input::Draft(1));
    second.answer_draft(Some(DraftAnswer::Save));
    assert_eq!(second.save_title(), "Save the old draft as");
    // Closing the prompt ends saving the draft; the next save is the selection's.
    second.set_input(Input::None);
    assert_eq!(second.save_title(), "Save the selection as");

    second.perform(Action::Add);
    second.perform(Action::Add);
    second.refresh();
    assert_eq!(second.input(), &Input::Draft(1));
    second.answer_draft(Some(DraftAnswer::Save));
    second.save_as("kept");
    assert_eq!(names(&second), [DRAFT, "kept"]);
    let draft = second.session().playlists()[0].id;
    let paths: Vec<String> = (second.session().playlist_tracks(draft).into_iter())
        .map(|t| t.path)
        .collect();
    assert_eq!(paths, ["/m/b.flac"]);
}

#[test]
fn a_in_the_queue_view_adds_the_track_to_the_selection_once() {
    use playr_core::audio::State;

    let dir = tempfile::tempdir().unwrap();
    // Long enough to still be playing when polled: a 50 ms track can start
    // and end between two polls on a busy machine.
    let file = dir.path().join("a.wav");
    common::silence(&file, 8000, 10.0);
    let a = track(&file.to_string_lossy());
    let mut model = Model::new(
        db::open(&dir.path().join("library.db")).unwrap(),
        common::fake_player().0,
        Vec::new(),
        Config::parse("draft = 'off'").unwrap(),
    );
    model.session_mut().enqueue(std::slice::from_ref(&a), false);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !(model.session().player().caught_up()
        && model.session().player().status().state != State::Stopped)
    {
        assert!(Instant::now() < deadline, "not playing");
        model.refresh();
    }
    model.refresh();
    model.perform(Action::ShowView(View::Queue));
    model.set_cursor(View::Queue, Some(0));
    model.perform(Action::Add);
    assert_eq!(model.session().selection().len(), 1);
    assert_eq!(model.session().selection()[0].path, a.path);
    model.perform(Action::Add);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::AlreadyInSelection)))
    );
    assert_eq!(model.session().selection().len(), 1);
}

#[test]
fn edit_fills_the_selection_asking_first_and_save_offers_the_playlist_s_name() {
    let (mut model, _dir) = model();
    let row = |m: &Model, name: &str| m.session().playlists().iter().position(|p| p.name == name);
    model.perform(Action::ShowView(View::Playlists));
    model.set_cursor(View::Playlists, row(&model, "late"));
    model.perform(Action::EditPlaylist);
    assert_eq!(model.view(), View::Selection);
    assert_eq!(model.session().selection().len(), 2);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::Editing {
            name: "late".into()
        })))
    );
    model.perform(Action::StartSave);
    assert_eq!(model.input(), &Input::SavePlaylist("late".into()));
    model.set_input(Input::None);

    // A selection in progress is replaced only once confirmed.
    model.perform(Action::ShowView(View::Playlists));
    model.set_cursor(View::Playlists, row(&model, "early"));
    model.perform(Action::EditPlaylist);
    assert!(matches!(
        model.input(),
        Input::Confirm(Confirm::EditPlaylist { replacing: 2, .. })
    ));
    assert_eq!(model.session().selection().len(), 2);
    model.answer(true);
    assert_eq!(model.session().selection().len(), 1);
    assert_eq!(
        model.session().editing().map(|p| p.name.as_str()),
        Some("early")
    );
}

#[test]
fn a_saved_search_is_listed_after_the_playlists_and_acts_on_what_it_finds() {
    let (mut model, _dir) = model();
    model.perform(Action::SaveSearch("all".into()));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::NoSearch)))
    );
    model.perform(Action::Search("path:/m/".into()));
    // Without a name, the save prompt asks for one.
    model.perform(Action::StartSaveSearch);
    assert_eq!(model.save_title(), "Save the search as");
    model.save_as("all");
    assert_eq!(model.session().searches().len(), 1);

    // Recounted each time: the draft playlist comes once tracks are selected.
    let on_search = |m: &mut Model| {
        let row = m.session().playlists().len();
        m.perform(Action::ShowView(View::Playlists));
        m.set_cursor(View::Playlists, Some(row));
    };
    model.perform(Action::ClearSearch);
    on_search(&mut model);
    model.perform(Action::Activate);
    assert_eq!(model.view(), View::Library);
    assert_eq!(model.results().map(<[Track]>::len), Some(3));

    on_search(&mut model);
    model.perform(Action::Add);
    assert_eq!(model.session().selection().len(), 3);

    on_search(&mut model);
    model.perform(Action::EditPlaylist);
    assert_eq!(model.input(), &Input::Search("path:/m/".into()));
    model.set_input(Input::None);

    on_search(&mut model);
    model.perform(Action::DeletePlaylist);
    assert!(matches!(
        model.input(),
        Input::Confirm(Confirm::DeleteSearch(_))
    ));
    model.answer(true);
    assert!(model.session().searches().is_empty());
}

/// Refreshes until the `:sql` statement running has reported.
fn after_sql(model: &mut Model) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.message() == Some(&Message::Querying) {
        assert!(Instant::now() < deadline, "the statement did not report");
        model.refresh();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn sql_lists_what_a_select_names_and_saves_as_a_search_to_edit_on_the_command_line() {
    use playr_core::db::query::Query;

    let (mut model, _dir) = model();
    let statement = "SELECT path FROM library WHERE path LIKE '%b%'";
    let paths = |m: &Model| -> Vec<String> {
        (m.results().unwrap_or_default().iter())
            .map(|t| t.path.clone())
            .collect()
    };
    model.run_command(&format!("sql {statement}"));
    assert_eq!(model.message(), Some(&Message::Querying));
    after_sql(&mut model);
    assert_eq!(paths(&model), ["/m/b.flac"]);
    assert_eq!(model.message(), Some(&Message::Found(1)));

    model.run_command("sql SELECT title FROM library");
    after_sql(&mut model);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::Sql(
            "the statement must select a column named path".into()
        ))))
    );
    assert_eq!(
        paths(&model),
        ["/m/b.flac"],
        "a refusal cleared the results"
    );

    // A statement sent while another runs replaces it.
    model.run_command("sql SELECT path FROM library WHERE path LIKE '%a%'");
    model.run_command(&format!("sql {statement}"));
    after_sql(&mut model);
    std::thread::sleep(Duration::from_millis(50));
    model.refresh();
    assert_eq!(paths(&model), ["/m/b.flac"]);

    model.perform(Action::SaveSearch("bees".into()));
    let search = model.session().searches()[0].clone();
    assert_eq!(search.query, Query::Sql(statement.into()));
    assert_eq!(search.sort, "", "SQL keeps its own order");

    model.perform(Action::ClearSearch);
    let row = model.session().playlists().len();
    let on_search = |m: &mut Model| {
        m.perform(Action::ShowView(View::Playlists));
        m.set_cursor(View::Playlists, Some(row));
    };
    on_search(&mut model);
    model.perform(Action::Activate);
    after_sql(&mut model);
    assert_eq!(paths(&model), ["/m/b.flac"]);

    on_search(&mut model);
    model.perform(Action::Add);
    after_sql(&mut model);
    let selected: Vec<&str> = (model.session().selection().iter())
        .map(|t| t.path.as_str())
        .collect();
    assert_eq!(selected, ["/m/b.flac"]);

    // The draft playlist, made by the selection, comes before the search.
    let row = model.session().playlists().len();
    model.perform(Action::ShowView(View::Playlists));
    model.set_cursor(View::Playlists, Some(row));
    model.perform(Action::EditPlaylist);
    match model.input() {
        Input::Command(line) => assert_eq!(line.text, format!("sql {statement}")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_selected_slice_snaps_to_the_nearest_rise() {
    use playr_app::action::Nudge;
    use playr_app::sampler::Scale;
    let dir = tempfile::tempdir().unwrap();
    let file = hit(dir.path());
    // Four slices of the 4 s track, the second starting at the hit, 1 s.
    let mut model = planned(dir.path(), &file, 4);
    model.set_scale(Scale {
        start: 0,
        per_column: 1600,
        per_frame: 1,
        columns: 100,
    });
    model.perform(Action::AuditionSlice(true));
    model.perform(Action::MoveSelected(Nudge::Columns(1)));
    assert_eq!(starts(&model), [0, 9_600, 16_000, 24_000]);

    model.perform(Action::SnapSelected);
    snapped(&mut model);
    let back = starts(&model)[1];
    assert!(back.abs_diff(8_000) <= 100, "slice 2 starts at {back}");
    let current = model.snapshot().status.current().cloned();
    assert_eq!(model.sampler().selected_slice(current.as_ref()), Some(back));
}
