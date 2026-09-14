# A GUI frontend

Design sketch, written 2026-09-14 against playr 0.5.0. Nothing here is built. `docs/architecture.md` describes the crates this builds on.

## Goal

A desktop window with the terminal interface's features, for people who prefer a GUI. It adds no features of its own; it shows the same features more clearly and lets the mouse reach them.

Decisions taken:

- **Toolkit:** egui, through `eframe` 0.36. The design in `docs/architecture.md` already assumes its frame loop.
- **Look:** egui's own dark style. No theme settings.
- **Platforms:** Linux, macOS and Windows, as the terminal.

## The parity rule

Parity is kept by construction, not by review:

1. Every control performs an `Action` through `playr_app::dispatch::dispatch`. No control calls `Session` directly.
2. Key bindings are the same `Keymap`, read from the same `settings.toml`, and `:` commands run through the same parser.
3. Messages use the same wording, from one function both frontends call.
4. A feature enters `playr-app` or `playr-core` first, then each frontend draws it.

A GUI user who learns a key or a command can use it in the terminal, and the reverse.

### Exceptions a GUI user needs

A GUI user may never open a terminal, but today a library is created only by `playr scan`, and files are played from outside the library only by `playr <path>`. The GUI needs both inside the window. To keep the parity rule, both become actions that the terminal gains too:

| action | command | GUI |
|-|-|-|
| scan a directory into the library, in the background, with progress | `:scan DIR` | Library menu, Add folder; a progress line in the status bar |
| play files or directories without adding them | `:open PATH...` | File menu, Open; dropping files on the window |

Startup errors differ too. The terminal prints bad settings to stderr and exits. A GUI started from a desktop has no visible stderr, so it lists the errors in a window with a Quit button.

## Feature map

Every row is a feature the terminal has today.

