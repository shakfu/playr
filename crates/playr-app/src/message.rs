//! Messages about what an action did: as data, and in words.
//!
//! A core operation reports a [`Notice`]; the rest are about the interface
//! itself: prompts, key bindings and views. [`text`] is the only place either
//! is turned into words, so every frontend says the same thing, and tests
//! assert on messages while one test checks the wording of each.

use std::path::Path;
use std::time::Duration;

use playr_core::notice::{Notice, Outcome, Refusal, Task};

use crate::action::{Action, Key};
use crate::command;
use crate::{Display, Theme, View};
use playr_core::analysis::{self, tempo, Analysis, Finding, Md5};
use playr_core::columns::{Column, SortKey};
use playr_core::db::Track;
use playr_core::gain::Gains;

/// A message for the person using the interface.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Core(Notice),
    /// A confirmation was answered with anything but `y`.
    Cancelled,
    NoMatches,
    NoPlaylistUnderCursor,
    /// `:enqueue all` with no search results or selection to queue.
    NothingToQueue,
    /// `:clear` in the Queue view, with nothing in the queue.
    QueueEmpty,
    Display(Display),
    Theme(Theme),
    /// The tone control's gains in dB, by band.
    Eq([f32; 3]),
    /// The columns a list shows, as `:columns` set them.
    Columns(Vec<Column>),
    /// What lists are sorted by, as `:sort` set it.
    Sorted(Vec<SortKey>),
    /// A nudge came before the sampler view showed a waveform.
    NoWaveform,
    Snap(bool),
    Fit(bool),
    /// The sampler's range, as set, in frames at `rate`.
    Range {
        start: Option<u64>,
        end: Option<u64>,
        rate: u32,
    },
    EmptyRange,
    Loop(bool),
    /// Loop `slot` recalled into the range, in frames at `rate`.
    LoopRecalled {
        slot: u8,
        start: u64,
        end: u64,
        rate: u32,
    },
    /// An empty loop slot, or a save, with no range to save.
    NoRangeToSave(u8),
    /// A loop was asked for with only one end of the range set.
    NoRangeToLoop,
    /// Nothing under the playhead has both ends, so there is nothing to hear.
    NothingToAudition,
    /// A span is playing once, and will pause at its end.
    Auditioning,
    /// The sound around a mark is being read, to find the rise to snap to.
    Snapping,
    /// This end of the range is selected.
    Edge(crate::sampler::Edge),
    /// An edge move found no such end of the range to move.
    NoEdge(crate::sampler::Edge),
    /// An edit key with nothing selected to act on.
    NothingSelected,
    Deselected,
    Undone,
    NothingToUndo,
    Redone,
    NothingToRedo,
    /// Planned slice `slice`, from 1, now starts at frame `at` of `rate`.
    SliceMoved {
        slice: usize,
        at: u64,
        rate: u32,
    },
    /// Planned slice `slice + 1` was joined to slice `slice`, from 1.
    SlicesJoined {
        slice: usize,
    },
    /// The first planned slice's start was asked to go.
    FirstSlice,
    /// No planned slice starts where one was asked for.
    NoSliceHere,
    /// A `map` command took effect; holds the `Action::Map`.
    Mapped(Action),
    Unmapped(Key),
    NotBound {
        key: Key,
        view: Option<View>,
    },
    NoSlicesPlanned,
    SlicesDiscarded,
    /// A `:` command could not be parsed or run; the parser's own words.
    Command(String),
    /// A `:sql` statement is running on its own thread.
    Querying,
    /// A `:sql` statement named this many library tracks.
    Found(usize),
    /// Saved searches are not renamed in place yet.
    SearchNotRenamed,
    /// Playback will stop once the track playing ends, or no longer will.
    StopAfter(bool),
    /// The sleep timer was set to this, or turned off.
    StopIn(Option<Duration>),
    /// The sleep timer ran out and stopped playback.
    Slept,
    Tape(crate::tape::TapeMessage),
    Dj(crate::dj::DjMessage),
}

impl From<Notice> for Message {
    fn from(notice: Notice) -> Self {
        Message::Core(notice)
    }
}

