//! The thread that owns the [`Model`].
//!
//! Clients send [`Request`]s over a channel, and this thread runs them one at a
//! time between refreshes. Nothing else touches the model, so no lock is held
//! across a refresh, and a request of several steps runs with nothing between
//! them.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use playr_app::action::{Action, Key};
use playr_app::dispatch::Frontend;
use playr_app::model::{Input, Model};
use playr_app::View;
use playr_core::audio::State;
use serde_json::Value;

use crate::web;

/// How often the model is refreshed while a track plays.
pub const FRAME: Duration = Duration::from_millis(33);

/// How often it is refreshed otherwise, for events such as a scan's progress.
pub const IDLE: Duration = Duration::from_millis(250);

/// How far short of its end a seek by fraction stops. A seek to the end moves
/// to the next track, so a fader dragged to its top would skip tracks.
pub const END_MARGIN: Duration = Duration::from_millis(100);

/// Why the owner refused a request: an HTTP status and the reason.
pub type Refused = (u16, String);

/// What a client asks of the model. A request that can be refused carries a
/// reply, so the refusal reaches the client that sent it.
#[derive(Debug)]
pub enum Request {
    /// An action OSC has already checked.
    Perform(Action),
    /// A `:` command line from the page, parsed in the view shown.
    Command {
        line: String,
        reply: Sender<Result<(), Refused>>,
    },
    /// A key pressed on the page, named as a binding names it, looked up in
    /// the view shown as the other frontends look keys up.
    Key {
        name: String,
        reply: Sender<Result<(), Refused>>,
    },
    /// Puts the cursor on `row` of `view`, then runs `command` there, if the
    /// row is still the one named `key`: a list can change between the page
    /// drawing it and a tap on it.
    Row {
        view: View,
        row: usize,
        key: String,
        command: Option<String>,
        reply: Sender<Result<(), Refused>>,
    },
    /// Shows the tracks matching `query` as it is typed; with `done`, closes
    /// the search, keeping the results unless the query is empty.
    Search { query: String, done: bool },
    /// Answers the question open.
    Answer(bool),
    /// The name typed into the save or rename prompt open.
    Name(String),
    /// Closes the prompt or list open.
    Close,
    /// JSON the page reads: rows, keys, help or completions.
    Read { query: Query, reply: Sender<Value> },
    /// Seeks to this fraction of the playing track. Of several queued, only
    /// the last runs: a fader's drag sends dozens.
    Seek(f64),
    /// Plays the playlist at this index, oldest first, so an index keeps its
    /// playlist when others are added or renamed.
    PlayPlaylistAt(usize),
}

/// What [`Request::Read`] asks for; see [`web`].
#[derive(Debug)]
pub enum Query {
    Rows {
        view: View,
        start: usize,
        count: usize,
    },
    Keys,
    Help {
        commands: bool,
    },
    Completions(String),
}

