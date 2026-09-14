//! Doing an action: the part of every key and `:` command a frontend shares.
//!
//! A frontend implements [`Frontend`] over its own state: which view shows,
//! where each view's cursor is, what the library view lists, and how it
//! prompts and asks. [`dispatch`] then does any [`Action`] the same way in
//! every frontend, calling the session for what changes the library or
//! playback and the frontend for what changes only the interface.

use playr_core::audio::Cmd;
use playr_core::db::query::Playlist;
use playr_core::db::Track;
use playr_core::notice::{Notice, Outcome, Refusal};
use playr_core::samples::{Cut, Plan};
use playr_core::session::Session;

use crate::action::{Action, Keymap, Slicing, Zoom};
use crate::message::Message;
use crate::{Display, View};

/// A destructive action held until the listener confirms it.
#[derive(Debug, Clone, PartialEq)]
pub enum Confirm {
    DeletePlaylist(Playlist),
    /// Overwrite the playlist of this name with the selection.
    ReplacePlaylist(String),
    /// Empty the selection, which holds this many tracks.
    ClearSelection(usize),
    /// Remove this many marks from the playing track.
    ClearMarks(usize),
}

impl Confirm {
    /// The question asked before the action, without how to answer it.
    pub fn question(&self) -> String {
        match self {
            Confirm::DeletePlaylist(p) => format!("delete playlist \"{}\"?", p.name),
            Confirm::ReplacePlaylist(name) => {
                format!("replace playlist \"{name}\" with the selection?")
            }
            Confirm::ClearSelection(n) => format!("clear all {n} tracks from the selection?"),
            Confirm::ClearMarks(n) => format!("clear all {n} marks from this track?"),
        }
    }
}

/// Text a frontend collects before an action can happen.
#[derive(Debug, Clone, PartialEq)]
pub enum Prompt {
    /// A search, shown as it is typed through [`search`].
    Search,
    /// A `:` command.
    Command,
    /// A name to save the selection as, then [`save_as`].
    Save,
    /// A new name for this playlist, then [`rename`].
    Rename(Playlist),
}

/// A change only the interface shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Presentation {
    Quit,
    /// The keys bound in the current view.
    KeyList,
    /// Every `:` command.
    CommandList,
    Zoom(Zoom),
    /// Draw the waveform this way, or the next way when `None`.
    Display(Option<Display>),
}

/// What [`dispatch`] needs from a frontend.
pub trait Frontend {
    /// The session, to read.
    fn session(&self) -> &Session;
    /// The session, to change.
    fn session_mut(&mut self) -> &mut Session;
    fn keys(&mut self) -> &mut Keymap;

    fn view(&self) -> View;
    fn set_view(&mut self, view: View);

    /// The row under `view`'s cursor, if it has rows and one is chosen.
    fn cursor(&self, view: View) -> Option<usize>;
    fn set_cursor(&mut self, view: View, row: Option<usize>);

    /// What the library view lists: the library, or search results.
    fn listed(&self) -> &[Track];
    /// Shows search results, or the whole library for `None`. Returns the
    /// results shown before.
    fn set_results(&mut self, results: Option<Vec<Track>>) -> Option<Vec<Track>>;

    /// The onset sensitivity `:slice onsets` uses when given none.
    fn onset_sensitivity(&self) -> f32;

    fn notify(&mut self, message: Message);
    /// Asks before `question`; on yes, the frontend calls [`confirmed`].
    fn confirm(&mut self, question: Confirm);
    fn prompt(&mut self, prompt: Prompt);
    fn present(&mut self, presentation: Presentation);

    /// Slices of the playing track are being planned.
    fn planning(&mut self);
    /// Takes the planned slices the frontend is showing, if any.
    fn take_plan(&mut self) -> Option<Plan>;
}

