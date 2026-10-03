//! Actions dispatched through a frontend that is not a terminal: plain fields
//! for cursors and a log of what it was asked to show. What passes here holds
//! for any frontend that implements `Frontend` the same way.

#[path = "../../playr-core/tests/common/mod.rs"]
mod common;

use std::path::PathBuf;

use playr_app::action::{Action, Key, Keymap};
use playr_app::command;
use playr_app::dispatch::{confirmed, dispatch, Confirm, Frontend, Presentation, Prompt};
use playr_app::message::Message;
use playr_app::View::{self, Library, Playlists, Sampler, Selection};
use playr_core::db::query::{self, Playlist};
use playr_core::db::{self, Track};
use playr_core::event::{self, JobId};
use playr_core::notice::{Notice, Outcome, Refusal};
use playr_core::samples::Plan;
use playr_core::session::Session;

/// A frontend with no drawing: state in fields, output in logs.
struct Headless {
    session: Session,
    keys: Keymap,
    view: View,
    cursors: [Option<usize>; 4],
    results: Option<Vec<Track>>,
    messages: Vec<Message>,
    asked: Option<Confirm>,
    prompts: Vec<Prompt>,
    shown: Vec<Presentation>,
    plan: Option<Plan>,
    planning: bool,
    sampler: playr_app::sampler::Sampler,
    tape: playr_app::tape::Deck,
}

fn slot(view: View) -> Option<usize> {
    match view {
        Library => Some(0),
        Selection => Some(1),
        Playlists => Some(2),
        View::Queue => Some(3),
        Sampler => None,
    }
}

impl Frontend for Headless {
    fn session(&self) -> &Session {
        &self.session
    }

    fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }
    fn keys(&mut self) -> &mut Keymap {
        &mut self.keys
    }
    fn view(&self) -> View {
        self.view
    }
    fn set_view(&mut self, view: View) {
        self.view = view;
    }
    fn cursor(&self, view: View) -> Option<usize> {
        slot(view).and_then(|i| self.cursors[i])
    }
    fn set_cursor(&mut self, view: View, row: Option<usize>) {
        if let Some(i) = slot(view) {
            self.cursors[i] = row;
        }
    }
    fn listed(&self) -> &[Track] {
        self.results.as_deref().unwrap_or(self.session.tracks())
    }
    fn set_results(&mut self, results: Option<Vec<Track>>) -> Option<Vec<Track>> {
        std::mem::replace(&mut self.results, results)
    }
    fn searching(&self) -> bool {
        self.results.is_some()
    }
    fn onset_sensitivity(&self) -> f32 {
        0.5
    }
    fn notify(&mut self, message: Message) {
        self.messages.push(message);
    }
    fn confirm(&mut self, question: Confirm) {
        self.asked = Some(question);
    }
    fn prompt(&mut self, prompt: Prompt) {
        self.prompts.push(prompt);
    }
    fn present(&mut self, presentation: Presentation) {
        self.shown.push(presentation);
    }
    fn sampler(&self) -> &playr_app::sampler::Sampler {
        &self.sampler
    }
    fn sampler_mut(&mut self) -> &mut playr_app::sampler::Sampler {
        &mut self.sampler
    }
    fn planning(&mut self, _job: JobId) {
        self.planning = true;
    }
    fn take_plan(&mut self) -> Option<Plan> {
        self.plan.take()
    }
    fn sql_started(&mut self, _: JobId, _: playr_app::dispatch::SqlThen) {}
    fn tape(&mut self) -> &mut playr_app::tape::Deck {
        &mut self.tape
    }
}

/// A headless frontend over a library file of tracks a, b and c, and
/// playlists "early" (c) and "late" (a, b), with its directory.
fn headless() -> (Headless, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = db::open(&dir.path().join("library.db")).unwrap();
    let ids: Vec<i64> = ["/m/a.flac", "/m/b.flac", "/m/c.flac"]
        .iter()
        .map(|path| {
            let t = Track {
                path: path.to_string(),
                mtime: 1,
                size: 1,
                ..Default::default()
            };
            db::upsert(&conn, &t).unwrap()
        })
        .collect();
    query::save_playlist(&mut conn, "late", &ids[..2]).unwrap();
    query::save_playlist(&mut conn, "early", &ids[2..]).unwrap();
    let session = Session::new(conn, common::fake_player().0, event::ignore());
    let frontend = Headless {
        session,
        keys: Keymap::default(),
        view: Library,
        cursors: [Some(0), None, Some(0), None],
        results: None,
        messages: Vec::new(),
        asked: None,
        prompts: Vec::new(),
        shown: Vec::new(),
        plan: None,
        planning: false,
        sampler: Default::default(),
        tape: playr_app::tape::Deck::manual(),
    };
    (frontend, dir)
}