impl From<Outcome> for Message {
    fn from(outcome: Outcome) -> Self {
        Message::Core(Notice::Done(outcome))
    }
}

impl From<Refusal> for Message {
    fn from(refusal: Refusal) -> Self {
        Message::Core(Notice::Refused(refusal))
    }
}

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
                Task::Loop => "could not save the loop",
                Task::MoveMark => "could not move mark",
                Task::RemoveMark => "could not remove mark",
                Task::ClearMarks => "could not clear marks",
                Task::Slice => "slicing failed",
                Task::Export => "export failed",
                Task::Convert => "conversion failed",
                Task::Scan => "scan failed",
                Task::Analyze => "analysis failed",
                Task::Prune => "prune failed",
                Task::Open => "could not open",
                Task::FixTempo => "could not correct the tempo",
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
        Message::NothingToQueue => {
            "nothing to queue; :enqueue all queues search results or the selection".into()
        }
        Message::QueueEmpty => "the queue is empty".into(),
        Message::Querying => "running the query...".into(),
        Message::Found(1) => "1 track".into(),
        Message::Found(n) => format!("{n} tracks"),
        Message::SearchNotRenamed => {
            "save it again under the new name, then delete the old one".into()
        }
        Message::StopAfter(true) => "stopping after this track".into(),
        Message::StopAfter(false) => "playing on after this track".into(),
        Message::StopIn(Some(d)) => format!("stopping in {}", fmt_time(*d)),
        Message::StopIn(None) => "sleep timer off".into(),
        Message::Slept => "sleep timer ran out; stopped".into(),
        Message::Tape(m) => tape_text(m),
        Message::Dj(m) => dj_text(m),
        Message::NoPlaylistUnderCursor => {
            "no playlist under the cursor in the playlists view".into()
        }
        Message::Display(display) => format!("display: {}", display.name()),
        Message::Theme(theme) => format!("theme: {}", theme.name()),
        Message::Eq(gains) => eq(*gains),
        Message::Columns(columns) => {
            let names: Vec<&str> = columns.iter().map(|c| c.name()).collect();
            format!("columns: {}", names.join(", "))
        }
        Message::Sorted(keys) if keys.is_empty() => "sorted: as the library was scanned".into(),
        Message::Sorted(keys) => {
            let names: Vec<String> = keys.iter().map(|k| k.text()).collect();
            format!("sorted by {}", names.join(", then "))
        }
        Message::NoWaveform => "no waveform to move along yet".into(),
        Message::Snap(on) => format!("snap to zero crossings: {}", if *on { "on" } else { "off" }),
        Message::Fit(on) => format!("fit the range: {}", if *on { "on" } else { "off" }),
        Message::Range { start, end, rate } => {
            let at = |f: &u64| crate::sampler::fmt_frames(*f, *rate);
            match (start, end) {
                (Some(a), Some(b)) => format!(
                    "range {}-{} ({:.3} s)",
                    at(a),
                    at(b),
                    (b - a) as f64 / *rate as f64
                ),
                (Some(a), None) => format!("range from {}", at(a)),
                (None, Some(b)) => format!("range to {}", at(b)),
                (None, None) => "range cleared".into(),
            }
        }
        Message::EmptyRange => "the range is empty".into(),
        Message::Loop(on) => format!("loop: {}", if *on { "on" } else { "off" }),
        Message::LoopRecalled {
            slot,
            start,
            end,
            rate,
        } => format!(
            "loop {slot}: {}-{}",
            crate::sampler::fmt_frames(*start, *rate),
            crate::sampler::fmt_frames(*end, *rate)
        ),
        Message::NoRangeToSave(slot) => {
            format!("no range to save as loop {slot}: set one with i and o, or drag")
        }
        Message::NoRangeToLoop => "the range has one end: set the other with i or o".into(),
        Message::NothingToAudition => "nothing to hear here: set a range, or mark one".into(),
        Message::Auditioning => "playing once".into(),
        Message::Snapping => "looking for the nearest rise".into(),
        Message::Edge(edge) => format!("selected the range {}", edge.name()),
        Message::NoEdge(edge) => format!("no range {} to select: set it with i or o", edge.name()),
        Message::NothingSelected => {
            "nothing selected: select a mark with { }, a range end with [ ], a slice with , ."
                .into()
        }
        Message::Deselected => "selected nothing".into(),
        Message::Undone => "undone".into(),
        Message::NothingToUndo => "nothing to undo".into(),
        Message::Redone => "redone".into(),
        Message::NothingToRedo => "nothing to redo".into(),
        Message::SliceMoved { slice, at, rate } => format!(
            "slice {slice} starts at {}",
            crate::sampler::fmt_frames(*at, *rate)
        ),
        Message::SlicesJoined { slice } => format!("joined slice {} to slice {slice}", slice + 1),
        Message::FirstSlice => "the first slice has none before it to join".into(),
        Message::NoSliceHere => "no planned slice starts here".into(),
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
        Outcome::Queued { tracks: 1, next } => match next {
            true => "plays next".into(),
            false => "queued".into(),
        },
        Outcome::Editing { name } => format!("editing \"{name}\"; s saves it"),
        Outcome::SearchSaved { name } => format!("saved search \"{name}\""),
        Outcome::DraftOverwritten => "the old draft gives way to the selection".into(),
        Outcome::DraftAppended => "the old draft is back in the selection".into(),
        Outcome::QueueCleared { tracks: 1 } => "took 1 track out of the queue".into(),
        Outcome::QueueCleared { tracks } => format!("took {tracks} tracks out of the queue"),
        Outcome::QueueReplaced { tracks: 1 } => "queue replaced; 1 track was waiting".into(),
        Outcome::QueueReplaced { tracks } => {
            format!("queue replaced; {tracks} tracks were waiting")
        }
        Outcome::Queued { tracks, next } => match next {
            true => format!("{tracks} tracks play next"),
            false => format!("queued {tracks} tracks"),
        },
        Outcome::Mode(mode) => format!("mode: {}", mode.name()),
        Outcome::ReplayGain(r) => format!("replaygain: {}", r.name()),
        Outcome::SliceEdges(e) => format!("slice edges: {}", e.name()),
        Outcome::LoopSaved { slot, kept } => format!(
            "loop {slot} saved{}",
            if *kept {
                ""
            } else {
                " (not kept: no library file)"
            }
        ),
        Outcome::LoopCleared { slot } => format!("loop {slot} cleared"),
        Outcome::LoopsCleared => "loops cleared".into(),
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
        Outcome::MarkMoved { from, to } => {
            format!("moved mark from {} to {}", fmt_time(*from), fmt_time(*to))
        }
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
        Outcome::ConvertStarted { format } => format!("converting to {format}"),
        Outcome::Converted { dir, warnings } => {
            // The loss first: a narrow status line cuts the end.
            let done = format!("converted to {}", home_as_tilde(dir));
            match warnings.as_slice() {
                [] => done,
                [one] => format!("ConvertWithMoss: {one} {done}"),
                [first, rest @ ..] => {
                    format!("ConvertWithMoss: {first} ({} more) {done}", rest.len())
                }
            }
        }
        Outcome::ScanStarted { dir: Some(dir) } => format!("scanning {}", home_as_tilde(dir)),
        Outcome::ScanStarted { dir: None } => "rescanning library".into(),
        Outcome::Scanning { seen, added } => format!("scanning: {seen} files, {added} added"),
        Outcome::AnalysisStarted { dir: None } => "analysing the library".into(),
        Outcome::AnalysisStarted { dir: Some(dir) } => {
            format!("analysing {}", home_as_tilde(dir))
        }
        Outcome::FindingTempo => "analysing the track for its tempo".into(),
        Outcome::TempoFixed { bpm, factor } => {
            let by = match *factor {
                1.0 => "as analysed".to_string(),
                f if f > 1.0 => format!("x{} the analysis", f as u32),
                f => format!("/{} the analysis", (1.0 / f) as u32),
            };
            format!("tempo {bpm:.1} BPM, {by}")
        }
        Outcome::Analysing { done, total } => format!("analysing: {done}/{total} tracks"),
        Outcome::Analysed { analysed: 0, .. } => {
            "nothing to analyse; every track is up to date".into()
        }
        Outcome::Analysed { analysed, failed } => {
            let failed = match failed {
                0 => String::new(),
                n => format!(", {n} unreadable"),
            };
            format!("analysed {analysed} tracks{failed}")
        }
        Outcome::Scanned { dir, report } => {
            let missing = match (report.missing, report.unavailable) {
                (0, _) => String::new(),
                (n, 0) => format!(", {n} missing"),
                // Said plainly: the tracks are counted missing, but the
                // directory could not be read, so they may well be there.
                (n, 1) => format!(", {n} missing, a directory unreadable"),
                (n, u) => format!(", {n} missing, {u} directories unreadable"),
            };
            let where_ = match dir {
                Some(dir) => home_as_tilde(dir),
                None => "library".into(),
            };
            format!(
                "scanned {where_}: {} added, {} unreadable{missing}; {} tracks",
                report.stats.added, report.stats.failed, report.total
            )
        }
        Outcome::PruneStarted { dir: Some(dir) } => {
            format!("pruning {}", home_as_tilde(dir))
        }
        Outcome::PruneStarted { dir: None } => "pruning library".into(),
        Outcome::Pruned { dir, removed } => {
            let where_ = match dir {
                Some(dir) => home_as_tilde(dir),
                None => "library".into(),
            };
            format!(
                "pruned {where_}: {} and {} of missing files",
                count(removed.tracks, "track"),
                count(removed.marks, "mark")
            )
        }
        Outcome::Forgot { dir, removed } => format!(
            "forgot {}: {} and {} removed",
            home_as_tilde(dir),
            count(removed.tracks, "track"),
            count(removed.marks, "mark")
        ),
        Outcome::Opening => "opening".into(),
        Outcome::Opened { tracks, skipped } => {
            let tracks = match tracks {
                1 => "1 track".to_string(),
                n => format!("{n} tracks"),
            };
            match skipped {
                0 => format!("playing {tracks}"),
                n => format!("playing {tracks}; {n} skipped"),
            }
        }
    }
}

