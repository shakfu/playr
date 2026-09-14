//! The words the terminal shows on its bottom line for each message.
//!
//! Core operations report a [`Notice`]; the rest are about the terminal
//! itself: prompts, key bindings and views. [`text`] is the only place either
//! is turned into words, so tests assert on messages and one test checks the
//! wording of each.

use playr_core::notice::{Notice, Outcome, Refusal, Task};

use playr_app::command;
pub use playr_app::message::Message;

use super::fmt_time;
use super::home_as_tilde;

/// `n` slices, as a count with its noun.
fn slices(n: usize) -> String {
    match n {
        1 => "1 slice".into(),
        n => format!("{n} slices"),
    }
}

/// The words for `message`.
pub fn text(message: &Message) -> String {
    match message {
        Message::Core(Notice::Done(outcome)) => outcome_text(outcome),
        Message::Core(Notice::Refused(refusal)) => refusal_text(refusal),
        Message::Core(Notice::Failed { task, error }) => {
            let what = match task {
                Task::Save => "could not save",
                Task::Rename => "could not rename",
                Task::Mark => "could not mark",
                Task::RemoveMark => "could not remove mark",
                Task::ClearMarks => "could not clear marks",
                Task::Slice => "slicing failed",
                Task::Export => "export failed",
            };
            format!("{what}: {error}")
        }
        // Only the latest error is kept, so a run of bad files would
        // otherwise show one name and hide the rest.
        Message::Core(Notice::PlaybackError { error, missed: 0 }) => error.clone(),
        Message::Core(Notice::PlaybackError { error, missed }) => {
            format!("{error} (and {missed} more)")
        }
        Message::Cancelled => "cancelled".into(),
        Message::NoMatches => "no matches".into(),
        Message::NoPlaylistUnderCursor => {
            "no playlist under the cursor in the playlists view".into()
        }
        Message::Display(display) => format!("display: {}", display.name()),
        Message::Mapped(map) => command::line(map, None),
        Message::Unmapped(key) => format!("unmapped {key}"),
        Message::NotBound { key, view } => {
            format!("{key} has no binding {}", command::scope(*view))
        }
        Message::NoSlicesPlanned => "no slices planned; :slice plans them".into(),
        Message::SlicesDiscarded => "slices discarded".into(),
        Message::Command(error) => error.clone(),
    }
}

fn outcome_text(outcome: &Outcome) -> String {
    match outcome {
        Outcome::AddedToSelection => "added to selection".into(),
        Outcome::AlreadyInSelection => "already in selection".into(),
        Outcome::RemovedFromSelection => "removed from selection".into(),
        Outcome::RemovedTrack { title } => format!("removed \"{title}\""),
        Outcome::SelectionCleared => "selection cleared".into(),
        Outcome::Saved {
            name,
            tracks,
            left_out,
        } => {
            let note = match left_out {
                0 => String::new(),
                n => format!(", {n} not in the library left out"),
            };
            format!("saved \"{name}\" ({tracks} tracks{note})")
        }
        Outcome::Renamed { from, to } => format!("renamed \"{from}\" to \"{to}\""),
        Outcome::Deleted { name } => format!("deleted \"{name}\""),
        Outcome::PlayingPlaylist { name } => format!("playing \"{name}\""),
        Outcome::Mode(mode) => format!("mode: {}", mode.name()),
        Outcome::Marked { at, kept } => {
            // As with playlists: in memory, the mark is gone when playr exits.
            let kept = if *kept {
                ""
            } else {
                " (not kept: no library file)"
            };
            format!("marked {}{kept}", fmt_time(*at))
        }
        Outcome::MarkRemoved { at } => format!("removed mark at {}", fmt_time(*at)),
        Outcome::MarksCleared => "marks cleared".into(),
        Outcome::AtMark { at } => format!("mark at {}", fmt_time(*at)),
        Outcome::PlanStarted => "planning slices".into(),
        Outcome::Planned { slices: n } => {
            format!("{} planned: enter writes, esc discards", slices(*n))
        }
        Outcome::ExportStarted => "exporting".into(),
        Outcome::Exported { dir, slices: n } => {
            format!("exported {} to {}", slices(*n), home_as_tilde(dir))
        }
    }
}

fn refusal_text(refusal: &Refusal) -> String {
    match refusal {
        Refusal::NothingPlaying => "nothing is playing".into(),
        Refusal::SelectionEmpty => "selection is empty".into(),
        Refusal::NoLibraryFile => "no library to save to; `playr scan <dir>` creates one".into(),
        Refusal::NameEmpty => "playlist name cannot be empty".into(),
        Refusal::NameUnchanged => "name unchanged".into(),
        Refusal::NameTaken(name) => format!("a playlist named \"{name}\" already exists"),
        Refusal::WouldReplace(name) => format!("a playlist named \"{name}\" would be replaced"),
        Refusal::NoPlaylistNamed(name) => format!("no single playlist named \"{name}\""),
        Refusal::PlaylistEmpty => "playlist is empty".into(),
        Refusal::AlreadyMarked { at } => format!("already marked at {}", fmt_time(*at)),
        Refusal::NoMarks => "no marks in this track".into(),
        Refusal::NoLaterMark => "no later mark".into(),
        Refusal::NoEarlierMark => "no earlier mark".into(),
    }
}
