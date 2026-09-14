//! Events from the engine and from a session's background work, received
//! through a sink as a frontend would.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use playr_core::audio::{Cmd, State};
use playr_core::db::{self, Track};
use playr_core::event::{Event, EventSink};
use playr_core::samples::Cut;
use playr_core::session::Session;
use playr_core::wave::Peaks;

/// A sink that sends to a channel, and the channel's receiving end.
fn channel() -> (EventSink, Receiver<Event>) {
    let (send, receive) = mpsc::channel();
    (
        Arc::new(move |event| {
            let _ = send.send(event);
        }),
        receive,
    )
}

/// Events received until `done` returns true for one, or five seconds pass.
fn until(events: &Receiver<Event>, done: impl Fn(&Event) -> bool) -> Vec<Event> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seen = Vec::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match events.recv_timeout(left) {
            Ok(event) => {
                let finished = done(&event);
                seen.push(event);
                if finished {
                    return seen;
                }
            }
            Err(_) => break,
        }
    }
    panic!("the event never came; saw {seen:?}");
}

fn track(path: &Path) -> Track {
    Track {
        path: path.to_string_lossy().into_owned(),
        ..Default::default()
    }
}

#[test]
fn the_engine_reports_errors_track_changes_and_state() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.wav");
    common::silence(&good, 8000, 5.0);
    let missing = dir.path().join("missing.wav");
    let (sink, events) = channel();
    let player = common::fake_player().0;
    player.set_events(sink);

    player.send(Cmd::Play(vec![missing.clone(), good.clone()], 0));
    let seen = until(&events, |e| {
        matches!(e, Event::StateChanged(State::Playing))
    });
    assert!(
        seen.iter()
            .any(|e| matches!(e, Event::PlaybackError(m) if m.contains("missing.wav"))),
        "{seen:?}"
    );
    assert!(
        seen.iter().any(|e| matches!(
            e,
            Event::TrackChanged { index: 1, path: Some(p) } if *p == good
        )),
        "{seen:?}"
    );

    player.send(Cmd::Stop);
    until(&events, |e| {
        matches!(e, Event::StateChanged(State::Stopped))
    });
}

/// A session playing 10 s of silence, its event channel, and the file.
fn playing(dir: &Path) -> (Session, Receiver<Event>, PathBuf) {
    let file = dir.join("long.wav");
    common::silence(&file, 8000, 10.0);
    let (sink, events) = channel();
    let conn = db::open(&dir.join("library.db")).unwrap();
    let mut session = Session::new(conn, common::fake_player().0, sink);
    session.set_samples_dir(dir.join("samples"));
    session.play(&[track(&file)], 0);
    until(&events, |e| {
        matches!(e, Event::StateChanged(State::Playing))
    });
    (session, events, file)
}

#[test]
fn background_work_reports_once_under_its_job() {
    let dir = tempfile::tempdir().unwrap();
    let (mut session, events, file) = playing(dir.path());

    let peaks = session.read_peaks(file.clone());
    let seen = until(&events, |e| matches!(e, Event::Peaks { .. }));
    match seen.last().unwrap() {
        Event::Peaks { job, track, result } => {
            assert_eq!((*job, track), (peaks, &file));
            assert_eq!(result.as_ref().unwrap().frames, 80_000);
        }
        _ => unreachable!(),
    }

    session.add_mark(Some(Duration::from_secs(4)));
    let planning = session.plan_slices(Cut::Marks).unwrap();
    assert_ne!(planning, peaks, "two jobs shared an id");
    let seen = until(&events, |e| matches!(e, Event::Planned { .. }));
    let plan = match seen.last().unwrap() {
        Event::Planned { job, track, result } => {
            assert_eq!((*job, track), (planning, &file));
            result.clone().unwrap()
        }
        _ => unreachable!(),
    };
    assert_eq!(plan.spans, [(0, Some(32_000)), (32_000, None)]);
    assert!(!dir.path().join("samples").exists(), "planning wrote files");

    let writing = session.write_slices(plan);
    let seen = until(&events, |e| matches!(e, Event::Exported { .. }));
    match seen.last().unwrap() {
        Event::Exported { job, result } => {
            assert_eq!(*job, writing);
            assert_eq!(
                result.as_ref().unwrap().slices,
                [(0, 32_000), (32_000, 80_000)]
            );
        }
        _ => unreachable!(),
    }

    let exporting = session.export(Cut::Region).unwrap();
    let seen = until(&events, |e| matches!(e, Event::Exported { .. }));
    assert!(
        matches!(seen.last(), Some(Event::Exported { job, result: Ok(_) }) if *job == exporting)
    );
}

#[test]
fn a_peaks_read_that_fails_reports_the_error() {
    let dir = tempfile::tempdir().unwrap();
    let (sink, events) = channel();
    let mut session = Session::new(db::open_memory().unwrap(), common::fake_player().0, sink);
    let job = session.read_peaks(dir.path().join("gone.wav"));
    let seen = until(&events, |e| matches!(e, Event::Peaks { .. }));
    assert!(
        matches!(seen.last(), Some(Event::Peaks { job: j, result: Err(_), .. }) if *j == job),
        "{seen:?}"
    );
}

#[test]
fn a_superseded_peaks_read_sends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    // Long enough that the first read is still decoding when it is replaced.
    let long = dir.path().join("long.wav");
    common::silence(&long, 48_000, 60.0);
    let short = dir.path().join("short.wav");
    common::silence(&short, 8000, 1.0);
    // How long the long file takes to read in full, so the test can wait for
    // a read that was not stopped to have finished.
    let started = Instant::now();
    Peaks::read(&long, &AtomicBool::new(false)).unwrap();
    let full_read = started.elapsed();

    let (sink, events) = channel();
    let mut session = Session::new(db::open_memory().unwrap(), common::fake_player().0, sink);

    let first = session.read_peaks(long);
    let second = session.read_peaks(short.clone());
    let seen = until(
        &events,
        |e| matches!(e, Event::Peaks { job, .. } if *job == second),
    );
    assert!(
        !seen
            .iter()
            .any(|e| matches!(e, Event::Peaks { job, .. } if *job == first)),
        "{seen:?}"
    );
    // Nor later: give the first read twice the time it needs, had it not stopped.
    std::thread::sleep(full_read * 2 + Duration::from_millis(200));
    assert!(
        events
            .try_iter()
            .all(|e| !matches!(e, Event::Peaks { job, .. } if job == first)),
        "the cancelled read reported"
    );
}
