//! The owner thread, driven through its channel as the web page and OSC will.

#[path = "../../playr-core/tests/common/mod.rs"]
mod common;

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use playr_app::action::Action;
use playr_app::config::Config;
use playr_app::dispatch::Frontend;
use playr_app::model::Input;
use playr_app::model::Model;
use playr_app::View;
use playr_core::db::{self, query, Track};
use playr_server::owner::{self, seek_target, Query, Request, END_MARGIN};

fn model() -> Model {
    let conn = db::open_memory().unwrap();
    Model::new(conn, common::fake_player().0, Vec::new(), Config::default())
}

/// Waits up to 2 s for `handle` to finish, and returns its model.
fn join(handle: thread::JoinHandle<Model>) -> Model {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !handle.is_finished() {
        assert!(Instant::now() < deadline, "the owner did not stop");
        thread::sleep(Duration::from_millis(5));
    }
    handle.join().unwrap()
}

#[test]
fn requests_run_in_order_and_each_is_published() {
    let (send, received) = mpsc::channel();
    send.send(Request::Perform(Action::ShowView(View::Playlists)))
        .unwrap();
    send.send(Request::Perform(Action::ShowView(View::Selection)))
        .unwrap();
    drop(send);
    let mut views = Vec::new();
    let model = owner::run(model(), received, |m| views.push(m.view()));
    assert_eq!(views, [View::Playlists, View::Selection]);
    assert_eq!(model.view(), View::Selection);
}

#[test]
fn it_stops_when_every_sender_is_gone() {
    let (send, received) = mpsc::channel::<Request>();
    let handle = thread::spawn(move || owner::run(model(), received, |_| {}));
    drop(send);
    join(handle);
}

#[test]
fn it_stops_on_quit_while_a_sender_remains() {
    let (send, received) = mpsc::channel();
    let handle = thread::spawn(move || owner::run(model(), received, |_| {}));
    send.send(Request::Perform(Action::Quit)).unwrap();
    join(handle);
    // The owner dropped its receiver.
    assert!(send.send(Request::Perform(Action::TogglePause)).is_err());
}

#[test]
fn it_refreshes_with_no_requests() {
    let (send, received) = mpsc::channel::<Request>();
    let handle = thread::spawn(move || {
        let mut frames = 0;
        let model = owner::run(model(), received, |_| frames += 1);
        (model, frames)
    });
    thread::sleep(owner::IDLE * 3);
    drop(send);
    let (_, frames) = handle.join().unwrap();
    assert!(frames >= 2, "published {frames} times with no requests");
}

/// A model over a library file of three tracks, two by Evans, and playlists
/// "late", of all three, and "early", of the second, made in that order.
fn library() -> (Model, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = db::open(&dir.path().join("library.db")).unwrap();
    let ids: Vec<i64> = [
        ("/m/a.flac", "Evans"),
        ("/m/b.flac", "Monk"),
        ("/m/c.flac", "Evans"),
    ]
    .iter()
    .map(|(path, artist)| {
        let track = Track {
            path: path.to_string(),
            artist: Some(artist.to_string()),
            mtime: 1,
            size: 1,
            ..Default::default()
        };
        db::upsert(&conn, &track).unwrap()
    })
    .collect();
    query::save_playlist(&mut conn, "late", &ids).unwrap();
    query::save_playlist(&mut conn, "early", &ids[1..2]).unwrap();
    let player = common::fake_player().0;
    (Model::new(conn, player, Vec::new(), Config::default()), dir)
}

/// Runs `requests` on `model` until they are done, and returns it.
fn run(model: Model, requests: Vec<Request>) -> Model {
    let (send, received) = mpsc::channel();
    for request in requests {
        send.send(request).unwrap();
    }
    drop(send);
    owner::run(model, received, |_| {})
}

/// A request with a reply, and where the reply arrives.
fn replied<T>(make: impl FnOnce(mpsc::Sender<T>) -> Request) -> (Request, mpsc::Receiver<T>) {
    let (reply, answer) = mpsc::channel();
    (make(reply), answer)
}

