//! What the web page is sent, and what it may do.
//!
//! The page draws the model as the window does: [`screen`] is pushed to it as
//! the model changes, and it fetches [`rows`] for the part of a list it shows.
//! Everything it does is an action through `dispatch`, checked by
//! [`allowed`]. It has no sampler.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use playr_app::action::Action;
use playr_app::command::{self, view_name};
use playr_app::dispatch::Frontend;
use playr_app::message::{self, fmt_time};
use playr_app::meter;
use playr_app::model::{Input, Model};
use playr_app::View;
use playr_core::audio::{speed_for, Mode, State};
use playr_core::db::Track;
use serde_json::{json, Value};

use crate::state;

/// The views the page shows, in tab order.
pub const VIEWS: [View; 4] = [View::Library, View::Queue, View::Selection, View::Playlists];

/// Whether the page may perform `action`. Not quitting, which would stop the
/// server; not a path from the page, which could name any file; not changing
/// the keys, which the page reads once; and nothing of the sampler or tape.
/// `:rescan` is allowed: it only covers directories already recorded by a scan.
///
/// Every variant is named, so a new action does not compile until it is
/// decided here.
pub fn allowed(action: &Action) -> bool {
    use Action::*;
    match action {
        Quit
        | Scan(_)
        | Analyze(Some(_))
        | Prune(Some(_))
        | ForgetRoot(_)
        | Open(_)
        | Map { .. }
        | Unmap { .. } => false,
        ShowView(View::Sampler | View::Tape | View::Dj | View::Mix)
        | Slice(_)
        | FixTempo(_)
        | Audition
        | AuditionSlice(_)
        | Scrub(_)
        | SelectMarkAt(_)
        | Deselect
        | MoveSelected(_)
        | MoveSelectedTo(_)
        | SelectSliceAt(_)
        | SnapSelected
        | RemoveSelected
        | Zoom(_)
        | Display(_)
        | Nudge(_)
        | Snap(_)
        | Fit(_)
        | SetSliceEdges(_)
        | RangeIn(_)
        | RangeOut(_)
        | SetRange(_)
        | Loop(_)
        | LoopSlot(..)
        | ClearLoops
        | PickEdge(_)
        | SelectEdge(_)
        | WriteSlices
        | DiscardSlices
        | MarkSlices
        | Convert(..)
        // Not in the first version: they would play on the server's device.
        | Tape(_)
        | Dj(_) => false,
        Help | CommandHelp | ShowView(_) | NextView | PrevView | Cursor(_) | CursorFirst
        | CursorLast | StartSearch | Search(_) | ClearSearch | StartCommand | Activate | Add
        | Enqueue(_) | EnqueueAll | ClearQueue | Remove | MoveTrack(_) | ClearSelection
        | StartSave | SaveAs(_) | DeletePlaylist | EditPlaylist | StartSaveSearch
        | SaveSearch(_) | Sql(_) | StartRename | Analyze(None) | ShowInfo | SetColumns(_)
        | SetSort(_) | RenameTo(_) | PlayPlaylist(_) | Rescan | ShowRoots | Prune(None)
        | TogglePause | Restart | Next | Prev | Stop | SeekBy(_) | SeekTo(_) | StopAfter
        | StopIn(_) | VolumeBy(_) | SetVolume(_) | SpeedBy(_) | SetSpeed(_) | SetEq(..)
        | EqBy(..) | FlatEq | CycleMode(_) | SetMode(_) | SetReplayGain(_) | Mark | MarkAt(_)
        | UndoMark | Undo | Redo | ClearMarks | NextMark | PrevMark | Theme(_) | Mix(_) => true,
    }
}

/// The view named `name` on the page.
pub fn view_named(name: &str) -> Option<View> {
    VIEWS.into_iter().find(|v| view_name(*v) == name)
}

