# One core, several frontends

Design note. Written 2026-09-14 against playr 0.4.0, unreleased. Step 1 is done: the workspace and `crates/playr-core`. The section Today describes the code before it.

The goal: the terminal interface is one frontend of a core that an egui app or a Tauri app could also drive. Everything but presentation is shared.

## What each frontend needs

The two candidates put different demands on the core. The design takes the stricter demand at every point, so either can be built.

| | egui | Tauri |
|-|-|-|
| language of the view | Rust, in process | JavaScript or TypeScript, over IPC |
| shares Rust types with the core | yes | no; values cross IPC as JSON |
| frame loop | yes; redraws continuously or on request | no; the page reacts to events |
| owner of the core | the app struct, on one thread | Tauri's state, shared by command threads |
| can call the core's formatting | yes | no |

So the core:

1. Depends on no presentation crate: no `ratatui`, `crossterm`, `egui` or `tauri`.
2. Takes explicit arguments. An operation names what it acts on, by id, index or path, never "the row under the cursor".
3. Returns data, not text. Outcomes and refusals are enums; a frontend words them.
4. Pushes events. Work that finishes later, and changes a frontend did not cause, reach it through one sink. A frame loop may also poll.
5. Is `Send`. A Tauri app wraps it in a `Mutex`; an egui app owns it. `Player`, `rusqlite::Connection` and `Peaks` are all `Send` today.
6. Serialises behind a cargo feature, `serde`, which only a Tauri or daemon frontend turns on.

## Today

The engine, library, samples and peaks already meet rules 1 and 2: `src/audio`, `src/db`, `src/scan.rs`, `src/samples.rs` and `src/wave.rs` import nothing from `ui`. The interface does not:

- `App` in `src/ui/mod.rs`, 1,387 lines and about 32 fields, holds library state (selection, playlist cache, marks), presentation state (view, cursors, prompts, zoom, messages), and the database connection, player and three worker channels.
- Operations read the cursor: `Action::Add` means the track under it.
- 60 messages are English strings made inside `App`, and tests assert on their wording.
- Cursors are `ratatui::widgets::ListState`, and drawing writes clamped values back into `App`.
- Worker results arrive on channels that `App::refresh` drains each frame.

## Crates

```
playr         terminal frontend  ---+
playr-gui     egui frontend      ---+--> playr-app --> playr-core
playr-tauri   Tauri backend      -----------------> playr-core [serde]
```

- **playr-core**: the engine, library, scanner, samples, peaks, settings, `Session` and events. No presentation dependency.
- **playr-app**: what Rust frontends share about interaction: `Action`, the `:` command parser with completion and history, the key map over a neutral `Key`, and `dispatch`, which turns an action into core calls. It is optional. A Tauri frontend skips it, or uses its parser on the Rust side for a command palette.
- **playr**: the terminal. Converts crossterm keys to `Key`, keeps cursors and prompts, draws with ratatui.

Views (library, selection, playlists, sampler) are a presentation idea, so they are not in the core. `playr-app` names them as scopes for key bindings and view-scoped commands; a GUI maps its panels or focus onto them, or ignores them.

## The core API

### Session

`Session` owns the library connection, the player, the loaded library, the selection and the settings. It is the only way a frontend changes them.

