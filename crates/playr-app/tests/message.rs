//! The words every frontend shows for each message.
//!
//! Tests elsewhere assert on messages as data; this is the one place their
//! wording is checked, so changing a phrase changes one line here.

use std::time::Duration;

use playr_app::action::Key;
use playr_app::command::parse;
use playr_app::message::{fmt_time, home_as_tilde, text, Message};
use playr_app::{Display, View};
use playr_core::audio::Mode;
use playr_core::notice::{Notice, Outcome, Refusal, Task};

fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

#[test]
fn outcomes_are_worded() {
    let home = std::env::home_dir().unwrap();
    for (outcome, words) in [
        (Outcome::AddedToSelection, "added to selection"),
        (Outcome::AlreadyInSelection, "already in selection"),
        (Outcome::RemovedFromSelection, "removed from selection"),
        (
            Outcome::RemovedTrack {
                title: "So What".into(),
            },
            "removed \"So What\"",
        ),
        (Outcome::SelectionCleared, "selection cleared"),
        (
            Outcome::Saved {
                name: "late".into(),
                tracks: 3,
                left_out: 0,
            },
            "saved \"late\" (3 tracks)",
        ),
        (
            Outcome::Saved {
                name: "mix".into(),
                tracks: 1,
                left_out: 1,
            },
            "saved \"mix\" (1 tracks, 1 not in the library left out)",
        ),
        (
            Outcome::Renamed {
                from: "late".into(),
                to: "night".into(),
            },
            "renamed \"late\" to \"night\"",
        ),
        (
            Outcome::Deleted {
                name: "late".into(),
            },
            "deleted \"late\"",
        ),
        (
            Outcome::PlayingPlaylist {
                name: "dusk".into(),
            },
            "playing \"dusk\"",
        ),
        (Outcome::Mode(Mode::RepeatOne), "mode: repeat one"),
        (
            Outcome::Marked {
                at: secs(30),
                kept: true,
            },
            "marked 0:30",
        ),
        (
            Outcome::Marked {
                at: secs(65),
                kept: false,
            },
            "marked 1:05 (not kept: no library file)",
        ),
        (Outcome::MarkRemoved { at: secs(5) }, "removed mark at 0:05"),
        (Outcome::MarksCleared, "marks cleared"),
        (Outcome::AtMark { at: secs(90) }, "mark at 1:30"),
        (Outcome::PlanStarted, "planning slices"),
        (
            Outcome::Planned { slices: 1 },
            "1 slice planned: enter writes, esc discards",
        ),
        (
            Outcome::Planned { slices: 4 },
            "4 slices planned: enter writes, esc discards",
        ),
        (Outcome::ExportStarted, "exporting"),
        (
            Outcome::Exported {
                dir: home.join("Music/playr/samples/amen"),
                slices: 1,
            },
            "exported 1 slice to ~/Music/playr/samples/amen",
        ),
        (
            Outcome::Exported {
                dir: "/tmp/cuts".into(),
                slices: 8,
            },
            "exported 8 slices to /tmp/cuts",
        ),
    ] {
        assert_eq!(text(&outcome.into()), words);
    }
}

#[test]
fn scans_and_opened_files_are_worded() {
    use playr_core::scan::{ScanReport, ScanStats};
    let home = std::env::home_dir().unwrap();
    let report = ScanReport {
        stats: ScanStats {
            seen: 1200,
            added: 300,
            skipped: 899,
            failed: 1,
        },
        missing: 2,
        total: 5000,
    };
    for (outcome, words) in [
        (
            Outcome::ScanStarted {
                dir: home.join("music"),
            },
            "scanning ~/music",
        ),
        (
            Outcome::Scanning {
                seen: 1200,
                added: 300,
            },
            "scanning: 1200 files, 300 added",
        ),
        (
            Outcome::Scanned {
                dir: home.join("music"),
                report,
            },
            "scanned ~/music: 300 added, 1 unreadable, 2 missing (:prune removes); 5000 tracks",
        ),
        (
            Outcome::Scanned {
                dir: home.join("music"),
                report: ScanReport {
                    missing: 0,
                    ..report
                },
            },
            "scanned ~/music: 300 added, 1 unreadable; 5000 tracks",
        ),
        (
            Outcome::PruneStarted {
                dir: home.join("music"),
            },
            "pruning ~/music",
        ),
        (
            Outcome::Pruned {
                dir: home.join("music"),
                removed: playr_core::db::Pruned {
                    tracks: 1,
                    marks: 3,
                },
            },
            "pruned ~/music: 1 track and 3 marks of missing files",
        ),
        (Outcome::Opening, "opening"),
        (
            Outcome::Opened {
                tracks: 1,
                skipped: 0,
            },
            "playing 1 track",
        ),
        (
            Outcome::Opened {
                tracks: 12,
                skipped: 2,
            },
            "playing 12 tracks; 2 skipped",
        ),
    ] {
        assert_eq!(text(&outcome.into()), words);
    }
}

