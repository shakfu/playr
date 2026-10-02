//! Doing an action: the part of every key and `:` command a frontend shares.
//!
//! A frontend implements [`Frontend`] over its own state: which view shows,
//! where each view's cursor is, what the library view lists, and how it
//! prompts and asks. [`dispatch`] then does any [`Action`] the same way in
//! every frontend, calling the session for what changes the library or
//! playback and the frontend for what changes only the interface.

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

use crate::action::{Action, Keymap, Slicing, Zoom};
use crate::message::Message;
use crate::sampler::{self, Before, Sampler, Selected};
use crate::{Display, Theme, View};

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
            f.set_view(View::Library);
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
    if matches!(action, Action::Undo | Action::Redo) {
        return undo(f, action == Action::Redo);
    }
    let before = before(f);
    act(action, f);
    settle(f, before);
}

fn act(action: Action, f: &mut impl Frontend) {
    match action {
        Action::Quit => f.present(Presentation::Quit),
        Action::Help => f.present(Presentation::KeyList),
        Action::CommandHelp => f.present(Presentation::CommandList),
        Action::ShowView(view) => f.set_view(view),
        Action::NextView => {
            let next = f.view().next();
            f.set_view(next);
        }
        Action::PrevView => {
            let prev = f.view().prev();
            f.set_view(prev);
        }
        Action::Cursor(rows) => move_cursor(f, rows),
        Action::CursorFirst => select(f, 0),
        Action::CursorLast => {
            let last = len(f, f.view()).saturating_sub(1);
            select(f, last);
        }
        Action::StartSearch => f.prompt(Prompt::Search(String::new())),
        Action::Search(query) => {
            search(f, &query);
            if f.listed().is_empty() {
                f.notify(Message::NoMatches);
            }
        }
        Action::ClearSearch => {
            f.session_mut().set_shown(None);
            if f.set_results(None).is_some() {
                f.set_cursor(View::Library, Some(0));
            }
        }
        Action::StartCommand => f.prompt(Prompt::Command(String::new())),
        Action::Activate => activate(f),

        Action::Add => add(f),
        Action::Enqueue(next) => enqueue(f, next),
        Action::EnqueueAll => {
            let tracks = match f.view() {
                View::Library if f.searching() => f.listed().to_vec(),
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
        Action::Analyze(dir) => match f.session_mut().analyze(dir.clone()) {
            Ok(_) => f.notify(Outcome::AnalysisStarted { dir }.into()),
            Err(refusal) => f.notify(refusal.into()),
        },
        Action::ShowRoots => f.present(Presentation::RootList),
        Action::ShowInfo => f.present(Presentation::TrackInfo),
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
        Action::VolumeBy(delta) => f.session().volume_by(delta),
        Action::SetVolume(v) => f.session().send(Cmd::SetVolume(v)),
        Action::SpeedBy(semitones) => f.session().send(Cmd::SpeedBy(semitones)),
        Action::SetSpeed(semitones) => f.session().send(Cmd::SetSpeed(semitones)),
        Action::SetEq(band, db) => set_eq(f, &[(band, db)]),
        Action::EqBy(band, db) => {
            let now = f.session().player().eq()[band as usize];
            set_eq(f, &[(band, now + db)]);
        }
        Action::FlatEq => set_eq(f, &Band::ALL.map(|b| (b, 0.0))),
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
        Action::PickEdge(edge) => {
            f.sampler_mut().edge = edge;
            f.sampler_mut().fit_edge = true;
            let path = f.session().player().status().current().cloned();
            let (start, end) = f.sampler().range_ends(path.as_ref());
            let set = match edge {
                sampler::Edge::Start => start.is_some(),
                sampler::Edge::End => end.is_some(),
            };
            match path.filter(|_| set) {
                Some(path) => {
                    f.sampler_mut().selected = Some((path, Selected::Edge(edge)));
                    f.notify(Message::Edge(edge));
                }
                None => f.notify(Message::NoEdge(edge)),
            }
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
        Action::RangeIn | Action::RangeOut => {
            let (path, rate) = match f.session().playing_track() {
                Ok(track) => track,
                Err(refusal) => return f.notify(refusal.into()),
            };
            let at = sampler::frame_of(snapped(f, f.session().player().position()), rate);
            if action == Action::RangeIn {
                f.sampler_mut().set_range_start(&path, at);
            } else {
                f.sampler_mut().set_range_end(&path, at);
            }
            let (start, end) = f.sampler().range_ends(Some(&path));
            follow_loop(f);
            f.notify(Message::Range { start, end, rate });
        }
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

/// Shows the tracks matching `query` in the library view, or the whole library
/// for an empty query, with the cursor on the first.
pub fn search(f: &mut impl Frontend, query: &str) {
    f.set_view(View::Library);
    let results = (!query.is_empty()).then(|| f.session().search(query));
    let shown = (!query.is_empty()).then(|| Query::Text(query.to_string()));
    f.session_mut().set_shown(shown);
    f.set_results(results);
    let row = (!f.listed().is_empty()).then_some(0);
    f.set_cursor(View::Library, row);
}

/// Shows what `search` finds now in the library view, with the cursor on
/// the first.
fn show_search(f: &mut impl Frontend, search: &SavedSearch) {
    if let Query::Sql(statement) = &search.query {
        return run_sql(f, statement.clone(), SqlThen::Show(statement.clone()));
    }
    f.set_view(View::Library);
    let results = f.session().run_search(search);
    let empty = results.is_empty();
    f.session_mut().set_shown(Some(search.query.clone()));
    f.set_results(Some(results));
    f.set_cursor(View::Library, (!empty).then_some(0));
    if empty {
        f.notify(Message::NoMatches);
    }
}

/// The saved search under the playlists view's cursor. They are listed after
/// the playlists.
fn search_under_cursor(f: &impl Frontend) -> Option<SavedSearch> {
    if f.view() != View::Playlists {
        return None;
    }
    let row = f.cursor(View::Playlists)?;
    let i = row.checked_sub(f.session().playlists().len())?;
    f.session().searches().get(i).cloned()
}

/// Puts `playlist`'s tracks in the selection to edit, and shows them.
fn edit(f: &mut impl Frontend, playlist: &Playlist) {
    let Some(outcome) = f.session_mut().edit_playlist(playlist.id) else {
        return;
    };
    f.set_view(View::Selection);
    let row = (!f.session().selection().is_empty()).then_some(0);
    f.set_cursor(View::Selection, row);
    f.notify(outcome.into());
}

/// Whether `:save` can save: the queue in its view, else the selection.
fn check_save(f: &impl Frontend) -> Result<(), Refusal> {
    match f.view() {
        View::Queue => f.session().check_save_queue(),
        _ => f.session().check_save(),
    }
}

/// Saves the queue in its view, else the selection, as `name`, asking first
/// if that replaces a playlist.
pub fn save_as(f: &mut impl Frontend, name: &str) {
    let queue = f.view() == View::Queue;
    let notice = match queue {
        true => f.session_mut().save_queue(name, false),
        false => f.session_mut().save_selection(name, false),
    };
    match notice {
        Notice::Refused(Refusal::WouldReplace(name)) => {
            f.confirm(Confirm::ReplacePlaylist { name, queue })
        }
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
        View::Playlists => f.session().playlists().len() + f.session().searches().len(),
        View::Queue => f.session().played().len() + f.session().queue_rows().len(),
        View::Sampler => 0,
    }
}

/// Sets each band to its gain in dB, and says what the tone control is now.
fn set_eq(f: &mut impl Frontend, bands: &[(Band, f32)]) {
    for &(band, db) in bands {
        f.session().send(Cmd::SetEq(band, db));
    }
    let gains = f.session().player().eq();
    f.notify(Message::Eq(gains));
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
/// The library view's track under the cursor, for keeping it there.
fn library_track(f: &impl Frontend) -> Option<Track> {
    f.listed().get(f.cursor(View::Library)?).cloned()
}

fn activate(f: &mut impl Frontend) {
    match f.view() {
        View::Library => {
            let Some(i) = f.cursor(View::Library) else {
                return;
            };
            let tracks = f.listed().to_vec();
            if tracks.is_empty() {
                return;
            }
            // Search results are a list chosen, so they fill the queue; the
            // library plays on by itself.
            match f.searching() {
                true => {
                    if let Some(replaced) = f.session_mut().play_queued(&tracks, i) {
                        f.notify(replaced.into());
                    }
                }
                false => f.session_mut().play(&tracks, i),
            }
        }
        View::Selection => {
            if let Some(i) = f.cursor(View::Selection) {
                let tracks = f.session().selection().to_vec();
                if let Some(replaced) = f.session_mut().play_queued(&tracks, i) {
                    f.notify(replaced.into());
                }
            }
        }
        View::Playlists => {
            if let Some(pl) = playlist_under_cursor(f) {
                let notice = f.session_mut().play_playlist(pl.id);
                f.notify(notice.into());
            } else if let Some(search) = search_under_cursor(f) {
                show_search(f, &search);
            }
        }
        View::Queue => {
            if let Some(row) = f.cursor(View::Queue) {
                f.session_mut().play_queue_row(row);
            }
        }
        View::Sampler => {}
    }
}

/// Queues the track under the cursor, or the playlist's tracks, and moves
/// the cursor on a row, so a run of tracks takes one key each.
fn enqueue(f: &mut impl Frontend, next: bool) {
    let tracks: Vec<Track> = match f.view() {
        View::Library => library_track(f).into_iter().collect(),
        View::Selection => f
            .cursor(View::Selection)
            .and_then(|i| f.session().selection().get(i).cloned())
            .into_iter()
            .collect(),
        View::Playlists => match (playlist_under_cursor(f), search_under_cursor(f)) {
            (Some(pl), _) => f.session().playlist_tracks(pl.id),
            (
                None,
                Some(SavedSearch {
                    query: Query::Sql(statement),
                    ..
                }),
            ) => {
                move_cursor(f, 1);
                return run_sql(f, statement, SqlThen::Enqueue(next));
            }
            (None, Some(search)) => f.session().run_search(&search),
            (None, None) => Vec::new(),
        },
        View::Sampler | View::Queue => Vec::new(),
    };
    if tracks.is_empty() {
        return;
    }
    let outcome = f.session_mut().enqueue(&tracks, next);
    move_cursor(f, 1);
    f.notify(outcome.into());
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
            let added = match (playlist_under_cursor(f), search_under_cursor(f)) {
                (Some(pl), _) => f.session_mut().add_playlist_to_selection(pl.id),
                (
                    None,
                    Some(SavedSearch {
                        query: Query::Sql(statement),
                        ..
                    }),
                ) => {
                    move_cursor(f, 1);
                    return run_sql(f, statement, SqlThen::Select);
                }
                (None, Some(search)) => {
                    let found = f.session().run_search(&search);
                    f.session_mut().add_all_to_selection(found)
                }
                (None, None) => None,
            };
            let Some(outcome) = added else {
                return;
            };
            outcome
        }
        View::Queue => {
            let Some(i) = f.cursor(View::Queue) else {
                return;
            };
            let Some(track) = f.session().queue_tracks().get(i).cloned() else {
                return;
            };
            f.session_mut().add_to_selection(track)
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
/// The playing track's peaks, once the sampler view has read them.
fn peaks(f: &impl Frontend) -> Option<std::sync::Arc<Peaks>> {
    let status = f.session().player().status();
    sampler::peaks_of(f.sampler(), status.current()).ok()
}

/// How far from a click a mark still counts as under it: one column of the
/// view, so what looks like a hit is one, or 10 ms before anything is drawn.
fn near(peaks: &Peaks, scale: Option<sampler::Scale>) -> u64 {
    match scale {
        Some(scale) => scale.per_column.max(1),
        None => (peaks.rate / 100).max(1) as u64,
    }
}

/// Plays once, and pauses at the end of: the selected mark up to the next,
/// or the range when an end of it is selected. Otherwise whichever of these
/// has both ends: the range, the planned slice the playhead is in, or the
/// region around it.
///
/// The range first, because setting one is how a listener says what they mean;
/// then the plan, which is what `:slice` is about to write; then the region,
/// which is what `:slice region` would take.
fn audition(f: &mut impl Frontend) {
    let Some((path, peaks, at)) = hearing(f) else {
        return;
    };
    let (rate, last) = (peaks.rate, peaks.frames);
    // A selection says what is meant more plainly than the playhead.
    match f.sampler().selected(Some(&path)) {
        Some(Selected::Mark(frame)) => {
            let end = (mark_frames(f, &path).into_iter())
                .find(|&m| m > frame)
                .unwrap_or(last);
            return play_once(f, path, (frame, end), rate);
        }
        Some(Selected::Edge(_)) => {
            if let Some(range) = f.sampler().range(Some(&path)) {
                return play_once(f, path, range, rate);
            }
        }
        Some(Selected::Slice(start)) => {
            if let Some((plan, i)) = planned_slice(f, start) {
                return play_once(f, path, (start, plan.spans[i].1.unwrap_or(last)), rate);
            }
        }
        None => {}
    }
    // A span with no end runs to the end of the track.
    let ends = |end: Option<u64>| end.unwrap_or(last);

    // An audition pauses on its span's end, which is the next span's start.
    let again = f
        .sampler()
        .auditioned
        .as_ref()
        .filter(|(p, _, end)| *p == path && at.abs_diff(*end) <= u64::from(rate) / 100)
        .map(|&(_, start, end)| (start, end));
    // A planned slice is cut from the range, so it is the narrower choice.
    let slice = f.sampler().pending.as_ref().and_then(|plan| {
        let spans: Vec<(u64, u64)> = plan.spans.iter().map(|&(s, e)| (s, ends(e))).collect();
        again
            .filter(|span| spans.contains(span))
            .or_else(|| spans.into_iter().rfind(|&(start, _)| start <= at))
    });
    let span = slice
        .or_else(|| f.sampler().range(Some(&path)))
        .unwrap_or_else(|| {
            let marks: Vec<u64> = f
                .session_mut()
                .marks_for(Some(&path))
                .iter()
                .map(|m| m.frame)
                .collect();
            let (start, end) = playr_core::samples::region(&marks, at);
            again.unwrap_or((start, ends(end)))
        });
    play_once(f, path, span, rate);
}

/// Selects and plays the planned slice after the selected one, or before it;
/// with none selected, after the one last heard, or the one under the
/// playhead. Past either end it wraps round.
fn audition_slice(f: &mut impl Frontend, forward: bool) {
    let Some((path, peaks, at)) = hearing(f) else {
        return;
    };
    let Some(plan) = f.sampler().pending.as_ref() else {
        return f.notify(Message::NoSlicesPlanned);
    };
    let spans: Vec<(u64, u64)> = plan
        .spans
        .iter()
        .map(|&(start, end)| (start, end.unwrap_or(peaks.frames)))
        .collect();
    let near = u64::from(peaks.rate) / 100;
    let heard = f
        .sampler()
        .auditioned
        .as_ref()
        .filter(|(p, start, end)| *p == path && at >= *start && at <= end + near)
        .and_then(|&(_, start, end)| spans.iter().position(|&s| s == (start, end)));
    let under = spans.iter().rposition(|&(start, _)| start <= at);
    let selected = (f.sampler().selected_slice(Some(&path)))
        .and_then(|s| spans.iter().position(|span| span.0 == s));
    let n = spans.len();
    let to = match (selected.or(heard).or(under), forward) {
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
        (None, true) => 0,
        (None, false) => n - 1,
    };
    f.sampler_mut().selected = Some((path.clone(), Selected::Slice(spans[to].0)));
    play_once(f, path, spans[to], peaks.rate);
}

/// The playing track, its peaks and the playhead in frames, or why not.
fn hearing(f: &mut impl Frontend) -> Option<(std::path::PathBuf, std::sync::Arc<Peaks>, u64)> {
    let status = f.session().player().status();
    let Some(path) = status.current().cloned() else {
        f.notify(Refusal::NothingPlaying.into());
        return None;
    };
    let Some(peaks) = peaks(f) else {
        f.notify(Message::NoWaveform);
        return None;
    };
    let at = sampler::frame_of(f.session().player().position(), peaks.rate);
    Some((path, peaks, at))
}

/// Plays `start..end` once, faded as an export would write it.
fn play_once(f: &mut impl Frontend, path: std::path::PathBuf, (start, end): (u64, u64), rate: u32) {
    if end <= start {
        return f.notify(Message::NothingToAudition);
    }
    // As `Session::slice_job` decides: a looping range is written unfaded.
    let status = f.session().player().status();
    let looped = status.looping.is_some() && f.sampler().range(Some(&path)) == Some((start, end));
    let fades = match f.session().slice_edges() {
        playr_core::samples::Edges::Fade if !looped => {
            let fades = f.session().fades();
            let frames = |d: Duration| sampler::frame_of(d, rate);
            (frames(fades.fade_in), frames(fades.fade_out))
        }
        _ => (0, 0),
    };
    f.session().send(Cmd::PlayOnce(start, end, fades));
    f.sampler_mut().auditioned = Some((path, start, end));
    f.notify(Message::Auditioning);
}

/// Plays [`sampler::SCRUB`] from `at` once, faded so grains do not click.
/// No message: a drag sends one a frame, which would hide the range's.
fn scrub(f: &mut impl Frontend, at: Duration) {
    let rate = match f.session().playing_track() {
        Ok((_, rate)) => rate,
        Err(refusal) => return f.notify(refusal.into()),
    };
    let frames = |d: Duration| sampler::frame_of(d, rate);
    let start = frames(at);
    let fade = frames(sampler::SCRUB_FADE);
    f.session().send(Cmd::PlayOnce(
        start,
        start + frames(sampler::SCRUB),
        (fade, fade),
    ));
}

/// `at`, moved to the nearest zero crossing when the sampler view shows with
/// snap on and its waveform read; otherwise as it is.
fn snapped(f: &impl Frontend, at: Duration) -> Duration {
    if f.view() != View::Sampler || !f.sampler().snap {
        return at;
    }
    match peaks(f) {
        Some(peaks) => sampler::time_of(
            sampler::snap(&peaks, sampler::frame_of(at, peaks.rate)),
            peaks.rate,
        ),
        None => at,
    }
}

/// Keeps a running loop on the range as it changes; a range cleared or left
/// with one end stops it.
fn follow_loop(f: &impl Frontend) {
    let status = f.session().player().status();
    if status.looping.is_some() {
        let range = f.sampler().range(status.current());
        f.session().send(Cmd::Loop(range));
    }
}

/// Sets the range to the region between the marks around the playhead.
fn range_to_region(f: &mut impl Frontend) -> Result<(), Message> {
    let status = f.session().player().status();
    let path = status.current().cloned().ok_or(Refusal::NothingPlaying)?;
    let peaks = peaks(f).ok_or(Message::NoWaveform)?;
    let at = sampler::frame_of(f.session().player().position(), peaks.rate);
    let marks: Vec<u64> = (f.session_mut().marks_for(Some(&path)).iter())
        .map(|m| m.frame)
        .collect();
    let (start, end) = playr_core::samples::region(&marks, at);
    let end = end.unwrap_or(peaks.frames);
    if end <= start {
        return Err(Message::EmptyRange);
    }
    f.sampler_mut().range = Some(sampler::Range {
        path,
        start: Some(start),
        end: Some(end),
    });
    Ok(())
}

/// Recalls loop `slot` into the range and loops it, or saves the range to
/// the slot, or empties it.
fn loop_slot(f: &mut impl Frontend, slot: u8, op: crate::action::SlotOp) {
    use crate::action::SlotOp;
    let (path, rate) = match f.session().playing_track() {
        Ok(track) => track,
        Err(refusal) => return f.notify(refusal.into()),
    };
    let saved = f.session_mut().loops_for(Some(&path))[usize::from(slot) - 1];
    match (op, saved) {
        (SlotOp::Use, Some((start, end))) => {
            f.sampler_mut().range = Some(sampler::Range {
                path,
                start: Some(start),
                end: Some(end),
            });
            act(Action::Loop(Some(true)), f);
            f.notify(Message::LoopRecalled {
                slot,
                start,
                end,
                rate,
            });
        }
        (SlotOp::Use | SlotOp::Save, _) => match f.sampler().range(Some(&path)) {
            Some(range) => {
                let notice = f.session_mut().save_loop(slot, range);
                f.notify(notice.into());
            }
            None => f.notify(Message::NoRangeToSave(slot)),
        },
        (SlotOp::Clear, _) => {
            let notice = f.session_mut().clear_loop(slot);
            f.notify(notice.into());
        }
    }
}

/// Moves the range's ends set on the playing track to the nearest zero
/// crossings, as ends set with snap on would be. A range too short to keep
/// both ends apart stays as it is.
fn snap_range(f: &mut impl Frontend) {
    let Some(peaks) = peaks(f) else { return };
    let Some(path) = f.session().player().status().current().cloned() else {
        return;
    };
    let (start, end) = f.sampler().range_ends(Some(&path));
    let snap = |e: Option<u64>| e.map(|e| sampler::snap(&peaks, e));
    let (start, end) = (snap(start), snap(end));
    if start.zip(end).is_some_and(|(a, b)| a >= b) || (start, end) == (None, None) {
        return;
    }
    f.sampler_mut().range = Some(sampler::Range { path, start, end });
    follow_loop(f);
}

/// Marks `at`, or the position now. In the sampler view the mark may snap,
/// and may fall as close as a frame to another, where fine cuts need it.
///
/// The new mark is selected.
fn mark(f: &mut impl Frontend, at: Option<Duration>) {
    let at = at.unwrap_or_else(|| f.session().player().position());
    let notice = if f.view() == View::Sampler {
        let at = snapped(f, at);
        f.session_mut().add_mark_within(Some(at), Duration::ZERO)
    } else {
        f.session_mut().add_mark(Some(at))
    };
    if let (Notice::Done(Outcome::Marked { at, .. }), Ok((path, rate))) =
        (&notice, f.session().playing_track())
    {
        let frame = sampler::frame_of(*at, rate);
        f.sampler_mut().selected = Some((path, Selected::Mark(frame)));
    }
    f.notify(notice.into());
}

/// The selection on the playing track.
fn selected(f: &impl Frontend) -> Option<(PathBuf, Selected)> {
    let path = f.session().player().status().current().cloned()?;
    let selected = f.sampler().selected(Some(&path))?;
    Some((path, selected))
}

/// The playing track's marks, in frames.
fn mark_frames(f: &mut impl Frontend, path: &PathBuf) -> Vec<u64> {
    (f.session_mut().marks_for(Some(path)).iter())
        .map(|m| m.frame)
        .collect()
}

/// Selects the next mark after the selected one, or the previous; from the
/// playhead with none selected. Plays it once, up to the mark after it.
fn select_mark(f: &mut impl Frontend, forward: bool) {
    let Some((path, peaks, at)) = hearing(f) else {
        return;
    };
    let from = f.sampler().selected_mark(Some(&path)).unwrap_or(at);
    let marks = mark_frames(f, &path);
    let next = marks
        .iter()
        .copied()
        .filter(|&m| if forward { m > from } else { m < from })
        .min_by_key(|&m| m.abs_diff(from));
    let Some(frame) = next else {
        return f.notify(if forward {
            Refusal::NoLaterMark.into()
        } else {
            Refusal::NoEarlierMark.into()
        });
    };
    f.sampler_mut().selected = Some((path.clone(), Selected::Mark(frame)));
    let end = marks
        .into_iter()
        .find(|&m| m > frame)
        .unwrap_or(peaks.frames);
    play_once(f, path, (frame, end), peaks.rate);
}

/// Moves the selected mark from `from` to `to`, keeping it selected.
fn move_selected_mark(f: &mut impl Frontend, from: u64, to: u64) {
    let notice = f.session_mut().move_mark(from, to);
    if let (Notice::Done(Outcome::MarkMoved { .. }), Some(path)) =
        (&notice, f.session().player().status().current().cloned())
    {
        f.sampler_mut().selected = Some((path, Selected::Mark(to)));
    }
    f.notify(notice.into());
}

/// The planned slices on the playing track, and the index of the one
/// starting at `start`.
fn planned_slice(f: &impl Frontend, start: u64) -> Option<(Plan, usize)> {
    let path = f.session().player().status().current().cloned()?;
    let plan = f.sampler().pending.clone().filter(|p| p.job.path == path)?;
    let i = plan.spans.iter().position(|s| s.0 == start)?;
    Some((plan, i))
}

/// Shows `plan` with its slices starting at `starts`, kept on its job so a
/// plan made again keeps them, and the slice starting at `selected` selected.
fn edit_plan(f: &mut impl Frontend, mut plan: Plan, starts: Vec<u64>, selected: Option<u64>) {
    let end = plan.spans.last().and_then(|s| s.1);
    plan.spans = playr_core::samples::spans_from(&starts, end);
    plan.job.cuts = Some(starts);
    let path = plan.job.path.clone();
    f.sampler_mut().pending = Some(plan);
    f.sampler_mut().selected = selected.map(|s| (path, Selected::Slice(s)));
}

/// Moves the start of the planned slice starting at `from` to `to`, which
/// moves the end of the slice before it too. It stays a frame inside its
/// neighbours, and the first slice's start stays inside the range or region.
pub fn move_slice(f: &mut impl Frontend, from: u64, to: u64) {
    let Some((plan, i)) = planned_slice(f, from) else {
        return f.notify(Message::NothingSelected);
    };
    let lowest = match i {
        0 => playr_core::samples::extent(&plan.job).0,
        _ => plan.spans[i - 1].0 + 1,
    };
    let highest = (plan.spans[i].1).map_or(u64::MAX, |end| end.saturating_sub(1));
    let to = to.clamp(lowest, highest.max(lowest));
    let rate = plan.job.rate;
    let mut starts: Vec<u64> = plan.spans.iter().map(|s| s.0).collect();
    starts[i] = to;
    edit_plan(f, plan, starts, Some(to));
    f.notify(Message::SliceMoved {
        slice: i + 1,
        at: to,
        rate,
    });
}

/// Joins the planned slice starting at `start` to the one before it. The
/// first slice has none before it, so its start stays.
fn remove_slice(f: &mut impl Frontend, start: u64) {
    let Some((plan, i)) = planned_slice(f, start) else {
        return f.notify(Message::NothingSelected);
    };
    if i == 0 {
        return f.notify(Message::FirstSlice);
    }
    let mut starts: Vec<u64> = plan.spans.iter().map(|s| s.0).collect();
    starts.remove(i);
    edit_plan(f, plan, starts, None);
    f.notify(Message::SlicesJoined { slice: i });
}

/// Moves `edge` of the range, as a nudge moves the playhead.
fn move_edge(f: &mut impl Frontend, edge: sampler::Edge, nudge: crate::action::Nudge) {
    let status = f.session().player().status();
    let Some(path) = status.current().cloned() else {
        return f.notify(Refusal::NothingPlaying.into());
    };
    let (Some(peaks), Some(scale)) = (peaks(f), f.sampler().scale) else {
        return f.notify(Message::NoWaveform);
    };
    let (start, end) = f.sampler().range_ends(Some(&path));
    let from = match edge {
        sampler::Edge::Start => start,
        sampler::Edge::End => end,
    };
    let Some(from) = from else {
        return f.notify(Message::NoEdge(edge));
    };
    let to = sampler::nudge(&peaks, from, scale.frames(nudge), f.sampler().snap);
    set_edge(f, &path, edge, to);
    let (start, end) = f.sampler().range_ends(Some(&path));
    f.notify(Message::Range {
        start,
        end,
        rate: peaks.rate,
    });
}

/// Puts `edge` of the range on `path` at `to`, a frame short of the other
/// end so the range stays, and keeps a running loop on it.
pub fn set_edge(f: &mut impl Frontend, path: &PathBuf, edge: sampler::Edge, to: u64) {
    let (start, end) = f.sampler().range_ends(Some(path));
    match edge {
        sampler::Edge::Start => {
            let to = end.map_or(to, |e| to.min(e.saturating_sub(1)));
            f.sampler_mut().set_range_start(path, to);
        }
        sampler::Edge::End => {
            let to = start.map_or(to, |s| to.max(s + 1));
            f.sampler_mut().set_range_end(path, to);
        }
    }
    follow_loop(f);
}

/// The playing track's marks and range, before an edit.
pub fn before(f: &mut impl Frontend) -> Option<Before> {
    let path = f.session().player().status().current().cloned()?;
    let marks = mark_frames(f, &path);
    let range = f.sampler().range.clone().filter(|r| r.path == path);
    let plan = f.sampler().pending.clone().filter(|p| p.job.path == path);
    let selected = f.sampler().selected(Some(&path));
    Some(Before {
        path,
        marks,
        range,
        plan,
        selected,
    })
}

/// Whether `a` and `b` are the same cut of the same track, with or without
/// slice starts set by hand.
fn same_cut(a: &Plan, b: &Plan) -> bool {
    let cut = |p: &Plan| Job {
        cuts: None,
        ..p.job.clone()
    };
    cut(a) == cut(b)
}

/// After an action: keeps `before` for undo if the action changed the marks
/// or range, or edited the planned slices, and drops a selection that is gone.
///
/// A plan made, replaced or discarded is not an edit: undo would put back a
/// cut that a later one replaced.
pub fn settle(f: &mut impl Frontend, before: Option<Before>) {
    let after = self::before(f);
    if let (Some(before), Some(after)) = (before, &after) {
        let edited = match (&before.plan, &after.plan) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        };
        let changed = before.marks != after.marks || before.range != after.range || edited;
        if before.path == after.path && changed {
            keep(&mut f.sampler_mut().history, before);
            f.sampler_mut().future.clear();
        }
    }
    let gone = match (f.sampler().selected.clone(), &after) {
        (None, _) => false,
        (Some((path, selected)), Some(after)) if path == after.path => match selected {
            Selected::Mark(frame) => !after.marks.contains(&frame),
            Selected::Edge(edge) => {
                let r = after.range.as_ref();
                match edge {
                    sampler::Edge::Start => r.and_then(|r| r.start).is_none(),
                    sampler::Edge::End => r.and_then(|r| r.end).is_none(),
                }
            }
            // Planned again, the new plan checks it when it lands.
            Selected::Slice(start) => match &after.plan {
                Some(plan) => !plan.spans.iter().any(|s| s.0 == start),
                None => f.sampler().planning.is_none(),
            },
        },
        _ => true,
    };
    if gone {
        f.sampler_mut().selected = None;
    }
}

/// Pushes `state` onto an undo or redo stack, dropping the oldest past
/// [`sampler::UNDO_DEPTH`].
fn keep(stack: &mut Vec<Before>, state: Before) {
    stack.push(state);
    let over = stack.len().saturating_sub(sampler::UNDO_DEPTH);
    stack.drain(..over);
}

/// Puts back the marks and range as they were before the last edit, keeping
/// the state now for redo; or, with `redo`, the other way round.
fn undo(f: &mut impl Frontend, redo: bool) {
    let (none, done) = match redo {
        false => (Message::NothingToUndo, Message::Undone),
        true => (Message::NothingToRedo, Message::Redone),
    };
    let (path, rate) = match f.session().playing_track() {
        Ok(track) => track,
        Err(refusal) => return f.notify(refusal.into()),
    };
    let Some(then) = stacks(f.sampler_mut(), redo).0.pop() else {
        return f.notify(none);
    };
    if then.path != path {
        f.sampler_mut().history.clear();
        f.sampler_mut().future.clear();
        return f.notify(none);
    }
    let now = before(f).expect("a track is playing");
    if let Err(notice) = restore(f, &then, rate) {
        // Kept, so the step can be tried again; marks put back so far stay.
        stacks(f.sampler_mut(), redo).0.push(then);
        return f.notify(notice.into());
    }
    keep(stacks(f.sampler_mut(), redo).1, now);
    settle(f, None);
    f.notify(done);
}

/// The stack an undo takes from and the one it keeps the state now on; the
/// other way round for a redo.
fn stacks(s: &mut Sampler, redo: bool) -> (&mut Vec<Before>, &mut Vec<Before>) {
    match redo {
        false => (&mut s.history, &mut s.future),
        true => (&mut s.future, &mut s.history),
    }
}

/// Makes the playing track's marks and range `state`'s.
fn restore(f: &mut impl Frontend, state: &Before, rate: u32) -> Result<(), Notice> {
    let now = mark_frames(f, &state.path);
    for &frame in now.iter().filter(|m| !state.marks.contains(m)) {
        if let notice @ Notice::Failed { .. } = f.session_mut().remove_mark(frame) {
            return Err(notice);
        }
    }
    for &frame in state.marks.iter().filter(|m| !now.contains(m)) {
        let at = sampler::time_of(frame, rate);
        if let notice @ Notice::Failed { .. } =
            f.session_mut().add_mark_within(Some(at), Duration::ZERO)
        {
            return Err(notice);
        }
    }
    f.sampler_mut().range = state.range.clone();
    follow_loop(f);
    // The plan shown, if it is still the cut `state` edited.
    let plan = f.sampler_mut().pending.take();
    f.sampler_mut().pending = match (plan, &state.plan) {
        (Some(now), Some(then)) if same_cut(&now, then) => Some(then.clone()),
        (plan, _) => plan,
    };
    f.sampler_mut().selected = state.selected.map(|s| (state.path.clone(), s));
    Ok(())
}

fn slice(f: &mut impl Frontend, slicing: Slicing) {
    let range = {
        let status = f.session().player().status();
        f.sampler().range(status.current())
    };
    let cut = match slicing {
        Slicing::Region => Cut::Region,
        Slicing::Marks => Cut::Marks,
        Slicing::Equal(n) => Cut::Equal(n),
        Slicing::Onsets(s) => Cut::Onsets(s.unwrap_or(f.onset_sensitivity())),
    };
    if f.view() == View::Sampler {
        // A slider being dragged asks every frame; a sensitivity that comes
        // while a plan is being made waits for it, and only the last is planned.
        if let (Cut::Onsets(s), Some(_)) = (cut, f.sampler().planning) {
            f.sampler_mut().onsets_wanted = Some(s);
            return;
        }
        f.sampler_mut().onsets_wanted = None;
        match f.session_mut().plan_slices(cut, range) {
            Ok(job) => {
                f.planning(job);
                f.notify(Outcome::PlanStarted.into());
            }
            Err(refusal) => f.notify(refusal.into()),
        }
    } else {
        match f.session_mut().export(cut, range) {
            Ok(_) => f.notify(Outcome::ExportStarted.into()),
            Err(refusal) => f.notify(refusal.into()),
        }
    }
}