fn refusal_text(refusal: &Refusal) -> String {
    match refusal {
        Refusal::NothingPlaying => "nothing is playing".into(),
        Refusal::NothingExported => "no slices written yet to convert; :slice first".into(),
        Refusal::ConvertOff => {
            ":convert is off; set convert-with-moss.enable = true under [extensions] in settings.toml"
                .into()
        }
        Refusal::NoConvertWithMoss(program) => playr_core::convertwithmoss::not_installed(program),
        Refusal::NoKit(dir) => format!(
            "no kit in {}; exports have one since playr 0.15",
            home_as_tilde(dir)
        ),
        Refusal::SelectionEmpty => "selection is empty".into(),
        Refusal::NoSearch => "no search results to save; search first".into(),
        Refusal::Sql(error) => format!("sql: {error}"),
        Refusal::NameReserved(name) => format!("\"{name}\" is kept for the selection's draft"),
        Refusal::QueueEmpty => "the queue is empty".into(),
        Refusal::NoLibraryFile => "no library to save to; `playr scan <dir>` creates one".into(),
        Refusal::NameEmpty => "playlist name cannot be empty".into(),
        Refusal::NameUnchanged => "name unchanged".into(),
        Refusal::NameTaken(name) => format!("a playlist named \"{name}\" already exists"),
        Refusal::WouldReplace(name) => format!("a playlist named \"{name}\" would be replaced"),
        Refusal::NoPlaylistNamed(name) => format!("no single playlist named \"{name}\""),
        Refusal::PlaylistEmpty => "playlist is empty".into(),
        Refusal::AlreadyMarked { at } => format!("already marked at {}", fmt_time(*at)),
        Refusal::NoMarks => "no marks in this track".into(),
        Refusal::NoTempo => "no clear pulse in this track to cut beats at".into(),
        Refusal::NoTempoToFix => "no tempo for this track to correct; :analyze it first".into(),
        Refusal::TempoFixLimit => "a tempo is corrected by at most 2 octaves either way".into(),
        Refusal::NoLoops => "no loops saved in this track".into(),
        Refusal::NoLaterMark => "no later mark".into(),
        Refusal::NoEarlierMark => "no earlier mark".into(),
        Refusal::NoLibraryPath => "no library file to scan into".into(),
        Refusal::NotADirectory(path) => format!("not a directory: {}", home_as_tilde(path)),
        Refusal::ScanRunning => "a scan or prune is already running".into(),
        Refusal::AnalysisRunning => "an analysis is already running".into(),
        Refusal::NoRoots => "no directories recorded; :scan DIR adds one".into(),
        Refusal::NoMarkHere => "no mark selected; { and } select one".into(),
        Refusal::MarkInTheWay { at } => {
            format!("a mark is already at {}", fmt_time(*at))
        }
        Refusal::NoOnsetNear => "no rise near that mark to snap to".into(),
        Refusal::NotARoot(path) => format!(
            "not one of the library's directories: {}; :roots lists them",
            home_as_tilde(path)
        ),
    }
}