```rust
impl Session {
    pub fn open(settings: CoreSettings, library: Library, events: EventSink) -> Result<Session, CoreError>;

    // Library
    pub fn tracks(&self) -> &[Track];
    pub fn search(&self, query: &str) -> Result<Vec<TrackId>, CoreError>;
    pub fn playlists(&self) -> &[Playlist];
    pub fn playlist_tracks(&self, id: PlaylistId) -> Result<Vec<Track>, CoreError>;

    // Playback
    pub fn play(&mut self, tracks: Vec<TrackId>, index: usize);
    pub fn transport(&mut self, t: Transport);          // pause, next, seek, volume, speed, mode
    pub fn playback(&self) -> Playback;                 // status, position, levels, marks

    // Selection
    pub fn selection(&self) -> &[Track];
    pub fn toggle_selected(&mut self, track: TrackId) -> Outcome;
    pub fn add_to_selection(&mut self, tracks: &[TrackId]) -> Outcome;
    pub fn remove_from_selection(&mut self, index: usize) -> Result<Outcome, Refusal>;
    pub fn move_in_selection(&mut self, index: usize, by: i64) -> Result<usize, Refusal>;
    pub fn clear_selection(&mut self) -> Outcome;

    // Playlists
    pub fn save_selection(&mut self, name: &str, replace: bool) -> Result<Outcome, Refusal>;
    pub fn rename_playlist(&mut self, id: PlaylistId, name: &str) -> Result<Outcome, Refusal>;
    pub fn delete_playlist(&mut self, id: PlaylistId) -> Result<Outcome, Refusal>;

    // Marks, on the playing track
    pub fn add_mark(&mut self, at: Option<Duration>) -> Result<Outcome, Refusal>;
    pub fn undo_mark(&mut self) -> Result<Outcome, Refusal>;
    pub fn clear_marks(&mut self) -> Result<Outcome, Refusal>;
    pub fn seek_to_mark(&mut self, forward: bool) -> Result<Outcome, Refusal>;

    // Work that takes seconds: started here, finished by an event
    pub fn read_peaks(&mut self, track: &Path) -> JobId;               // Event::Peaks
    pub fn plan_slices(&mut self, cut: Cut) -> Result<JobId, Refusal>; // Event::Planned
    pub fn write_slices(&mut self, plan: &Plan) -> JobId;              // Event::Exported
}
```

Three conventions carry through it:

- **Confirmation by refusal.** The core never asks a question. `save_selection(name, replace: false)` over an existing playlist returns `Refusal::WouldReplace(name)`. The frontend asks in its own way, a `y/n` prompt or a dialog, and calls again with `replace: true`.
- **Plans are values.** `Event::Planned` carries a `Plan`, spans and the job that made them. The frontend shows it and passes it back to `write_slices`, or drops it. Discarding needs no call, and a plan held on screen is presentation state.
- **Peaks cross IPC as columns.** `Event::Peaks` carries `Arc<Peaks>` to a Rust frontend. A Tauri backend keeps the `Arc` and answers the page with `Peaks::columns(start, frames_per_column, count) -> Vec<Extent>`, a few hundred numbers per redraw.

### Outcomes and refusals

```rust
pub enum Outcome {
    Selected, Unselected, AlreadySelected,
    Saved { name: String, skipped: usize },
    Renamed { from: String, to: String },
    Marked { at: Duration, kept: bool },
    Unmarked { at: Duration },
    // one variant per message the terminal shows today
}

pub enum Refusal {
    NothingPlaying, SelectionEmpty, NoLibraryFile,
    NameEmpty, NameUnchanged, NameTaken(String), WouldReplace(String),
    AlreadyMarked { at: Duration }, NoLaterMark, NoEarlierMark,
    // ...
}
```

The terminal words them in one function, `notice_text`, which replaces the strings now spread through `App`. Tests assert on variants, not wording.

### Events

```rust
pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

pub enum Event {
    // From the engine thread
    TrackChanged { index: usize, path: PathBuf },
    StateChanged(State),
    PlaybackError(String),
    // From workers
    Peaks { job: JobId, track: PathBuf, result: Result<Arc<Peaks>, String> },
    Planned { job: JobId, result: Result<Plan, String> },
    Exported { job: JobId, result: Result<Exported, String> },
}
```

Each frontend adapts the sink:

| frontend | sink |
|-|-|
| terminal | sends to a channel the draw loop drains, as now |
| egui | sends to a channel, then calls `ctx.request_repaint()` so the next frame drains it |
| Tauri | keeps the `Arc<Peaks>` of an `Event::Peaks` and emits only that they are ready; emits every other event with `app.emit("playr", event)`, which needs the `serde` feature |

Position, loudness and peak level change continuously, so they are not events. `Session::playback` returns them; the terminal and egui read it each frame, and a Tauri backend emits it on a timer, 20 to 30 times a second.

### Settings

`settings.toml` stays one file. The core owns the top-level keys (`volume`, `mode`, `speed`, `samples`, `onset_sensitivity`). Each frontend owns tables it names: `playr-app` owns `[keys]`, and a GUI might own `[gui]`. The core parses the file with the frontend's table names, reports unknown top-level keys and unknown tables as it does now, and hands each named table back for the frontend to check.

## playr-app

