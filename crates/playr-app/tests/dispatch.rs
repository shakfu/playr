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
    cursors: [Option<usize>; 3],
    results: Option<Vec<Track>>,
    messages: Vec<Message>,
    asked: Option<Confirm>,
    prompts: Vec<Prompt>,
    shown: Vec<Presentation>,
    plan: Option<Plan>,
    planning: bool,
    sampler: playr_app::sampler::Sampler,
}

fn slot(view: View) -> Option<usize> {
    match view {
        Library => Some(0),
        Selection => Some(1),
        Playlists => Some(2),
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
        cursors: [Some(0), None, Some(0)],
        results: None,
        messages: Vec::new(),
        asked: None,
        prompts: Vec::new(),
        shown: Vec::new(),
        plan: None,
        planning: false,
        sampler: Default::default(),
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
    assert_eq!(f.asked, Some(Confirm::ReplacePlaylist("late".into())));
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
    assert_eq!(f.cursors, [Some(0), None, Some(0)]);
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
    assert_eq!(f.prompts, [Prompt::Search, Prompt::Command]);
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