fn last(f: &Headless) -> Option<&Message> {
    f.messages.last()
}

fn selection(f: &Headless) -> Vec<String> {
    f.session
        .selection()
        .iter()
        .map(|t| t.path.clone())
        .collect()
}

fn queue(f: &Headless) -> Vec<PathBuf> {
    f.session.player().queue().to_vec()
}

#[test]
fn adding_moves_the_cursor_and_playing_starts_from_it() {
    let (mut f, _dir) = headless();
    dispatch(Action::Add, &mut f);
    dispatch(Action::Add, &mut f);
    assert_eq!(selection(&f), ["/m/a.flac", "/m/b.flac"]);
    assert_eq!(f.cursor(Library), Some(2));
    assert_eq!(
        f.cursor(Selection),
        Some(0),
        "the selection cursor was not placed"
    );
    assert_eq!(last(&f), Some(&Outcome::AddedToSelection.into()));

    // On c, the last row, the cursor stays: the first add selects c and the
    // second unselects it.
    dispatch(Action::Add, &mut f);
    dispatch(Action::Add, &mut f);
    assert_eq!(selection(&f), ["/m/a.flac", "/m/b.flac"]);
    assert_eq!(last(&f), Some(&Outcome::RemovedFromSelection.into()));

    dispatch(Action::CursorFirst, &mut f);
    dispatch(Action::Cursor(1), &mut f);
    dispatch(Action::Activate, &mut f);
    assert_eq!(queue(&f).len(), 3);
    assert_eq!(f.session.player().status().queue.len(), 3);

    // In the selection, the cursor names the track to move and remove.
    dispatch(Action::ShowView(Selection), &mut f);
    dispatch(Action::MoveTrack(1), &mut f);
    assert_eq!(selection(&f), ["/m/b.flac", "/m/a.flac"]);
    assert_eq!(f.cursor(Selection), Some(1));
    dispatch(Action::Remove, &mut f);
    assert_eq!(selection(&f), ["/m/b.flac"]);
    assert_eq!(f.cursor(Selection), Some(0));
}

#[test]
fn enqueueing_adds_the_row_under_the_cursor_and_moves_on() {
    let (mut f, _dir) = headless();
    // Nothing playing: the track queued is the whole queue.
    f.set_cursor(Library, Some(1));
    dispatch(Action::Enqueue(true), &mut f);
    assert_eq!(queued(&f), [PathBuf::from("/m/b.flac")]);
    assert_eq!(
        last(&f),
        Some(
            &Outcome::Queued {
                tracks: 1,
                next: true
            }
            .into()
        )
    );
    assert_eq!(f.cursor(Library), Some(2), "the cursor did not move on");

    // On a playlist, its tracks; the playlists are listed by name.
    dispatch(Action::ShowView(Playlists), &mut f);
    f.set_cursor(Playlists, Some(1));
    dispatch(Action::Enqueue(false), &mut f);
    assert_eq!(
        last(&f),
        Some(
            &Outcome::Queued {
                tracks: 2,
                next: false
            }
            .into()
        )
    );

    // The sampler and the queue list no rows to queue.
    let before = f.messages.len();
    for view in [Sampler, View::Queue] {
        dispatch(Action::ShowView(view), &mut f);
        dispatch(Action::Enqueue(false), &mut f);
    }
    assert_eq!(f.messages.len(), before);

    // `:enqueue all` takes search results or the selection, not the library.
    dispatch(Action::ShowView(Library), &mut f);
    dispatch(Action::EnqueueAll, &mut f);
    assert_eq!(last(&f), Some(&Message::NothingToQueue));
}

