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
    dj: playr_app::dj::Decks,
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
    fn dj(&mut self) -> &mut playr_app::dj::Decks {
        &mut self.dj
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
        dj: playr_app::dj::Decks::manual(),
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

/// A tape loaded with 8000 frames of a tone playing at 8 kHz.
fn loaded_tape() -> (Headless, tempfile::TempDir) {
    use playr_app::tape::{TapeAction as T, TapeMessage as M};
    let (mut f, dir) = headless();
    f.session.set_samples_dir(dir.path().join("samples"));
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
    f.sampler.range = Some(playr_app::sampler::Range {
        path: file.clone(),
        start: Some(4000),
        end: Some(12_000),
    });
    dispatch(Action::Tape(T::Load(None)), &mut f);
    tape_until(&mut f, |m| m != &Message::Tape(M::Loading));
    (f, dir)
}

#[test]
fn a_save_cut_short_is_reported_and_does_not_block_the_next() {
    use playr_app::tape::{Pos, TapeAction as T, TapeMessage as M};
    let tape = |f: &mut Headless, t| dispatch(Action::Tape(t), f);
    let (mut f, _dir) = loaded_tape();

    // The new write window arrives with the snapshot and aborts it.
    tape(&mut f, T::Save);
    tape(
        &mut f,
        T::WriteWindow(Pos::Percent(0.0), Pos::Percent(50.0)),
    );
    run_tape(&mut f, 512);
    assert_eq!(last(&f), Some(&Message::Tape(M::SaveAborted)));
    tape(&mut f, T::Save);
    tape_until(&mut f, |m| matches!(m, Message::Tape(M::Saved(_))));
}

#[test]
fn a_recording_whose_stop_failed_can_be_stopped_again() {
    use playr_app::tape::{TapeAction as T, TapeMessage as M};
    let tape = |f: &mut Headless, t| dispatch(Action::Tape(t), f);
    let (mut f, _dir) = loaded_tape();
    tape(&mut f, T::Record);
    assert!(f.tape.recording());
    // A full command ring refuses the stop; the recording runs on.
    while !matches!(last(&f), Some(Message::Tape(M::Failed(_)))) {
        tape(&mut f, T::Feedback(0.5));
    }
    tape(&mut f, T::Record);
    assert!(matches!(last(&f), Some(Message::Tape(M::Failed(_)))));
    assert!(f.tape.recording());
    run_tape(&mut f, 512);
    tape(&mut f, T::Record);
    assert!(matches!(last(&f), Some(Message::Tape(M::Recorded { .. }))));
    assert!(!f.tape.recording());
}

#[test]
fn a_lost_device_ends_the_tape_and_its_recording() {
    use playr_app::tape::{TapeAction as T, TapeMessage as M};
    use playr_core::audio::output::DeviceEvent;
    let tape = |f: &mut Headless, t| dispatch(Action::Tape(t), f);
    let (mut f, _dir) = loaded_tape();
    tape(&mut f, T::Play);
    tape(&mut f, T::Record);
    let events = f.tape.device_events();
    events.send(DeviceEvent::Rerouted).unwrap();
    events.send(DeviceEvent::Lost("unplugged".into())).unwrap();
    let before = f.messages.len();
    run_tape(&mut f, 512);
    let said: Vec<_> = f.messages[before..].to_vec();
    assert!(
        matches!(
            said[..],
            [
                Message::Tape(M::Recorded { .. }),
                Message::Tape(M::DeviceLost(_))
            ]
        ),
        "{said:?}"
    );
    assert_eq!(f.tape.loaded(), None);
    assert!(!f.tape.recording());
    tape(&mut f, T::Play);
    assert_eq!(last(&f), Some(&Message::Tape(M::NoTape)));
    tape(&mut f, T::Load(None));
    tape_until(&mut f, |m| matches!(m, Message::Tape(M::Loaded { .. })));
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
    s.thin = 0.3;
    assert_eq!(idle(&s, C::Write), None, "the loop now thins");
    s.thin = 0.0;
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
        C::Thin,
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
        C::Ping(1),
        C::Slew(1),
        C::Drive(1),
        C::Cutoff(1),
        C::Filter(1),
    ] {
        assert_eq!(idle(&s, c), Some(Idle::VoiceOff), "{c:?}");
    }
    // Soloing a voice that is off still silences the others.
    assert_eq!(idle(&s, C::Solo(1)), None);
    s.voices[0].level = 0.0;
    assert_eq!(idle(&s, C::Pan(0)), Some(Idle::Silent));
    s.voices[0].rate = 0.0;
    assert_eq!(idle(&s, C::Fade(0)), Some(Idle::Still));
    assert_eq!(idle(&s, C::Ping(0)), Some(Idle::Still));
    // A window over the whole buffer has nothing either side to fade into.
    s.voices[0].rate = 1.0;
    s.voices[0].window = Window::new(0, 10_000);
    assert_eq!(idle(&s, C::Fade(0)), Some(Idle::NoRoom));
    s.voices[0].window = e.range;
    assert_eq!(idle(&s, C::Fade(0)), None);
    s.voices[0].ping = true;
    assert_eq!(idle(&s, C::Fade(0)), Some(Idle::Turns));
}