/// `m:ss`, or `h:mm:ss` past an hour.
pub fn fmt_time(d: Duration) -> String {
    let total = d.as_secs();
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// The question [`crate::model::Input::Draft`] asks, about a draft of `tracks`.
pub fn draft_question(tracks: usize) -> String {
    let tracks = match tracks {
        1 => "1 track".into(),
        n => format!("{n} tracks"),
    };
    format!("the draft from before holds {tracks}: overwrite it, append it, or save it as?")
}

/// A pending stop, for the status bar: after this track, on the sleep
/// timer's `left`, or both; `None` when neither is set.
pub fn stopping(after: bool, left: Option<Duration>) -> Option<String> {
    match (after, left) {
        (false, None) => None,
        (true, None) => Some("stop after track".into()),
        (false, Some(d)) => Some(format!("stop in {}", fmt_time(d))),
        (true, Some(d)) => Some(format!("stop after track, or in {}", fmt_time(d))),
    }
}

/// `path`, with the home directory shown as `~` to keep messages short.
/// Why a tape control does nothing, as its tooltip says.
pub fn idle_text(idle: crate::tape::Idle) -> &'static str {
    use crate::tape::Idle;
    match idle {
        Idle::VoiceOff => "no effect: the voice is off",
        Idle::WriteOff => "no effect: Write is off",
        Idle::NoSend => "no effect: Send is 0",
        Idle::Silent => "no effect: Level is 0",
        Idle::Still => "no effect: Rate is 0, so the head never wraps",
        Idle::NoRoom => "no effect: no audio either side of the window to crossfade into",
        Idle::Unchanging => {
            "nothing changes: Feedback is 1, Wear and Thin are 0 and every Send is 0"
        }
        Idle::Turns => "no effect: Ping turns the head at the edges, so it never wraps",
    }
}