```rust
pub trait Frontend {
    fn scope(&self) -> Scope;                         // library, selection, playlists, sampler
    fn cursor_track(&self) -> Option<TrackId>;
    fn cursor_playlist(&self) -> Option<PlaylistId>;
    fn cursor_index(&self) -> Option<usize>;
    fn show(&mut self, notice: Result<Outcome, Refusal>);
    fn confirm(&mut self, question: Refusal, then: Action);
    fn present(&mut self, change: Presentation);      // cursor, view, prompt, zoom, help
}

pub fn dispatch(action: Action, session: &mut Session, frontend: &mut impl Frontend);
```

`dispatch` is today's `App::perform` without the state it touches directly. `Action::Add` in the library scope becomes `session.toggle_selected(frontend.cursor_track()?)`, and `Action::CursorFirst` becomes `frontend.present(Presentation::CursorFirst)`. An egui app implements `Frontend` over its own selection model and gets every key binding and `:` command.

`Key` becomes playr-app's own type: a code and modifiers, with the names and parsing it has now. The terminal converts crossterm events to it, and egui converts its `egui::Key` events.

## Where today's code goes

| today | goes to |
|-|-|
| `src/audio`, `src/db`, `src/scan.rs`, `src/samples.rs`, `src/wave.rs` | playr-core, unchanged |
| `App` fields: `conn`, `player`, `all`, `selection`, `playlists`, `marks`, worker channels | `Session` |
| `App` fields: `view`, `results`, the three `ListState`s, `input`, `history`, `help_scroll`, `sampler` (zoom, display, pending plan, planning flag), `message` | terminal presentation state |
| `App::perform` | `playr_app::dispatch`, with presentation effects through `Frontend` |
| `append_selection`, `toggle_selected_track`, `remove_from_selection`, `move_in_selection`, `save_selection`, `rename_playlist`, `delete_playlist`, `add_mark`, `undo_mark`, `jump_to_mark`, `cycle_mode`, `nudge_volume`, `export`, `plan`, `slice_job`, `play` | `Session` methods with explicit arguments |
| `can_save`, `save_as`, `rename_to`, `confirm` | split: checks become refusals in `Session`; prompting stays in the terminal |
| `command_key`, `search_key`, `save_key`, `rename_key`, `on_key` | terminal |
| `ui/action.rs`, `ui/command.rs`, key parts of `ui/config.rs`, `ui/settings.toml` `[keys]` | playr-app |
| the rest of `ui/config.rs` | playr-core settings |
| `ui/render.rs`, `ui/sampler.rs` glyph code | terminal |
| `ui/sampler.rs` `window`, `fmt_frames` | playr-app, or the core, as any waveform view needs them |

## Steps

Each step leaves `make test` passing and changes no behaviour.

| step | change | size |
|-|-|-|
| 1 | Done. Cargo workspace. Move `audio`, `db`, `scan`, `samples` and `wave` and their tests into playr-core | small; imports only |
| 2 | `Outcome` and `Refusal` in the core; the terminal words them in `notice_text`; tests assert variants | medium; about 60 message sites |
| 3 | `Session` in the core. Move library state and the operations above into it with explicit arguments; `App` holds a `Session` | large; most of `ui/mod.rs` and the app tests |
| 4 | `Event` and `EventSink`. Workers and the engine push events; `App` drains one channel instead of three | medium |
| 5 | playr-app: `Action`, commands, key map with its own `Key`, `Frontend`, `dispatch`. The terminal implements `Frontend` | medium |
| 6 | Presentation cleanup in the terminal: cursors as indices, `ListState` built per frame, `Screen` built with defaults | small |
| 7 | Settings split between core and frontend tables | small |
| 8 | `serde` feature on core types, with a test that serialises each public type | small; can wait for a Tauri frontend |

Steps 2 and 3 carry the risk. Step 2 first makes step 3 a move of code that already returns data.

## Decisions left open

- **Daemon.** A daemon holding the `Session`, with frontends as clients over a socket, would let a terminal and a GUI control one playback at once, and fits the OSC idea in `sampler.md`. It costs a protocol and a process to manage. The in-process `Session` does not rule it out: a daemon wraps it, and the `serde` feature is its wire format.
- **Library loading.** `Session::tracks` returns the whole library in memory, as `App::all` does now. At 100,000 tracks a GUI table wants pages or a query per scroll; the API would add `tracks_page(offset, count)` then.
- **cpal versions.** rtrack uses cpal 0.15 and playr 0.18. A waveform crate shared with rtrack must not depend on cpal.
