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