fn tape_text(m: &crate::tape::TapeMessage) -> String {
    use crate::tape::{TapeAction, TapeMessage};
    let secs = |frames: f64, rate: u32| frames / f64::from(rate.max(1));
    match m {
        TapeMessage::Loading => "reading the tape".into(),
        TapeMessage::Loaded { frames, rate } => {
            format!(
                "tape loaded: {:.2} s; :tape play",
                secs(*frames as f64, *rate)
            )
        }
        TapeMessage::Done(TapeAction::Play) => "tape playing".into(),
        TapeMessage::Done(TapeAction::Stop) => "tape stopped".into(),
        TapeMessage::Done(TapeAction::Reset) => "tape reset to the loop as loaded".into(),
        TapeMessage::Done(a) => command::line(&Action::Tape(*a), None),
        TapeMessage::NoTape => "no tape loaded; :tape load first".into(),
        TapeMessage::NoRange => "no range to load; set one, or :tape load N for a loop".into(),
        TapeMessage::EmptySlot(n) => format!("loop {n} is empty"),
        TapeMessage::EmptyWindow => {
            "the window is empty; its start must come before its end".into()
        }
        TapeMessage::AlreadySaving => "the tape is already being saved".into(),
        TapeMessage::Saving => "saving the tape".into(),
        TapeMessage::Saved(path) => format!("saved the tape to {}", home_as_tilde(path)),
        TapeMessage::Recording(path) => format!("recording the tape to {}", home_as_tilde(path)),
        TapeMessage::Recorded {
            path,
            frames,
            rate,
            dropped,
        } => {
            let done = format!(
                "recorded {:.2} s to {}",
                secs(*frames as f64, *rate),
                home_as_tilde(path)
            );
            match dropped {
                0 => done,
                n => format!("{done}; {n} blocks lost, the disk was too slow"),
            }
        }
        TapeMessage::Failed(e) => format!("tape: {e}"),
    }
}

