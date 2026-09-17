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
use playr_app::message::fmt_time;
use playr_app::meter;
use playr_app::model::{Input, Model};
use playr_app::View;
use playr_core::audio::{speed_for, Mode, State};
use playr_core::db::Track;
use serde_json::{json, Value};

use crate::state;

/// The views the page shows, in tab order.
pub const VIEWS: [View; 3] = [View::Library, View::Selection, View::Playlists];

/// Whether the page may perform `action`. Not quitting, which would stop the
/// server; not a path from the page, which could name any file; not changing
/// the keys, which the page reads once; and nothing of the sampler.
///
/// Every variant is named, so a new action does not compile until it is
/// decided here.
pub fn allowed(action: &Action) -> bool {
    use Action::*;
    match action {
        Quit | Scan(_) | Prune(_) | Open(_) | Map { .. } | Unmap { .. } => false,
        ShowView(View::Sampler)
        | Slice(_)
        | Zoom(_)
        | Display(_)
        | Nudge(_)
        | Snap(_)
        | RangeIn
        | RangeOut
        | SetRange(_)
        | Loop(_)
        | PickEdge(_)
        | MoveEdge(_)
        | WriteSlices
        | DiscardSlices => false,
        Help | CommandHelp | ShowView(_) | NextView | Cursor(_) | CursorFirst | CursorLast
        | StartSearch | Search(_) | ClearSearch | StartCommand | Activate | Add | Remove
        | MoveTrack(_) | ClearSelection | StartSave | SaveAs(_) | DeletePlaylist | StartRename
        | RenameTo(_) | PlayPlaylist(_) | TogglePause | Next | Prev | Stop | SeekBy(_)
        | SeekTo(_) | VolumeBy(_) | SetVolume(_) | SpeedBy(_) | SetSpeed(_) | CycleMode(_)
        | SetMode(_) | Mark | MarkAt(_) | UndoMark | ClearMarks | NextMark | PrevMark
        | Theme(_) => true,
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
            "playlists": model.session().playlists().len(),
        },
        "searching": model.results().is_some(),
        "cursors": {
            "library": cursors.library,
            "selection": cursors.selection,
            "playlists": cursors.playlists,
        },
        "lists": lists_revision(model),
        "input": input(model.input()),
        "message": model.message_text(),
        "theme": model.theme().name(),
        "state": match status.state {
            State::Stopped => "stopped",
            State::Playing => "playing",
            State::Paused => "paused",
        },
        "path": status.current().map(|p| p.to_string_lossy()),
        "title": state::title(model),
        "artist": track.map(Track::display_artist),
        "format": format,
        "position": seconds(snapshot.position),
        "duration": status.duration.map(seconds),
        "marks": snapshot.marks.iter().copied().map(seconds).collect::<Vec<_>>(),
        "volume": tenths(snapshot.volume * 100.0),
        "speed": status.semitones,
        "speed_label": format!("{:.2}x", speed_for(status.semitones)),
        "mode": mode_name(status.mode),
        "loudness": snapshot.loudness.map(tenths),
        "peak": snapshot.peak.filter(|p| p.is_finite()).map(tenths),
        "clipping": snapshot.peak.is_some_and(meter::clipping),
    })
}

/// A number that changes when a list's rows may have, so the page fetches
/// them again. A list replaced is a new allocation, so its address and length
/// stand for the library; the selection and playlists are short enough to hash.
fn lists_revision(model: &Model) -> u64 {
    let mut hasher = DefaultHasher::new();
    let listed = model.listed();
    (listed.as_ptr() as usize, listed.len()).hash(&mut hasher);
    for t in model.session().selection() {
        t.path.hash(&mut hasher);
    }
    for p in model.session().playlists() {
        (p.id, &p.name, p.len).hash(&mut hasher);
    }
    hasher.finish()
}

/// The prompt, question or list open, as the page draws it.
fn input(input: &Input) -> Value {
    match input {
        Input::None => json!({ "kind": "none" }),
        Input::Search(query) => json!({ "kind": "search", "text": query }),
        Input::SavePlaylist(name) => json!({ "kind": "save", "text": name }),
        Input::RenamePlaylist { from, name } => {
            json!({ "kind": "rename", "text": name, "from": from.name })
        }
        Input::Confirm(question) => json!({ "kind": "confirm", "question": question.question() }),
        Input::Help => json!({ "kind": "keys" }),
        Input::CommandHelp => json!({ "kind": "commands" }),
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
        let rows: Vec<Value> = lists
            .iter()
            .skip(start)
            .take(count)
            .map(|p| json!({ "key": p.id.to_string(), "name": p.name, "tracks": p.len }))
            .collect();
        return json!({ "total": lists.len(), "start": start, "rows": rows });
    }
    let tracks = match view {
        View::Library => model.listed(),
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
        View::Playlists => model
            .session()
            .playlists()
            .get(row)
            .map(|p| p.id.to_string()),
        View::Sampler => None,
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
        command::command_rows()
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
    const REFUSED: [&str; 7] = ["quit", "scan", "prune", "open", "map", "unmap", "slice"];
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
        "completions": command::completions(text, model.view(), &names),
        "history": model.history().lines(),
    })
}

/// Whether a `:` line is one the page may run, in the view shown. The error
/// is the parser's words, or why the page may not.
pub fn check(model: &Model, line: &str) -> Result<(), (u16, String)> {
    let action = command::parse(line.trim(), model.view()).map_err(|e| (400, e))?;
    if allowed(&action) {
        Ok(())
    } else {
        Err((
            403,
            format!("not available from the web page: :{}", line.trim()),
        ))
    }
}