#[test]
fn a_command_runs_in_the_view_shown_and_a_refusal_is_answered() {
    let (model, _dir) = library();
    let (to_playlists, first) = replied(|reply| Request::Command {
        line: "view playlists".into(),
        reply,
    });
    // `rename` works only in the playlists view, where the first put it.
    let (rename, second) = replied(|reply| Request::Command {
        line: "rename".into(),
        reply,
    });
    let (quit, third) = replied(|reply| Request::Command {
        line: "quit".into(),
        reply,
    });
    let (sneeze, fourth) = replied(|reply| Request::Command {
        line: "sneeze".into(),
        reply,
    });
    let model = run(model, vec![to_playlists, rename, quit, sneeze]);
    assert_eq!(first.recv().unwrap(), Ok(()));
    assert_eq!(second.recv().unwrap(), Ok(()));
    assert!(matches!(model.input(), Input::RenamePlaylist { .. }));
    assert_eq!(third.recv().unwrap().unwrap_err().0, 403);
    assert!(!model.quitting());
    assert_eq!(fourth.recv().unwrap().unwrap_err().0, 400);
}

#[test]
fn a_key_does_what_it_is_bound_to_unless_the_page_may_not() {
    let (model, _dir) = library();
    let (down, first) = replied(|reply| Request::Key {
        name: "j".into(),
        reply,
    });
    let (quit, second) = replied(|reply| Request::Key {
        name: "q".into(),
        reply,
    });
    let (unbound, third) = replied(|reply| Request::Key {
        name: "ctrl-y".into(),
        reply,
    });
    let (sampler, fourth) = replied(|reply| Request::Key {
        name: "4".into(),
        reply,
    });
    let model = run(model, vec![down, quit, unbound, sampler]);
    assert_eq!(first.recv().unwrap(), Ok(()));
    assert_eq!(model.cursors().library, Some(1));
    assert_eq!(second.recv().unwrap().unwrap_err().0, 403);
    assert!(!model.quitting());
    assert_eq!(third.recv().unwrap(), Ok(()));
    assert_eq!(fourth.recv().unwrap().unwrap_err().0, 403);
    assert_eq!(model.view(), View::Library);
}

#[test]
fn a_row_takes_the_cursor_and_its_command_if_it_is_still_that_row() {
    let (model, _dir) = library();
    let (play, first) = replied(|reply| Request::Row {
        view: View::Library,
        row: 1,
        key: "/m/c.flac".into(),
        command: Some("play".into()),
        reply,
    });
    let (moved, second) = replied(|reply| Request::Row {
        view: View::Library,
        row: 2,
        key: "/m/a.flac".into(),
        command: Some("toggle".into()),
        reply,
    });
    let (refused, third) = replied(|reply| Request::Row {
        view: View::Library,
        row: 0,
        key: "/m/a.flac".into(),
        command: Some("quit".into()),
        reply,
    });
    let model = run(model, vec![play]);
    assert_eq!(first.recv().unwrap(), Ok(()));
    assert_eq!(model.cursors().library, Some(1));
    // Library order is by artist: Evans, Evans, Monk.
    assert_eq!(queue(&model), ["/m/a.flac", "/m/c.flac", "/m/b.flac"]);
    let model = run(model, vec![moved, refused]);
    assert_eq!(second.recv().unwrap().unwrap_err().0, 409);
    assert!(model.session().selection().is_empty());
    assert_eq!(third.recv().unwrap().unwrap_err().0, 403);
    assert_eq!(model.cursors().library, Some(0));
}

#[test]
fn a_search_filters_as_typed_and_closes_when_done() {
    let (model, _dir) = library();
    let typed = Request::Search {
        query: "evans".into(),
        done: false,
    };
    let model = run(model, vec![typed]);
    assert_eq!(model.listed().len(), 2);
    assert!(matches!(model.input(), Input::Search(_)));
    let done = Request::Search {
        query: "evans".into(),
        done: true,
    };
    let model = run(model, vec![done]);
    assert_eq!((model.listed().len(), model.input()), (2, &Input::None));
    let cleared = Request::Search {
        query: String::new(),
        done: true,
    };
    let model = run(model, vec![cleared]);
    assert_eq!(model.listed().len(), 3);
    assert!(model.results().is_none());
}