fn dj_text(m: &crate::dj::DjMessage) -> String {
    use crate::dj::{DjAction, DjMessage};
    let deck = |s: &crate::dj::Side| crate::command::side_name(*s);
    match m {
        DjMessage::Loading(s) => format!("reading the track for deck {}", deck(s)),
        DjMessage::Loaded {
            side,
            title,
            bpm: Some(bpm),
        } => format!("deck {}: {title}, {bpm:.2} BPM", deck(side)),
        DjMessage::Loaded {
            side,
            title,
            bpm: None,
        } => format!("deck {}: {title}, no beat grid", deck(side)),
        DjMessage::FindingGrid(s) => {
            format!("deck {}: analysing the track for its beat grid", deck(s))
        }
        DjMessage::NoGrid(s) => format!(
            "deck {} has no beat grid; :dj {} tap sets one",
            deck(s),
            deck(s)
        ),
        DjMessage::Done(DjAction::Play(s)) => format!("deck {} playing", deck(s)),
        DjMessage::Done(DjAction::Pause(s)) => format!("deck {} paused", deck(s)),
        DjMessage::Done(a) => command::line(&Action::Dj(*a), None),
        DjMessage::Empty(s) => format!("deck {} is empty; :dj {} load first", deck(s), deck(s)),
        DjMessage::NoTrack => "no track under the cursor to load".into(),
        DjMessage::Playing(s) => format!("deck {} is playing; pause it first", deck(s)),
        DjMessage::OutOfReach(s) => format!(
            "deck {} cannot reach the other deck's tempo within 50%",
            deck(s)
        ),
        DjMessage::NoCueChannels(n) => {
            format!("the output device has {n} channels; the cue on 3 and 4 needs 4")
        }
        DjMessage::Tapped(s) => format!("deck {}: tap again on the beat", deck(s)),
        DjMessage::Grid { side, bpm, t0 } => {
            format!(
                "deck {} grid: {bpm:.2} BPM, a beat at {t0:.3} s",
                deck(side)
            )
        }
        DjMessage::Failed(e) => format!("dj: {e}"),
    }
}

