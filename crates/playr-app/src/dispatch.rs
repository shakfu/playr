//! Doing an action: the part of every key and `:` command a frontend shares.
//!
//! A frontend implements [`Frontend`] over its own state: which view shows,
//! where each view's cursor is, what the library view lists, and how it
//! prompts and asks. [`dispatch`] then does any [`Action`] the same way in
//! every frontend, calling the session for what changes the library or
//! playback and the frontend for what changes only the interface.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use playr_core::audio::eq::Band;
use playr_core::audio::Cmd;
use playr_core::db::query::{Playlist, Query, SavedSearch};
use playr_core::db::{SavedQueue, Track};
use playr_core::event::JobId;
use playr_core::notice::{Notice, Outcome, Refusal};
use playr_core::samples::{Cut, Job, Plan};
use playr_core::session::Session;
use playr_core::wave::Peaks;

use crate::action::{Action, FilterBy, Keymap, LoopAction, Slicing, Zoom};
use crate::command::view_name;
use crate::message::Message;
use crate::mix::{MixAction, Strip};
use crate::sampler::{self, Before, Sampler, Selected};
use crate::{Display, Theme, View};

mod editing;
mod rows;

pub(crate) use editing::tempo_found;
use editing::*;
pub use editing::{before, move_slice, remember, set_edge, settle};
pub(crate) use rows::cursor_track;
use rows::*;
pub use rows::{rename, save_as, search};

/// A destructive action held until the listener confirms it.
#[derive(Debug, Clone, PartialEq)]
pub enum Confirm {
    DeletePlaylist(Playlist),
    /// Replace the selection, which holds `replacing` tracks, with the
    /// playlist's, to edit it.
    EditPlaylist {
        playlist: Playlist,
        replacing: usize,
    },
    /// Overwrite the saved search `name` with the search shown.
    ReplaceSearch(String),
    DeleteSearch(SavedSearch),
    /// Overwrite the playlist `name` with the selection, or the queue.
    ReplacePlaylist {
        name: String,
        queue: bool,
    },
    /// Empty the selection, which holds this many tracks.
    ClearSelection(usize),
    /// Take this many tracks, played and waiting, out of the queue.
    ClearQueue(usize),
    /// Remove the tracks and marks under this directory whose files are gone,
    /// or under every recorded root when `None`.
    Prune(Option<PathBuf>),
    /// Forget this root, and every track and mark under it.
    ForgetRoot(PathBuf),
    /// Take up this track again, at the position playr closed on, and the
    /// queue stored with it.
    Resume {
        path: PathBuf,
        at: Duration,
        queue: SavedQueue,
    },
    /// Put back the tape kept at the last close.
    ResumeTape(Box<crate::tape::SavedTape>),
    /// Remove `count` marks from the track at `path`, which was playing when asked.
    ClearMarks {
        path: PathBuf,
        count: usize,
    },
    /// Empty the `count` loop slots the track at `path` fills.
    ClearLoops {
        path: PathBuf,
        count: usize,
    },
}

impl Confirm {
    /// The question asked before the action, without how to answer it.
    pub fn question(&self) -> String {
        match self {
            Confirm::DeletePlaylist(p) => format!("delete playlist \"{}\"?", p.name),
            Confirm::DeleteSearch(s) => format!("delete saved search \"{}\"?", s.name),
            Confirm::ReplaceSearch(name) => {
                format!("replace saved search \"{name}\" with the search shown?")
            }
            Confirm::EditPlaylist {
                playlist,
                replacing,
            } => {
                let tracks = match replacing {
                    1 => "1 selected track".into(),
                    n => format!("{n} selected tracks"),
                };
                format!(
                    "replace the {tracks} with \"{}\" to edit it?",
                    playlist.name
                )
            }
            Confirm::ReplacePlaylist { name, queue } => {
                let with = if *queue { "the queue" } else { "the selection" };
                format!("replace playlist \"{name}\" with {with}?")
            }
            Confirm::ClearSelection(n) => format!("clear all {n} tracks from the selection?"),
            Confirm::ClearQueue(n) => format!("clear all {n} tracks from the queue?"),
            Confirm::Prune(Some(dir)) => format!(
                "remove tracks and marks under {} whose files are gone?",
                crate::message::home_as_tilde(dir)
            ),
            Confirm::Prune(None) => {
                "remove tracks and marks of missing files from the library?".into()
            }
            Confirm::ForgetRoot(dir) => format!(
                "forget {}, and every track and mark under it?",
                crate::message::home_as_tilde(dir)
            ),
            Confirm::Resume { path, at, queue } => {
                let name = path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy();
                let queued = match queue.len() {
                    0 => String::new(),
                    1 => ", with its queue of 1 track".into(),
                    n => format!(", with its queue of {n} tracks"),
                };
                let at = crate::message::fmt_time(*at);
                format!("take up {name} again at {at}{queued}?")
            }
            Confirm::ResumeTape(saved) => {
                let loops = match saved.files() {
                    1 => "1 loop".into(),
                    n => format!("{n} loops"),
                };
                let playing = if saved.playing() { ", playing" } else { "" };
                format!("take up the tape again, with its {loops}{playing}?")
            }
            Confirm::ClearMarks { path, count } => {
                let name = path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy();
                format!("clear all {count} marks from {name}?")
            }
            Confirm::ClearLoops { path, count } => {
                let name = path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy();
                format!("clear all {count} loops from {name}?")
            }
        }
    }
}

/// Text a frontend collects before an action can happen.
#[derive(Debug, Clone, PartialEq)]
pub enum Prompt {
    /// A search, shown as it is typed through [`search`], starting from
    /// this text.
    Search(String),
    /// A `:` command, starting from this text.
    Command(String),
    /// A name to save the selection as, then [`save_as`].
    Save,
    /// A name to keep the search shown under.
    SaveSearch,
    /// A new name for this playlist, then [`rename`].
    Rename(Playlist),
}