/// The tracks the player holds as queued, in order.
fn queued(f: &Headless) -> Vec<PathBuf> {
    let status = f.session.player().status();
    status
        .queue
        .iter()
        .zip(status.queued.iter())
        .filter(|(_, q)| **q)
        .map(|(p, _)| p.clone())
        .collect()
}

#[test]
fn the_queue_view_takes_tracks_out_and_keeps_the_one_playing_first() {
    // Real files, so the queue holds still while it plays.
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("library.db")).unwrap();
    for name in ["a", "b", "c"] {
        let path = dir.path().join(format!("{name}.wav"));
        common::silence(&path, 8000, 5.0);
        let t = Track {
            path: path.to_string_lossy().into_owned(),
            mtime: 1,
            size: 1,
            ..Default::default()
        };
        db::upsert(&conn, &t).unwrap();
    }
    let (mut f, _other) = headless();
    f.session = Session::new(conn, common::fake_player().0, event::ignore());
    let name = |f: &Headless, row: usize| {
        let rows = f.session.queue_rows();
        let path = f.session.player().queue()[rows[row]].clone();
        path.file_stem().unwrap().to_string_lossy().into_owned()
    };
    let wait = |f: &Headless, done: &dyn Fn(&Headless) -> bool| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !done(f) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };

    f.set_cursor(Library, Some(0));
    dispatch(Action::Activate, &mut f);
    wait(&f, &|f| {
        f.session.player().status().state == playr_core::audio::State::Playing
    });
    assert!(
        f.session.queue_rows().is_empty(),
        "the library filled the queue"
    );
    // b interrupts the library; c waits behind it.
    f.set_cursor(Library, Some(1));
    dispatch(Action::Enqueue(false), &mut f);
    wait(&f, &|f| {
        let status = f.session.player().status();
        f.session.queue_rows().first() == Some(&status.index) && name(f, 0) == "b"
    });
    dispatch(Action::Enqueue(false), &mut f);
    assert_eq!((name(&f, 0), name(&f, 1)), ("b".into(), "c".into()));

    dispatch(Action::ShowView(View::Queue), &mut f);
    f.set_cursor(View::Queue, Some(1));
    dispatch(Action::MoveTrack(-1), &mut f);
    assert_eq!(
        name(&f, 0),
        "b",
        "a waiting track moved above the one playing"
    );
    dispatch(Action::Remove, &mut f);
    assert_eq!(f.session.queue_rows().len(), 1);
    assert!(matches!(
        last(&f),
        Some(Message::Core(Notice::Done(Outcome::RemovedTrack { .. })))
    ));
    // Only b, playing, is left; clearing takes it out of the queue, and it
    // plays on.
    dispatch(Action::ClearQueue, &mut f);
    assert_eq!(f.asked, Some(Confirm::ClearQueue(1)));
    confirmed(Confirm::ClearQueue(1), &mut f);
    assert!(f.session.queue_rows().is_empty());
    let status = f.session.player().status();
    assert_eq!(status.current().unwrap().file_stem().unwrap(), "b");
    dispatch(Action::ClearQueue, &mut f);
    assert_eq!(last(&f), Some(&Message::QueueEmpty));
}

#[test]
fn enter_in_the_queue_jumps_without_replacing_it() {
    let (mut f, _dir) = headless();
    dispatch(Action::Activate, &mut f);
    let playing = f.session.player().queue();
    dispatch(Action::ShowView(View::Queue), &mut f);
    f.set_cursor(View::Queue, Some(2));
    dispatch(Action::Activate, &mut f);
    assert!(
        std::sync::Arc::ptr_eq(&playing, &f.session.player().queue()),
        "the queue was replaced"
    );
}

#[test]
fn searching_lists_results_and_clearing_lists_the_library() {
    let (mut f, _dir) = headless();
    dispatch(Action::ShowView(Playlists), &mut f);
    dispatch(Action::Search("b".into()), &mut f);
    assert_eq!(f.view, Library);
    assert_eq!(f.listed().len(), 1);
    assert_eq!(f.cursor(Library), Some(0));
    dispatch(Action::Search("zzz".into()), &mut f);
    assert_eq!(last(&f), Some(&Message::NoMatches));
    assert_eq!(f.cursor(Library), None);
    dispatch(Action::ClearSearch, &mut f);
    assert_eq!(f.listed().len(), 3);
    assert_eq!(f.cursor(Library), Some(0));
}