/// Everything the page draws apart from list rows.
pub fn screen(model: &Model) -> Value {
    let snapshot = model.snapshot();
    let status = &snapshot.status;
    let track = state::playing(model);
    let seconds = |d: Duration| (d.as_secs_f64() * 100.0).round() / 100.0;
    let tenths = |v: f32| (f64::from(v) * 10.0).round() / 10.0;
    let format = status.source.map(|src| {
        let mut text = format!("{:.1} kHz {} ch", src.rate as f64 / 1000.0, src.channels);
        // As in the other frontends: only a real rate conversion is worth showing.
        if status.output_rate != 0 && status.output_rate != src.rate {
            text.push_str(&format!(
                " -> {:.1} kHz",
                status.output_rate as f64 / 1000.0
            ));
        }
        text
    });
    let cursors = model.cursors();
    json!({
        "view": view_name(model.view()),
        "counts": {
            "library": model.listed().len(),
            "selection": model.session().selection().len(),
            "playlists": model.session().playlists().len() + model.session().searches().len(),
            "queue": model.queue().len(),
        },
        "searching": model.results().is_some(),
        "editing": model.session().editing().map(|p| p.name.clone()),
        "cursors": {
            "library": cursors.library,
            "selection": cursors.selection,
            "playlists": cursors.playlists,
            "queue": cursors.queue,
        },
        "lists": lists_revision(model),
        "input": input(model),
        "message": model.message_text(),
        "theme": model.theme().name(),
        "state": match status.state {
            State::Stopped => "stopped",
            State::Playing => "playing",
            State::Paused => "paused",
        },
        "path": status.current().map(|p| p.to_string_lossy()),
        // The queue marks the playing row by this, since a track queued
        // twice is two rows with one path.
        "queue_playing": model.queue_playing(),
        // The queue's first rows, which have played, are dimmed.
        "queue_played": model.queue_played(),
        "title": state::title(model),
        "artist": track.map(Track::display_artist),
        "format": format,
        "bpm": model.bpm().map(|b| b.round()),
        "position": seconds(snapshot.position),
        "duration": status.duration.map(seconds),
        "marks": snapshot.marks.iter().copied().map(seconds).collect::<Vec<_>>(),
        "volume": tenths(snapshot.volume * 100.0),
        "muted": model.mixer().muted(playr_app::mix::Strip::Master),
        "speed": status.semitones,
        "speed_label": format!("{:.2}x", speed_for(status.semitones)),
        "mode": mode_name(status.mode),
        "sort": model.session().sort().first().map(|k| k.text()),
        "replaygain": model.replaygain().name(),
        "gain_label": status.gain_db.map(message::replaygain),
        "stopping": message::stopping(status.stop_after, snapshot.sleep),
        "loudness": snapshot.loudness.map(tenths),
        "peak": snapshot.peak.filter(|p| p.is_finite()).map(tenths),
        "clipping": snapshot.peak.is_some_and(meter::clipping),
    })
}

/// A number that changes when a list's rows may have, so the page fetches
/// them again. The session counts changes to the library; search results are
/// a new allocation each time, so their address and length stand for them.
/// The selection and playlists are short enough to hash.
fn lists_revision(model: &Model) -> u64 {
    let mut hasher = DefaultHasher::new();
    model.session().revision().hash(&mut hasher);
    let listed = model.listed();
    (listed.as_ptr() as usize, listed.len()).hash(&mut hasher);
    // Sorting reorders a list in place, leaving its address and length alone,
    // so without this the page would keep showing the old order.
    for key in model.session().sort() {
        (key.column as u8, key.descending).hash(&mut hasher);
    }
    for t in model.session().selection() {
        t.path.hash(&mut hasher);
    }
    for p in model.session().playlists() {
        (p.id, &p.name, p.len).hash(&mut hasher);
    }
    for s in model.session().searches() {
        (s.id, &s.name, s.query.text()).hash(&mut hasher);
    }
    for t in model.queue() {
        t.path.hash(&mut hasher);
    }
    hasher.finish()
}

/// A saved search's row key, apart from any playlist's id.
fn search_key(search: &playr_core::db::query::SavedSearch) -> String {
    format!("search:{}", search.id)
}

/// The prompt, question or list open, as the page draws it.
fn input(model: &Model) -> Value {
    match model.input() {
        Input::None => json!({ "kind": "none" }),
        Input::Search(query) => json!({ "kind": "search", "text": query }),
        Input::SavePlaylist(name) => {
            json!({ "kind": "save", "text": name, "title": model.save_title() })
        }
        Input::RenamePlaylist { from, name } => {
            json!({ "kind": "rename", "text": name, "from": from.name })
        }
        Input::Confirm(question) => json!({ "kind": "confirm", "question": question.question() }),
        Input::Draft(tracks) => {
            json!({ "kind": "draft", "question": message::draft_question(*tracks) })
        }
        Input::Help => json!({ "kind": "keys" }),
        Input::CommandHelp => json!({ "kind": "commands" }),
        // Carried with the state: the page has no path of its own to ask on.
        Input::Roots(roots) => json!({
            "kind": "roots",
            "rows": roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>(),
        }),
        Input::Info(info) => json!({
            "kind": "info",
            "heading": info.title,
            "rows": info.rows,
        }),
        Input::Command(line) => json!({ "kind": "command", "text": line.text }),
    }
}

/// The name `:mode` takes, which `Mode::name` spells with a space.
pub fn mode_name(mode: Mode) -> &'static str {
    Mode::NAMES
        .iter()
        .find(|(_, m)| *m == mode)
        .map_or("normal", |(name, _)| name)
}

/// Most rows sent at once.
pub const ROWS: usize = 500;

