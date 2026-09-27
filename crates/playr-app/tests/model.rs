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

    model.perform(Action::Loop(None));
    assert_eq!(model.message(), Some(&Message::NoRangeToLoop));

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
    model.perform(Action::MoveEdge(Nudge::Columns(-2)));
    let current = model.snapshot().status.current().cloned();
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((16_000, 19_872))
    );
    settle(&model, &|s| s.looping == Some((16_000, 19_872)));
    model.perform(Action::PickEdge(Edge::Start));
    model.perform(Action::MoveEdge(Nudge::Percent(100)));
    assert_eq!(
        model.sampler().range(current.as_ref()),
        Some((19_871, 19_872))
    );
    settle(&model, &|s| s.looping == Some((19_871, 19_872)));
    model.perform(Action::SetRange(Some((ms(2_000), ms(2_500)))));

    // Escape, with no slices planned, clears the range, which ends the loop.
    model.perform(Action::DiscardSlices);
    assert_eq!(model.sampler().range, None);
    settle(&model, &|s| s.looping.is_none());
    model.perform(Action::DiscardSlices);
    assert_eq!(model.message(), Some(&Message::NoSlicesPlanned));
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

    let mut model = model_in(dir.path(), false);
    assert_eq!(
        model.input(),
        &Input::Confirm(Confirm::Resume {
            path: first.clone(),
            at: Duration::from_secs(42)
        })
    );
    model.answer(true);
    model.refresh();
    assert_eq!(
        model.session().player().status().queue.as_ref(),
        std::slice::from_ref(&first),
        "not playing the track taken up"
    );

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

#[test]
fn the_cursor_moves_apart_from_the_playhead_and_returns_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = sampler_model(dir.path(), &file);

    assert_eq!(
        model.sampler().cursor,
        None,
        "a cursor before one was asked for"
    );
    model.perform(Action::SetCursor(Some(Duration::from_secs(4))));
    assert_eq!(model.sampler().cursor, Some(32_000));

    // Seeking does not drag the cursor along with the playhead.
    model.perform(Action::SeekTo(Duration::from_secs(1)));
    assert_eq!(model.sampler().cursor, Some(32_000));

    model.perform(Action::SetCursor(None));
    assert_eq!(
        model.sampler().cursor,
        None,
        "cursor did not return to the playhead"
    );
}

#[test]
fn a_mark_is_picked_by_the_cursor_then_moved_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.wav");
    common::silence(&file, 8000, 10.0);
    let mut model = sampler_model(dir.path(), &file);
    let secs = |n| Duration::from_secs(n);

    model.perform(Action::MarkAt(secs(2)));
    model.perform(Action::MarkAt(secs(6)));

    // Nothing under the cursor: refused rather than acting on the nearest.
    model.perform(Action::SetCursor(Some(secs(4))));
    model.perform(Action::DeleteMark);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::NoMarkHere)))
    );

    // The cursor picks a mark up, and moving it carries the cursor along.
    model.perform(Action::PickMark(false));
    assert_eq!(model.sampler().cursor, Some(16_000));
    model.perform(Action::MoveMarkTo(secs(3)));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::MarkMoved {
            from: secs(2),
            to: secs(3)
        })))
    );
    assert_eq!(model.sampler().cursor, Some(24_000));

    // It moved rather than being added: still two marks, and one is at 0:03.
    model.perform(Action::PickMark(true));
    assert_eq!(
        model.sampler().cursor,
        Some(48_000),
        "the later mark moved too"
    );
    model.perform(Action::PickMark(false));
    model.perform(Action::DeleteMark);
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Done(Outcome::MarkRemoved {
            at: secs(3)
        })))
    );
    model.perform(Action::PickMark(false));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::NoEarlierMark))),
        "the removed mark was still there"
    );
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

    model.perform(Action::SetCursor(Some(secs(2))));
    model.perform(Action::MoveMarkTo(secs(6)));
    assert_eq!(
        model.message(),
        Some(&Message::Core(Notice::Refused(Refusal::MarkInTheWay {
            at: secs(6)
        })))
    );
    // Both are still there, and neither moved.
    model.perform(Action::SetCursor(Some(secs(2))));
    model.perform(Action::PickMark(true));
    assert_eq!(model.sampler().cursor, Some(48_000));
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