#[test]
fn playlists_are_renamed_deleted_and_replaced_through_confirmation() {
    let (mut f, _dir) = headless();
    let names = |f: &Headless| -> Vec<String> {
        f.session
            .playlists()
            .iter()
            .map(|p| p.name.clone())
            .collect()
    };
    dispatch(Action::RenameTo("x".into()), &mut f);
    assert_eq!(last(&f), Some(&Message::NoPlaylistUnderCursor));

    dispatch(Action::ShowView(Playlists), &mut f);
    dispatch(Action::StartRename, &mut f);
    let early: Playlist = f.session.playlists()[0].clone();
    assert_eq!(f.prompts.last(), Some(&Prompt::Rename(early)));
    // "early" sorts first; renamed "zzz", it sorts last and the cursor follows.
    dispatch(Action::RenameTo("zzz".into()), &mut f);
    assert_eq!(names(&f), ["late", "zzz"]);
    assert_eq!(f.cursor(Playlists), Some(1));

    dispatch(Action::DeletePlaylist, &mut f);
    let question = f.asked.take().unwrap();
    assert!(matches!(&question, Confirm::DeletePlaylist(p) if p.name == "zzz"));
    assert_eq!(names(&f), ["late", "zzz"], "deleted before a yes");
    confirmed(question, &mut f);
    assert_eq!(names(&f), ["late"]);
    assert_eq!(f.cursor(Playlists), Some(0));

    // Saving over "late" asks first.
    dispatch(Action::SaveAs("late".into()), &mut f);
    assert_eq!(last(&f), Some(&Refusal::SelectionEmpty.into()));
    dispatch(Action::Add, &mut f);
    dispatch(Action::SaveAs("late".into()), &mut f);
    assert_eq!(
        f.asked,
        Some(Confirm::ReplacePlaylist {
            name: "late".into(),
            queue: false
        })
    );
    confirmed(f.asked.take().unwrap(), &mut f);
    assert!(matches!(
        last(&f),
        Some(Message::Core(Notice::Done(Outcome::Saved {
            tracks: 2,
            ..
        })))
    ));
}

#[test]
fn commands_parse_and_dispatch_in_the_frontend_s_view() {
    let (mut f, _dir) = headless();
    let run = |f: &mut Headless, line: &str| {
        let action = command::parse(line, f.view).unwrap();
        dispatch(action, f);
    };
    run(&mut f, "view sampler");
    run(&mut f, "zoom +");
    run(&mut f, "display braille");
    run(&mut f, "theme light");
    run(&mut f, "keys");
    assert_eq!(
        f.shown,
        [
            Presentation::Zoom(playr_app::action::Zoom::In),
            Presentation::Display(Some(playr_app::Display::Braille)),
            Presentation::Theme(playr_app::Theme::Light),
            Presentation::KeyList
        ]
    );
    // No rows in the sampler: cursor commands change nothing.
    run(&mut f, "down");
    assert_eq!(f.cursors, [Some(0), None, Some(0), None]);
    run(&mut f, "slice 4");
    assert_eq!(last(&f), Some(&Refusal::NothingPlaying.into()));
    assert!(!f.planning);
    run(&mut f, "write");
    assert_eq!(last(&f), Some(&Message::NoSlicesPlanned));

    run(&mut f, "map sampler ctrl-q quit");
    let ctrl_q = Key::parse("ctrl-q").unwrap();
    assert_eq!(f.keys.lookup(ctrl_q, Sampler), Some(&Action::Quit));
    run(&mut f, "unmap ctrl-q");
    assert_eq!(
        last(&f),
        Some(&Message::NotBound {
            key: ctrl_q,
            view: None
        })
    );
    run(&mut f, "next-view");
    assert_eq!(f.view, Library);
    run(&mut f, "search");
    // `command` names the prompt only as a key binding's target.
    dispatch(Action::StartCommand, &mut f);
    assert_eq!(
        f.prompts,
        [
            Prompt::Search(String::new()),
            Prompt::Command(String::new())
        ]
    );
}