#[test]
fn refusals_are_worded() {
    for (refusal, words) in [
        (Refusal::NothingPlaying, "nothing is playing"),
        (Refusal::SelectionEmpty, "selection is empty"),
        (
            Refusal::NoLibraryFile,
            "no library to save to; `playr scan <dir>` creates one",
        ),
        (Refusal::NameEmpty, "playlist name cannot be empty"),
        (Refusal::NameUnchanged, "name unchanged"),
        (
            Refusal::NameTaken("early".into()),
            "a playlist named \"early\" already exists",
        ),
        (
            Refusal::NoPlaylistNamed("nope".into()),
            "no single playlist named \"nope\"",
        ),
        (Refusal::PlaylistEmpty, "playlist is empty"),
        (
            Refusal::AlreadyMarked { at: secs(0) },
            "already marked at 0:00",
        ),
        (Refusal::NoMarks, "no marks in this track"),
        (Refusal::NoLaterMark, "no later mark"),
        (Refusal::NoEarlierMark, "no earlier mark"),
        (Refusal::NoLibraryPath, "no library file to scan into"),
        (
            Refusal::NotADirectory("/opt/nothing".into()),
            "not a directory: /opt/nothing",
        ),
        (Refusal::ScanRunning, "a scan or prune is already running"),
    ] {
        assert_eq!(text(&refusal.into()), words);
    }
}

#[test]
fn failures_and_playback_errors_are_worded() {
    let failed = |task| {
        text(&Message::Core(Notice::Failed {
            task,
            error: "disk full".into(),
        }))
    };
    assert_eq!(failed(Task::Save), "could not save: disk full");
    assert_eq!(failed(Task::Rename), "could not rename: disk full");
    assert_eq!(failed(Task::Mark), "could not mark: disk full");
    assert_eq!(failed(Task::RemoveMark), "could not remove mark: disk full");
    assert_eq!(failed(Task::ClearMarks), "could not clear marks: disk full");
    assert_eq!(failed(Task::Slice), "slicing failed: disk full");
    assert_eq!(failed(Task::Export), "export failed: disk full");
    assert_eq!(failed(Task::Scan), "scan failed: disk full");
    assert_eq!(failed(Task::Prune), "prune failed: disk full");
    assert_eq!(failed(Task::Open), "could not open: disk full");

    let playback = |missed| {
        text(&Message::Core(Notice::PlaybackError {
            error: "cannot play b.flac".into(),
            missed,
        }))
    };
    assert_eq!(playback(0), "cannot play b.flac");
    assert_eq!(playback(2), "cannot play b.flac (and 2 more)");
}

#[test]
fn terminal_messages_are_worded() {
    let d = Key::parse("d").unwrap();
    for (message, words) in [
        (Message::Cancelled, "cancelled"),
        (Message::NoMatches, "no matches"),
        (
            Message::NoPlaylistUnderCursor,
            "no playlist under the cursor in the playlists view",
        ),
        (Message::Display(Display::Decibels), "display: db"),
        (
            Message::Mapped(parse("map selection ctrl-x clear", View::Library).unwrap()),
            "map selection ctrl-x clear",
        ),
        (Message::Unmapped(d), "unmapped d"),
        (
            Message::NotBound {
                key: d,
                view: Some(View::Selection),
            },
            "d has no binding in the selection view",
        ),
        (
            Message::NotBound { key: d, view: None },
            "d has no binding for all views",
        ),
        (
            Message::NoSlicesPlanned,
            "no slices planned; :slice plans them",
        ),
        (Message::SlicesDiscarded, "slices discarded"),
        (
            Message::Command("not a time: soon (try 1:23 or 90)".into()),
            "not a time: soon (try 1:23 or 90)",
        ),
    ] {
        assert_eq!(text(&message), words);
    }
}

#[test]
fn paths_under_home_are_shown_from_tilde() {
    use std::path::Path;
    let home = std::env::home_dir().unwrap();
    assert_eq!(
        home_as_tilde(&home.join("Music/playr/samples/amen")),
        "~/Music/playr/samples/amen"
    );
    assert_eq!(home_as_tilde(Path::new("/opt/cuts")), "/opt/cuts");
}

#[test]
fn times_are_minutes_and_seconds_then_hours() {
    assert_eq!(fmt_time(Duration::from_millis(59_999)), "0:59");
    assert_eq!(fmt_time(secs(754)), "12:34");
    assert_eq!(fmt_time(secs(3723)), "1:02:03");
}