/// Rows `start` to `start + count` of `view`, and how many it has. A track row
/// has the path the page names it by; a playlist row, its id.
pub fn rows(model: &Model, view: View, start: usize, count: usize) -> Value {
    let count = count.min(ROWS);
    if view == View::Playlists {
        let lists = model.session().playlists();
        let searches = model.session().searches();
        let rows: Vec<Value> =
            (lists.iter())
                .map(|p| json!({ "key": p.id.to_string(), "name": p.name, "tracks": p.len }))
                .chain(searches.iter().map(
                    |s| json!({ "key": search_key(s), "name": s.name, "search": s.query.text() }),
                ))
                .skip(start)
                .take(count)
                .collect();
        let total = lists.len() + searches.len();
        return json!({ "total": total, "start": start, "rows": rows });
    }
    let tracks = match view {
        View::Library => model.listed(),
        View::Queue => model.queue(),
        _ => model.session().selection(),
    };
    let selected: std::collections::HashSet<&str> = match view {
        View::Library => model
            .session()
            .selection()
            .iter()
            .map(|t| t.path.as_str())
            .collect(),
        _ => Default::default(),
    };
    let rows: Vec<Value> = tracks
        .iter()
        .skip(start)
        .take(count)
        .map(|t| {
            json!({
                "key": t.path,
                "title": t.display_title(),
                "artist": t.display_artist(),
                "album": t.display_album(),
                "time": t.duration_ms.map(|ms| fmt_time(Duration::from_millis(ms.max(0) as u64))),
                "selected": selected.contains(t.path.as_str()),
            })
        })
        .collect();
    json!({ "total": tracks.len(), "start": start, "rows": rows })
}

/// The key that names row `row` of `view` for [`rows`], if it has that row.
pub fn row_key(model: &Model, view: View, row: usize) -> Option<String> {
    match view {
        View::Library => model.listed().get(row).map(|t| t.path.clone()),
        View::Selection => model.session().selection().get(row).map(|t| t.path.clone()),
        View::Playlists => {
            let lists = model.session().playlists();
            match lists.get(row) {
                Some(p) => Some(p.id.to_string()),
                None => (model.session().searches())
                    .get(row - lists.len())
                    .map(search_key),
            }
        }
        View::Queue => model.queue().get(row).map(|t| t.path.clone()),
        View::Sampler | View::Tape | View::Dj | View::Mix => None,
    }
}

/// The page's key bindings: for each view, each key bound and the command it
/// runs, which the page sends back by name. A view's own binding wins over one
/// for every view. A key bound to nothing, or to an action the page may not
/// do, is `null`: the page takes it and does nothing.
pub fn keys(model: &Model) -> Value {
    let bindings = model.keymap().bindings();
    let mut views = serde_json::Map::new();
    for view in VIEWS {
        let mut keys = serde_json::Map::new();
        for scope in [None, Some(view)] {
            for b in bindings.iter().filter(|b| b.view == scope) {
                let command = match &b.action {
                    Some(action) if allowed(action) => json!(command::line(action, Some(view))),
                    _ => Value::Null,
                };
                // Later, the view's own, replaces earlier.
                keys.insert(b.key.to_string(), command);
            }
        }
        views.insert(view_name(view).into(), Value::Object(keys));
    }
    Value::Object(views)
}

/// The keys list for the view shown, or the commands list, without what the
/// page may not do: rows of two columns, a heading having no second.
pub fn help(model: &Model, commands: bool) -> Value {
    let view = model.view();
    let rows = if commands {
        // The page runs no extension.
        command::command_rows(false)
    } else {
        command::key_rows(model.keymap(), view)
    };
    let mut kept: Vec<(String, String)> = Vec::new();
    let mut heading = None;
    for (a, b) in rows {
        if b.is_empty() {
            heading = Some(a);
            continue;
        }
        let usable = if commands {
            usable_command(heading.as_deref(), &a)
        } else {
            command::key_target(b.trim_start_matches(':'), Some(view))
                .is_ok_and(|action| action.as_ref().is_none_or(allowed))
        };
        if usable {
            // A heading goes in once a row under it does.
            if let Some(h) = heading.take() {
                kept.push((h, String::new()));
            }
            kept.push((a, b));
        }
    }
    kept.into_iter().map(|(a, b)| json!([a, b])).collect()
}

/// Whether the command `usage`, listed under `heading`, is one the page may
/// run. Its arguments are unknown here, so it is judged by name.
fn usable_command(heading: Option<&str>, usage: &str) -> bool {
    const REFUSED: [&str; 8] = [
        "loop",
        "quit",
        "scan",
        "open",
        "map",
        "unmap",
        "slice",
        "slice-edges",
    ];
    let name = usage
        .trim_start_matches(':')
        .split(' ')
        .next()
        .unwrap_or_default();
    heading != Some(view_name(View::Sampler)) && !REFUSED.contains(&name)
}

/// What Tab completes `text` to, and the command lines entered so far.
pub fn completions(model: &Model, text: &str) -> Value {
    let names: Vec<String> = model
        .session()
        .playlists()
        .iter()
        .map(|p| p.name.clone())
        .collect();
    json!({
        // `:convert`, whose exports are listed, is not offered here.
        "completions": command::completions(text, model.view(), &names, &Vec::new, false),
        "history": model.history().lines(),
    })
}

/// The action of a `:` line the page may run, in the view shown. The error
/// is the parser's words, or why the page may not.
pub fn check(model: &Model, line: &str) -> Result<Action, (u16, String)> {
    // The page runs no extension, so its errors name none.
    let action = command::parse_typed(line.trim(), model.view(), false).map_err(|e| (400, e))?;
    if allowed(&action) {
        Ok(action)
    } else {
        Err((
            403,
            format!("not available from the web page: :{}", line.trim()),
        ))
    }
}
