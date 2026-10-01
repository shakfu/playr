//! A session: one library and one player, and what a frontend can do with them.
//!
//! Operations take what they act on as arguments, a track, an index into the
//! selection or a playlist id, never a cursor or a view, and report what they
//! did as a [`Notice`]. A frontend turns its own cursor or click into those
//! arguments and words the notice. `docs/architecture.md` sets out the
//! design. Work that takes seconds runs on its own thread and reports through
//! the session's [`EventSink`], as the engine does.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::analysis;
use crate::audio::{Cmd, Mode, Player, State};
use crate::columns::{self, Measures, SortKey};
use crate::db;
use crate::db::query::{self, Mark, Playlist, Query, SavedSearch};
use crate::db::{Pruned, SavedQueue, Track};
use crate::event::{Event, EventSink, JobId};
use crate::gain::ReplayGain;
use crate::notice::{Notice, Outcome, Refusal, Task};
use crate::samples::{self, Cut, Edges, Fades, Job, OnsetAudio, Plan};
use crate::scan;
use crate::settings::Draft;
use crate::wave::Peaks;

/// Marks closer than this to one another are the same mark.
pub const MARK_NEAR: Duration = Duration::from_millis(500);
/// How far past a mark playback must be before seeking back returns to it
/// rather than the one before.
pub const MARK_BACK: Duration = Duration::from_secs(1);

/// The playlist the selection is saved to as it changes. The name is
/// reserved: no other playlist may take it.
pub const DRAFT: &str = "draft";

/// What the listener chose to do with a draft an earlier session left.
#[derive(Debug, Clone, PartialEq)]
pub enum DraftChoice {
    Overwrite,
    /// Put its tracks back in the selection, before the new ones.
    Append,
    /// Keep it as a playlist of this name; the selection starts a new draft.
    SaveAs(String),
}

/// The `state` key the playlist being edited is remembered under.
const EDITING: &str = "editing";

/// Whether `name` is the draft's, which no other playlist may take.
fn reserved(name: &str) -> bool {
    name.eq_ignore_ascii_case(DRAFT)
}

/// How many loops a track can keep.
pub const LOOP_SLOTS: u8 = 8;

/// A track's loops by slot, from 1 at index 0, as source frames, end exclusive.
pub type Loops = [Option<(u64, u64)>; LOOP_SLOTS as usize];

pub struct Session {
    conn: Connection,
    player: Player,
    /// Every track in the library.
    tracks: Vec<Track>,
    playlists: Vec<Playlist>,
    /// Searches kept by name, which share the playlists' names.
    searches: Vec<SavedSearch>,
    /// What the search results a frontend shows came from, to save.
    shown: Option<Query>,
    /// Tracks collected to edit and save as a playlist. It does not change
    /// what plays unless it is played itself.
    selection: Vec<Track>,
    /// Queued tracks that have played, oldest first. The Queue view lists
    /// them above the track playing, so a queue can be saved once heard.
    played: Vec<Track>,
    /// When the sleep timer stops playback.
    sleep_at: Option<Instant>,
    /// Whether the queue is stored for the next start.
    keep_queue: bool,
    /// What to do with a draft an earlier session left, and whether this
    /// session has settled it, so its selection may be written as the draft.
    draft: Draft,
    draft_settled: bool,
    /// A change waits on the listener's answer about the old draft.
    draft_question: bool,
    /// The playlist whose tracks the selection holds to edit, by id.
    editing: Option<i64>,
    /// Marks in the track `marks_for`, earliest first.
    marks: Vec<Mark>,
    marks_for: Option<PathBuf>,
    /// Loops saved in the track `loops_for`, by slot, from 1.
    loops: Loops,
    loops_for: Option<PathBuf>,
    /// Where exported slices are written.
    samples: PathBuf,
    /// The directory of the last export, which a conversion reads.
    exported: Option<PathBuf>,
    /// ConvertWithMoss's command line, or `None` while the extension is off.
    convertwithmoss: Option<PathBuf>,
    /// Whether an export writes an Octatrack `.ot` file.
    ot_file: bool,
    /// What an export does at slice edges, and how long a fade takes.
    edges: Edges,
    fades: Fades,
    /// The region onsets were last found in, for planning them again.
    onset_audio: Arc<OnsetAudio>,
    events: EventSink,
    /// The number given to the next piece of background work.
    next_job: JobId,
    /// Stops the peaks read in progress, which a newer read replaces.
    reading: Option<Arc<AtomicBool>>,
    /// The library file a scan writes to.
    library: Option<PathBuf>,
    /// Set while a scan or a prune runs.
    scanning: Arc<AtomicBool>,
    /// Set while an analysis runs. Separate from `scanning`: an analysis
    /// writes only its own tables, so the two may run together.
    analysing: Arc<AtomicBool>,
    replaygain: ReplayGain,
    /// What every track list is sorted by. It orders the library and every
    /// search, so the list a frontend shows is the list it plays.
    sort: Vec<SortKey>,
    /// What `playr analyze` measured, for the columns that show it and the
    /// keys that sort by it. Read with the library.
    measures: HashMap<String, Measures>,
}

impl Session {
    /// A session over the library `conn`, playing through `player`. Events
    /// from the engine and from background work go to `events`.
    pub fn new(conn: Connection, player: Player, events: EventSink) -> Session {
        player.set_events(events.clone());
        let library = conn.path().filter(|p| !p.is_empty()).map(PathBuf::from);
        let mut session = Session {
            conn,
            player,
            tracks: Vec::new(),
            playlists: Vec::new(),
            searches: Vec::new(),
            shown: None,
            selection: Vec::new(),
            played: Vec::new(),
            sleep_at: None,
            keep_queue: true,
            draft: Draft::Ask,
            draft_settled: false,
            draft_question: false,
            editing: None,
            marks: Vec::new(),
            marks_for: None,
            loops: [None; LOOP_SLOTS as usize],
            loops_for: None,
            samples: PathBuf::new(),
            exported: None,
            convertwithmoss: None,
            ot_file: false,
            edges: Edges::Exact,
            fades: Fades::default(),
            onset_audio: Arc::default(),
            events,
            next_job: 1,
            reading: None,
            library,
            scanning: Arc::default(),
            analysing: Arc::default(),
            replaygain: ReplayGain::Off,
            sort: Vec::new(),
            measures: HashMap::new(),
        };
        session.reload();
        session
    }

    /// Sets the directory exported slices are written under.
    pub fn set_samples_dir(&mut self, dir: PathBuf) {
        self.samples = dir;
    }

    /// The directory exported slices are written under.
    pub fn samples_dir(&self) -> &Path {
        &self.samples
    }