/// Plays `track` alone, waits until it plays, and marks it at `secs`.
fn play_and_mark(f: &mut Headless, track: &Track, secs: &[u64]) {
    f.session.play(std::slice::from_ref(track), 0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while f.session.playing_track().map(|(p, _)| p) != Ok(PathBuf::from(&track.path)) {
        assert!(
            std::time::Instant::now() < deadline,
            "never started playing"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    for s in secs {
        f.session.add_mark(Some(std::time::Duration::from_secs(*s)));
    }
}

#[test]
fn clearing_marks_clears_the_track_asked_about_after_the_track_changes() {
    let (mut f, dir) = headless();
    let wav = |name: &str| {
        let path = dir.path().join(name);
        common::silence(&path, 8000, 20.0);
        Track {
            path: path.to_string_lossy().into_owned(),
            mtime: 1,
            size: 1,
            ..Default::default()
        }
    };
    let (a, b) = (wav("a.wav"), wav("b.wav"));
    let marks =
        |f: &mut Headless, t: &Track| f.session.marks_for(Some(&PathBuf::from(&t.path))).len();

    play_and_mark(&mut f, &a, &[5, 10]);
    dispatch(Action::ClearMarks, &mut f);
    let question = f.asked.take().unwrap();
    assert!(matches!(&question, Confirm::ClearMarks { count: 2, .. }));
    // The track changes while the question is open, and the frontend reads
    // the new track's marks, as `Model::refresh` does each frame.
    play_and_mark(&mut f, &b, &[5, 10, 15]);
    confirmed(question, &mut f);

    assert_eq!(marks(&mut f, &a), 0, "the track asked about kept its marks");
    assert_eq!(
        marks(&mut f, &b),
        3,
        "a track not asked about lost its marks"
    );
}

#[test]
fn pruning_asks_first_and_starts_on_a_yes() {
    let (mut f, dir) = headless();
    let nowhere = dir.path().join("nowhere");
    dispatch(Action::Prune(Some(nowhere.clone())), &mut f);
    assert_eq!(last(&f), Some(&Refusal::NotADirectory(nowhere).into()));
    assert_eq!(f.asked, None, "asked about a directory that is not there");

    let music = dir.path().to_path_buf();
    dispatch(Action::Prune(Some(music.clone())), &mut f);
    let question = f.asked.take().unwrap();
    assert_eq!(question, Confirm::Prune(Some(music.clone())));
    assert_ne!(
        last(&f),
        Some(
            &Outcome::PruneStarted {
                dir: Some(music.clone())
            }
            .into()
        ),
        "pruned before a yes"
    );
    confirmed(question, &mut f);
    assert_eq!(
        last(&f),
        Some(&Outcome::PruneStarted { dir: Some(music) }.into())
    );
}

#[test]
fn the_new_questions_say_what_they_will_do() {
    assert_eq!(
        Confirm::ForgetRoot("/mnt/music".into()).question(),
        "forget /mnt/music, and every track and mark under it?"
    );
    assert_eq!(
        Confirm::Resume {
            path: "/mnt/music/amen.flac".into(),
            at: std::time::Duration::from_secs(95),
            queue: Default::default(),
        }
        .question(),
        "take up amen.flac again at 1:35?"
    );
    let queue = playr_core::db::SavedQueue {
        played: vec!["/mnt/music/a.flac".into()],
        playing: true,
        waiting: vec!["/mnt/music/b.flac".into()],
    };
    assert_eq!(
        Confirm::Resume {
            path: "/mnt/music/amen.flac".into(),
            at: std::time::Duration::from_secs(95),
            queue,
        }
        .question(),
        "take up amen.flac again at 1:35, with its queue of 3 tracks?"
    );
}

/// Runs `f`'s manual tape for `frames` stereo frames, then takes in what
/// it finished.
fn run_tape(f: &mut Headless, frames: usize) {
    let mut out = vec![0.0; 512 * 2];
    for _ in 0..frames.div_ceil(512) {
        f.tape.process(&mut out);
    }
    playr_app::tape::poll(f);
}

/// Runs `f`'s tape until `done` holds of the last message.
fn tape_until(f: &mut Headless, done: impl Fn(&Message) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !last(f).is_some_and(&done) {
        assert!(std::time::Instant::now() < deadline, "{:?}", last(f));
        run_tape(f, 512);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn the_tape_loads_the_range_and_saves_the_loop_and_the_mix_to_the_library() {
    use playr_app::tape::{Pos, TapeAction as T, TapeMessage as M, VoiceSetting as V};
    use playr_core::audio::State;
    let tape = |f: &mut Headless, t| dispatch(Action::Tape(t), f);
    let said = |f: &Headless, m: M| assert_eq!(last(f), Some(&Message::Tape(m)));

    let (mut f, dir) = headless();
    let samples = dir.path().join("samples");
    f.session.set_samples_dir(samples.clone());
    tape(&mut f, T::Play);
    said(&f, M::NoTape);
    tape(&mut f, T::Load(None));
    assert_eq!(last(&f), Some(&Refusal::NothingPlaying.into()));

    let file = dir.path().join("song.wav");
    common::tone(&file, 8000, 2.0, -6.0);
    let track = Track {
        path: file.to_string_lossy().into(),
        ..Default::default()
    };
    f.session.play(&[track], 0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while f.session.playing_track().is_err() {
        assert!(std::time::Instant::now() < deadline, "never played");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    tape(&mut f, T::Load(None));
    said(&f, M::NoRange);
    tape(&mut f, T::Load(Some(1)));
    said(&f, M::EmptySlot(1));

    f.sampler.range = Some(playr_app::sampler::Range {
        path: file.clone(),
        start: Some(4000),
        end: Some(12_000),
    });
    tape(&mut f, T::Load(None));
    said(&f, M::Loading);
    tape_until(&mut f, |m| m != &Message::Tape(M::Loading));
    said(
        &f,
        M::Loaded {
            frames: 8000,
            rate: 8000,
        },
    );
    // The range, with what the track holds of a second either side as pre-roll
    // and post-roll.
    assert_eq!(
        f.tape.loaded(),
        Some(playr_app::tape::Extent {
            frames: 16_000,
            range: playr_looper::Window::new(4000, 12_000),
            rate: 8000,
        })
    );

    // Playing the tape pauses the player.
    tape(&mut f, T::Play);
    said(&f, M::Done(T::Play));
    while f.session.player().status().state != State::Paused {
        assert!(std::time::Instant::now() < deadline, "never paused");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    for t in [
        T::Write(true),
        T::Feedback(0.7),
        T::Voice(2, V::On(true)),
        T::Voice(2, V::Rate(-0.5)),
        T::Voice(2, V::Send(0.5)),
        T::Voice(2, V::Wear(0.4)),
    ] {
        tape(&mut f, t);
        said(&f, M::Done(t));
    }
    // What the controls show follows what was sent.
    let state = *f.tape.state().unwrap();
    assert_eq!((state.voices[1].rate, state.voices[1].wear), (-0.5, 0.4));
    assert!(state.playing && state.write && state.voices[1].on);
    tape(
        &mut f,
        T::Voice(1, V::Window(Pos::Percent(50.0), Pos::Percent(25.0))),
    );
    said(&f, M::EmptyWindow);

    tape(&mut f, T::Record);
    let Some(Message::Tape(M::Recording(mix))) = last(&f).cloned() else {
        panic!("{:?}", last(&f));
    };
    run_tape(&mut f, 4000);
    tape(&mut f, T::Save);
    tape(&mut f, T::Save);
    said(&f, M::AlreadySaving);
    tape_until(&mut f, |m| matches!(m, Message::Tape(M::Saved(_))));
    let Some(Message::Tape(M::Saved(lp))) = last(&f).cloned() else {
        unreachable!()
    };
    // Recording stops on its own when the callback does not acknowledge it;
    // the manual tape here never does.
    tape(&mut f, T::Record);
    let Some(Message::Tape(M::Recorded { path, frames, .. })) = last(&f).cloned() else {
        panic!("{:?}", last(&f));
    };
    assert_eq!(path, mix);

    let wav = |p: &std::path::Path| {
        let r = hound::WavReader::open(p).unwrap();
        let spec = r.spec();
        (spec.channels, spec.sample_rate, r.duration())
    };
    assert_eq!(wav(&lp), (2, 8000, 8000));
    assert_eq!(wav(&mix), (2, 8000, frames as u32));
    assert!(frames >= 4000, "{frames}");
    for p in [&lp, &mix] {
        assert!(p.starts_with(&samples), "{}", p.display());
        let canonical = p.canonicalize().unwrap();
        let canonical = canonical.to_string_lossy();
        assert!(
            f.session.tracks().iter().any(|t| t.path == canonical),
            "{canonical} is not in the library"
        );
    }
    assert_ne!(lp.parent(), mix.parent(), "each save has its own directory");
}

#[test]
fn tape_windows_count_from_the_range_and_reach_into_the_rolls() {
    use playr_app::tape::{settings, Extent, Pos, TapeAction as T, TapeMessage, VoiceSetting as V};
    use playr_looper::{Setting, Window};
    use std::time::Duration;
    // A range of 8000 frames with 1000 frames of pre-roll and post-roll.
    let e = Extent {
        frames: 10_000,
        range: Window::new(1000, 9000),
        rate: 4000,
    };
    let at = |a, b| T::WriteWindow(a, b);
    assert_eq!(
        settings(at(Pos::Percent(25.0), Pos::Percent(100.0)), e),
        Ok(vec![Setting::WriteWindow(Window::new(3000, 9000))])
    );
    assert_eq!(
        settings(
            T::Voice(
                3,
                V::Window(
                    Pos::Time(Duration::from_millis(500)),
                    Pos::Time(Duration::from_secs(9))
                )
            ),
            e
        ),
        Ok(vec![Setting::Window(2, Window::new(3000, 10_000))])
    );
    assert_eq!(
        settings(at(Pos::Percent(-12.5), Pos::Percent(150.0)), e),
        Ok(vec![Setting::WriteWindow(Window::new(0, 10_000))])
    );
    assert_eq!(
        settings(at(Pos::Percent(50.0), Pos::Percent(50.0)), e),
        Err(TapeMessage::EmptyWindow)
    );
}

#[test]
fn tape_controls_say_when_they_have_no_effect() {
    use playr_app::tape::{no_effect, Control as C, Extent, Idle, TapeState};
    use playr_looper::Window;
    let e = Extent {
        frames: 10_000,
        range: Window::new(1000, 9000),
        rate: 8000,
    };
    let idle = |s: &TapeState, c| no_effect(s, e, c);
    let mut s = TapeState::new(e.range);

    // One voice, its wear set, writing on, nothing else changed.
    s.voices[0].wear = 0.1;
    s.write = true;
    assert_eq!(idle(&s, C::Wear(0)), Some(Idle::NoSend));
    assert_eq!(idle(&s, C::Write), Some(Idle::Unchanging));
    s.feedback = 0.9;
    assert_eq!(idle(&s, C::Write), None, "the loop now fades");
    assert_eq!(
        idle(&s, C::Wear(0)),
        Some(Idle::NoSend),
        "but does not darken"
    );
    s.voices[0].send = 0.5;
    assert_eq!(idle(&s, C::Wear(0)), None);
    assert_eq!(idle(&s, C::Send(0)), None);

    // Write off idles what only writing uses.
    s.write = false;
    for c in [
        C::Send(0),
        C::Wear(0),
        C::Feedback,
        C::WriteWear,
        C::WriteWindow,
    ] {
        assert_eq!(idle(&s, c), Some(Idle::WriteOff), "{c:?}");
    }
    assert_eq!(idle(&s, C::Write), None);

    // A voice off idles all of its own controls first.
    for c in [
        C::Rate(1),
        C::Level(1),
        C::Pan(1),
        C::Send(1),
        C::Wear(1),
        C::Fade(1),
    ] {
        assert_eq!(idle(&s, c), Some(Idle::VoiceOff), "{c:?}");
    }
    s.voices[0].level = 0.0;
    assert_eq!(idle(&s, C::Pan(0)), Some(Idle::Silent));
    s.voices[0].rate = 0.0;
    assert_eq!(idle(&s, C::Fade(0)), Some(Idle::Still));
    // A window over the whole buffer has nothing either side to fade into.
    s.voices[0].rate = 1.0;
    s.voices[0].window = Window::new(0, 10_000);
    assert_eq!(idle(&s, C::Fade(0)), Some(Idle::NoRoom));
    s.voices[0].window = e.range;
    assert_eq!(idle(&s, C::Fade(0)), None);
}
