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
use std::sync::{Arc, Mutex};
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

mod jobs;
mod lists;
mod marks;
mod playback;

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

/// The bytes of peaks kept for tracks read recently: about six 4-minute
/// tracks at 44.1 kHz.
pub const PEAKS_CACHE: usize = 100_000_000;

/// A track's loops by slot, from 1 at index 0, as source frames, end exclusive.
pub type Loops = [Option<(u64, u64)>; LOOP_SLOTS as usize];

/// How long files must be still before a watched change rescans.
pub const WATCH_QUIET: std::time::Duration = std::time::Duration::from_secs(2);

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
    /// The master being recorded.
    master: Option<crate::audio::record::MasterRecording>,
    /// The playlist whose tracks the selection holds to edit, by id.
    editing: Option<i64>,
    /// Marks in the track `marks_for`, earliest first.
    marks: Vec<Mark>,
    marks_for: Option<PathBuf>,
    /// The labelled marks of `marks_for`: frame and label, earliest first.
    labels: Vec<(u64, String)>,
    /// Loops saved in the track `loops_for`, by slot, from 1.
    loops: Loops,
    /// The loop library as the Loops view lists it: every loop, or those
    /// `loop_search` matches.
    loop_list: Vec<db::loops::Loop>,
    loop_search: Option<String>,
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
    /// Stops the read ahead in progress; see [`Session::read_ahead`].
    ahead: Option<Arc<AtomicBool>>,
    /// Peaks read recently, so a return to a track does not decode it again.
    peaks: Arc<Mutex<crate::wave::Cache>>,
    /// The library file a scan writes to.
    library: Option<PathBuf>,
    /// Set while a scan or a prune runs.
    scanning: Arc<AtomicBool>,
    /// Watches the roots while the `watch` setting is on; see [`Session::watch`].
    watcher: Option<notify::RecommendedWatcher>,
    /// Set while an analysis runs. Separate from `scanning`: an analysis
    /// writes only its own tables, so the two may run together.
    analysing: Arc<AtomicBool>,
    /// Tempo corrections made, for [`Session::tempo_rev`].
    tempo_rev: u64,
    replaygain: ReplayGain,
    /// What every track list is sorted by. It orders the library and every
    /// search, so the list a frontend shows is the list it plays.
    sort: Vec<SortKey>,
    /// What `playr analyze` measured, for the columns that show it and the
    /// keys that sort by it. Read with the library and after an analysis.
    measures: HashMap<String, Measures>,
    /// Counts the times `tracks` or `measures` changed; see [`Session::revision`].
    revision: u64,
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
            master: None,
            editing: None,
            marks: Vec::new(),
            marks_for: None,
            labels: Vec::new(),
            loops: [None; LOOP_SLOTS as usize],
            loops_for: None,
            loop_list: Vec::new(),
            loop_search: None,
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
            ahead: None,
            peaks: Arc::new(Mutex::new(crate::wave::Cache::new(PEAKS_CACHE))),
            library,
            scanning: Arc::default(),
            watcher: None,
            analysing: Arc::default(),
            tempo_rev: 0,
            replaygain: ReplayGain::Off,
            sort: Vec::new(),
            measures: HashMap::new(),
            revision: 0,
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
        self.reload_loops();
        self.measure();
        self.send_gains();
    }

    /// Reads what analysis measured of the tracks, and sorts them again, since
    /// a sort can be by a measurement.
    fn measure(&mut self) {
        self.measures = db::analysis::measures(&self.conn, &self.tracks).unwrap_or_default();
        let mut tracks = std::mem::take(&mut self.tracks);
        self.order(&mut tracks);
        self.tracks = tracks;
        self.revision += 1;
    }

    /// A number that changes whenever the tracks, their order or their
    /// measurements may have, for a frontend that keeps rows between frames.
    pub fn revision(&self) -> u64 {
        self.revision
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
        self.revision += 1;
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
