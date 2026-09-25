//! Doing an action: the part of every key and `:` command a frontend shares.
//!
//! A frontend implements [`Frontend`] over its own state: which view shows,
//! where each view's cursor is, what the library view lists, and how it
//! prompts and asks. [`dispatch`] then does any [`Action`] the same way in
//! every frontend, calling the session for what changes the library or
//! playback and the frontend for what changes only the interface.

use std::path::PathBuf;
use std::time::Duration;

use playr_core::audio::Cmd;
use playr_core::db::query::Playlist;
use playr_core::db::Track;
use playr_core::event::JobId;
use playr_core::notice::{Notice, Outcome, Refusal};
use playr_core::samples::{Cut, Plan};
use playr_core::session::Session;
use playr_core::wave::Peaks;

use crate::action::{Action, Keymap, Slicing, Zoom};
use crate::message::Message;
use crate::sampler::{self, Sampler};
use crate::{Display, Theme, View};

/// A destructive action held until the listener confirms it.
#[derive(Debug, Clone, PartialEq)]
pub enum Confirm {
    DeletePlaylist(Playlist),
    /// Overwrite the playlist of this name with the selection.
    ReplacePlaylist(String),
    /// Empty the selection, which holds this many tracks.
    ClearSelection(usize),
    /// Remove the tracks and marks under this directory whose files are gone,
    /// or under every recorded root when `None`.
    Prune(Option<PathBuf>),
    /// Forget this root, and every track and mark under it.
    ForgetRoot(PathBuf),
    /// Take up this track again, at the position playr closed on.
    Resume {
        path: PathBuf,
        at: Duration,
    },
    /// Remove `count` marks from the track at `path`, which was playing when asked.
    ClearMarks {
        path: PathBuf,
        count: usize,
    },
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
            Confirm::Resume { path, at } => {
                let name = path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy();
                format!("take up {name} again at {}?", crate::message::fmt_time(*at))
            }
            Confirm::ClearMarks { path, count } => {
                let name = path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy();
                format!("clear all {count} marks from {name}?")
            }
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
            if status.state != playr_core::audio::State::Playing {
                f.session().send(Cmd::TogglePause);
            }
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
        Action::CycleMode(forward) => {
            let notice = f.session().cycle_mode(forward);
            f.notify(notice.into());
        }
        Action::SetMode(mode) => {
            let notice = f.session().set_mode(mode);
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
        Action::MoveCursor(nudge) => match (peaks(f), f.sampler().scale) {
            (Some(peaks), Some(scale)) => {
                let at = cursor_frame(f, peaks.rate);
                let to = sampler::nudge(&peaks, at, scale.frames(nudge), f.sampler().snap);
                f.sampler_mut().cursor = Some(to);
            }
            _ => f.notify(Message::NoWaveform),
        },
        Action::SetCursor(at) => match (at, peaks(f)) {
            (None, _) => f.sampler_mut().cursor = None,
            (Some(at), Some(peaks)) => {
                f.sampler_mut().cursor = Some(sampler::frame_of(at, peaks.rate))
            }
            (Some(_), None) => f.notify(Message::NoWaveform),
        },
        Action::PickMark(forward) => match peaks(f) {
            Some(peaks) => {
                let at = cursor_frame(f, peaks.rate);
                let Some(path) = f.session().player().status().current().cloned() else {
                    return f.notify(Refusal::NothingPlaying.into());
                };
                let next = f
                    .session_mut()
                    .marks_for(Some(&path))
                    .iter()
                    .map(|m| m.frame)
                    .filter(|&m| if forward { m > at } else { m < at })
                    .min_by_key(|&m| m.abs_diff(at));
                match next {
                    Some(frame) => f.sampler_mut().cursor = Some(frame),
                    None => f.notify(if forward {
                        Refusal::NoLaterMark.into()
                    } else {
                        Refusal::NoEarlierMark.into()
                    }),
                }
            }
            None => f.notify(Message::NoWaveform),
        },
        Action::MoveMark(nudge) => match (peaks(f), f.sampler().scale) {
            (Some(peaks), Some(scale)) => {
                let at = cursor_frame(f, peaks.rate);
                let Some(mark) = f.session_mut().mark_near(at, near(&peaks, Some(scale))) else {
                    return f.notify(Refusal::NoMarkHere.into());
                };
                let to = sampler::nudge(&peaks, mark.frame, scale.frames(nudge), f.sampler().snap);
                let notice = f.session_mut().move_mark(mark.frame, to);
                if matches!(notice, Notice::Done(Outcome::MarkMoved { .. })) {
                    f.sampler_mut().cursor = Some(to);
                }
                f.notify(notice.into());
            }
            _ => f.notify(Message::NoWaveform),
        },
        Action::MoveMarkTo(to) => match peaks(f) {
            Some(peaks) => {
                let at = cursor_frame(f, peaks.rate);
                let within = near(&peaks, f.sampler().scale);
                let Some(mark) = f.session_mut().mark_near(at, within) else {
                    return f.notify(Refusal::NoMarkHere.into());
                };
                let to = sampler::frame_of(snapped(f, to), peaks.rate);
                let notice = f.session_mut().move_mark(mark.frame, to);
                if matches!(notice, Notice::Done(Outcome::MarkMoved { .. })) {
                    f.sampler_mut().cursor = Some(to);
                }
                f.notify(notice.into());
            }
            None => f.notify(Message::NoWaveform),
        },
        Action::SnapMark => match peaks(f) {
            Some(peaks) => {
                let at = cursor_frame(f, peaks.rate);
                let within = near(&peaks, f.sampler().scale);
                let Some(mark) = f.session_mut().mark_near(at, within) else {
                    return f.notify(Refusal::NoMarkHere.into());
                };
                let sensitivity = f.onset_sensitivity();
                match f.session_mut().snap_mark(mark.frame, sensitivity) {
                    Ok(_) => f.notify(Message::Snapping),
                    Err(refusal) => f.notify(refusal.into()),
                }
            }
            None => f.notify(Message::NoWaveform),
        },
        Action::DeleteMark => match peaks(f) {
            Some(peaks) => {
                let at = cursor_frame(f, peaks.rate);
                let within = near(&peaks, f.sampler().scale);
                match f.session_mut().mark_near(at, within) {
                    Some(mark) => {
                        let notice = f.session_mut().remove_mark(mark.frame);
                        f.notify(notice.into());
                    }
                    None => f.notify(Refusal::NoMarkHere.into()),
                }
            }
            None => f.notify(Message::NoWaveform),
        },
        Action::Loop(on) => {
            let status = f.session().player().status();
            let on = on.unwrap_or(status.looping.is_none());
            if !on {
                f.session().send(Cmd::Loop(None));
                return f.notify(Message::Loop(false));
            }
            let Some(range) = f.sampler().range(status.current()) else {
                return f.notify(Message::NoRangeToLoop);
            };
            f.session().send(Cmd::Loop(Some(range)));
            // Looping is for hearing the range, so a paused or stopped track plays.
            if status.state != playr_core::audio::State::Playing {
                f.session().send(Cmd::TogglePause);
            }
            f.notify(Message::Loop(true));
        }
        Action::PickEdge(edge) => {
            f.sampler_mut().edge = edge;
            f.sampler_mut().fit_edge = true;
            f.notify(Message::Edge(edge));
        }
        Action::MoveEdge(nudge) => {
            let status = f.session().player().status();
            let Some(path) = status.current().cloned() else {
                return f.notify(Refusal::NothingPlaying.into());
            };
            let (Some(peaks), Some(scale)) = (peaks(f), f.sampler().scale) else {
                return f.notify(Message::NoWaveform);
            };
            let edge = f.sampler().edge;
            let (start, end) = f.sampler().range_ends(Some(&path));
            let from = match edge {
                sampler::Edge::Start => start,
                sampler::Edge::End => end,
            };
            let Some(from) = from else {
                return f.notify(Message::NoEdge(edge));
            };
            let to = sampler::nudge(&peaks, from, scale.frames(nudge), f.sampler().snap);
            // An end stops a frame short of the other, so the range stays.
            match edge {
                sampler::Edge::Start => {
                    let to = end.map_or(to, |e| to.min(e.saturating_sub(1)));
                    f.sampler_mut().set_range_start(&path, to);
                }
                sampler::Edge::End => {
                    let to = start.map_or(to, |s| to.max(s + 1));
                    f.sampler_mut().set_range_end(&path, to);
                }
            }
            follow_loop(f);
            let (start, end) = f.sampler().range_ends(Some(&path));
            f.notify(Message::Range {
                start,
                end,
                rate: peaks.rate,
            });
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
        // Escape backs out a step: planned slices first, then the range.
        Action::DiscardSlices => match f.take_plan() {
            Some(_) => f.notify(Message::SlicesDiscarded),
            None if f.sampler().range.is_some() => dispatch(Action::SetRange(None), f),
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
        Confirm::ClearMarks { path, .. } => {
            if let Some(notice) = f.session_mut().clear_marks(&path) {
                f.notify(notice.into());
            }
        }
        Confirm::Prune(dir) => match f.session_mut().prune(dir.clone()) {
            Ok(_) => f.notify(Outcome::PruneStarted { dir }.into()),
            Err(refusal) => f.notify(refusal.into()),
        },
        Confirm::ForgetRoot(dir) => match f.session_mut().forget_root(&dir) {
            Ok(removed) => f.notify(Outcome::Forgot { dir, removed }.into()),
            Err(refusal) => f.notify(refusal.into()),
        },
        Confirm::Resume { path, at } => f.session_mut().resume(path, at),
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
/// The playing track's peaks, once the sampler view has read them.
fn peaks(f: &impl Frontend) -> Option<std::sync::Arc<Peaks>> {
    let status = f.session().player().status();
    sampler::peaks_of(f.sampler(), status.current()).ok()
}

/// The frame the sampler points at: its cursor, or the playhead when the
/// cursor is following it.
fn cursor_frame(f: &impl Frontend, rate: u32) -> u64 {
    let playhead = sampler::frame_of(f.session().player().position(), rate);
    f.sampler().cursor.unwrap_or(playhead)
}

/// How far from the cursor a mark still counts as under it: one column of the
/// view, so what looks like a hit is one, or 10 ms before anything is drawn.
fn near(peaks: &Peaks, scale: Option<sampler::Scale>) -> u64 {
    match scale {
        Some(scale) => scale.per_column.max(1),
        None => (peaks.rate / 100).max(1) as u64,
    }
}

/// Plays once, and pauses at the end of, whichever of these has both ends:
/// the range, the planned slice the playhead is in, or the region around it.
///
/// The range first, because setting one is how a listener says what they mean;
/// then the plan, which is what `:slice` is about to write; then the region,
/// which is what `:slice region` would take. The playhead is the sampler's
/// cursor, so this is the span under the cursor in each case.
fn audition(f: &mut impl Frontend) {
    let status = f.session().player().status();
    let Some(path) = status.current().cloned() else {
        return f.notify(Refusal::NothingPlaying.into());
    };
    let Some(peaks) = peaks(f) else {
        return f.notify(Message::NoWaveform);
    };
    let (rate, last) = (peaks.rate, peaks.frames);
    let at = sampler::frame_of(f.session().player().position(), rate);
    // A span with no end runs to the end of the track.
    let ends = |end: Option<u64>| end.unwrap_or(last);

    // An audition pauses on its span's end, which is the next span's start.
    let again = f
        .sampler()
        .auditioned
        .as_ref()
        .filter(|(p, _, end)| *p == path && at.abs_diff(*end) <= u64::from(rate) / 100)
        .map(|&(_, start, end)| (start, end));
    let span = match f.sampler().range(Some(&path)).or(again) {
        Some(span) => Some(span),
        None => match f.sampler().pending.as_ref().and_then(|plan| {
            plan.spans
                .iter()
                .filter(|(start, _)| *start <= at)
                .max_by_key(|(start, _)| *start)
                .copied()
        }) {
            Some((start, end)) => Some((start, ends(end))),
            None => {
                let marks: Vec<u64> = f
                    .session_mut()
                    .marks_for(Some(&path))
                    .iter()
                    .map(|m| m.frame)
                    .collect();
                let (start, end) = playr_core::samples::region(&marks, at);
                Some((start, ends(end)))
            }
        },
    };
    match span {
        Some((start, end)) if end > start => {
            f.session().send(Cmd::PlayOnce(start, end));
            f.sampler_mut().auditioned = Some((path, start, end));
            f.notify(Message::Auditioning);
        }
        _ => f.notify(Message::NothingToAudition),
    }
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
fn mark(f: &mut impl Frontend, at: Option<Duration>) {
    let notice = if f.view() == View::Sampler {
        let at = at.unwrap_or_else(|| f.session().player().position());
        let at = snapped(f, at);
        f.session_mut().add_mark_within(Some(at), Duration::ZERO)
    } else {
        f.session_mut().add_mark(at)
    };
    f.notify(notice.into());
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