/// A change only the interface shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Presentation {
    Quit,
    /// The keys bound in the current view.
    KeyList,
    /// Every `:` command.
    CommandList,
    /// The directories the library covers.
    RootList,
    /// Show these columns, in this order.
    Columns(Vec<playr_core::columns::Column>),
    /// Lists are now sorted by these keys.
    Sorted(Vec<playr_core::columns::SortKey>),
    /// What analysis measured about one track.
    TrackInfo,
    /// Every message since start.
    MessageList,
    Zoom(Zoom),
    /// Draw the waveform this way, or the next way when `None`.
    Display(Option<Display>),
    Theme(Theme),
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
    /// The views this frontend draws, in tab order; it is not shown the others.
    fn views(&self) -> &'static [View];

    /// The row under `view`'s cursor, if it has rows and one is chosen.
    fn cursor(&self, view: View) -> Option<usize>;
    fn set_cursor(&mut self, view: View, row: Option<usize>);

    /// What the library view lists: the library, or search results.
    fn listed(&self) -> &[Track];
    /// Shows search results, or the whole library for `None`. Returns the
    /// results shown before.
    fn set_results(&mut self, results: Option<Vec<Track>>) -> Option<Vec<Track>>;
    /// Whether the library view lists search results.
    fn searching(&self) -> bool;

    /// The onset sensitivity `:slice onsets` uses when given none.
    fn onset_sensitivity(&self) -> f32;

    fn notify(&mut self, message: Message);
    /// Asks before `question`; on yes, the frontend calls [`confirmed`].
    fn confirm(&mut self, question: Confirm);
    fn prompt(&mut self, prompt: Prompt);
    fn present(&mut self, presentation: Presentation);

    /// The sampler view's state.
    fn sampler(&self) -> &Sampler;
    fn sampler_mut(&mut self) -> &mut Sampler;

    /// Slices of the playing track are being planned, as background job `job`.
    fn planning(&mut self, job: JobId);
    /// Takes the planned slices the frontend is showing, if any.
    fn take_plan(&mut self) -> Option<Plan>;
    /// A `:sql` statement runs as `job`; once it finishes, the frontend
    /// passes its result to [`sql_done`] with `then`. A later statement
    /// replaces an earlier one still running.
    fn sql_started(&mut self, job: JobId, then: SqlThen);

    /// The tape looper's state.
    fn tape(&mut self) -> &mut crate::tape::Deck;
    /// The DJ decks' state.
    fn dj(&mut self) -> &mut crate::dj::Decks;
    /// The mixer's faders.
    fn mix(&mut self) -> &mut crate::mix::Mix;
    /// The rows picked in each view.
    fn picks(&self) -> &Picks;
    fn picks_mut(&mut self) -> &mut Picks;
}

/// The rows [`Action::Pick`] picked, by track path in each view, so a pick
/// holds while a list is sorted or edited. A path queued twice is picked
/// in both rows. The library's are dropped when its list is replaced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Picks {
    pub library: HashSet<String>,
    pub selection: HashSet<String>,
    pub queue: HashSet<String>,
}

impl Picks {
    /// The picks of `view`, which has none unless it lists tracks.
    pub fn of(&self, view: View) -> Option<&HashSet<String>> {
        match view {
            View::Library | View::Dj => Some(&self.library),
            View::Selection => Some(&self.selection),
            View::Queue => Some(&self.queue),
            _ => None,
        }
    }

    fn of_mut(&mut self, view: View) -> Option<&mut HashSet<String>> {
        match view {
            View::Library | View::Dj => Some(&mut self.library),
            View::Selection => Some(&mut self.selection),
            View::Queue => Some(&mut self.queue),
            _ => None,
        }
    }
}

/// The tracks `view` lists, as its rows are numbered.
fn rows_of(f: &impl Frontend, view: View) -> Vec<Track> {
    match view {
        View::Library | View::Dj => f.listed().to_vec(),
        View::Selection => f.session().selection().to_vec(),
        View::Queue => f.session().queue_tracks(),
        _ => Vec::new(),
    }
}

/// The rows picked in `view`, with their indices, in list order; `None`
/// when none is, so the action falls back to the cursor row.
fn picked(f: &impl Frontend, view: View) -> Option<Vec<(usize, Track)>> {
    let picks = f.picks().of(view).filter(|p| !p.is_empty())?;
    let rows: Vec<(usize, Track)> = rows_of(f, view)
        .into_iter()
        .enumerate()
        .filter(|(_, t)| picks.contains(&t.path))
        .collect();
    (!rows.is_empty()).then_some(rows)
}

/// Does [`Action::Pick`] in the view shown.
fn pick(f: &mut impl Frontend, which: crate::action::Pick) {
    use crate::action::Pick;
    let view = f.view();
    if f.picks().of(view).is_none() {
        return;
    }
    let rows = rows_of(f, view);
    let range = match which {
        Pick::Clear => {
            if let Some(p) = f.picks_mut().of_mut(view) {
                p.clear();
            }
            return f.notify(Message::Picked(0));
        }
        Pick::All => 0..rows.len(),
        Pick::Rows(from, to) => from.min(rows.len())..(to + 1).min(rows.len()),
        Pick::Cursor => match f.cursor(view) {
            Some(i) if i < rows.len() => i..i + 1,
            _ => return,
        },
    };
    let paths: Vec<String> = rows[range].iter().map(|t| t.path.clone()).collect();
    let Some(picks) = f.picks_mut().of_mut(view) else {
        return;
    };
    // All picks; :pick all always picks, as :toggle all selects first.
    let unpick = which != Pick::All && !paths.is_empty() && paths.iter().all(|p| picks.contains(p));
    for path in paths {
        match unpick {
            true => picks.remove(&path),
            false => picks.insert(path),
        };
    }
    let n = picks.len();
    if which == Pick::Cursor {
        move_cursor(f, 1);
    }
    f.notify(Message::Picked(n));
}