/// Runs `f`'s manual decks for `blocks` blocks of 512 frames, then takes
/// in what they finished.
fn run_decks(f: &mut Headless, blocks: usize) {
    let mut out = vec![0.0; 512 * 2];
    for _ in 0..blocks {
        f.dj.process(&mut out);
    }
    playr_app::dj::poll(f);
}

/// Runs `f`'s decks until `done` holds of the last message.
fn decks_until(f: &mut Headless, done: impl Fn(&Message) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !last(f).is_some_and(&done) {
        assert!(std::time::Instant::now() < deadline, "{:?}", last(f));
        run_decks(f, 1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// A click track at `bpm` in the library, with its grid set by hand.
fn click_track(f: &mut Headless, dir: &std::path::Path, name: &str, bpm: f64, t0: f64) -> Track {
    use playr_core::analysis::tempo::Grid;
    let file = dir.join(name);
    common::clicks(&file, 8000, bpm as f32, t0 as f32, 20.0);
    let track = Track {
        path: file.to_string_lossy().into(),
        sample_rate: Some(8000),
        ..Default::default()
    };
    f.session.set_grid(&file, Some(Grid { bpm, t0 })).unwrap();
    track
}

#[test]
fn the_decks_load_from_the_cursor_sync_and_keep_grid_edits() {
    use playr_app::dj::{DjAction as D, DjMessage as M, GridEdit as G, Side::*};
    use playr_core::audio::State;
    let dj = |f: &mut Headless, d| dispatch(Action::Dj(d), f);
    let said = |f: &Headless, m: M| assert_eq!(last(f), Some(&Message::Dj(m)));

    let (mut f, dir) = headless();
    // Strict: a track picked for a playing deck waits.
    dj(&mut f, D::Strict(true));
    dj(&mut f, D::Play(A));
    said(&f, M::Empty(A));
    f.view = Sampler;
    dj(&mut f, D::Load(A));
    said(&f, M::NoTrack);
    f.view = Library;

    let a = click_track(&mut f, dir.path(), "a.wav", 128.0, 0.1);
    let b = click_track(&mut f, dir.path(), "b.wav", 125.0, 0.3);
    f.results = Some(vec![a.clone(), b.clone()]);
    f.cursors[0] = Some(0);
    dj(&mut f, D::Load(A));
    said(&f, M::Loading(A));
    decks_until(&mut f, |m| matches!(m, Message::Dj(M::Loaded { .. })));
    said(
        &f,
        M::Loaded {
            side: A,
            title: "a".into(),
            bpm: Some(128.0),
        },
    );
    f.cursors[0] = Some(1);
    dj(&mut f, D::Load(B));
    decks_until(&mut f, |m| matches!(m, Message::Dj(M::Loaded { .. })));
    assert_eq!(f.dj.loaded(B).unwrap().frames, 160_000);
    assert_eq!(f.dj.rate(), Some(8000));

    // Playing a deck pauses the player.
    f.session.play(std::slice::from_ref(&a), 0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while f.session.player().status().state != State::Playing {
        assert!(std::time::Instant::now() < deadline, "never played");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    dj(&mut f, D::Play(A));
    said(&f, M::Done(D::Play(A)));
    while f.session.player().status().state != State::Paused {
        assert!(std::time::Instant::now() < deadline, "never paused");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // A deck started since the engine last ran refuses a track; one known
    // to play keeps it for when it stops.
    dj(&mut f, D::Load(A));
    said(&f, M::Loading(A));
    decks_until(&mut f, |m| m != &Message::Dj(M::Loading(A)));
    said(&f, M::Playing(A));
    dj(&mut f, D::Load(A));
    said(
        &f,
        M::Queued {
            side: A,
            title: "b".into(),
        },
    );
    assert_eq!(f.dj.loaded(A).unwrap().title, "a");
    dj(&mut f, D::Unqueue(A));
    assert!(f.dj.next(A).is_none());

    // Sync follows deck A's tempo.
    dj(&mut f, D::Sync(B, true));
    said(&f, M::Done(D::Sync(B, true)));
    run_decks(&mut f, 4);
    let status = f.dj.status().unwrap().deck(B);
    assert!(status.synced());
    assert!((status.pct() - (128.0 / 125.0 - 1.0) * 100.0).abs() < 1e-9);

    // Grid edits move the engine's grid and stay with the track.
    dj(&mut f, D::Grid(B, G::Double));
    said(
        &f,
        M::Grid {
            side: B,
            bpm: 250.0,
            t0: 0.3,
        },
    );
    dj(&mut f, D::Grid(B, G::Later));
    dj(&mut f, D::Grid(B, G::Offset(10.0)));
    let g = f.session.grid(std::path::Path::new(&b.path)).unwrap();
    assert_eq!(g.bpm, 250.0);
    assert!(
        (g.t0 - (0.3 + 60.0 / 250.0 + 0.01)).abs() < 1e-9,
        "{}",
        g.t0
    );
    assert_eq!(f.dj.loaded(B).unwrap().grid, Some(g));
    // Doubled, deck B still follows deck A at half its new tempo.
    run_decks(&mut f, 1);
    assert!(f.dj.status().unwrap().deck(B).synced());

    // 500 BPM, halved, is 69% faster than 128 at -40%: beyond any range.
    dj(&mut f, D::Grid(B, G::Double));
    dj(&mut f, D::Sync(B, false));
    dj(&mut f, D::Range(A, playr_app::dj::Range::Wide));
    dj(&mut f, D::Rate(A, -40.0));
    run_decks(&mut f, 1);
    dj(&mut f, D::Sync(B, true));
    said(&f, M::OutOfReach(B));

    // Taps on deck A, 15 blocks apart at rate 0.6: 0.576 track seconds, or
    // 104.17 BPM. The first two keep the grid's tempo and move its beat.
    dj(&mut f, D::Grid(A, G::Tap));
    said(&f, M::Tapped(A));
    let mut taps = vec![f.dj.status().unwrap().deck(A).pos()];
    for _ in 0..3 {
        run_decks(&mut f, 15);
        taps.push(f.dj.status().unwrap().deck(A).pos());
        dj(&mut f, D::Grid(A, G::Tap));
    }
    let g = f.session.grid(std::path::Path::new(&a.path)).unwrap();
    assert!((g.bpm - 60.0 / 0.576).abs() < 1e-6, "{}", g.bpm);
    assert_eq!(g.t0, taps[3] / 8000.0);

    dj(&mut f, D::Grid(A, G::Reset));
    assert_eq!(f.session.grid(std::path::Path::new(&a.path)), None);
    said(&f, M::NoGrid(A));
}

#[test]
fn hot_cues_are_kept_with_the_track_and_loops_need_a_grid() {
    use playr_app::dj::{CueOut, DjAction as D, DjMessage as M, Side::*};
    let dj = |f: &mut Headless, d| dispatch(Action::Dj(d), f);
    let said = |f: &Headless, m: M| assert_eq!(last(f), Some(&Message::Dj(m)));
    let (mut f, dir) = headless();
    let a = click_track(&mut f, dir.path(), "a.wav", 120.0, 0.0);
    let file = std::path::PathBuf::from(&a.path);
    f.results = Some(vec![a]);
    f.cursors[0] = Some(0);
    let load = |f: &mut Headless| {
        dj(f, D::Load(A));
        decks_until(f, |m| matches!(m, Message::Dj(M::Loaded { .. })));
    };
    load(&mut f);

    // Set at the head, 10 blocks in: 5120 frames at 8 kHz.
    dj(&mut f, D::Play(A));
    run_decks(&mut f, 10);
    dj(&mut f, D::HotCue(A, 2));
    run_decks(&mut f, 1);
    assert_eq!(f.session.hot_cues(&file), [(2, 0.64)]);
    assert_eq!(f.dj.loaded(A).unwrap().hot[1], Some(5120.0));

    // Loaded again, the deck has it back.
    dj(&mut f, D::Pause(A));
    run_decks(&mut f, 2);
    load(&mut f);
    // Stored throughout: the status of the new load never lacks it.
    assert_eq!(f.session.hot_cues(&file), [(2, 0.64)]);
    run_decks(&mut f, 1);
    assert_eq!(f.session.hot_cues(&file), [(2, 0.64)]);
    assert_eq!(f.dj.status().unwrap().deck(A).hot_cues()[1], Some(5120.0));
    dj(&mut f, D::HotClear(A, 2));
    run_decks(&mut f, 1);
    assert!(f.session.hot_cues(&file).is_empty());

    dj(&mut f, D::Loop(A, Some(4.0)));
    run_decks(&mut f, 1);
    assert!(f.dj.status().unwrap().deck(A).looping().is_some());
    dj(&mut f, D::CueOut(CueOut::Channels));
    said(&f, M::NoCueChannels(2));

    // Without a grid, a jump or a loop in beats has nothing to count.
    dj(&mut f, D::Grid(A, playr_app::dj::GridEdit::Reset));
    dj(&mut f, D::Jump(A, 4.0));
    said(&f, M::NoGrid(A));
    dj(&mut f, D::Loop(A, Some(4.0)));
    said(&f, M::NoGrid(A));
}

#[test]
fn a_lost_device_clears_the_decks_and_a_load_opens_them_again() {
    use playr_app::dj::{DjAction as D, DjMessage as M, Side::*};
    use playr_core::audio::output::DeviceEvent;
    let dj = |f: &mut Headless, d| dispatch(Action::Dj(d), f);
    let (mut f, dir) = headless();
    let a = click_track(&mut f, dir.path(), "a.wav", 120.0, 0.0);
    f.results = Some(vec![a]);
    f.cursors[0] = Some(0);
    dj(&mut f, D::Load(A));
    decks_until(&mut f, |m| matches!(m, Message::Dj(M::Loaded { .. })));
    dj(&mut f, D::Play(A));
    run_decks(&mut f, 2);

    f.dj.device_events()
        .send(DeviceEvent::Lost("unplugged".into()))
        .unwrap();
    run_decks(&mut f, 1);
    assert_eq!(
        last(&f),
        Some(&Message::Dj(M::DeviceLost("unplugged".into())))
    );
    assert!(f.dj.loaded(A).is_none());
    assert!(f.dj.status().is_none());
    dj(&mut f, D::Load(A));
    decks_until(&mut f, |m| matches!(m, Message::Dj(M::Loaded { .. })));
    assert!(f.dj.loaded(A).is_some());
}

#[test]
fn a_deck_takes_over_the_player_s_track_where_it_is() {
    use playr_app::dj::{DjAction as D, DjMessage as M, Range, Side::*};
    use playr_core::audio::State;
    let dj = |f: &mut Headless, d| dispatch(Action::Dj(d), f);
    let (mut f, dir) = headless();
    dj(&mut f, D::Take(A));
    assert_eq!(last(&f), Some(&Refusal::NothingPlaying.into()));

    // Outside the library, at -2 semitones and half volume.
    let file = dir.path().join("song.wav");
    common::tone(&file, 8000, 30.0, -6.0);
    let track = Track {
        path: file.to_string_lossy().into(),
        ..Default::default()
    };
    f.session.play(&[track], 0);
    dispatch(Action::SetSpeed(-2), &mut f);
    dispatch(Action::SetVolume(0.5), &mut f);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while f.session.player().position().as_secs_f64() < 0.5 {
        assert!(std::time::Instant::now() < deadline, "never played");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    dj(&mut f, D::Take(A));
    assert_eq!(last(&f), Some(&Message::Dj(M::Loading(A))));
    decks_until(&mut f, |m| m != &Message::Dj(M::Loading(A)));
    let player_at = f.session.player().position().as_secs_f64();
    assert_eq!(last(&f), Some(&Message::Dj(M::Took(A))));
    run_decks(&mut f, 1);
    let rate = f64::from(f.dj.rate().unwrap());
    let deck = f.dj.status().unwrap().deck(A);
    let deck_at = deck.pos() / rate;
    assert!(deck.playing());
    assert!(
        (deck_at - player_at).abs() < 0.1,
        "deck {deck_at} s, player {player_at} s"
    );
    assert_eq!(deck.range(), Range::Medium);
    assert!((deck.pct() - (2f64.powf(-2.0 / 12.0) - 1.0) * 100.0).abs() < 1e-9);
    while f.session.player().status().state != State::Paused {
        assert!(std::time::Instant::now() < deadline, "the player played on");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // The player's volume is the decks' master: a -6 dBFS tone at half. The
    // crossfader moved to deck A, so its centre takes nothing.
    assert_eq!(f.dj.state().xfade, 0.0);
    let mut out = vec![0.0; 4096 * 2];
    f.dj.process(&mut out);
    let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!((peak - 0.5 * 0.501).abs() < 0.01, "{peak}");

    // A playing deck is not taken over.
    f.session.send(playr_core::audio::Cmd::TogglePause);
    dj(&mut f, D::Take(A));
    assert_eq!(last(&f), Some(&Message::Dj(M::Playing(A))));
}

#[test]
fn a_track_queued_for_a_playing_deck_loads_when_it_stops() {
    use playr_app::dj::{DjAction as D, DjMessage as M, Side::*};
    use playr_app::tape::Pos;
    let dj = |f: &mut Headless, d| dispatch(Action::Dj(d), f);
    let said = |f: &Headless, m: M| assert_eq!(last(f), Some(&Message::Dj(m)));
    let (mut f, dir) = headless();
    // Strict: a track picked for a playing deck waits.
    dj(&mut f, D::Strict(true));
    let a = click_track(&mut f, dir.path(), "a.wav", 120.0, 0.0);
    let b = click_track(&mut f, dir.path(), "b.wav", 124.0, 0.0);
    f.results = Some(vec![a, b]);
    f.cursors[0] = Some(0);
    dj(&mut f, D::Load(A));
    decks_until(&mut f, |m| matches!(m, Message::Dj(M::Loaded { .. })));
    dj(&mut f, D::Play(A));
    run_decks(&mut f, 1);

    f.cursors[0] = Some(1);
    dj(&mut f, D::Load(A));
    said(
        &f,
        M::Queued {
            side: A,
            title: "b".into(),
        },
    );
    run_decks(&mut f, 4);
    assert_eq!(
        f.dj.loaded(A).unwrap().title,
        "a",
        "a playing deck keeps its track"
    );

    // A seek, by percentage, while it plays; then a pause lets b load.
    dj(&mut f, D::Seek(A, Pos::Percent(50.0)));
    run_decks(&mut f, 1);
    let pos = f.dj.status().unwrap().deck(A).pos();
    assert!((pos - 80_000.0).abs() < 1024.0, "{pos}");
    dj(&mut f, D::Pause(A));
    decks_until(&mut f, |m| matches!(m, Message::Dj(M::Loaded { .. })));
    assert_eq!(f.dj.loaded(A).unwrap().title, "b");
    assert!(f.dj.next(A).is_none());

    dj(&mut f, D::Mute(B, true));
    assert!(f.dj.state().mute[1]);
}

#[test]
fn not_strict_a_pick_replaces_the_playing_track_and_marks_are_jumped_to() {
    use playr_app::dj::{DjAction as D, DjMessage as M, Side::*};
    let dj = |f: &mut Headless, d| dispatch(Action::Dj(d), f);
    let said = |f: &Headless, m: M| assert_eq!(last(f), Some(&Message::Dj(m)));
    let (mut f, dir) = headless();
    let a = click_track(&mut f, dir.path(), "a.wav", 120.0, 0.0);
    let b = click_track(&mut f, dir.path(), "b.wav", 124.0, 0.0);
    // Marks at 2 s and 5 s of b, set as the sampler sets them.
    let conn = db::open(&dir.path().join("library.db")).unwrap();
    for frame in [16_000, 40_000] {
        query::add_mark(&conn, &b.path, query::Mark { frame, rate: 8000 }).unwrap();
    }
    f.results = Some(vec![a, b]);
    f.cursors[0] = Some(0);
    dj(&mut f, D::Load(A));
    decks_until(&mut f, |m| matches!(m, Message::Dj(M::Loaded { .. })));
    dj(&mut f, D::Play(A));
    run_decks(&mut f, 2);

    f.cursors[0] = Some(1);
    dj(&mut f, D::Load(A));
    said(&f, M::Loading(A));
    decks_until(&mut f, |m| matches!(m, Message::Dj(M::Loaded { .. })));
    run_decks(&mut f, 1);
    assert_eq!(f.dj.loaded(A).unwrap().title, "b");
    assert!(f.dj.status().unwrap().deck(A).playing(), "it plays on");
    assert_eq!(f.dj.loaded(A).unwrap().marks, [16_000.0, 40_000.0]);

    dj(&mut f, D::Mark(A, true));
    run_decks(&mut f, 1);
    let pos = f.dj.status().unwrap().deck(A).pos();
    assert!((16_000.0..16_600.0).contains(&pos), "{pos}");
    dj(&mut f, D::Mark(A, true));
    run_decks(&mut f, 1);
    let pos = f.dj.status().unwrap().deck(A).pos();
    assert!((40_000.0..40_600.0).contains(&pos), "{pos}");
    dj(&mut f, D::Mark(A, true));
    said(&f, M::NoMark(A));
    // Within half a second of a mark, previous goes to the one before.
    dj(&mut f, D::Mark(A, false));
    run_decks(&mut f, 1);
    let pos = f.dj.status().unwrap().deck(A).pos();
    assert!((16_000.0..16_600.0).contains(&pos), "{pos}");
}