    /// The exports under the samples directory that hold a kit, by directory
    /// name, sorted: what `:convert FORMAT EXPORT` takes.
    pub fn exports(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.samples) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| samples::kit_path(&e.path()).is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        names
    }

    /// Turns the `convert-with-moss` extension on, with ConvertWithMoss's
    /// command line at `program`, or off with `None`, as it starts.
    pub fn set_convertwithmoss(&mut self, program: Option<PathBuf>) {
        self.convertwithmoss = program;
    }

    /// Whether the `convert-with-moss` extension is on, so that its command
    /// is offered.
    pub fn convert_enabled(&self) -> bool {
        self.convertwithmoss.is_some()
    }

    /// Whether the extension is on and ConvertWithMoss is where the settings
    /// say, looked up each time, so installing it takes effect without a
    /// restart.
    pub fn can_convert(&self) -> bool {
        self.convertwithmoss.as_deref().is_some_and(Path::is_file)
    }

    /// Where ConvertWithMoss is expected, while the extension is on.
    pub fn convertwithmoss(&self) -> Option<&Path> {
        self.convertwithmoss.as_deref()
    }

    /// Records the directory an export wrote, which the frontend tells the
    /// session of once [`Event::Exported`] arrives, for [`Session::convert`].
    pub fn exported(&mut self, dir: PathBuf) {
        self.exported = Some(dir);
    }

    /// Chooses what later exports do at slice edges.
    pub fn set_slice_edges(&mut self, edges: Edges) -> Notice {
        self.edges = edges;
        Outcome::SliceEdges(edges).into()
    }

    pub fn slice_edges(&self) -> Edges {
        self.edges
    }

    /// Sets how long [`Edges::Fade`] fades each end of a slice.
    /// Sets whether an export writes an Octatrack `.ot` file.
    pub fn set_ot_file(&mut self, on: bool) {
        self.ot_file = on;
    }

    pub fn set_fades(&mut self, fades: Fades) {
        self.fades = fades;
    }

    pub fn fades(&self) -> Fades {
        self.fades
    }

    /// Sets the library file a scan writes to, for a session over an
    /// in-memory library. A session over a file scans into that file.
    pub fn set_library_path(&mut self, path: PathBuf) {
        self.library = Some(path);
    }

    /// Reads the library's tracks and playlists again, and its gains while
    /// ReplayGain is on.
    pub fn reload(&mut self) {
        self.tracks = query::all(&self.conn).unwrap_or_default();
        self.playlists = query::playlists(&self.conn).unwrap_or_default();
        self.searches = query::searches(&self.conn).unwrap_or_default();
        self.measures = db::analysis::measures(&self.conn, &self.tracks).unwrap_or_default();
        let mut tracks = std::mem::take(&mut self.tracks);
        self.order(&mut tracks);
        self.tracks = tracks;
        self.send_gains();
    }

    /// Sorts `tracks` by the current keys. Every list a frontend shows goes
    /// through here, so ordering never depends on where the list came from.
    fn order(&self, tracks: &mut [Track]) {
        self.order_by(tracks, &self.sort);
    }

    fn order_by(&self, tracks: &mut [Track], sort: &[SortKey]) {
        if sort.is_empty() {
            return;
        }
        let measures = |t: &Track| self.measures.get(&t.path).copied().unwrap_or_default();
        tracks.sort_by(|a, b| columns::compare((a, measures(a)), (b, measures(b)), sort));
    }

    /// `tracks` in the order every list is shown in.
    pub fn sorted(&self, mut tracks: Vec<Track>) -> Vec<Track> {
        self.order(&mut tracks);
        tracks
    }

    /// What `playr analyze` measured, by path, for the columns that show it.
    pub fn measures(&self) -> &HashMap<String, Measures> {
        &self.measures
    }

    /// What `playr analyze` measured about `track`, for its columns.
    pub fn measures_of(&self, path: &str) -> Measures {
        self.measures.get(path).copied().unwrap_or_default()
    }

    /// Sorts every track list by `sort`, from now on.
    pub fn set_sort(&mut self, sort: Vec<SortKey>) {
        self.sort = sort;
        let mut tracks = std::mem::take(&mut self.tracks);
        self.order(&mut tracks);
        self.tracks = tracks;
    }

    pub fn sort(&self) -> &[SortKey] {
        &self.sort
    }

    /// Hands the player the library's gains; read only while ReplayGain is
    /// on, so a library that never uses it never loads them.
    fn send_gains(&self) {
        if self.replaygain != ReplayGain::Off {
            let gains = db::analysis::gains(&self.conn, &self.tracks).unwrap_or_default();
            self.player.send(Cmd::SetGains(Arc::new(gains)));
        }
    }

    /// The player, for reading its state. Changes go through the session.
    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn playlists(&self) -> &[Playlist] {
        &self.playlists
    }

    pub fn searches(&self) -> &[SavedSearch] {
        &self.searches
    }

    /// Records what the search results shown came from, or that none are,
    /// so they can be saved by name.
    pub fn set_shown(&mut self, query: Option<Query>) {
        self.shown = query;
    }

    pub fn shown(&self) -> Option<&Query> {
        self.shown.as_ref()
    }

    /// Whether a playlist or a saved search has `name`.
    fn name_taken(&self, name: &str) -> bool {
        self.playlists.iter().any(|p| p.name == name)
            || self.searches.iter().any(|s| s.name == name)
    }

    /// Saves the search shown as `name`, with the sort it is shown in. A saved
    /// search of that name is replaced only with `replace`, so a frontend can
    /// ask first; a playlist of that name is never replaced.
    pub fn save_search(&mut self, name: &str, replace: bool) -> Notice {
        let name = name.trim();
        let Some(query) = self.shown.clone() else {
            return Refusal::NoSearch.into();
        };
        if name.is_empty() {
            return Refusal::NameEmpty.into();
        }
        if reserved(name) {
            return Refusal::NameReserved(name.to_string()).into();
        }
        if self.playlists.iter().any(|p| p.name == name) {
            return Refusal::NameTaken(name.to_string()).into();
        }
        if !replace && self.searches.iter().any(|s| s.name == name) {
            return Refusal::WouldReplace(name.to_string()).into();
        }
        // SQL keeps its own ORDER BY.
        let sort = match query {
            Query::Text(_) => self
                .sort
                .iter()
                .map(|k| k.text())
                .collect::<Vec<_>>()
                .join(","),
            Query::Sql(_) => String::new(),
        };
        if let Err(e) = query::save_search(&self.conn, name, &query, &sort) {
            return Notice::Failed {
                task: Task::Save,
                error: e.to_string(),
            };
        }
        self.searches = query::searches(&self.conn).unwrap_or_default();
        Outcome::SearchSaved {
            name: name.to_string(),
        }
        .into()
    }

    /// The tracks `search` finds now, in the order it was saved with.
    pub fn run_search(&self, search: &SavedSearch) -> Vec<Track> {
        match &search.query {
            Query::Text(q) => {
                let mut hits = query::search(&self.conn, q).unwrap_or_default();
                let sort: Vec<SortKey> =
                    search.sort.split(',').filter_map(SortKey::named).collect();
                self.order_by(&mut hits, &sort);
                hits
            }
            Query::Sql(statement) => self.sql(statement).unwrap_or_default(),
        }
    }

    /// The library tracks a `:sql` statement names, in its order, run on
    /// this thread for up to [`db::sql::TIME_LIMIT`]. See [`db::sql`] for what
    /// it may read and its limits; a frontend uses
    /// [`sql_in_background`](Self::sql_in_background).
    pub fn sql(&self, statement: &str) -> Result<Vec<Track>, Refusal> {
        // Its own read-only connection needs the file.
        let Some(library) = self.library.as_deref() else {
            return Err(Refusal::NoLibraryFile);
        };
        let paths = db::sql::paths(library, statement).map_err(Refusal::Sql)?;
        Ok(self.tracks_at(&paths))
    }

    /// Runs a `:sql` statement on its own thread, which finishes with
    /// [`Event::Sql`]; [`tracks_at`](Self::tracks_at) turns its paths into
    /// tracks.
    pub fn sql_in_background(&mut self, statement: String) -> Result<JobId, Refusal> {
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryFile);
        };
        Ok(self.spawn(move |job| {
            let result = db::sql::paths(&library, &statement);
            Some(Event::Sql { job, result })
        }))
    }

    /// The library's tracks at `paths`, in that order; others are left out.
    pub fn tracks_at(&self, paths: &[PathBuf]) -> Vec<Track> {
        let by_path: HashMap<&str, &Track> =
            self.tracks.iter().map(|t| (t.path.as_str(), t)).collect();
        (paths.iter())
            .filter_map(|p| by_path.get(p.to_str()?).map(|t| (*t).clone()))
            .collect()
    }

    /// Deletes saved search `id`, saying which it was.
    pub fn delete_search(&mut self, id: i64) -> Option<Notice> {
        let name = self.searches.iter().find(|s| s.id == id)?.name.clone();
        query::delete_search(&self.conn, id).ok()?;
        self.searches = query::searches(&self.conn).unwrap_or_default();
        Some(Outcome::Deleted { name }.into())
    }

    pub fn selection(&self) -> &[Track] {
        &self.selection
    }

    /// Tracks matching `input`, in the order the library is sorted in; none
    /// if the query fails.
    pub fn search(&self, input: &str) -> Vec<Track> {
        let mut hits = query::search(&self.conn, input).unwrap_or_default();
        self.order(&mut hits);
        hits
    }

    /// The tracks of playlist `id`, in order; none if it cannot be read.
    pub fn playlist_tracks(&self, id: i64) -> Vec<Track> {
        query::playlist_tracks(&self.conn, id).unwrap_or_default()
    }

    /// Whether the library is a file, so playlists and marks outlast the session.
    pub fn has_library_file(&self) -> bool {
        self.conn.path().is_some_and(|p| !p.is_empty())
    }

    // --- playback ---

    // The player holds one list: the list played, with the tracks the
    // listener queued spliced in after the track playing. `Status::queued`
    // tells them apart. A queued track leaves the list once it has played.

    /// Plays `tracks` from `index`, as a list that goes on by itself, as the
    /// library does. Tracks waiting in the queue are kept and play next.
    pub fn play(&mut self, tracks: &[Track], index: usize) {
        let status = self.player.status();
        let waiting: Vec<PathBuf> = waiting(&status).map(|i| status.queue[i].clone()).collect();
        let index = index.min(tracks.len().saturating_sub(1));
        let paths = |ts: &[Track]| {
            ts.iter()
                .map(|t| PathBuf::from(&t.path))
                .collect::<Vec<_>>()
        };
        let (head, tail) = tracks.split_at((index + 1).min(tracks.len()));
        let mut list = paths(head);
        let mut queued = vec![false; list.len()];
        queued.extend(vec![true; waiting.len()]);
        list.extend(waiting);
        queued.extend(vec![false; tail.len()]);
        list.extend(paths(tail));
        self.player.send(Cmd::PlayWith(list, queued, index));
    }

    /// Replaces the queue with `tracks` from `index` and plays them, over the
    /// list playing, which resumes after them. Says how many tracks were
    /// waiting, when some were.
    pub fn play_queued(&mut self, tracks: &[Track], index: usize) -> Option<Outcome> {
        let status = self.player.status();
        let dropped = waiting(&status).count();
        let new: Vec<PathBuf> = tracks
            .iter()
            .skip(index)
            .map(|t| PathBuf::from(&t.path))
            .collect();
        let listed = |i: &usize| !status.queued.get(*i).copied().unwrap_or(false);
        let played = |range: std::ops::Range<usize>| -> Vec<PathBuf> {
            range
                .filter(listed)
                .map(|i| status.queue[i].clone())
                .collect()
        };
        let len = status.queue.len();
        let (before, after) = match len {
            0 => (Vec::new(), Vec::new()),
            _ => (played(0..status.index + 1), played(status.index + 1..len)),
        };
        let start = before.len();
        let mut queued = vec![false; before.len()];
        queued.extend(vec![true; new.len()]);
        queued.extend(vec![false; after.len()]);
        let list: Vec<PathBuf> = before.into_iter().chain(new).chain(after).collect();
        self.player.send(Cmd::PlayWith(list, queued, start));
        (dropped > 0).then_some(Outcome::QueueReplaced { tracks: dropped })
    }

    /// Queues `tracks`: after the tracks waiting, or before them when `next`.
    /// The first tracks queued over a list playing interrupt it; with
    /// nothing playing, they play at once.
    pub fn enqueue(&mut self, tracks: &[Track], next: bool) -> Outcome {
        let outcome = Outcome::Queued {
            tracks: tracks.len(),
            next,
        };
        let status = self.player.status();
        if self.stopped(&status) || status.queue.is_empty() {
            let _ = self.play_queued(tracks, 0);
            return outcome;
        }
        let last_waiting = waiting(&status).last();
        let playing_queued = status.queued.get(status.index).copied().unwrap_or(false);
        let at = match (next, last_waiting) {
            (false, Some(last)) => last + 1,
            _ => status.index + 1,
        };
        let paths = tracks.iter().map(|t| PathBuf::from(&t.path)).collect();
        self.player.send(Cmd::Insert(at, paths));
        if !next && last_waiting.is_none() && !playing_queued {
            self.player.send(Cmd::Next);
        }
        outcome
    }

    /// The rows of the queue, as indices into the player's list: the track
    /// playing, if it was queued, then the queued tracks waiting.
    pub fn queue_rows(&self) -> Vec<usize> {
        let status = self.player.status();
        let playing =
            !self.stopped(&status) && status.queued.get(status.index).copied().unwrap_or(false);
        playing
            .then_some(status.index)
            .into_iter()
            .chain(waiting(&status))
            .collect()
    }

    /// Queued tracks that have played, oldest first: the Queue view's first
    /// rows, above [`queue_rows`](Self::queue_rows).
    pub fn played(&self) -> &[Track] {
        &self.played
    }

    /// Every row of the Queue view, played first, for saving as a playlist.
    pub fn queue_tracks(&self) -> Vec<Track> {
        let list = self.player.queue();
        let live = self
            .queue_rows()
            .into_iter()
            .map(|i| self.track_for(&list[i]));
        self.played.iter().cloned().chain(live).collect()
    }

    /// The library's track at `path`, or a bare one for a file outside it.
    fn track_for(&self, path: &Path) -> Track {
        match self.tracks.iter().find(|t| Path::new(&t.path) == path) {
            Some(t) => t.clone(),
            None => Track {
                path: path.to_string_lossy().into_owned(),
                ..Default::default()
            },
        }
    }

    /// Takes Queue view row `row` out, and says which track it was. Taking
    /// out the track playing plays the next.
    pub fn dequeue(&mut self, row: usize) -> Option<Outcome> {
        if row < self.played.len() {
            let title = self.played.remove(row).display_title();
            return Some(Outcome::RemovedTrack { title });
        }
        let index = *self.queue_rows().get(row - self.played.len())?;
        let path = self.player.queue().get(index)?.clone();
        let title = self.track_for(&path).display_title();
        self.player.send(Cmd::Remove(index));
        Some(Outcome::RemovedTrack { title })
    }

    /// Moves Queue view row `row` by `by` places, returning where it is now,
    /// or nothing if the move leaves the played rows or the waiting rows it
    /// started in. The track playing keeps its place.
    pub fn move_in_queue(&mut self, row: usize, by: i64) -> Option<usize> {
        let to = usize::try_from(row as i64 + by).ok()?;
        let h = self.played.len();
        if row < h {
            if to >= h {
                return None;
            }
            let track = self.played.remove(row);
            self.played.insert(to, track);
            return Some(to);
        }
        let (row, to) = (row - h, to.checked_sub(h)?);
        let rows = self.queue_rows();
        let status = self.player.status();
        let stopped = self.stopped(&status);
        let first = rows.iter().position(|&i| i > status.index || stopped)?;
        if row < first || to < first || row >= rows.len() || to >= rows.len() {
            return None;
        }
        self.player.send(Cmd::Move(rows[row], rows[to]));
        Some(to + h)
    }

    /// Plays Queue view row `row`: a waiting track at once, or a played one
    /// again, which then leaves the played rows until it has played.
    pub fn play_queue_row(&mut self, row: usize) {
        let h = self.played.len();
        if row >= h {
            if let Some(&index) = self.queue_rows().get(row - h) {
                self.player.send(Cmd::Jump(index));
            }
            return;
        }
        let path = PathBuf::from(&self.played.remove(row).path);
        let status = self.player.status();
        match status.queue.is_empty() {
            true => self.player.send(Cmd::PlayWith(vec![path], vec![true], 0)),
            false => {
                self.player.send(Cmd::Insert(status.index + 1, vec![path]));
                self.player.send(Cmd::Jump(status.index + 1));
            }
        }
    }

    /// Empties the queue: the tracks waiting and those played go, and the
    /// track playing plays on as part of the list, no longer queued.
    pub fn clear_queue(&mut self) -> Outcome {
        let rows = self.queue_rows();
        let status = self.player.status();
        let waiting: Vec<usize> = waiting(&status).collect();
        for &i in waiting.iter().rev() {
            self.player.send(Cmd::Remove(i));
        }
        // Not taken out, which would cut it off: it plays on as part of the
        // list, and leaves the queue's rows.
        let playing = rows.first().filter(|&&i| i == status.index);
        if let Some(&i) = playing {
            self.player.send(Cmd::Unqueue(i));
        }
        let played = std::mem::take(&mut self.played).len();
        Outcome::QueueCleared {
            tracks: waiting.len() + played + usize::from(playing.is_some()),
        }
    }

    /// Whether playback is stopped, and not about to start: right after a
    /// play is sent the engine has yet to leave the stopped state.
    fn stopped(&self, status: &crate::audio::Status) -> bool {
        status.state == State::Stopped && self.player.caught_up()
    }

    /// Moves the queued tracks that have played out of the list and into
    /// [`played`](Self::played), once playback has moved past them.
    pub fn drop_played(&mut self) {
        let status = self.player.status();
        let played: Vec<usize> = (0..status.index.min(status.queued.len()))
            .filter(|&i| status.queued[i])
            .collect();
        for &i in &played {
            let track = self.track_for(&status.queue[i]);
            self.played.push(track);
        }
        for &i in played.iter().rev() {
            self.player.send(Cmd::Remove(i));
        }
    }

    /// Plays `path` from `at`, then the waiting tracks of `queue`, without
    /// touching the selection. For taking up where playr left off; the list
    /// that was playing is not kept, so playback stops after the queue.
    pub fn resume(&mut self, path: PathBuf, at: Duration, queue: SavedQueue) {
        self.played = queue.played.iter().map(|p| self.track_for(p)).collect();
        let mut list = vec![path];
        list.extend(queue.waiting);
        let mut queued = vec![true; list.len()];
        queued[0] = queue.playing;
        self.player.send(Cmd::PlayWith(list, queued, 0));
        self.player.send(Cmd::Seek(at));
    }

    /// What playr was playing when it last closed, if the file is still
    /// there, with the queue stored beside it, less files that have gone.
    ///
    /// A file that has gone is not offered: the question would be about a
    /// track that cannot play, and answering yes would do nothing.
    pub fn resumable(&self) -> Option<(PathBuf, Duration, SavedQueue)> {
        let (path, at) = db::resume(&self.conn).ok().flatten()?;
        let mut queue = match self.keep_queue {
            true => db::resume_queue(&self.conn).unwrap_or_default(),
            false => SavedQueue::default(),
        };
        queue.played.retain(|p| p.is_file());
        queue.waiting.retain(|p| p.is_file());
        path.is_file().then_some((path, at, queue))
    }

    /// Remembers `path` and `at` as where to take up next time, with the queue.
    pub fn remember(&self, path: &Path, at: Duration) {
        let _ = db::set_resume(&self.conn, path, at);
        self.remember_queue();
    }

    /// Chooses whether the queue is stored for the next start. It is unless
    /// this says not; a queue stored before is then forgotten.
    pub fn keep_queue(&mut self, keep: bool) {
        self.keep_queue = keep;
        if !keep {
            let _ = db::set_resume_queue(&self.conn, &SavedQueue::default(), Path::new(""));
        }
    }

    // The selection starts empty each session and is written, as it changes,
    // to the playlist `DRAFT`. A draft an earlier session left is settled, as
    // `draft` says, before the first write replaces it.

    /// The playlist the selection holds to edit, if it still exists.
    pub fn editing(&self) -> Option<&Playlist> {
        let id = self.editing?;
        self.playlists.iter().find(|p| p.id == id)
    }

    /// Records the playlist being edited, in the library too, so an edit the
    /// draft holds can go on in a later session.
    fn set_editing(&mut self, id: Option<i64>) {
        self.editing = id;
        self.set_state(EDITING, &id.map_or(String::new(), |id| id.to_string()));
    }

    /// Replaces the selection with playlist `id`'s tracks to edit them;
    /// saving under its name then replaces it without asking. The draft
    /// itself is taken up as an old draft appended; nothing, once settled.
    pub fn edit_playlist(&mut self, id: i64) -> Option<Outcome> {
        let name = self.playlist_name(id)?;
        if name == DRAFT {
            // Settled, the draft already is the selection.
            if self.draft_settled {
                return None;
            }
            self.selection.clear();
            self.settle_draft(DraftChoice::Append);
            return Some(Outcome::DraftAppended);
        }
        self.selection = self.playlist_tracks(id);
        self.set_editing(Some(id));
        self.selection_changed();
        Some(Outcome::Editing { name })
    }

    /// Chooses what happens to a draft an earlier session left.
    pub fn set_draft(&mut self, draft: Draft) {
        self.draft = draft;
    }

    /// The draft playlist, if there is one.
    fn draft_playlist(&self) -> Option<&Playlist> {
        self.playlists.iter().find(|p| p.name == DRAFT)
    }

    /// Writes the selection as the draft once the old draft is settled, or
    /// settles it first as `draft` says, or waits on the listener's answer.
    fn selection_changed(&mut self) {
        if self.draft == Draft::Off || !self.has_library_file() {
            return;
        }
        if !self.draft_settled {
            let old = self.draft_playlist().map_or(0, |p| p.len);
            match (old, self.draft) {
                (0, _) | (_, Draft::Overwrite) => self.set_editing(self.editing),
                (_, Draft::Append) => self.append_old_draft(),
                _ => {
                    self.draft_question = true;
                    return;
                }
            }
            self.draft_settled = true;
        }
        self.write_draft();
    }

    /// Puts the old draft's tracks before the selection's, leaving out
    /// those the selection already holds. An edit the old draft held goes
    /// on, unless one has begun since.
    fn append_old_draft(&mut self) {
        let Some(id) = self.draft_playlist().map(|p| p.id) else {
            return;
        };
        let editing = self.state(EDITING).and_then(|id| id.parse().ok());
        self.set_editing(self.editing.or(editing));
        let new = std::mem::take(&mut self.selection);
        self.selection = self.playlist_tracks(id);
        let old: HashSet<String> = self.selection.iter().map(|t| t.path.clone()).collect();
        self.selection
            .extend(new.into_iter().filter(|t| !old.contains(&t.path)));
    }

    /// Makes the draft playlist the selection's library tracks, or removes it
    /// when there are none.
    fn write_draft(&mut self) {
        let ids: Vec<i64> = (self.selection.iter().map(|t| t.id))
            .filter(|id| *id != 0)
            .collect();
        let _ = match (ids.is_empty(), self.draft_playlist().map(|p| p.id)) {
            (true, Some(id)) => query::delete_playlist(&self.conn, id),
            (true, None) => Ok(()),
            (false, _) => query::save_playlist(&mut self.conn, DRAFT, &ids).map(|_| ()),
        };
        self.playlists = query::playlists(&self.conn).unwrap_or_default();
    }

    /// How many tracks the old draft holds, once, when a change waits on the
    /// listener: they answer with [`settle_draft`](Self::settle_draft).
    /// Unanswered, the question comes again with the next change.
    pub fn take_draft_question(&mut self) -> Option<usize> {
        if !std::mem::take(&mut self.draft_question) || self.draft_settled {
            return None;
        }
        self.draft_playlist().map(|p| p.len as usize)
    }

    /// Settles the old draft as the listener chose, then writes the selection
    /// as the draft. Saving under a name that is refused leaves it unsettled,
    /// and asks again.
    pub fn settle_draft(&mut self, choice: DraftChoice) -> Notice {
        let notice = match choice {
            DraftChoice::Overwrite => {
                self.set_editing(self.editing);
                Outcome::DraftOverwritten.into()
            }
            DraftChoice::Append => {
                self.append_old_draft();
                Outcome::DraftAppended.into()
            }
            DraftChoice::SaveAs(name) => {
                let Some(id) = self.draft_playlist().map(|p| p.id) else {
                    return Refusal::NoPlaylistNamed(DRAFT.into()).into();
                };
                let notice = self.rename_playlist(id, &name);
                if !matches!(notice, Notice::Done(_)) {
                    self.draft_question = true;
                    return notice;
                }
                self.set_editing(self.editing);
                notice
            }
        };
        self.draft_settled = true;
        self.write_draft();
        notice
    }

    /// Remembers the queue, to offer with the track playing next time, if
    /// it is kept.
    pub fn remember_queue(&self) {
        if !self.keep_queue {
            return;
        }
        let status = self.player.status();
        let rows = self.queue_rows();
        let playing = rows.first() == Some(&status.index) && !self.stopped(&status);
        let queue = SavedQueue {
            played: self.played.iter().map(|t| PathBuf::from(&t.path)).collect(),
            playing,
            waiting: rows[usize::from(playing)..]
                .iter()
                .map(|&i| status.queue[i].clone())
                .collect(),
        };
        let current = status.current().map_or(Path::new(""), |p| p.as_path());
        let _ = db::set_resume_queue(&self.conn, &queue, current);
    }

    /// The session value remembered under `key`; none without a library file.
    pub fn state(&self, key: &str) -> Option<String> {
        db::state(&self.conn, key).ok().flatten()
    }

    /// Remembers a session value under `key`, if there is a library file.
    pub fn set_state(&self, key: &str, value: &str) {
        if self.has_library_file() {
            let _ = db::set_state(&self.conn, key, value);
        }
    }

    /// Forgets where to take up, after a refused offer or a stop.
    pub fn forget_resume(&self) {
        let _ = db::clear_resume(&self.conn);
    }

    /// Sets the sleep timer to stop playback `after` from now, or turns it
    /// off with `None`.
    pub fn sleep_in(&mut self, after: Option<Duration>) {
        self.sleep_at = after.map(|d| Instant::now() + d);
    }

    /// Time left on the sleep timer, if set.
    pub fn sleep_left(&self) -> Option<Duration> {
        self.sleep_at
            .map(|at| at.saturating_duration_since(Instant::now()))
    }

    /// Stops playback once the sleep timer has run out; says whether it did.
    pub fn sleep_due(&mut self) -> bool {
        if self.sleep_left() != Some(Duration::ZERO) {
            return false;
        }
        self.sleep_at = None;
        self.player.send(Cmd::Stop);
        true
    }

    /// Sends a playback command: pause, next, seek, speed and so on.
    pub fn send(&self, cmd: Cmd) {
        self.player.send(cmd);
    }

    /// Changes the volume by `delta`, from 0 to 1.
    pub fn volume_by(&self, delta: f32) {
        // The player's own value, not a frame-old copy, so quick presses all count.
        let v = (self.player.volume() + delta).clamp(0.0, 1.0);
        self.player.send(Cmd::SetVolume(v));
    }

    /// Which ReplayGain applies, from the audible position on.
    pub fn set_replaygain(&mut self, replaygain: ReplayGain) -> Notice {
        let loading = self.replaygain == ReplayGain::Off;
        self.replaygain = replaygain;
        if loading {
            self.send_gains();
        }
        self.player.send(Cmd::SetReplayGain(replaygain));
        Outcome::ReplayGain(replaygain).into()
    }

    pub fn replaygain(&self) -> ReplayGain {
        self.replaygain
    }

    pub fn set_mode(&self, mode: Mode) -> Notice {
        self.player.send(Cmd::SetMode(mode));
        Outcome::Mode(mode).into()
    }

    /// Moves to the next playback mode, or the previous one.
    pub fn cycle_mode(&self, forward: bool) -> Notice {
        let current = self.player.mode();
        self.set_mode(if forward {
            current.next()
        } else {
            current.prev()
        })
    }

    /// Plays playlist `id` from its first track.
    pub fn play_playlist(&mut self, id: i64) -> Notice {
        let Some(name) = self.playlist_name(id) else {
            return Refusal::PlaylistEmpty.into();
        };
        let tracks = self.playlist_tracks(id);
        if tracks.is_empty() {
            return Refusal::PlaylistEmpty.into();
        }
        match self.play_queued(&tracks, 0) {
            Some(replaced) => replaced.into(),
            None => Outcome::PlayingPlaylist { name }.into(),
        }
    }

    /// Plays the one playlist named `name`, matched as `playr playlist` does.
    pub fn play_playlist_named(&mut self, name: &str) -> Notice {
        let name = name.trim();
        match query::find_playlist(&self.playlists, name) {
            Some(pl) => self.play_playlist(pl.id),
            None => Refusal::NoPlaylistNamed(name.to_string()).into(),
        }
    }

    /// The playing track's path and source rate, read from the player now.
    pub fn playing_track(&self) -> Result<(PathBuf, u32), Refusal> {
        let status = self.player.status();
        match (status.state, status.current(), status.source) {
            (State::Playing | State::Paused, Some(path), Some(source)) => {
                Ok((path.clone(), source.rate))
            }
            _ => Err(Refusal::NothingPlaying),
        }
    }

    // --- selection ---

    /// Replaces the selection, as when files are given on the command line.
    pub fn set_selection(&mut self, tracks: Vec<Track>) {
        self.selection = tracks;
        self.selection_changed();
    }

    /// Selects `track`, or unselects every copy of it if it is selected.
    pub fn toggle_selected(&mut self, track: Track) -> Outcome {
        let outcome = if self.selection.iter().any(|t| t.path == track.path) {
            self.selection.retain(|t| t.path != track.path);
            Outcome::RemovedFromSelection
        } else {
            self.selection.push(track);
            Outcome::AddedToSelection
        };
        self.selection_changed();
        outcome
    }

    /// Selects `track`, unless it is selected already.
    pub fn add_to_selection(&mut self, track: Track) -> Outcome {
        if self.selection.iter().any(|t| t.path == track.path) {
            return Outcome::AlreadyInSelection;
        }
        self.selection.push(track);
        self.selection_changed();
        Outcome::AddedToSelection
    }

    /// Adds the tracks of playlist `id` that are not selected yet, or nothing
    /// if the playlist has no tracks. Repeats within the playlist stay, so a
    /// playlist that repeats a track on purpose keeps doing so.
    pub fn add_playlist_to_selection(&mut self, id: i64) -> Option<Outcome> {
        let added = self.playlist_tracks(id);
        self.add_all_to_selection(added)
    }

    /// Adds the `added` tracks not selected yet, or nothing if there are
    /// none; repeats within `added` stay.
    pub fn add_all_to_selection(&mut self, added: Vec<Track>) -> Option<Outcome> {
        if added.is_empty() {
            return None;
        }
        let selected: HashSet<&str> = self.selection.iter().map(|t| t.path.as_str()).collect();
        let new: Vec<Track> = added
            .into_iter()
            .filter(|t| !selected.contains(t.path.as_str()))
            .collect();
        if new.is_empty() {
            return Some(Outcome::AlreadyInSelection);
        }
        self.selection.extend(new);
        self.selection_changed();
        Some(Outcome::AddedToSelection)
    }

    /// Removes the track at `index`, or nothing if there is none.
    pub fn remove_from_selection(&mut self, index: usize) -> Option<Outcome> {
        let track = (index < self.selection.len()).then(|| self.selection.remove(index))?;
        self.selection_changed();
        Some(Outcome::RemovedTrack {
            title: track.display_title(),
        })
    }

    /// Moves the track at `index` by `by` places, returning where it is now,
    /// or nothing if either place is outside the selection.
    pub fn move_in_selection(&mut self, index: usize, by: i64) -> Option<usize> {
        let to = usize::try_from(index as i64 + by).ok()?;
        if index >= self.selection.len() || to >= self.selection.len() {
            return None;
        }
        // Moved, not swapped: the tracks between keep their order.
        let track = self.selection.remove(index);
        self.selection.insert(to, track);
        self.selection_changed();
        Some(to)
    }

    pub fn clear_selection(&mut self) -> Outcome {
        self.selection.clear();
        if self.editing.is_some() {
            self.set_editing(None);
        }
        self.selection_changed();
        Outcome::SelectionCleared
    }

    // --- playlists ---

    /// Whether the selection can be saved as a playlist at all.
    pub fn check_save(&self) -> Result<(), Refusal> {
        if self.selection.is_empty() {
            Err(Refusal::SelectionEmpty)
        } else if !self.has_library_file() {
            // In memory, the playlist would be lost on exit.
            Err(Refusal::NoLibraryFile)
        } else {
            Ok(())
        }
    }

    /// Saves the selection as the playlist `name`. Replacing a playlist of that
    /// name is refused with [`Refusal::WouldReplace`] unless `replace` is set,
    /// so a frontend can ask first.
    pub fn save_selection(&mut self, name: &str, replace: bool) -> Notice {
        // Saving an edit over its playlist is what the edit was for.
        let edited = self.editing().is_some_and(|p| p.name == name.trim());
        let tracks = self.selection.clone();
        let notice = self.save_tracks(name, &tracks, replace || edited);
        if matches!(notice, Notice::Done(_)) && self.editing.is_some() {
            self.set_editing(None);
        }
        notice
    }

    /// As [`check_save`](Self::check_save), for the queue.
    pub fn check_save_queue(&self) -> Result<(), Refusal> {
        if self.played.is_empty() && self.queue_rows().is_empty() {
            Err(Refusal::QueueEmpty)
        } else if !self.has_library_file() {
            Err(Refusal::NoLibraryFile)
        } else {
            Ok(())
        }
    }

    /// As [`save_selection`](Self::save_selection), for every row of the
    /// Queue view: played, playing and waiting.
    pub fn save_queue(&mut self, name: &str, replace: bool) -> Notice {
        let tracks = self.queue_tracks();
        self.save_tracks(name, &tracks, replace)
    }

    fn save_tracks(&mut self, name: &str, tracks: &[Track], replace: bool) -> Notice {
        let name = name.trim();
        if name.is_empty() {
            return Refusal::NameEmpty.into();
        }
        if reserved(name) {
            return Refusal::NameReserved(name.to_string()).into();
        }
        if self.searches.iter().any(|s| s.name == name) {
            return Refusal::NameTaken(name.to_string()).into();
        }
        if !replace && self.playlists.iter().any(|p| p.name == name) {
            return Refusal::WouldReplace(name.to_string()).into();
        }
        // A playlist holds only library tracks; files given on the command line
        // are selected without being in the library.
        let ids: Vec<i64> = tracks.iter().map(|t| t.id).filter(|id| *id != 0).collect();
        match query::save_playlist(&mut self.conn, name, &ids) {
            Ok(_) => {
                self.playlists = query::playlists(&self.conn).unwrap_or_default();
                Outcome::Saved {
                    name: name.to_string(),
                    tracks: ids.len(),
                    left_out: tracks.len() - ids.len(),
                }
                .into()
            }
            Err(e) => Notice::Failed {
                task: Task::Save,
                error: e.to_string(),
            },
        }
    }

    /// Renames playlist `id` to `name`, unless the name is empty, unchanged
    /// or taken.
    pub fn rename_playlist(&mut self, id: i64, name: &str) -> Notice {
        let name = name.trim();
        let from = self.playlist_name(id).unwrap_or_default();
        if name.is_empty() {
            return Refusal::NameEmpty.into();
        }
        if name == from {
            return Refusal::NameUnchanged.into();
        }
        if reserved(name) {
            return Refusal::NameReserved(name.to_string()).into();
        }
        if self.name_taken(name) {
            // Renaming onto it would have to merge or replace two playlists.
            return Refusal::NameTaken(name.to_string()).into();
        }
        if let Err(e) = query::rename_playlist(&self.conn, id, name) {
            return Notice::Failed {
                task: Task::Rename,
                error: e.to_string(),
            };
        }
        self.playlists = query::playlists(&self.conn).unwrap_or_default();
        Outcome::Renamed {
            from,
            to: name.to_string(),
        }
        .into()
    }

    /// Deletes playlist `id`. Returns nothing if the library refuses.
    pub fn delete_playlist(&mut self, id: i64) -> Option<Notice> {
        let name = self.playlist_name(id)?;
        query::delete_playlist(&self.conn, id).ok()?;
        self.playlists = query::playlists(&self.conn).unwrap_or_default();
        Some(Outcome::Deleted { name }.into())
    }

    fn playlist_name(&self, id: i64) -> Option<String> {
        self.playlists
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
    }

    // --- marks ---

    /// The marks in the track at `path`, earliest first, read from the
    /// library when `path` is not the track they were last read for.
    pub fn marks_for(&mut self, path: Option<&PathBuf>) -> &[Mark] {
        if self.marks_for.as_ref() != path {
            self.marks = path
                .and_then(|p| query::marks(&self.conn, &p.to_string_lossy()).ok())
                .unwrap_or_default();
            self.marks_for = path.cloned();
        }
        &self.marks
    }

    /// The loops saved in the track at `path`, by slot from 1, in source frames.
    pub fn loops_for(&mut self, path: Option<&PathBuf>) -> Loops {
        if self.loops_for.as_ref() != path {
            self.loops = [None; LOOP_SLOTS as usize];
            let rows = path.and_then(|p| query::loops(&self.conn, &p.to_string_lossy()).ok());
            for (slot, start, end) in rows.into_iter().flatten() {
                if let Some(held) = self.loops.get_mut(usize::from(slot).wrapping_sub(1)) {
                    *held = Some((start, end));
                }
            }
            self.loops_for = path.cloned();
        }
        self.loops
    }

    /// Saves `span` as loop `slot`, from 1, of the playing track.
    pub fn save_loop(&mut self, slot: u8, span: (u64, u64)) -> Notice {
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.loops_for = None;
        match query::save_loop(&self.conn, &path.to_string_lossy(), slot, span, rate) {
            Ok(()) => Outcome::LoopSaved {
                slot,
                kept: self.has_library_file(),
            }
            .into(),
            Err(e) => Notice::Failed {
                task: Task::Loop,
                error: e.to_string(),
            },
        }
    }

    /// Empties loop `slot` of the playing track.
    pub fn clear_loop(&mut self, slot: u8) -> Notice {
        let (path, _) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.loops_for = None;
        match query::clear_loop(&self.conn, &path.to_string_lossy(), slot) {
            Ok(_) => Outcome::LoopCleared { slot }.into(),
            Err(e) => Notice::Failed {
                task: Task::Loop,
                error: e.to_string(),
            },
        }
    }

    /// The playing track and how many loops it keeps, before asking to clear
    /// them all.
    pub fn loops_to_clear(&mut self) -> Result<(PathBuf, usize), Refusal> {
        let (path, _) = self.playing_track()?;
        match self.loops_for(Some(&path)).iter().flatten().count() {
            0 => Err(Refusal::NoLoops),
            n => Ok((path, n)),
        }
    }

    /// Empties every loop slot of the track at `path`, the one asked about.
    pub fn clear_loops(&mut self, path: &Path) -> Notice {
        self.loops_for = None;
        match query::clear_loops(&self.conn, &path.to_string_lossy()) {
            Ok(_) => Outcome::LoopsCleared.into(),
            Err(e) => Notice::Failed {
                task: Task::Loop,
                error: e.to_string(),
            },
        }
    }

    /// Marks `at` in the playing track, or its position now, unless a mark is
    /// already within [`MARK_NEAR`].
    pub fn add_mark(&mut self, at: Option<Duration>) -> Notice {
        self.add_mark_within(at, MARK_NEAR)
    }

    /// As [`Session::add_mark`], refusing a mark within `near` of another, or
    /// on the same frame.
    pub fn add_mark_within(&mut self, at: Option<Duration>, near: Duration) -> Notice {
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        let at = at.unwrap_or_else(|| self.player.position());
        let mark = Mark::at_time(at, rate);
        if let Some(other) = self
            .marks
            .iter()
            .find(|m| m.frame == mark.frame || m.time().abs_diff(at) < near)
        {
            return Refusal::AlreadyMarked { at: other.time() }.into();
        }
        if let Err(e) = query::add_mark(&self.conn, &path.to_string_lossy(), mark) {
            return Notice::Failed {
                task: Task::Mark,
                error: e.to_string(),
            };
        }
        self.marks.push(mark);
        self.marks.sort_by_key(|m| m.frame);
        Outcome::Marked {
            at,
            kept: self.has_library_file(),
        }
        .into()
    }

    /// The mark within `within` frames of `frame` in the playing track, nearest
    /// first. For a frontend acting on "the mark under the cursor".
    pub fn mark_near(&mut self, frame: u64, within: u64) -> Option<Mark> {
        let path = self.playing_track().ok()?.0;
        self.marks_for(Some(&path));
        self.marks
            .iter()
            .filter(|m| m.frame.abs_diff(frame) <= within)
            .min_by_key(|m| m.frame.abs_diff(frame))
            .copied()
    }

    /// Moves the mark at `from` to `to` in the playing track.
    pub fn move_mark(&mut self, from: u64, to: u64) -> Notice {
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        let time = |f: u64| Duration::from_secs_f64(f as f64 / rate.max(1) as f64);
        if !self.marks.iter().any(|m| m.frame == from) {
            return Refusal::NoMarkHere.into();
        }
        if let Some(other) = self.marks.iter().find(|m| m.frame == to && m.frame != from) {
            return Refusal::MarkInTheWay { at: other.time() }.into();
        }
        match query::move_mark(&self.conn, &path.to_string_lossy(), from, to) {
            Ok(true) => {
                self.marks_for(None);
                self.marks_for(Some(&path));
                Outcome::MarkMoved {
                    from: time(from),
                    to: time(to),
                }
                .into()
            }
            Ok(false) => Refusal::NoMarkHere.into(),
            Err(e) => Notice::Failed {
                task: Task::MoveMark,
                error: e.to_string(),
            },
        }
    }

    /// Removes the mark at `frame` in the playing track, wherever it sits in
    /// the order marks were made.
    pub fn remove_mark(&mut self, frame: u64) -> Notice {
        let (path, rate) = match self.playing_track() {
            Ok(track) => track,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        match query::remove_mark(&self.conn, &path.to_string_lossy(), frame) {
            Ok(true) => {
                self.marks.retain(|m| m.frame != frame);
                Outcome::MarkRemoved {
                    at: Duration::from_secs_f64(frame as f64 / rate.max(1) as f64),
                }
                .into()
            }
            Ok(false) => Refusal::NoMarkHere.into(),
            Err(e) => Notice::Failed {
                task: Task::RemoveMark,
                error: e.to_string(),
            },
        }
    }

    /// Looks for the onset nearest the mark at `from`, on a job, since it
    /// decodes. Finishes with [`Event::Snapped`].
    pub fn snap_mark(&mut self, from: u64, sensitivity: f32) -> Result<JobId, Refusal> {
        let (track, rate) = self.playing_track()?;
        Ok(self.spawn(move |job| {
            let result = crate::samples::nearest_onset(&track, rate, from, sensitivity);
            Some(Event::Snapped {
                job,
                track,
                from,
                result,
            })
        }))
    }

    /// Removes the mark added most recently to the playing track: marks are a
    /// chain, undone in the order they were made.
    pub fn undo_mark(&mut self) -> Notice {
        let path = match self.playing_track() {
            Ok((path, _)) => path,
            Err(refusal) => return refusal.into(),
        };
        self.marks_for(Some(&path));
        match query::remove_last_mark(&self.conn, &path.to_string_lossy()) {
            Ok(Some(mark)) => {
                self.marks.retain(|m| m.frame != mark.frame);
                Outcome::MarkRemoved { at: mark.time() }.into()
            }
            Ok(None) => Refusal::NoMarks.into(),
            Err(e) => Notice::Failed {
                task: Task::RemoveMark,
                error: e.to_string(),
            },
        }
    }

    /// The playing track and how many marks clearing it would remove, for a
    /// frontend to confirm before [`Session::clear_marks`].
    pub fn marks_to_clear(&mut self) -> Result<(PathBuf, usize), Refusal> {
        let (path, _) = self.playing_track()?;
        match self.marks_for(Some(&path)).len() {
            0 => Err(Refusal::NoMarks),
            n => Ok((path, n)),
        }
    }

    /// Clears the marks of the track at `path`, or does nothing if it has none.
    ///
    /// The path is the one the frontend asked about, since the playing track
    /// can change before the answer.
    pub fn clear_marks(&mut self, path: &Path) -> Option<Notice> {
        match query::clear_marks(&self.conn, &path.to_string_lossy()) {
            Ok(0) => None,
            Ok(_) => {
                if self.marks_for.as_deref() == Some(path) {
                    self.marks.clear();
                }
                Some(Outcome::MarksCleared.into())
            }
            Err(e) => Some(Notice::Failed {
                task: Task::ClearMarks,
                error: e.to_string(),
            }),
        }
    }

    /// Seeks to the next mark, or back to the previous one.
    ///
    /// Back skips a mark less than [`MARK_BACK`] behind, as going to the
    /// previous track restarts one rather than leaving it, so stepping back
    /// twice passes two marks.
    pub fn seek_to_mark(&mut self, forward: bool) -> Notice {
        let path = match self.playing_track() {
            Ok((path, _)) => path,
            Err(refusal) => return refusal.into(),
        };
        let at = self.player.position();
        let mut times = self.marks_for(Some(&path)).iter().map(Mark::time);
        let target = if forward {
            times.find(|t| *t > at + MARK_NEAR / 2)
        } else {
            times.rev().find(|t| *t + MARK_BACK < at)
        };
        match target {
            Some(t) => {
                self.player.send(Cmd::Seek(t));
                Outcome::AtMark { at: t }.into()
            }
            None if forward => Refusal::NoLaterMark.into(),
            None => Refusal::NoEarlierMark.into(),
        }
    }

    // --- background work ---

    /// Runs `work` on its own thread, which sends its result as an event.
    fn spawn(&mut self, work: impl FnOnce(JobId) -> Option<Event> + Send + 'static) -> JobId {
        let job = self.next_job;
        self.next_job += 1;
        let events = self.events.clone();
        std::thread::spawn(move || {
            if let Some(event) = work(job) {
                events(event);
            }
        });
        job
    }

    /// Reads the peaks of `track`, stopping any read still running, which
    /// then sends nothing. Finishes with [`Event::Peaks`].
    pub fn read_peaks(&mut self, track: PathBuf) -> JobId {
        self.cancel_peaks();
        let cancel = Arc::new(AtomicBool::new(false));
        self.reading = Some(cancel.clone());
        self.spawn(move |job| {
            let result = match Peaks::read(&track, &cancel) {
                Ok(Some(peaks)) => Ok(Arc::new(peaks)),
                Ok(None) => return None,
                Err(e) => Err(e),
            };
            Some(Event::Peaks { job, track, result })
        })
    }

    /// Decodes frames `start..end` of `track`, which counts frames at `rate`,
    /// for a view too close for its peaks. Finishes with [`Event::Detail`].
    pub fn read_detail(&mut self, track: PathBuf, rate: u32, start: u64, end: u64) -> JobId {
        self.spawn(move |job| {
            let result = crate::wave::Detail::read(&track, rate, start, end).map(Arc::new);
            Some(Event::Detail { job, track, result })
        })
    }

    /// Stops the peaks read in progress, if there is one.
    pub fn cancel_peaks(&mut self) {
        if let Some(cancel) = self.reading.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Plans slices of the playing track, `cut`'s way. Finishes with
    /// [`Event::Planned`]; nothing is written.
    pub fn plan_slices(&mut self, cut: Cut, range: Option<(u64, u64)>) -> Result<JobId, Refusal> {
        let job = self.slice_job(cut, range)?;
        Ok(self.plan_job(job))
    }

    /// Plans `job` again with the slice edges and fades set now, as when they
    /// change after it was planned. Finishes with [`Event::Planned`].
    pub fn replan(&mut self, mut job: Job) -> JobId {
        job.edges = self.edges;
        job.fades = self.fades;
        self.plan_job(job)
    }

    /// Whether `job` was planned with the slice edges and fades set now.
    pub fn plans_current(&self, job: &Job) -> bool {
        (job.edges, job.fades) == (self.edges, self.fades)
    }

    fn plan_job(&mut self, job: Job) -> JobId {
        let audio = self.onset_audio.clone();
        self.spawn(move |id| {
            let result = samples::plan_with(&job, &audio).map(|spans| Plan {
                job: job.clone(),
                spans,
            });
            Some(Event::Planned {
                job: id,
                track: job.path,
                result,
            })
        })
    }

    /// Writes `plan`'s slices. Finishes with [`Event::Exported`].
    pub fn write_slices(&mut self, plan: Plan) -> JobId {
        self.spawn(move |job| {
            Some(Event::Exported {
                job,
                result: samples::write(&plan.job, &plan.spans),
            })
        })
    }

    /// Plans and writes slices of the playing track at once, `cut`'s way.
    /// Finishes with [`Event::Exported`].
    pub fn export(&mut self, cut: Cut, range: Option<(u64, u64)>) -> Result<JobId, Refusal> {
        let work = self.slice_job(cut, range)?;
        Ok(self.spawn(move |job| {
            Some(Event::Exported {
                job,
                result: samples::export(&work),
            })
        }))
    }

    /// Converts `export`, or the last export, to `format` with
    /// ConvertWithMoss, into a directory of that name inside the export's.
    /// A relative `export` is under the samples directory. Finishes with
    /// [`Event::Converted`].
    pub fn convert(&mut self, format: String, export: Option<PathBuf>) -> Result<JobId, Refusal> {
        let program = self.convertwithmoss.clone().ok_or(Refusal::ConvertOff)?;
        if !program.is_file() {
            return Err(Refusal::NoConvertWithMoss(program));
        }
        let dir = match export {
            Some(dir) => self.samples.join(dir),
            None => self.exported.clone().ok_or(Refusal::NothingExported)?,
        };
        if !samples::kit_path(&dir).is_file() {
            return Err(Refusal::NoKit(dir));
        }
        Ok(self.spawn(move |job| {
            let result =
                crate::convertwithmoss::convert(&program, &samples::kit_path(&dir), &format);
            Some(Event::Converted { job, result })
        }))
    }

    /// Scans `dir` into the library file, creating it if there is none.
    /// Sends [`Event::ScanProgress`] as it goes and finishes with
    /// [`Event::Scanned`], after which the frontend calls [`Session::scanned`].
    /// The directory is recorded so a later [`Session::rescan`] covers it.
    pub fn scan(&mut self, dir: PathBuf) -> Result<JobId, Refusal> {
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryPath);
        };
        if !dir.is_dir() {
            return Err(Refusal::NotADirectory(dir));
        }
        if self.scanning.swap(true, Ordering::Relaxed) {
            return Err(Refusal::ScanRunning);
        }
        let (scanning, events) = (self.scanning.clone(), self.events.clone());
        Ok(self.spawn(move |job| {
            let result = caught("scan", || {
                scan::scan_into(&library, &dir, |stats| {
                    if stats.seen % crate::event::SCAN_PROGRESS_EVERY == 0 {
                        events(Event::ScanProgress {
                            job,
                            seen: stats.seen,
                            added: stats.added,
                        });
                    }
                })
            });
            scanning.store(false, Ordering::Relaxed);
            Some(Event::Scanned {
                job,
                dir: Some(dir),
                result,
            })
        }))
    }

    /// Re-scans every directory previously given to [`Session::scan`] or
    /// `playr scan`. Sends the same progress events as a single scan.
    pub fn rescan(&mut self) -> Result<JobId, Refusal> {
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryPath);
        };
        if !self.has_library_file() {
            return Err(Refusal::NoLibraryFile);
        }
        let roots = db::roots(&self.conn).unwrap_or_default();
        if roots.is_empty() {
            return Err(Refusal::NoRoots);
        }
        if self.scanning.swap(true, Ordering::Relaxed) {
            return Err(Refusal::ScanRunning);
        }
        let (scanning, events) = (self.scanning.clone(), self.events.clone());
        Ok(self.spawn(move |job| {
            let result = caught("scan", || {
                scan::scan_roots(&library, &roots, |stats| {
                    if stats.seen > 0 && stats.seen % crate::event::SCAN_PROGRESS_EVERY == 0 {
                        events(Event::ScanProgress {
                            job,
                            seen: stats.seen,
                            added: stats.added,
                        });
                    }
                })
            });
            scanning.store(false, Ordering::Relaxed);
            Some(Event::Scanned {
                job,
                dir: None,
                result,
            })
        }))
    }

    /// Analyses the library tracks under `dir`, or every track for `None`,
    /// whose stored analysis is missing or out of date. Sends
    /// [`Event::AnalyzeProgress`] per file and finishes with
    /// [`Event::Analysed`], after which the frontend calls
    /// [`Session::analysed`].
    ///
    /// It runs alongside playback and alongside a scan: it writes only the
    /// `analysis` and `album_loudness` tables, which nothing here caches.
    pub fn analyze(&mut self, dir: Option<PathBuf>) -> Result<JobId, Refusal> {
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryPath);
        };
        if !self.has_library_file() {
            return Err(Refusal::NoLibraryFile);
        }
        if self.analysing.swap(true, Ordering::Relaxed) {
            return Err(Refusal::AnalysisRunning);
        }
        let paths: Vec<PathBuf> = dir.into_iter().collect();
        let (analysing, events) = (self.analysing.clone(), self.events.clone());
        Ok(self.spawn(move |job| {
            let result = caught("analysis", || {
                analysis::run_into(
                    &library,
                    &paths,
                    false,
                    analysis::default_workers(),
                    |done, total| events(Event::AnalyzeProgress { job, done, total }),
                )
            });
            analysing.store(false, Ordering::Relaxed);
            Some(Event::Analysed { job, result })
        }))
    }

    /// Takes in a finished analysis: reads the library's gains again, so
    /// tracks analysed just now play at their measured level.
    pub fn analysed(&mut self) {
        self.send_gains();
    }

    /// What `playr analyze` measured about `path`, when the row still
    /// describes the file as the library knows it.
    pub fn analysis_of(&self, path: &Path) -> Option<analysis::Analysis> {
        let key = path.to_str()?;
        let (stat, a) = db::analysis::row_of(&self.conn, key).ok().flatten()?;
        let track = self.tracks.iter().find(|t| t.path == key)?;
        analysis::is_current(track, Some(&stat)).then_some(a)
    }

    /// The gains `path` would play at, whether or not ReplayGain is on.
    ///
    /// It reads the whole library's gains, as turning ReplayGain on does, so
    /// it belongs to a dialog rather than to a frame.
    pub fn gains_of(&self, path: &Path) -> crate::gain::Gains {
        db::analysis::gains(&self.conn, &self.tracks)
            .ok()
            .and_then(|mut gains| gains.remove(path))
            .unwrap_or_default()
    }

    /// The tempo to show for `path`, from `playr analyze`.
    pub fn bpm(&self, path: &Path) -> Option<f32> {
        db::analysis::bpm_of(&self.conn, path.to_str()?)
            .ok()
            .flatten()
    }

    /// Takes in a finished scan: reads the library again, from its file if
    /// the session was running without one. Marks added to a library that was
    /// only in memory are not in the file, and are gone.
    pub fn scanned(&mut self) {
        if !self.has_library_file() {
            if let Some(conn) = self.library.as_deref().and_then(|p| db::open(p).ok()) {
                self.conn = conn;
                self.marks_for = None;
                self.marks.clear();
            }
        }
        self.reload();
    }

    /// Whether `dir` can be pruned, or every recorded root when `None`, for a
    /// frontend to refuse before it asks.
    pub fn check_prune(&self, dir: Option<&Path>) -> Result<(), Refusal> {
        if !self.has_library_file() {
            Err(Refusal::NoLibraryFile)
        } else if self.scanning.load(Ordering::Relaxed) {
            Err(Refusal::ScanRunning)
        } else {
            match dir {
                Some(dir) if !dir.is_dir() => Err(Refusal::NotADirectory(dir.to_path_buf())),
                None if db::roots(&self.conn).ok().is_none_or(|r| r.is_empty()) => {
                    Err(Refusal::NoRoots)
                }
                _ => Ok(()),
            }
        }
    }

    /// Removes the tracks under `dir` whose files are gone, and the marks of
    /// every file under it that is gone; with `None`, every recorded root.
    /// Finishes with [`Event::Pruned`], after which the frontend calls
    /// [`Session::pruned`]. Shares a scan's turn: one of either runs at a time.
    pub fn prune(&mut self, dir: Option<PathBuf>) -> Result<JobId, Refusal> {
        self.check_prune(dir.as_deref())?;
        let Some(library) = self.library.clone() else {
            return Err(Refusal::NoLibraryFile);
        };
        let dirs = match &dir {
            Some(d) => vec![d.clone()],
            None => db::roots(&self.conn).unwrap_or_default(),
        };
        if self.scanning.swap(true, Ordering::Relaxed) {
            return Err(Refusal::ScanRunning);
        }
        let scanning = self.scanning.clone();
        Ok(self.spawn(move |job| {
            let result = caught("prune", || {
                let fail = |e: rusqlite::Error| format!("{}: {e}", library.display());
                let conn = db::open(&library).map_err(fail)?;
                let mut removed = crate::db::Pruned::default();
                for d in &dirs {
                    let batch = db::prune_missing(&conn, d).map_err(fail)?;
                    removed.tracks += batch.tracks;
                    removed.marks += batch.marks;
                }
                Ok(removed)
            });
            scanning.store(false, Ordering::Relaxed);
            Some(Event::Pruned { job, dir, result })
        }))
    }

    /// Directories previously scanned into this library.
    pub fn roots(&self) -> Vec<PathBuf> {
        db::roots(&self.conn).unwrap_or_default()
    }

    /// Whether `dir` can be forgotten, for a frontend to refuse before it asks.
    pub fn check_forget(&self, dir: &Path) -> Result<(), Refusal> {
        if !self.has_library_file() {
            Err(Refusal::NoLibraryFile)
        } else if self.scanning.load(Ordering::Relaxed) {
            Err(Refusal::ScanRunning)
        } else if db::stored_root(&self.conn, dir).ok().flatten().is_none() {
            Err(Refusal::NotARoot(dir.to_path_buf()))
        } else {
            Ok(())
        }
    }

    /// Forgets `dir` as a root and removes the tracks and marks under it,
    /// then reads the library again.
    ///
    /// Done here rather than on a job: it asks the filesystem nothing, so it
    /// is one transaction rather than a walk of every file. It still waits for
    /// a scan, which writes the same rows.
    pub fn forget_root(&mut self, dir: &Path) -> Result<Pruned, Refusal> {
        self.check_forget(dir)?;
        let removed = db::forget_root(&self.conn, dir)
            .ok()
            .flatten()
            .ok_or_else(|| Refusal::NotARoot(dir.to_path_buf()))?;
        self.pruned();
        Ok(removed)
    }

    /// Takes in a finished prune: reads the library and its marks again.
    pub fn pruned(&mut self) {
        self.marks_for = None;
        self.marks.clear();
        self.reload();
    }

    /// Gathers `paths`, files and directories, into tracks to play, with tags
    /// from the library where it has them. Finishes with [`Event::Opened`].
    pub fn open(&mut self, paths: Vec<PathBuf>) -> JobId {
        let library = self.library.clone().filter(|_| self.has_library_file());
        self.spawn(move |job| {
            let conn = library.and_then(|p| db::open(&p).ok());
            let known = |key: &str| {
                conn.as_ref()
                    .and_then(|c| query::by_path(c, key).ok().flatten())
            };
            Some(Event::Opened {
                job,
                playable: scan::playable(&paths, known),
            })
        })
    }

    // --- slices ---

    /// The job for cutting the playing track `cut`'s way, at its marks and
    /// position now, or within `range`.
    pub fn slice_job(&mut self, cut: Cut, range: Option<(u64, u64)>) -> Result<Job, Refusal> {
        let (path, rate) = self.playing_track()?;
        // The loop follows the range, so a loop on is the range's.
        let loops = cut == Cut::Region && range.is_some() && self.player.status().looping.is_some();
        let marks = self
            .marks_for(Some(&path))
            .iter()
            .map(|m| m.frame)
            .collect();
        Ok(Job {
            path,
            rate,
            marks,
            at: Mark::at_time(self.player.position(), rate).frame,
            cut,
            range,
            samples: self.samples.clone(),
            edges: self.edges,
            fades: self.fades,
            loops,
            ot_file: self.ot_file,
        })
    }
}

/// `work`'s result, or its panic as an error. A panic in a tag reader or a
/// decoder would otherwise end a job's thread with no event sent.
fn caught<T>(what: &str, work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|panic| {
        let text = panic
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("no message");
        Err(format!("the {what} stopped: {text}"))
    })
}

/// The queued tracks waiting after the one playing, as indices into the
/// player's list.
fn waiting(status: &crate::audio::Status) -> impl Iterator<Item = usize> + '_ {
    (status.index + 1..status.queued.len()).filter(|&i| status.queued[i])
}