| feature | terminal | GUI | actions |
|-|-|-|-|
| views | tabs; `1` to `4`, `tab` | tab bar | `ShowView`, `NextView` |
| cursor | highlighted row; `j` `k` `g` `G` | highlighted row; click; the same keys | `Cursor`, `CursorFirst`, `CursorLast` |
| library | columns fitted to the width | a table with resizable columns, only visible rows laid out | |
| play from a row | `enter` | double-click; `enter` | `Activate` |
| search | `/` prompt, filters as typed; `esc` clears | a search field above the library, always visible; `/` focuses it | `Search`, `ClearSearch` |
| select a track | `a`; `+` marks selected rows | a checkbox column; `a` | `Add` |
| selection | list; `d` `J` `K` `c` | table; drag a row to move it; Delete key; right-click menu | `Remove`, `MoveTrack`, `ClearSelection` |
| save selection | `s`, name prompt, y/n before replacing | Save button, name dialog, replace dialog | `StartSave`, `SaveAs` |
| playlists | list; `a` `d` `r` `enter` | table with track counts; right-click menu; double-click plays | `Add`, `DeletePlaylist`, `StartRename`, `RenameTo`, `Activate` |
| confirmations | bottom-line question; `y` | modal dialog with Yes and No; `y` still answers | `dispatch::confirmed` |
| transport | `space` `n` `p` `x` | buttons | `TogglePause`, `Next`, `Prev`, `Stop` |
| seek | arrows; `:seek` | click or drag on the progress bar; arrows | `SeekTo`, `SeekBy` |
| progress and marks | gauge, `^` under it | progress bar with a tick per mark; hovering a tick shows its time | |
| marks | `b` `B` `C` `,` `.` | buttons beside the progress bar; the same keys | `Mark`, `UndoMark`, `ClearMarks`, `PrevMark`, `NextMark` |
| volume | `+` `-` | slider | `SetVolume`, `VolumeBy` |
| varispeed | `[` `]` `\`; `1.19x (+3 st)` | slider in semitones, with the same label; a reset button | `SetSpeed`, `SpeedBy` |
| mode | `m` `M`; named when not normal | drop-down | `SetMode`, `CycleMode` |
| now playing | title, artist, rate and channels | the same, in the status bar | |
| level meter | loudness bar in colour zones, LUFS and peak readout | the same bar drawn as a meter, same zones and hold | |
| messages | bottom line for 4 s | status bar line for 4 s | |
| key list | `?` | a window listing the current view's keys | `Help` |
| command list | `:help` | a window listing every command by view | `CommandHelp` |
| command line | `:` prompt, Tab completion, history | a command bar that `:` opens, with completions in a drop-down and the same history | `StartCommand`, `command::parse` |
| key bindings | `:map`, `:unmap` | the same commands | `Map`, `Unmap` |
| sampler waveform | eighth blocks, dB, Braille | painted: envelope, dB, and a min/max line waveform | `Display` |
| zoom | `z` `Z` `0` | mouse wheel over the waveform; the same keys | `Zoom` |
| sampler seek and mark | keys only | click to seek; a modifier-click adds a mark at that point | `SeekTo`, `MarkAt` |
| slicing | `:slice`; planned edges as `+`; `enter` `esc` | a Slice menu for the four cuts, with a sensitivity slider for onsets; planned edges as lines; Write and Discard buttons | `Slice`, `WriteSlices`, `DiscardSlices` |
| region detail | line under the waveform | the same line; the region shaded | |

The display named `braille` keeps its name in commands and settings, so `:display braille` works in both frontends. In the GUI it draws the same min/max shape as lines.

## Layout

```
+-----------------------------------------------------------------------------+
| File  Library  Slice  Help                                                  |
| [Library] [Selection 12] [Playlists 4] [Sampler]      Search [artist:evans] |
+-----------------------------------------------------------------------------+
| [x] Title                 Artist           Album                   Time     |
| [ ] Peace Piece           Bill Evans       Everybody Digs          6:33     |
| [x] Waltz for Debby       Bill Evans       Waltz for Debby         6:55     |
|  ...                                                                        |
+-----------------------------------------------------------------------------+
| [|<] [>||] [#] [>|]   Peace Piece - Bill Evans           44.1kHz 2ch         |
| 1:02 [=======O=====|========|================================]  6:33  [b][B] |
| Vol [======----] 80%   Speed [--|--] 0 st   Mode [normal v]   -14 LUFS [###-] |
| saved "late night": 12 tracks                                  ? keys        |
+-----------------------------------------------------------------------------+
```

The sampler view replaces the table:

```
+-----------------------------------------------------------------------------+
| envelope  0:48.000-1:12.000  1 col = 12.0 ms  amen.flac                      |
|        |                    ||||                |                           |
|   ||| ||||   ||    |||||||||||||||||   ||  ||   |                           |
| ||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||  |
|        |          +         ^         +         |                           |
| region 0:52.310-1:04.870 (12.560 s)   3 slices planned  [Write] [Discard]   |
+-----------------------------------------------------------------------------+
```

`|` in the axis row are marks, `^` the playhead and `+` planned edges; the region between the marks is shaded.

## Crates

```
playr       terminal  --+
                        +--> playr-app (dispatch, commands, keys, Model) --> playr-core
playr-gui   egui      --+
```

- **`crates/playr-gui`**, a second binary. The terminal binary stays free of winit and wgpu, and the GUI binary can hide its console window on Windows with `#![windows_subsystem = "windows"]`.
- **Shared model in `playr-app`.** The terminal's `App` holds logic that has nothing to do with a terminal. Copying it into the GUI would break the parity rule the first time one copy changed. It moves into a `playr_app::model::Model` that implements `Frontend`, and both frontends wrap it:

  | moves to `Model` | from |
  |-|-|
  | view, cursor row per view, search results | `App::view`, `Lists` rows, `App::results` |
  | the list the player is playing, rebuilt when its queue changes | `App::follow_player` |
  | the per-frame snapshot: status, position, volume, loudness, held peak, marks | `App::refresh`, `hold_peak` |
  | draining events: peaks, plans, exports, coalesced playback errors | `App::drain_events` |
  | reading and cancelling peaks as the sampler view opens and the track changes | `App::follow_wave` |
  | sampler state: wave, display, zoom, planning, pending plan | `ui::sampler::Sampler` |
  | column geometry: `window`, `fmt_frames`, `db_height` | `ui::sampler` |
  | the message shown and when it expires | `App::message` |
  | message wording | `ui::notice::text`, which uses no terminal type |
  | the confirmation or prompt pending | `Input::Confirm`, and which prompt is open |

  Each frontend keeps what is about its own drawing and input: text being typed, scroll offsets, `Drawn` clamping, glyphs, key event conversion, and which widget has focus.

## The GUI crate

### Frame loop

`eframe::App::update` runs once per frame:

1. `model.refresh()`: sample the player, drain events.
2. Turn input into actions: key events through `Keymap::lookup` when no text field has focus (`ctx.wants_keyboard_input()` is false), and clicks through the feature map above.
3. `dispatch` each action with the model as the frontend.
4. Draw the panels from the model.
5. Ask for the next frame: every 33 ms while playing or reading peaks, otherwise only on input. A stopped player then uses no CPU between events.

The event sink sends to the model's channel and calls `ctx.request_repaint()`, so a finished export shows without waiting for input.

### Widgets

- **Tables:** `egui_extras::TableBuilder`, laying out only visible rows, as `draw_tracks` does for 50,000 tracks. A cursor moved by a key scrolls its row into view.
- **Waveform:** `egui::Painter` rectangles per column, from `Peaks::range` over the columns `window` gives. Envelope and dB draw RMS inside peak in two shades; the line display draws min and max. Hit-testing a click maps an x position to a frame with the same `window`.
- **Level meter:** painted rectangles in the terminal's colour zones.
- **Dialogs:** `egui::Modal` for confirmations and names; windows for the key and command lists.
- **File dialogs:** `rfd` 0.17, which uses the native dialog on each platform.
- **Dropped files:** `ctx.input(|i| i.raw.dropped_files)`, as `:open`.

### Settings and state

- The same `settings.toml`, `[keys]` included. The GUI adds no table, so the terminal and the GUI can read one file; the shared-table problem in `docs/architecture.md` does not arise yet.
- Window size and position persist through eframe's `persistence` feature, in its own file in the platform's app-data directory, not in `settings.toml`.
- The library is the terminal's `library.db`, so both frontends see the same playlists and marks.

## Platforms and packaging

- **Rendering:** eframe 0.36 defaults to wgpu: Metal on macOS, DirectX 12 or Vulkan on Windows, Vulkan or OpenGL on Linux, with X11 and Wayland both enabled.
- **Linux:** the build packages winit and wgpu need are to be pinned down on the first CI run. A `.desktop` file and icon go in the release archive.
- **macOS:** a Finder or Dock launch needs an `.app` bundle with an `Info.plist`. The release workflow builds one around the binary.
- **Windows:** `playr-gui.exe` with no console window and an embedded icon. playr has not yet run on Windows at all, so the terminal's tests run there first.
- **Release:** `.github/workflows/release.yml` builds `playr-gui` beside `playr` for the same five targets.

## Testing

- **Model:** the terminal's app tests move with the logic into `playr-app` and drive `Model` without drawing, as `crates/playr-app/tests/dispatch.rs` does now.
- **UI:** `egui_kittest` 0.36 runs the GUI headless and finds widgets by their accessibility labels: click Play, check the model's state, check the message.
- **Parity:** one table in `playr-gui` maps each control to the `Action` it performs. A test checks that every `Action` variant has a control or a key binding, with a named list of the variants that deliberately have neither. A new `Action` then fails the test until the GUI can reach it.
- **Screenshots:** `egui_kittest` can compare rendered images; keep that to the sampler view, where the drawing is the feature.

## Steps

| step | change | size |
|-|-|-|
| 1 | CI: `make test` on Linux, macOS and Windows on every push | small |
| 2 | Move the shared model into `playr-app`; the terminal wraps it; no behaviour change | large |
| 3 | `:scan DIR` as a session job with progress events, and `:open PATH...`; the terminal gains both | medium |
| 4 | `playr-gui`: window, tabs, library table, search, transport bar, messages, keys | medium |
| 5 | Selection and playlists, dialogs, command bar, key and command lists, parity test | medium |
| 6 | Sampler view: waveform, zoom, click to seek and mark, slicing | medium |
| 7 | Packaging: release archives for `playr-gui`, macOS bundle, Windows icon, Linux desktop file | small |

Step 2 carries the risk, as step 3 of the core split did: it moves most of `src/ui/mod.rs`.

## Open questions

- **Fonts.** egui's default fonts cover Latin, Greek and Cyrillic, not CJK. A terminal shows CJK tags with the terminal's font; the GUI shows boxes unless it loads a font with those glyphs. Bundling Noto Sans CJK adds about 16 MB per binary (estimate); loading a system font needs a font-lookup crate and differs per platform.
- **Two processes, one library.** Can the terminal and the GUI run at once? Reads are safe under SQLite's locking; two writers can hit `SQLITE_BUSY`. A busy timeout on the connection may be enough, but it is untested.
- **Native paths.** Both frontends use `~/.local/share/playr` and `~/.config/playr` on every platform, which is unusual on Windows and macOS. Moving to each platform's directories would move existing libraries.
- **Media keys.** GUI users expect the keyboard's play and pause keys, and the macOS and Windows now-playing panels, to work. The terminal has none of this, so under the parity rule it waits, or both frontends gain it.
- **After parity.** Multi-row selection, sorting by column, and dragging a mark each change `Frontend` or `Session`, and are listed as open issues in `docs/architecture.md`.