#[test]
fn a_name_saves_or_renames_and_a_question_is_answered() {
    let (model, _dir) = library();
    let model = run(
        model,
        vec![
            Request::Perform(Action::ShowView(View::Library)),
            Request::Perform(Action::Add),
            Request::Perform(Action::StartSave),
            Request::Name("third".into()),
        ],
    );
    let names: Vec<&str> = model
        .session()
        .playlists()
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["early", "late", "third"]);
    let model = run(
        model,
        vec![
            Request::Perform(Action::ShowView(View::Playlists)),
            Request::Perform(Action::DeletePlaylist),
        ],
    );
    assert!(matches!(model.input(), Input::Confirm(_)));
    let model = run(model, vec![Request::Answer(true)]);
    assert_eq!(model.session().playlists().len(), 2);
    let model = run(model, vec![Request::Perform(Action::Help), Request::Close]);
    assert_eq!(model.input(), &Input::None);
}

#[test]
fn reads_answer_from_the_model() {
    let (model, _dir) = library();
    let (rows, answer) = replied(|reply| Request::Read {
        query: Query::Rows {
            view: View::Library,
            start: 1,
            count: 5,
        },
        reply,
    });
    run(model, vec![rows]);
    let rows = answer.recv().unwrap();
    assert_eq!(rows["total"], 3);
    assert_eq!(rows["rows"].as_array().unwrap().len(), 2);
    assert_eq!(rows["rows"][0]["key"], "/m/c.flac");
}

/// The paths the player's queue holds.
fn queue(model: &Model) -> Vec<String> {
    let status = model.session().player().status();
    status
        .queue
        .iter()
        .map(|p| p.display().to_string())
        .collect()
}

#[test]
fn a_playlist_index_counts_from_the_oldest() {
    let (mut model, _dir) = library();
    for (index, expected) in [
        (0, vec!["/m/a.flac", "/m/b.flac", "/m/c.flac"]),
        (1, vec!["/m/b.flac"]),
    ] {
        let (send, received) = mpsc::channel();
        send.send(Request::PlayPlaylistAt(index)).unwrap();
        drop(send);
        model = owner::run(model, received, |_| {});
        assert_eq!(queue(&model), expected, "index {index}");
    }
    let (send, received) = mpsc::channel();
    send.send(Request::PlayPlaylistAt(2)).unwrap();
    drop(send);
    model = owner::run(model, received, |_| {});
    assert_eq!(queue(&model), ["/m/b.flac"]);
}

#[test]
fn a_seek_stops_short_of_the_end() {
    let duration = Duration::from_secs(10);
    assert_eq!(seek_target(duration, 0.25), Duration::from_millis(2500));
    assert_eq!(seek_target(duration, 1.0), duration - END_MARGIN);
    assert_eq!(seek_target(duration, 7.0), duration - END_MARGIN);
    assert_eq!(seek_target(duration, -1.0), Duration::ZERO);
    assert_eq!(seek_target(Duration::from_millis(50), 0.5), Duration::ZERO);
}

#[test]
fn queued_seeks_run_once_at_the_last_position() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("tone.wav");
    common::tone(&wav, 44_100, 20.0, -20.0);
    let track = Track {
        path: wav.display().to_string(),
        ..Default::default()
    };
    let conn = db::open_memory().unwrap();
    let model = Model::new(
        conn,
        common::fake_player().0,
        vec![track],
        Config::default(),
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while model.session().player().status().duration.is_none() {
        assert!(Instant::now() < deadline, "the track did not start");
        thread::sleep(Duration::from_millis(5));
    }

    let (send, received) = mpsc::channel();
    for fraction in [0.1, 0.9, 0.5] {
        send.send(Request::Seek(fraction)).unwrap();
    }
    send.send(Request::Perform(Action::ShowView(View::Playlists)))
        .unwrap();
    drop(send);
    let mut published = 0;
    let model = owner::run(model, received, |_| published += 1);
    // One pass for the three seeks, one for the request after them.
    assert_eq!(published, 2);
    assert_eq!(model.view(), View::Playlists);
    let deadline = Instant::now() + Duration::from_secs(2);
    let at = loop {
        let at = model.session().player().position().as_secs_f64();
        if at >= 10.0 || Instant::now() > deadline {
            break at;
        }
        thread::sleep(Duration::from_millis(5));
    };
    assert!((10.0..11.0).contains(&at), "at {at} s");
}