/// Does `action`.
pub fn dispatch(action: Action, f: &mut impl Frontend) {
    match action {
        Action::Quit => f.present(Presentation::Quit),
        Action::Help => f.present(Presentation::KeyList),
        Action::CommandHelp => f.present(Presentation::CommandList),
        Action::ShowView(view) => f.set_view(view),
        Action::NextView => {
            let next = f.view().next();
            f.set_view(next);
        }
        Action::Cursor(rows) => move_cursor(f, rows),
        Action::CursorFirst => select(f, 0),
        Action::CursorLast => {
            let last = len(f, f.view()).saturating_sub(1);
            select(f, last);
        }
        Action::StartSearch => f.prompt(Prompt::Search),
        Action::Search(query) => {
            search(f, &query);
            if f.listed().is_empty() {
                f.notify(Message::NoMatches);
            }
        }
        Action::ClearSearch => {
            if f.set_results(None).is_some() {
                f.set_cursor(View::Library, Some(0));
            }
        }
        Action::StartCommand => f.prompt(Prompt::Command),
        Action::Activate => activate(f),

        Action::Add => add(f),
        Action::Remove => {
            let Some(i) = f.cursor(View::Selection) else {
                return;
            };
            if let Some(outcome) = f.session_mut().remove_from_selection(i) {
                select(f, i);
                f.notify(outcome.into());
            }
        }
        Action::MoveTrack(by) => {
            let Some(i) = f.cursor(View::Selection) else {
                return;
            };
            if let Some(to) = f.session_mut().move_in_selection(i, by) {
                f.set_cursor(View::Selection, Some(to));
            }
        }
        Action::ClearSelection => match f.session().selection().len() {
            0 => f.notify(Refusal::SelectionEmpty.into()),
            n => f.confirm(Confirm::ClearSelection(n)),
        },
        Action::StartSave => match f.session().check_save() {
            Ok(()) => f.prompt(Prompt::Save),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::SaveAs(name) => match f.session().check_save() {
            Ok(()) => save_as(f, &name),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::DeletePlaylist => {
            if let Some(pl) = playlist_under_cursor(f) {
                f.confirm(Confirm::DeletePlaylist(pl));
            }
        }
        Action::StartRename => match playlist_under_cursor(f) {
            // The prompt starts from the current name, usually a small edit away.
            Some(pl) => f.prompt(Prompt::Rename(pl)),
            None => f.notify(Message::NoPlaylistUnderCursor),
        },
        Action::RenameTo(name) => match playlist_under_cursor(f) {
            Some(pl) => rename(f, &pl, &name),
            None => f.notify(Message::NoPlaylistUnderCursor),
        },
        Action::Scan(dir) => match f.session_mut().scan(dir.clone()) {
            Ok(_) => f.notify(Outcome::ScanStarted { dir }.into()),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::Open(paths) => {
            f.session_mut().open(paths);
            f.notify(Outcome::Opening.into());
        }
        Action::PlayPlaylist(name) => {
            let notice = f.session_mut().play_playlist_named(&name);
            f.notify(notice.into());
        }

        Action::TogglePause => f.session().send(Cmd::TogglePause),
        Action::Next => f.session().send(Cmd::Next),
        Action::Prev => f.session().send(Cmd::Prev),
        Action::Stop => f.session().send(Cmd::Stop),
        Action::SeekBy(seconds) => f.session().send(Cmd::SeekBy(seconds)),
        Action::SeekTo(at) => f.session().send(Cmd::Seek(at)),
        Action::VolumeBy(delta) => f.session().volume_by(delta),
        Action::SetVolume(v) => f.session().send(Cmd::SetVolume(v)),
        Action::SpeedBy(semitones) => f.session().send(Cmd::SpeedBy(semitones)),
        Action::SetSpeed(semitones) => f.session().send(Cmd::SetSpeed(semitones)),
        Action::CycleMode(forward) => {
            let notice = f.session().cycle_mode(forward);
            f.notify(notice.into());
        }
        Action::SetMode(mode) => {
            let notice = f.session().set_mode(mode);
            f.notify(notice.into());
        }

        Action::Mark => {
            let notice = f.session_mut().add_mark(None);
            f.notify(notice.into());
        }
        Action::MarkAt(at) => {
            let notice = f.session_mut().add_mark(Some(at));
            f.notify(notice.into());
        }
        Action::UndoMark => {
            let notice = f.session_mut().undo_mark();
            f.notify(notice.into());
        }
        Action::ClearMarks => match f.session_mut().marks_to_clear() {
            Ok(n) => f.confirm(Confirm::ClearMarks(n)),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::NextMark => {
            let notice = f.session_mut().seek_to_mark(true);
            f.notify(notice.into());
        }
        Action::PrevMark => {
            let notice = f.session_mut().seek_to_mark(false);
            f.notify(notice.into());
        }

        Action::Slice(slicing) => slice(f, slicing),
        Action::Zoom(zoom) => f.present(Presentation::Zoom(zoom)),
        Action::Display(display) => f.present(Presentation::Display(display)),
        Action::WriteSlices => match f.take_plan() {
            Some(plan) => {
                f.session_mut().write_slices(plan);
                f.notify(Outcome::ExportStarted.into());
            }
            None => f.notify(Message::NoSlicesPlanned),
        },
        Action::DiscardSlices => match f.take_plan() {
            Some(_) => f.notify(Message::SlicesDiscarded),
            None => f.notify(Message::NoSlicesPlanned),
        },

        Action::Map { view, key, action } => {
            let shown = Action::Map {
                view,
                key,
                action: action.clone(),
            };
            f.keys().bind(view, key, action.map(|a| *a));
            f.notify(Message::Mapped(shown));
        }
        Action::Unmap { view, key } => {
            if f.keys().unbind(view, key) {
                f.notify(Message::Unmapped(key));
            } else {
                f.notify(Message::NotBound { key, view });
            }
        }
    }
}

/// Does what `question` asked about, once the listener has said yes.
pub fn confirmed(question: Confirm, f: &mut impl Frontend) {
    match question {
        Confirm::ReplacePlaylist(name) => {
            let notice = f.session_mut().save_selection(&name, true);
            f.notify(notice.into());
        }
        Confirm::ClearMarks(_) => {
            if let Some(notice) = f.session_mut().clear_marks() {
                f.notify(notice.into());
            }
        }
        Confirm::ClearSelection(_) => {
            let outcome = f.session_mut().clear_selection();
            f.set_cursor(View::Selection, None);
            f.notify(outcome.into());
        }
        Confirm::DeletePlaylist(pl) => {
            if let Some(notice) = f.session_mut().delete_playlist(pl.id) {
                f.set_view(View::Playlists);
                let row = f.cursor(View::Playlists).unwrap_or(0);
                select(f, row);
                f.notify(notice.into());
            }
        }
    }
}

/// Shows the tracks matching `query` in the library view, or the whole library
/// for an empty query, with the cursor on the first.
pub fn search(f: &mut impl Frontend, query: &str) {
    f.set_view(View::Library);
    let results = (!query.is_empty()).then(|| f.session().search(query));
    f.set_results(results);
    let row = (!f.listed().is_empty()).then_some(0);
    f.set_cursor(View::Library, row);
}

/// Saves the selection as `name`, asking first if that replaces a playlist.
pub fn save_as(f: &mut impl Frontend, name: &str) {
    match f.session_mut().save_selection(name, false) {
        Notice::Refused(Refusal::WouldReplace(name)) => f.confirm(Confirm::ReplacePlaylist(name)),
        notice => f.notify(notice.into()),
    }
}

/// Renames `from` to `name`; the playlists cursor follows it to its new place.
pub fn rename(f: &mut impl Frontend, from: &Playlist, name: &str) {
    let notice = f.session_mut().rename_playlist(from.id, name);
    if matches!(notice, Notice::Done(_)) {
        // The list is sorted by name, so the renamed playlist may have moved.
        let at = f.session().playlists().iter().position(|p| p.id == from.id);
        f.set_cursor(View::Playlists, at);
    }
    f.notify(notice.into());
}

/// The number of rows `view` lists.
fn len(f: &impl Frontend, view: View) -> usize {
    match view {
        View::Library => f.listed().len(),
        View::Selection => f.session().selection().len(),
        View::Playlists => f.session().playlists().len(),
        View::Sampler => 0,
    }
}

/// Puts the current view's cursor on row `i`, or its last row.
fn select(f: &mut impl Frontend, i: usize) {
    let view = f.view();
    if view == View::Sampler {
        return;
    }
    let row = match len(f, view) {
        0 => None,
        n => Some(i.min(n - 1)),
    };
    f.set_cursor(view, row);
}

/// Moves the current view's cursor `rows` rows, stopping at either end.
fn move_cursor(f: &mut impl Frontend, rows: i64) {
    let view = f.view();
    let n = len(f, view);
    if n == 0 {
        return;
    }
    let at = f.cursor(view).unwrap_or(0) as i64;
    f.set_cursor(view, Some((at + rows).clamp(0, n as i64 - 1) as usize));
}

/// The playlist under the cursor, when the playlists view is showing.
fn playlist_under_cursor(f: &impl Frontend) -> Option<Playlist> {
    if f.view() != View::Playlists {
        return None;
    }
    let row = f.cursor(View::Playlists)?;
    f.session().playlists().get(row).cloned()
}

/// Plays the list in view from the cursor, or the playlist under it.
fn activate(f: &mut impl Frontend) {
    match f.view() {
        View::Library => {
            let Some(i) = f.cursor(View::Library) else {
                return;
            };
            let tracks = f.listed().to_vec();
            if !tracks.is_empty() {
                f.session_mut().play(&tracks, i);
            }
        }
        View::Selection => {
            if let Some(i) = f.cursor(View::Selection) {
                let tracks = f.session().selection().to_vec();
                f.session_mut().play(&tracks, i);
            }
        }
        View::Playlists => {
            if let Some(pl) = playlist_under_cursor(f) {
                let notice = f.session_mut().play_playlist(pl.id);
                f.notify(notice.into());
            }
        }
        View::Sampler => {}
    }
}

/// In the library, selects the track under the cursor or unselects it; on a
/// playlist, adds its tracks. Either way the cursor moves on a row, so a run
/// of tracks takes one key each.
fn add(f: &mut impl Frontend) {
    let outcome = match f.view() {
        View::Library => {
            let Some(i) = f.cursor(View::Library) else {
                return;
            };
            let Some(track) = f.listed().get(i).cloned() else {
                return;
            };
            f.session_mut().toggle_selected(track)
        }
        View::Playlists => {
            let Some(pl) = playlist_under_cursor(f) else {
                return;
            };
            let Some(outcome) = f.session_mut().add_playlist_to_selection(pl.id) else {
                return;
            };
            outcome
        }
        View::Selection | View::Sampler => return,
    };
    // Keep the selection's cursor on a track: on the first once a track is
    // added, and within the list when unselecting shortens it.
    let n = f.session().selection().len();
    match outcome {
        Outcome::RemovedFromSelection => {
            let row = f
                .cursor(View::Selection)
                .map(|i| i.min(n.saturating_sub(1)));
            f.set_cursor(View::Selection, if n == 0 { None } else { row });
        }
        Outcome::AddedToSelection if f.cursor(View::Selection).is_none() => {
            f.set_cursor(View::Selection, Some(0));
        }
        _ => {}
    }
    move_cursor(f, 1);
    f.notify(outcome.into());
}

/// Plans slices of the playing track in the sampler view, which shows them
/// before they are written; elsewhere, writes them at once.
fn slice(f: &mut impl Frontend, slicing: Slicing) {
    let cut = match slicing {
        Slicing::Region => Cut::Region,
        Slicing::Marks => Cut::Marks,
        Slicing::Equal(n) => Cut::Equal(n),
        Slicing::Onsets(s) => Cut::Onsets(s.unwrap_or(f.onset_sensitivity())),
    };
    if f.view() == View::Sampler {
        match f.session_mut().plan_slices(cut) {
            Ok(_) => {
                f.planning();
                f.notify(Outcome::PlanStarted.into());
            }
            Err(refusal) => f.notify(refusal.into()),
        }
    } else {
        match f.session_mut().export(cut) {
            Ok(_) => f.notify(Outcome::ExportStarted.into()),
            Err(refusal) => f.notify(refusal.into()),
        }
    }
}