/// What to do with the tracks a `:sql` statement names, once it finishes.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlThen {
    /// Show them as search results, from this statement.
    Show(String),
    /// Add them to the selection.
    Select,
    /// Queue them, next when set.
    Enqueue(bool),
}

/// Starts `statement` on its own thread, to do `then` with its tracks.
fn run_sql(f: &mut impl Frontend, statement: String, then: SqlThen) {
    match f.session_mut().sql_in_background(statement) {
        Ok(job) => {
            f.sql_started(job, then);
            f.notify(Message::Querying);
        }
        Err(refusal) => f.notify(refusal.into()),
    }
}

/// Does what [`SqlThen`] says with a finished statement's `result`.
pub fn sql_done(f: &mut impl Frontend, then: SqlThen, result: Result<Vec<PathBuf>, String>) {
    let tracks = match result {
        Ok(paths) => f.session().tracks_at(&paths),
        Err(error) => return f.notify(Refusal::Sql(error).into()),
    };
    match then {
        SqlThen::Show(statement) => {
            show_library(f);
            let empty = tracks.is_empty();
            f.session_mut().set_shown(Some(Query::Sql(statement)));
            f.set_results(Some(tracks));
            f.set_cursor(View::Library, (!empty).then_some(0));
            f.notify(match empty {
                true => Message::NoMatches,
                false => Message::Found(f.listed().len()),
            });
        }
        SqlThen::Select => {
            if let Some(outcome) = f.session_mut().add_all_to_selection(tracks) {
                f.notify(outcome.into());
            }
        }
        SqlThen::Enqueue(_) if tracks.is_empty() => f.notify(Message::NoMatches),
        SqlThen::Enqueue(next) => {
            let outcome = f.session_mut().enqueue(&tracks, next);
            f.notify(outcome.into());
        }
    }
}

/// Does `action`, keeping what it changes in the marks or range for undo.
pub fn dispatch(action: Action, f: &mut impl Frontend) {
    if let Some(p) = &f.sampler().preview {
        if f.view() == View::Sampler && edits_sampler(&action) {
            let path = p.path.clone();
            return f.notify(Message::PreviewOnly(path));
        }
    }
    if matches!(action, Action::Undo | Action::Redo) {
        return undo(f, action == Action::Redo);
    }
    let before = before(f);
    act(action, f);
    settle(f, before);
}

/// Whether `action` moves the playhead, or edits the playing track's marks,
/// range, loops or slices: what the sampler refuses while it previews.
fn edits_sampler(action: &Action) -> bool {
    use Action::*;
    matches!(
        action,
        Restart
            | SeekBy(_)
            | SeekTo(_)
            | Mark
            | MarkAt(_)
            | UndoMark
            | ClearMarks
            | NextMark
            | PrevMark
            | Undo
            | Redo
            | Slice(_)
            | Nudge(_)
            | RangeIn(_)
            | RangeOut(_)
            | SetRange(_)
            | Loop(_)
            | LoopSlot(..)
            | ClearLoops
            | SelectMarkAt(_)
            | SelectSliceAt(_)
            | MoveSelected(_)
            | MoveSelectedTo(_)
            | SnapSelected
            | RemoveSelected
            | Audition
            | AuditionSlice(_)
            | Scrub(_)
            | PickEdge(_)
            | SelectEdge(_)
            | WriteSlices
            | DiscardSlices
            | MarkSlices
            | Label(_)
            | StartLabel
            | ExportMarks(_)
    )
}

/// Labels the selected mark `text`, or with `None` opens the command line on
/// its label, to edit.
fn label(f: &mut impl Frontend, text: Option<String>) {
    let current = f.session().player().status().current().cloned();
    let Some(frame) = f.sampler().selected_mark(current.as_ref()) else {
        return f.notify(Message::NothingSelected);
    };
    match text {
        Some(text) => {
            let notice = f.session_mut().label_mark(frame, &text);
            f.notify(notice.into());
        }
        None => {
            let labels = f.session_mut().labels_for(current.as_ref());
            let now = labels
                .iter()
                .find(|(m, _)| *m == frame)
                .map(|(_, l)| l.as_str());
            let line = format!("label {}", now.unwrap_or_default());
            f.prompt(Prompt::Command(line));
        }
    }
}

/// Shows the track under the cursor in the sampler without playing it, or
/// in the sampler ends the preview. The playing track needs none.
fn preview(f: &mut impl Frontend) {
    if f.view() == View::Sampler {
        let message = match f.sampler_mut().preview.take() {
            Some(_) => Message::PreviewEnded,
            None => Message::NotPreviewing,
        };
        return f.notify(message);
    }
    let track = match f.view() {
        View::Queue => f
            .cursor(View::Queue)
            .and_then(|i| f.session().queue_tracks().get(i).cloned()),
        _ => cursor_track(f),
    };
    let Some(track) = track else {
        return f.notify(Message::NothingToPreview);
    };
    let path = PathBuf::from(&track.path);
    let playing = f.session().player().status().current() == Some(&path);
    f.sampler_mut().preview = match playing {
        true => None,
        false => {
            let marks = f
                .session()
                .marks_of(&path)
                .iter()
                .map(playr_core::db::query::Mark::time)
                .collect();
            Some(crate::sampler::Preview {
                path: path.clone(),
                marks,
            })
        }
    };
    show_view(f, View::Sampler);
    if !playing {
        f.notify(Message::Previewing(path));
    }
}