pub fn home_as_tilde(path: &Path) -> String {
    let home = std::env::home_dir();
    match home.and_then(|h| path.strip_prefix(h).ok().map(|rest| rest.to_path_buf())) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// `n` of `thing`, as `1 mark` or `3 marks`.
fn count(n: usize, thing: &str) -> String {
    match n {
        1 => format!("1 {thing}"),
        n => format!("{n} {thing}s"),
    }
}

/// The ReplayGain applied, as the bottom line shows it: `rg -6.2 dB`.
/// The tone control's bands away from 0: `eq bass +2 treble -3.5`, or `eq flat`.
pub fn eq(gains: [f32; 3]) -> String {
    let bands: Vec<String> = playr_core::audio::eq::Band::ALL
        .iter()
        .zip(gains)
        .filter(|(_, db)| *db != 0.0)
        .map(|(band, db)| format!("{} {:+}", band.name(), (db * 10.0).round() / 10.0))
        .collect();
    match bands.is_empty() {
        true => "eq flat".into(),
        false => format!("eq {}", bands.join(" ")),
    }
}

pub fn replaygain(db: f32) -> String {
    // Rounding to one decimal would show a tiny cut as -0.0.
    let db = if db.abs() < 0.05 { 0.0 } else { db };
    format!("rg {db:+.1} dB")
}

/// What a [`Finding`] says beyond its name, for a report or a dialog.
pub fn finding_detail(f: &Finding) -> String {
    match f {
        Finding::Unreadable(e) => e.clone(),
        Finding::Damaged { skipped, md5_bad } => match (skipped, md5_bad) {
            (0, _) => "audio does not match its MD5".into(),
            (n, false) => format!("{n} packets skipped"),
            (n, true) => format!("{n} packets skipped; audio does not match its MD5"),
        },
        Finding::NoChecksum => "no MD5 in the FLAC header".into(),
        Finding::WrongLength { decoded, header } => {
            format!("{decoded} frames decoded, header says {header}")
        }
        Finding::Padded { used, bits } => format!("{used} of {bits} bits used"),
        Finding::PossibleLossySource { hz } | Finding::PossibleUpsampling { hz } => {
            format!("content stops at {:.1} kHz", *hz as f32 / 1000.0)
        }
    }
}

/// The rows `:info` shows for `track`: what the library knows from its tags,
/// then what `playr analyze` measured, then the problems it found.
///
/// `analysis` is `None` until the track is analysed, or once the file has
/// changed since; `gains` is what ReplayGain would apply.
pub fn info_rows(
    track: &Track,
    analysis: Option<&Analysis>,
    gains: Gains,
) -> Vec<(String, String)> {
    let mut rows = vec![("file".into(), home_as_tilde(Path::new(&track.path)))];
    if let Some(album) = &track.album {
        rows.push(("album".into(), album.clone()));
    }
    let mut format = String::new();
    if let Some(rate) = track.sample_rate {
        format.push_str(&format!("{:.1} kHz", f64::from(rate) / 1000.0));
    }
    if let Some(channels) = track.channels {
        format.push_str(&format!(", {channels} ch"));
    }
    if let Some(bits) = track.bit_depth {
        format.push_str(&format!(", {bits} bit"));
    }
    if let Some(ms) = track.duration_ms {
        format.push_str(&format!(", {}", fmt_time(Duration::from_millis(ms as u64))));
    }
    rows.push(("format".into(), format.trim_start_matches(", ").to_string()));

    let Some(a) = analysis else {
        rows.push(("measured".into(), "not yet; :analyze measures it".into()));
        return rows;
    };

    match a.loudness {
        Some(lufs) => rows.push(("loudness".into(), format!("{lufs:.1} LUFS"))),
        None => rows.push(("loudness".into(), "silent".into())),
    }
    if let Some(peak) = a.peak {
        let dbfs = match peak > 0.0 {
            true => format!("{:.1} dBFS", 20.0 * peak.log10()),
            false => "silent".into(),
        };
        rows.push(("peak".into(), format!("{peak:.3} ({dbfs})")));
    }
    let gain = |name: &str, g: Option<playr_core::gain::Gain>| {
        let g = g?;
        let capped = match g.linear() < 10f32.powf(g.db / 20.0) - 1e-6 {
            true => ", held back by the peak",
            false => "",
        };
        Some((name.to_string(), format!("{:+.1} dB{capped}", g.db)))
    };
    rows.extend(gain("track gain", gains.track));
    rows.extend(gain("album gain", gains.album));

    let tempo = match (a.bpm_tag, a.tempo) {
        (Some(tag), _) => format!("{tag:.0} BPM, from the file's tag"),
        (None, Some(t)) if t.confidence >= tempo::MIN_CONFIDENCE => {
            let alt = match t.alt {
                Some(alt) => format!(", or {alt:.0} BPM"),
                None => String::new(),
            };
            format!("{:.0} BPM{alt} (confidence {:.2})", t.bpm, t.confidence)
        }
        (None, Some(t)) => format!("no clear pulse (confidence {:.2})", t.confidence),
        (None, None) => "too short to measure".into(),
    };
    rows.push(("tempo".into(), tempo));

    if let Some(c) = a.cutoff {
        rows.push((
            "content to".into(),
            format!(
                "{:.1} kHz, falling {:.0} dB",
                c.hz as f32 / 1000.0,
                c.fall_db
            ),
        ));
    }
    if let (Some(used), Some(bits)) = (a.bits_used, a.bits) {
        rows.push(("bits used".into(), format!("{used} of {bits}")));
    }
    if let Some(md5) = a.md5 {
        let says = match md5 {
            Md5::Ok => "matches the FLAC header",
            Md5::Bad => "does not match the FLAC header",
            Md5::Absent => "none in the FLAC header",
        };
        rows.push(("checksum".into(), says.into()));
    }
    for finding in analysis::findings(a) {
        rows.push((finding.name().into(), finding_detail(&finding)));
    }
    rows
}
