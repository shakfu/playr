//! The words the terminal shows for each message.
//!
//! Tests elsewhere assert on messages as data; this is the one place their
//! wording is checked, so changing a phrase changes one line here.

use std::time::Duration;

use playr::ui::notice::{text, Message};
use playr::ui::sampler::Display;
use playr::ui::View;
use playr_app::action::Key;
use playr_app::command::parse;
use playr_core::audio::Mode;
use playr_core::notice::{Notice, Outcome, Refusal, Task};

fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

#[test]
fn outcomes_are_worded() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
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