/// Shows `view`, or says the frontend has no such view.
fn show_view(f: &mut impl Frontend, view: View) {
    if f.views().contains(&view) {
        return f.set_view(view);
    }
    let name = view_name(view);
    let text = match view {
        View::Tape | View::Dj | View::Mix => format!("no {} view here; use :{name}", view.title()),
        _ => format!("no {} view here", view.title()),
    };
    f.notify(Message::Command(text));
}

fn act(action: Action, f: &mut impl Frontend) {
    match action {
        Action::Quit => f.present(Presentation::Quit),
        Action::Help => f.present(Presentation::KeyList),
        Action::CommandHelp => f.present(Presentation::CommandList),
        Action::ShowView(view) => show_view(f, view),
        Action::NextView => {
            let next = f.view().step(f.views(), 1);
            f.set_view(next);
        }
        Action::PrevView => {
            let prev = f.view().step(f.views(), -1);
            f.set_view(prev);
        }
        Action::Cursor(rows) => move_cursor(f, rows),
        Action::CursorFirst => select(f, 0),
        Action::CursorLast => {
            let last = len(f, f.view()).saturating_sub(1);
            select(f, last);
        }
        Action::StartSearch => f.prompt(Prompt::Search(String::new())),
        Action::Search(query) if f.view() == View::Loops => {
            f.session_mut().search_loops(Some(&query));
            let found = !f.session().loop_list().is_empty();
            f.set_cursor(View::Loops, found.then_some(0));
            if !found {
                f.notify(Message::NoMatches);
            }
        }
        Action::Filter(by) => {
            let playing = || {
                let path = f
                    .session()
                    .player()
                    .status()
                    .current()?
                    .to_str()?
                    .to_string();
                f.session()
                    .tracks()
                    .iter()
                    .find(|t| t.path == path)
                    .cloned()
            };
            let Some(track) = cursor_track(f).or_else(playing) else {
                return f.notify(Refusal::NothingPlaying.into());
            };
            let (field, value) = match by {
                FilterBy::Album => ("album", track.album),
                FilterBy::Artist => ("artist", track.artist),
            };
            let Some(value) = value.filter(|v| !v.trim().is_empty()) else {
                return f.notify(Refusal::NoTag(field).into());
            };
            // A phrase, matched from the start of its words: `album:"Kind of
            // Blue"` also finds "Kind of Blue (Legacy Edition)".
            search(f, &format!("{field}:\"{}\"", value.replace('"', "")));
            if f.listed().is_empty() {
                f.notify(Message::NoMatches);
            }
        }
        Action::Search(query) => {
            search(f, &query);
            if f.listed().is_empty() {
                f.notify(Message::NoMatches);
            }
        }
        Action::ClearSearch if f.view() == View::Loops => {
            f.session_mut().search_loops(None);
            let any = !f.session().loop_list().is_empty();
            f.set_cursor(View::Loops, any.then_some(0));
        }
        Action::ClearSearch => {
            f.session_mut().set_shown(None);
            if f.set_results(None).is_some() {
                f.set_cursor(View::Library, Some(0));
            }
        }
        Action::StartCommand => f.prompt(Prompt::Command(String::new())),
        Action::Activate => activate(f),
        Action::LoopLib(a) => loop_lib(f, a),

        Action::Pick(which) => pick(f, which),
        Action::Add if matches!(f.view(), View::Library | View::Dj | View::Queue) => {
            let view = f.view();
            let Some(rows) = picked(f, view) else {
                return add(f);
            };
            let tracks = rows.into_iter().map(|(_, t)| t).collect();
            if let Some(p) = f.picks_mut().of_mut(view) {
                p.clear();
            }
            if let Some(outcome) = f.session_mut().add_all_to_selection(tracks) {
                if f.cursor(View::Selection).is_none() && !f.session().selection().is_empty() {
                    f.set_cursor(View::Selection, Some(0));
                }
                f.notify(outcome.into());
            }
        }
        Action::Add => add(f),
        Action::ToggleAll => {
            let listed = f.listed().to_vec();
            if let Some(outcome) = f.session_mut().toggle_all_selected(listed) {
                f.notify(outcome.into());
            }
        }
        Action::Enqueue(next) if matches!(f.view(), View::Library | View::Dj | View::Selection) => {
            let view = f.view();
            let Some(rows) = picked(f, view) else {
                return enqueue(f, next);
            };
            let tracks: Vec<Track> = rows.into_iter().map(|(_, t)| t).collect();
            if let Some(p) = f.picks_mut().of_mut(view) {
                p.clear();
            }
            let outcome = f.session_mut().enqueue(&tracks, next);
            f.notify(outcome.into());
        }
        Action::Enqueue(next) => enqueue(f, next),
        Action::EnqueueAll => {
            let tracks = match f.view() {
                View::Library | View::Dj if f.searching() => f.listed().to_vec(),
                View::Selection => f.session().selection().to_vec(),
                _ => Vec::new(),
            };
            match tracks.is_empty() {
                true => f.notify(Message::NothingToQueue),
                false => {
                    let outcome = f.session_mut().enqueue(&tracks, false);
                    f.notify(outcome.into());
                }
            }
        }
        Action::ClearQueue => match f.session().played().len() + f.session().queue_rows().len() {
            0 => f.notify(Message::QueueEmpty),
            n => f.confirm(Confirm::ClearQueue(n)),
        },
        Action::Remove if f.view() == View::Loops => {
            let Some((i, l)) = loop_under_cursor(f) else {
                return f.notify(Message::NoLoopUnderCursor);
            };
            match f.session_mut().remove_loop(l.id) {
                Ok(name) => {
                    select(f, i);
                    f.notify(Message::LoopRemoved(name));
                }
                Err(e) => f.notify(Message::LoopFailed(e)),
            }
        }
        Action::Remove
            if matches!(f.view(), View::Queue | View::Selection)
                && picked(f, f.view()).is_some() =>
        {
            let view = f.view();
            let rows = picked(f, view).expect("checked");
            let first = rows[0].0;
            // From the last, so each index still names its row.
            for &(i, _) in rows.iter().rev() {
                match view {
                    View::Queue => f.session_mut().dequeue(i),
                    _ => f.session_mut().remove_from_selection(i),
                };
            }
            if let Some(p) = f.picks_mut().of_mut(view) {
                p.clear();
            }
            select(f, first);
            f.notify(Message::RemovedPicked(rows.len()));
        }
        Action::Remove => {
            // The queue's, or else the selection's.
            let view = match f.view() {
                View::Queue => View::Queue,
                _ => View::Selection,
            };
            let Some(i) = f.cursor(view) else {
                return;
            };
            let outcome = match view {
                View::Queue => f.session_mut().dequeue(i),
                _ => f.session_mut().remove_from_selection(i),
            };
            if let Some(outcome) = outcome {
                select(f, i);
                f.notify(outcome.into());
            }
        }
        Action::MoveTrack(by)
            if matches!(f.view(), View::Queue | View::Selection)
                && picked(f, f.view()).is_some() =>
        {
            let view = f.view();
            let mut rows: Vec<usize> = picked(f, view)
                .expect("checked")
                .into_iter()
                .map(|(i, _)| i)
                .collect();
            // The row nearest where they go moves first; when it cannot,
            // none does, so the picked rows keep their spacing.
            if by > 0 {
                rows.reverse();
            }
            let cursor = f.cursor(view);
            for i in rows {
                let moved = match view {
                    View::Queue => f.session_mut().move_in_queue(i, by),
                    _ => f.session_mut().move_in_selection(i, by),
                };
                if moved.is_none() {
                    break;
                }
                if cursor == Some(i) {
                    f.set_cursor(view, moved);
                }
            }
        }
        Action::MoveTrack(by) => {
            let view = match f.view() {
                View::Queue => View::Queue,
                _ => View::Selection,
            };
            let Some(i) = f.cursor(view) else {
                return;
            };
            let to = match view {
                View::Queue => f.session_mut().move_in_queue(i, by),
                _ => f.session_mut().move_in_selection(i, by),
            };
            if let Some(to) = to {
                f.set_cursor(view, Some(to));
            }
        }
        Action::ClearSelection => match f.session().selection().len() {
            0 => f.notify(Refusal::SelectionEmpty.into()),
            n => f.confirm(Confirm::ClearSelection(n)),
        },
        Action::StartSave => match check_save(f) {
            Ok(()) => f.prompt(Prompt::Save),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::SaveAs(name) => match check_save(f) {
            Ok(()) => save_as(f, &name),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::StartSaveSearch => match f.searching() {
            true => f.prompt(Prompt::SaveSearch),
            false => f.notify(Refusal::NoSearch.into()),
        },
        Action::SaveSearch(name) => {
            if !f.searching() {
                return f.notify(Refusal::NoSearch.into());
            }
            match f.session_mut().save_search(&name, false) {
                Notice::Refused(Refusal::WouldReplace(name)) => {
                    f.confirm(Confirm::ReplaceSearch(name))
                }
                notice => f.notify(notice.into()),
            }
        }
        Action::EditPlaylist if search_under_cursor(f).is_some() => {
            let search = search_under_cursor(f).expect("checked");
            show_search(f, &search);
            f.prompt(match &search.query {
                Query::Text(q) => Prompt::Search(q.clone()),
                Query::Sql(q) => Prompt::Command(format!("sql {q}")),
            });
        }
        Action::Sql(statement) => run_sql(f, statement.clone(), SqlThen::Show(statement)),
        Action::EditPlaylist => match playlist_under_cursor(f) {
            None => f.notify(Message::NoPlaylistUnderCursor),
            Some(playlist) => match f.session().selection().len() {
                0 => edit(f, &playlist),
                replacing => f.confirm(Confirm::EditPlaylist {
                    playlist,
                    replacing,
                }),
            },
        },
        Action::DeletePlaylist if search_under_cursor(f).is_some() => {
            let search = search_under_cursor(f).expect("checked");
            f.confirm(Confirm::DeleteSearch(search));
        }
        Action::DeletePlaylist => {
            if let Some(pl) = playlist_under_cursor(f) {
                f.confirm(Confirm::DeletePlaylist(pl));
            }
        }
        Action::StartRename if search_under_cursor(f).is_some() => {
            f.notify(Message::SearchNotRenamed)
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
            Ok(_) => f.notify(Outcome::ScanStarted { dir: Some(dir) }.into()),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::Rescan => match f.session_mut().rescan() {
            Ok(_) => f.notify(Outcome::ScanStarted { dir: None }.into()),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::FixTempo(fix) => {
            let notice = f.session_mut().fix_tempo(fix);
            f.notify(notice.into());
        }
        Action::Analyze(dir) => match f.session_mut().analyze(dir.clone()) {
            Ok(_) => f.notify(Outcome::AnalysisStarted { dir }.into()),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::ShowRoots => f.present(Presentation::RootList),
        Action::ShowInfo => f.present(Presentation::TrackInfo),
        Action::ShowMessages => f.present(Presentation::MessageList),
        Action::ShowDevice => {
            let device = f.session().player().device().cloned();
            let sink = device
                .as_ref()
                .and_then(playr_core::audio::output::pipewire_sink);
            let player = f.session().player();
            let status = player.status();
            let (volume, eq) = (player.volume(), player.eq());
            let inexact = match (&device, status.current()) {
                (Some(d), Some(path)) if status.source.is_some() => {
                    let bits = f.session().tracks_at(std::slice::from_ref(path));
                    let bits = bits.first().and_then(|t| t.bit_depth);
                    let shared = f.tape().status().is_some() || f.dj().status().is_some();
                    let why = playr_core::audio::output::inexact(
                        &d.id, &status, volume, eq, bits, shared,
                    );
                    Some(why)
                }
                _ => None,
            };
            f.notify(Message::Output {
                device,
                sink,
                inexact,
            });
        }
        Action::ForgetRoot(dir) => match f.session().check_forget(&dir) {
            Ok(()) => f.confirm(Confirm::ForgetRoot(dir)),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::Prune(dir) => match f.session().check_prune(dir.as_deref()) {
            Ok(()) => f.confirm(Confirm::Prune(dir)),
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
        Action::StopAfter => {
            let status = f.session().player().status();
            if status.state == playr_core::audio::State::Stopped {
                return f.notify(Refusal::NothingPlaying.into());
            }
            let on = !status.stop_after;
            f.session().send(Cmd::StopAfter(on));
            f.notify(Message::StopAfter(on));
        }
        Action::StopIn(after) => {
            f.session_mut().sleep_in(after);
            f.notify(Message::StopIn(after));
        }
        Action::Restart => {
            let status = f.session().player().status();
            let Some(path) = status.current().cloned() else {
                return f.notify(Refusal::NothingPlaying.into());
            };
            // Range ends are source frames, as the peaks count them.
            let start = f.sampler().range_ends(Some(&path)).0;
            let at = start
                .zip(peaks(f))
                .map_or(Duration::ZERO, |(s, p)| sampler::time_of(s, p.rate));
            // Stopped, the seek cues the track paused; either way it then plays.
            f.session().send(Cmd::Seek(at));
            f.session().send(Cmd::Resume);
        }
        Action::SeekBy(seconds) => f.session().send(Cmd::SeekBy(seconds)),
        Action::SeekTo(at) => {
            let at = snapped(f, at);
            f.session().send(Cmd::Seek(at));
        }
        Action::VolumeBy(delta) => crate::mix::act(f, MixAction::By(Strip::Master, delta)),
        Action::SetVolume(v) => crate::mix::act(f, MixAction::Set(Strip::Master, v)),
        Action::SpeedBy(cents) => f.session().send(Cmd::CentsBy(cents)),
        Action::SetSpeed(cents) => f.session().send(Cmd::SetCents(cents)),
        Action::Tempo(bpm) => tempo(f, bpm),
        Action::Interp(interp) => {
            use playr_dsp::Interp;
            let other = match f.mix().interp() {
                Interp::Sinc => Interp::Hermite,
                Interp::Hermite => Interp::Sinc,
            };
            let interp = interp.unwrap_or(other);
            f.mix().set_interp(interp);
            f.notify(Message::Interp(interp));
        }
        Action::Crossfade(d) => {
            f.session().send(Cmd::SetCrossfade(d));
            f.notify(Message::Crossfade(d));
        }
        Action::Fade(d) => {
            f.session().send(Cmd::SetFade(d));
            f.notify(Message::Fade(d));
        }
        Action::SetEq(band, db) => set_eq(f, &[(band, db)]),
        Action::EqBy(band, db) => {
            let now = f.session().player().eq()[band as usize];
            set_eq(f, &[(band, now + db)]);
        }
        Action::FlatEq => set_eq(f, &Band::ALL.map(|b| (b, 0.0))),
        Action::BypassEq => bypass_eq(f),
        Action::Preview => preview(f),
        Action::Label(text) => label(f, Some(text)),
        Action::StartLabel => label(f, None),
        Action::ExportMarks(format) => {
            let notice = f.session_mut().export_marks(format);
            f.notify(notice.into());
        }
        Action::CycleMode(forward) => {
            let notice = f.session().cycle_mode(forward);
            f.notify(notice.into());
        }
        Action::SetMode(mode) => {
            let notice = f.session().set_mode(mode);
            f.notify(notice.into());
        }
        Action::SetSliceEdges(edges) => {
            let notice = f.session_mut().set_slice_edges(edges);
            // Slices already planned are planned again, so what is written
            // follows the choice made last. One still planning is when it lands.
            if let Some(plan) = f.take_plan() {
                let id = f.session_mut().replan(plan.job);
                f.planning(id);
            }
            f.notify(notice.into());
        }
        Action::SetReplayGain(replaygain) => {
            let notice = f.session_mut().set_replaygain(replaygain);
            f.notify(notice.into());
        }

        Action::Mark => mark(f, None),
        Action::MarkAt(at) => mark(f, Some(at)),
        Action::UndoMark => {
            let notice = f.session_mut().undo_mark();
            f.notify(notice.into());
        }
        Action::ClearMarks => match f.session_mut().marks_to_clear() {
            Ok((path, count)) => f.confirm(Confirm::ClearMarks { path, count }),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::NextMark | Action::PrevMark if f.view() == View::Sampler => {
            select_mark(f, action == Action::NextMark)
        }
        Action::NextMark | Action::PrevMark => {
            let notice = f.session_mut().seek_to_mark(action == Action::NextMark);
            f.notify(notice.into());
        }
        // `dispatch` undoes and redoes before it gets here.
        Action::Undo | Action::Redo => {}

        Action::Slice(slicing) => slice(f, slicing),
        Action::Zoom(zoom) => f.present(Presentation::Zoom(zoom)),
        Action::Display(display) => f.present(Presentation::Display(display)),
        Action::Theme(theme) => f.present(Presentation::Theme(theme)),
        Action::SetColumns(columns) => f.present(Presentation::Columns(columns)),
        Action::SetSort(keys) => {
            // The cursor follows its track rather than its row number, and
            // search results are re-ordered in place: sorting is not a
            // reason to lose a search.
            let under = library_track(f);
            f.session_mut().set_sort(keys.clone());
            if let Some(results) = f.set_results(None) {
                let sorted = f.session().sorted(results);
                f.set_results(Some(sorted));
            }
            if let Some(track) = under {
                let row = f.listed().iter().position(|t| t.path == track.path);
                f.set_cursor(View::Library, row.or(Some(0)));
            }
            f.present(Presentation::Sorted(keys));
        }
        Action::Nudge(nudge) => match (peaks(f), f.sampler().scale) {
            (Some(peaks), Some(scale)) => {
                let from = sampler::frame_of(f.session().player().position(), peaks.rate);
                let to = sampler::nudge(&peaks, from, scale.frames(nudge), f.sampler().snap);
                f.session()
                    .send(Cmd::Seek(sampler::time_of(to, peaks.rate)));
            }
            _ => f.notify(Message::NoWaveform),
        },
        Action::Audition => audition(f),
        Action::AuditionSlice(forward) => audition_slice(f, forward),
        Action::Scrub(at) => scrub(f, at),
        Action::SelectMarkAt(at) => match peaks(f) {
            Some(peaks) => {
                let Some(path) = f.session().player().status().current().cloned() else {
                    return f.notify(Refusal::NothingPlaying.into());
                };
                let at = sampler::frame_of(at, peaks.rate);
                let within = near(&peaks, f.sampler().scale);
                match f.session_mut().mark_near(at, within) {
                    Some(mark) => {
                        f.sampler_mut().selected = Some((path, Selected::Mark(mark.frame)))
                    }
                    None => f.notify(Refusal::NoMarkHere.into()),
                }
            }
            None => f.notify(Message::NoWaveform),
        },
        Action::SelectSliceAt(at) => match peaks(f) {
            Some(peaks) => {
                let at = sampler::frame_of(at, peaks.rate);
                let within = near(&peaks, f.sampler().scale);
                let path = f.session().player().status().current().cloned();
                let plan =
                    (f.sampler().pending.as_ref()).filter(|p| Some(&p.job.path) == path.as_ref());
                let start = (plan.iter().flat_map(|p| &p.spans))
                    .map(|s| s.0)
                    .filter(|s| s.abs_diff(at) <= within)
                    .min_by_key(|s| s.abs_diff(at));
                match (path, start) {
                    (Some(path), Some(start)) => {
                        f.sampler_mut().selected = Some((path, Selected::Slice(start)))
                    }
                    _ => f.notify(Message::NoSliceHere),
                }
            }
            None => f.notify(Message::NoWaveform),
        },
        Action::Deselect => {
            f.sampler_mut().selected = None;
            f.notify(Message::Deselected);
        }
        Action::MoveSelected(nudge) => match selected(f) {
            Some((_, Selected::Edge(edge))) => move_edge(f, edge, nudge),
            Some((_, selected)) => match (peaks(f), f.sampler().scale) {
                (Some(peaks), Some(scale)) => {
                    let step = scale.frames(nudge);
                    let snap = f.sampler().snap;
                    match selected {
                        Selected::Mark(from) => {
                            move_selected_mark(f, from, sampler::nudge(&peaks, from, step, snap))
                        }
                        Selected::Slice(from) => {
                            move_slice(f, from, sampler::nudge(&peaks, from, step, snap))
                        }
                        Selected::Edge(_) => unreachable!("moved above"),
                    }
                }
                _ => f.notify(Message::NoWaveform),
            },
            None => f.notify(Message::NothingSelected),
        },
        Action::MoveSelectedTo(to) => match (selected(f), peaks(f)) {
            (None, _) => f.notify(Message::NothingSelected),
            (Some(_), None) => f.notify(Message::NoWaveform),
            (Some((path, selected)), Some(peaks)) => {
                let to = sampler::frame_of(snapped(f, to), peaks.rate);
                match selected {
                    Selected::Mark(from) => move_selected_mark(f, from, to),
                    Selected::Slice(from) => move_slice(f, from, to),
                    Selected::Edge(edge) => {
                        set_edge(f, &path, edge, to);
                        let (start, end) = f.sampler().range_ends(Some(&path));
                        f.notify(Message::Range {
                            start,
                            end,
                            rate: peaks.rate,
                        });
                    }
                }
            }
        },
        Action::SnapSelected => {
            let Some((path, selected)) = selected(f) else {
                return f.notify(Message::NothingSelected);
            };
            let from = match selected {
                Selected::Mark(frame) | Selected::Slice(frame) => Some(frame),
                Selected::Edge(sampler::Edge::Start) => f.sampler().range_ends(Some(&path)).0,
                Selected::Edge(sampler::Edge::End) => f.sampler().range_ends(Some(&path)).1,
            };
            let Some(from) = from else {
                return f.notify(Message::NothingSelected);
            };
            let sensitivity = f.onset_sensitivity();
            match f.session_mut().snap_mark(from, sensitivity) {
                Ok(job) => {
                    f.sampler_mut().snapping = Some((job, selected));
                    f.notify(Message::Snapping);
                }
                Err(refusal) => f.notify(refusal.into()),
            }
        }
        Action::RemoveSelected => match selected(f) {
            Some((_, Selected::Mark(frame))) => {
                let notice = f.session_mut().remove_mark(frame);
                f.notify(notice.into());
            }
            Some((_, Selected::Slice(start))) => remove_slice(f, start),
            Some((_, Selected::Edge(_))) => act(Action::SetRange(None), f),
            None if f.sampler().range.is_some() => act(Action::SetRange(None), f),
            None => f.notify(Message::NothingSelected),
        },
        Action::Loop(on) => {
            let status = f.session().player().status();
            let on = on.unwrap_or(status.looping.is_none());
            if !on {
                f.session().send(Cmd::Loop(None));
                return f.notify(Message::Loop(false));
            }
            if f.sampler().range_ends(status.current()) == (None, None) {
                if let Err(message) = range_to_region(f) {
                    return f.notify(message);
                }
            }
            let Some(range) = f.sampler().range(status.current()) else {
                return f.notify(Message::NoRangeToLoop);
            };
            f.session().send(Cmd::Loop(Some(range)));
            // Looping is for hearing the range, so a paused or stopped track plays.
            f.session().send(Cmd::Resume);
            f.notify(Message::Loop(true));
        }
        Action::LoopSlot(slot, op) => loop_slot(f, slot, op),
        Action::ClearLoops => match f.session_mut().loops_to_clear() {
            Ok((path, count)) => f.confirm(Confirm::ClearLoops { path, count }),
            Err(refusal) => f.notify(refusal.into()),
        },
        // Centring needs Fit: without it the view follows the playhead.
        Action::PickEdge(edge) => {
            if select_edge(f, edge) {
                f.sampler_mut().fit = true;
            }
        }
        Action::SelectEdge(edge) => {
            select_edge(f, edge);
        }
        Action::Snap(on) => {
            let on = on.unwrap_or(!f.sampler().snap);
            f.sampler_mut().snap = on;
            if on {
                snap_range(f);
            }
            f.notify(Message::Snap(on));
        }
        Action::Fit(on) => {
            let on = on.unwrap_or(!f.sampler().fit);
            f.sampler_mut().fit = on;
            f.sampler_mut().fit_edge = false;
            let playing = f.session().player().status().current().cloned();
            let range = f.sampler().range(playing.as_ref());
            if let (true, Some(peaks), Some(scale), Some((a, b))) =
                (on, peaks(f), f.sampler().scale, range)
            {
                f.sampler_mut().zoom = sampler::zoom_to_fit(peaks.frames, scale.columns, b - a);
            }
            f.notify(Message::Fit(on));
        }
        Action::RangeIn(at) => range_end_at(f, sampler::Edge::Start, at),
        Action::RangeOut(at) => range_end_at(f, sampler::Edge::End, at),
        Action::SetRange(times) => {
            let (path, rate) = match f.session().playing_track() {
                Ok(track) => track,
                Err(refusal) => return f.notify(refusal.into()),
            };
            let Some((a, b)) = times else {
                f.sampler_mut().range = None;
                follow_loop(f);
                return f.notify(Message::Range {
                    start: None,
                    end: None,
                    rate,
                });
            };
            let frame = |t| sampler::frame_of(snapped(f, t), rate);
            let (a, b) = (frame(a), frame(b));
            if a == b {
                return f.notify(Message::EmptyRange);
            }
            f.sampler_mut().range = Some(sampler::Range {
                path,
                start: Some(a.min(b)),
                end: Some(a.max(b)),
            });
            follow_loop(f);
            f.notify(Message::Range {
                start: Some(a.min(b)),
                end: Some(a.max(b)),
                rate,
            });
        }
        Action::WriteSlices => match f.take_plan() {
            Some(plan) => {
                f.session_mut().write_slices(plan);
                f.notify(Outcome::ExportStarted.into());
            }
            None => f.notify(Message::NoSlicesPlanned),
        },
        Action::Convert(format, export) => match f.session_mut().convert(format.clone(), export) {
            Ok(_) => f.notify(Outcome::ConvertStarted { format }.into()),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::Tape(t) => crate::tape::act(f, t),
        Action::Dj(d) => crate::dj::act(f, d),
        Action::Mix(m) => crate::mix::act(f, m),
        Action::MarkSlices => mark_slices(f),
        Action::DiscardSlices => {
            // Edits set by hand would otherwise be lost for good.
            let then = before(f).filter(|b| edited(&b.plan));
            match f.take_plan() {
                Some(_) => {
                    if let Some(then) = then {
                        remember(f, then);
                    }
                    f.notify(Message::SlicesDiscarded);
                }
                None => f.notify(Message::NoSlicesPlanned),
            }
        }

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
    let before = before(f);
    confirmed_now(question, f);
    settle(f, before);
}

fn confirmed_now(question: Confirm, f: &mut impl Frontend) {
    match question {
        Confirm::ReplacePlaylist { name, queue } => {
            let notice = match queue {
                true => f.session_mut().save_queue(&name, true),
                false => f.session_mut().save_selection(&name, true),
            };
            f.notify(notice.into());
        }
        Confirm::ClearMarks { path, .. } => {
            if let Some(notice) = f.session_mut().clear_marks(&path) {
                f.notify(notice.into());
            }
        }
        Confirm::ClearLoops { path, .. } => {
            let notice = f.session_mut().clear_loops(&path);
            f.notify(notice.into());
        }
        Confirm::Prune(dir) => match f.session_mut().prune(dir.clone()) {
            Ok(_) => f.notify(Outcome::PruneStarted { dir }.into()),
            Err(refusal) => f.notify(refusal.into()),
        },
        Confirm::ForgetRoot(dir) => match f.session_mut().forget_root(&dir) {
            Ok(removed) => f.notify(Outcome::Forgot { dir, removed }.into()),
            Err(refusal) => f.notify(refusal.into()),
        },
        Confirm::Resume { path, at, queue } => f.session_mut().resume(path, at, queue),
        Confirm::ResumeTape(saved) => crate::tape::resume(f, *saved),
        Confirm::ClearQueue(_) => {
            let outcome = f.session_mut().clear_queue();
            f.notify(outcome.into());
        }
        Confirm::ClearSelection(_) => {
            let outcome = f.session_mut().clear_selection();
            f.set_cursor(View::Selection, None);
            f.notify(outcome.into());
        }
        Confirm::EditPlaylist { playlist, .. } => edit(f, &playlist),
        Confirm::ReplaceSearch(name) => {
            let notice = f.session_mut().save_search(&name, true);
            f.notify(notice.into());
        }
        Confirm::DeleteSearch(search) => {
            if let Some(notice) = f.session_mut().delete_search(search.id) {
                let row = f.cursor(View::Playlists).unwrap_or(0);
                select(f, row);
                f.notify(notice.into());
            }
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