/// Runs `requests` on `model`, refreshing it and calling `publish` after each
/// request and each frame. Returns the model once every sender is gone or a
/// command quits.
pub fn run(
    mut model: Model,
    requests: Receiver<Request>,
    mut publish: impl FnMut(&Model),
) -> Model {
    // A request taken from the queue while merging seeks, to run next.
    let mut held = None;
    loop {
        let wait = match model.snapshot().status.state {
            State::Playing => FRAME,
            _ => IDLE,
        };
        let request = match held.take() {
            Some(request) => Ok(request),
            None => requests.recv_timeout(wait),
        };
        match request {
            Ok(Request::Perform(action)) => model.perform(action),
            Ok(Request::Command { line, reply }) => {
                let checked = web::check(&model, &line);
                if checked.is_ok() {
                    model.run_command(&line);
                }
                let _ = reply.send(checked);
            }
            Ok(Request::Key { name, reply }) => {
                let _ = reply.send(key(&mut model, &name));
            }
            Ok(Request::Row {
                view,
                row,
                key,
                command,
                reply,
            }) => {
                let _ = reply.send(on_row(&mut model, view, row, &key, command.as_deref()));
            }
            Ok(Request::Search { query, done }) => search(&mut model, query, done),
            Ok(Request::Answer(yes)) => model.answer(yes),
            Ok(Request::Name(name)) => match model.input().clone() {
                Input::SavePlaylist(_) => model.save_as(&name),
                Input::RenamePlaylist { from, .. } => model.rename_to(&from, &name),
                _ => {}
            },
            Ok(Request::Close) => match model.input() {
                Input::Search(_) => model.end_search(true),
                _ => model.set_input(Input::None),
            },
            Ok(Request::Read { query, reply }) => {
                let _ = reply.send(match query {
                    Query::Rows { view, start, count } => web::rows(&model, view, start, count),
                    Query::Keys => web::keys(&model),
                    Query::Help { commands } => web::help(&model, commands),
                    Query::Completions(text) => web::completions(&model, &text),
                });
            }
            Ok(Request::Seek(mut fraction)) => {
                for queued in requests.try_iter() {
                    match queued {
                        Request::Seek(later) => fraction = later,
                        other => {
                            held = Some(other);
                            break;
                        }
                    }
                }
                seek(&mut model, fraction);
            }
            Ok(Request::PlayPlaylistAt(index)) => {
                let mut lists = model.session().playlists().to_vec();
                // Ids follow creation, and replacing a playlist keeps its id.
                lists.sort_by_key(|p| p.id);
                if let Some(list) = lists.get(index) {
                    model.perform(Action::PlayPlaylist(list.name.clone()));
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        // The page has no sampler; a key or command that reaches it goes back.
        if model.view() == View::Sampler {
            model.set_view(View::Library);
        }
        model.refresh();
        model.expire_message();
        publish(&model);
        if model.quitting() {
            break;
        }
    }
    model
}

/// Seeks to `fraction` of the playing track, short of its end. The player is
/// asked for its length, since a seek can come before the first refresh.
fn seek(model: &mut Model, fraction: f64) {
    if let Some(duration) = model.session().player().status().duration {
        model.perform(Action::SeekTo(seek_target(duration, fraction)));
    }
}

/// Where a seek to `fraction` of `duration` lands: [`END_MARGIN`] short of the
/// end at most.
pub fn seek_target(duration: Duration, fraction: f64) -> Duration {
    let at = duration.mul_f64(fraction.clamp(0.0, 1.0));
    at.min(duration.saturating_sub(END_MARGIN))
}

/// Performs what `name` is bound to in the view shown, if the page may.
fn key(model: &mut Model, name: &str) -> Result<(), Refused> {
    let key = Key::parse(name).map_err(|e| (400, e))?;
    match model.keymap().lookup(key, model.view()).cloned() {
        Some(action) if web::allowed(&action) => {
            model.perform(action);
            Ok(())
        }
        Some(_) => Err((403, format!("not available from the web page: {name}"))),
        None => Ok(()),
    }
}

/// Puts the cursor on `row` of `view` and runs `command` there, as a click
/// and a key would in the window.
fn on_row(
    model: &mut Model,
    view: View,
    row: usize,
    key: &str,
    command: Option<&str>,
) -> Result<(), Refused> {
    if web::row_key(model, view, row).as_deref() != Some(key) {
        return Err((409, "the list has changed; try again".into()));
    }
    model.set_view(view);
    model.set_cursor(view, Some(row));
    match command {
        Some(line) => {
            web::check(model, line)?;
            model.run_command(line);
            Ok(())
        }
        None => Ok(()),
    }
}

/// Filters the library as a query is typed, and closes the search once done.
fn search(model: &mut Model, query: String, done: bool) {
    if done && query.is_empty() {
        model.search_as_typed(query);
        model.end_search(false);
    } else {
        model.search_as_typed(query);
        if done {
            model.end_search(true);
        }
    }
}
