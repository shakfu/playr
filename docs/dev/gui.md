# A GUI frontend

Design, written 2026-09-14 against playr 0.5.0, as a sketch before any of it was built. Steps 1 to 7 are done; the sections on where each step differs record what changed on the way. `docs/architecture.md` describes the crates this builds on.

## Goal

A desktop window with the terminal interface's features, for people who prefer a GUI. It adds no features of its own; it shows the same features more clearly and lets the mouse reach them.

Decisions taken:

- **Toolkit:** egui, through `eframe` 0.36. The design in `docs/architecture.md` already assumes its frame loop.

- **Look:** egui's own dark style, unless `theme` or `:theme` chooses light or the system's appearance.

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
| re-scan the recorded roots | `:rescan` | File menu, Rescan library |
| list the library's directories, and forget one | `:roots`, `:roots rm DIR` | File menu, Library directories, with a Forget button per row |
| remove the tracks and marks of missing files | `:prune [DIR]` | File menu, Remove missing files, and Remove missing under folder |
| play files or directories without adding them | `:open PATH...` | File menu, Open; dropping files on the window |
| move the sampler's cursor, and the mark under it | `:cursor`, `:pick`, `:nudge-mark`, `:snap-mark`, `:del-mark` | buttons under the waveform; a mark dragged along it |
| media keys and the now-playing panel | the keys themselves | the keys themselves; `Model::attach_media` in both |

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
| transport | `space` `n` `p` `x` | buttons showing media symbols, named in their tooltips with their keys | `TogglePause`, `Next`, `Prev`, `Stop` |
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
| theme | `:theme`; `system` and `dark` use the terminal's ANSI colours, `light` a 256-colour set | View, Theme; `system` follows the system's appearance | `Theme` |
| sampler waveform | eighth blocks, dB, half-block spectrogram, Braille | painted: envelope, dB, a spectrogram texture, and a min/max line waveform | `Display` |
| zoom | `z` `Z` `0` | mouse wheel over the waveform; the same keys | `Zoom` |
| sampler seek and mark | arrows nudge a column or a tenth of the view; `S` snaps | click to seek; a modifier-click adds a mark at that point; the same keys; a Snap to zero tick box | `SeekTo`, `MarkAt`, `Nudge`, `Snap` |
| range | `<` `>` `backspace` `esc`, `:range`; ends drawn as `[` `]` | a drag across the waveform, or from an edge to move it; Range in, Range out and Clear range buttons; `esc`; ends drawn as lines | `RangeIn`, `RangeOut`, `SetRange`, `DiscardSlices` |
| loop | `l`, `:loop`; `loop` in the title | a Loop range tick box; the same key | `Loop` |
| range ends | `[` `]` pick an end, drawn reversed; `{` `}` move it a column | Move start and Move end, drawn chosen, and Earlier and Later buttons; the chosen end's line is thicker; the same keys | `PickEdge`, `MoveEdge` |
| slicing | `:slice`; planned edges as `+`; `enter` `esc` | a Slice menu for three cuts; under the waveform, Slice region or range, Slice at marks, a count with Equal slices, and a sensitivity slider with Slice at onsets; planned edges as lines; Write and Discard buttons | `Slice`, `WriteSlices`, `DiscardSlices` |
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

- **Model:** `crates/playr-app/tests/model.rs` drives `Model` without drawing. The terminal's key-driven tests in `tests/app.rs` stay with the terminal and cover the same logic through keys.

- **UI:** `egui_kittest` 0.36 runs the GUI headless and finds widgets by their accessibility labels: click Play, check the model's state, check the message.

- **Parity:** one table in `playr-gui` maps each control to the `Action` it performs. A test checks that every `Action` variant has a control or a key binding, with a named list of the variants that deliberately have neither. A new `Action` then fails the test until the GUI can reach it.

- **Screenshots:** `egui_kittest` can compare rendered images; keep that to the sampler view, where the drawing is the feature.

## Steps

| step | change | size |
|-|-|-|
| 1 | CI: `make test` on Linux, macOS and Windows on every push | small |
| 2 | Done. Move the shared model into `playr-app`; the terminal wraps it; no behaviour change | large |
| 3 | Done. `:scan DIR` as a session job with progress events, and `:open PATH...`; the terminal gains both | medium |
| 4 | Done. `playr-gui`: window, tabs, library table, search, transport bar, messages, keys | medium |
| 5 | Done. Selection and playlists, dialogs, command bar, key and command lists, parity test | medium |
| 6 | Done. Sampler view: waveform, zoom, click to seek and mark, slicing | medium |
| 7 | Done. Packaging: release archives for `playr-gui`, macOS bundle, Windows icon, Linux desktop file | small |

Step 2 carried the risk, as step 3 of the core split did: it moved most of `src/ui/mod.rs`.

### Where step 7 differs from the sketch

- **One archive a platform.** `packaging/package.sh` puts `playr` and `playr-gui` in the same archive rather than separate ones, so a release keeps five archives. The release workflow builds both with Opus, which `playr-gui` gains as a feature of its own.

- **The icon is drawn from an SVG.** `crates/playr-gui/assets/playr.svg` is rendered by `packaging/icons.sh` (`make icons`, macOS only, since it needs `iconutil`) into `playr.png` for the window and Linux, `playr.icns` for the bundle, and `playr.ico`, which `build.rs` embeds in the Windows executable with `embed-resource`.

- **The macOS bundle is unsigned.** Signing and notarization need an Apple Developer account. A downloaded `playr.app` is refused until allowed in System Settings or its quarantine attribute is removed; the README says how.

- **Linux gets a desktop entry, not a package.** `playr.desktop` and `playr.png` ship in the archive for the user to copy; no `.deb`, Flatpak or AppImage. The window's app id, `playr`, matches the entry's name and `StartupWMClass`.

- **Verified here:** the macOS arm64 archive was built as the workflow builds it, `playr.app` launched, and both binaries report 0.5.1 and list Opus. The Linux branch of the packaging script was checked with placeholder binaries. The Windows icon resource and both Linux builds are unverified until the next release run.

- **Still not published to crates.io.** `playr-gui` keeps `publish = false`.

### Sampler editing, after step 7

Fine movement, snapping, a range and looping it, in both frontends.

- **The loop is the engine's.** `Cmd::Loop` gives it source frames. The decoder cuts the chunk that reaches the end, seeks back to the start and keeps filling the ring, so the device never waits and the return is sample-exact; a mark at the output frame where the start is heard carries its position offset, as a gapless track change carries its own. Changing the bounds after the decoder has read past them re-seeks to the position, a short gap, rather than playing the old bounds out for up to two seconds. Looping in the frontend, by seeking at the end, would have left a gap and missed the end by a frame's time.

- **Escape backs out a step.** `DiscardSlices` discards a plan, or with none, clears the range, so one key undoes the last thing set in the view.

- **A nudge counts columns, not time.** `Model::set_scale` records the columns each frame drew, so a step follows zoom. A column is a cell in the terminal and a point in the window, so one nudge moves further in the terminal at the same zoom.

- **Signs are kept, not decoded.** `Peaks` keeps a bit a frame for the sign of the channels' mean, about 1.3 MB for 4 minutes at 44.1 kHz, so a snap reads memory. Decoding around each snap would stall a frame on a slow seek.

- **Snapping is decided in `dispatch`,** by the view: `SeekTo`, `Mark`, `MarkAt`, the range's ends and nudges snap in the sampler view with snap on. The transport's progress bar sends `SeekTo` too, so a click on it snaps while the sampler view shows, by at most 10 ms.

- **The range lives in `Sampler`,** keyed by track path and not saved; `Job::range` carries it to the core, which cuts it in place of the region.

- **A drag starts at the press.** egui reports a drag once the pointer has moved past a threshold, so the start comes from `pointer.press_origin()`, and the end is kept from the last frame with a pointer.

- **Zoom goes past the peaks.** Below 64 frames a column, `Model` reads the frames in view and 2 s either side through `Session::read_detail`, and reads again once the view leaves them; columns draw from peaks until then. The terminal stops at a frame a cell. The window goes on to 16 points a frame, where the line display draws each frame's channels' mean, the signal snaps use, around a zero line. Decoding on demand was chosen over finer peak buckets, 4 to 32 times the memory for every track, and over a mono copy of each track, about 21 MB for 4 minutes.

### Where step 6 differs from the sketch

- **The layout is shared.** `playr_app::sampler::Layout` holds one frame's geometry: which frames each column shows, the playhead's column, the marks, the region, each column's levels for a display, and the time under a point. It also words the scale, the times shown and the region line, and `peaks_of` and `plan_text` word the waiting and planning states. The terminal's sampler view draws from it too, so both show the same region and numbers; its render tests pass unchanged.

- **A column is a point wide.** The window asks the layout for as many columns as it has points, so zoom stops at 64 frames a point, as it stops at 64 frames a cell in the terminal. Columns start on whole peak buckets, so a track can end short of the right edge, as in the terminal.

- **The spectrogram is one texture.** A pixel a column and a row a point, set into one `TextureHandle` kept in the view's state, replaced each frame it shows. Painting it as shapes would be one per point, about 300,000 a frame.

- **The third display draws lines.** Each column is a line from its lowest to its highest sample around the centre. Its button reads "Waveform", while `:display braille` and the message still name it braille, for the terminal's sake.

- **Controls.** Zoom in, Zoom out, Whole track, the four displays, and Write slices and Discard slices, which are enabled only while slices are planned, sit under the waveform and are in `controls::SAMPLER_BAR`. The Slice menu from step 5 plans slices while this view shows.

- **Tests.** Window tests shift-click the waveform to mark, click it to seek, zoom with the wheel, and plan and write a region slice into a temporary directory. `crates/playr-app/tests/sampler.rs` checks the layout's columns, region, levels, extents and words.

- **Not yet:** dragging a mark, and hearing a region on its own, which the terminal cannot do either.

### Where step 5 differs from the sketch

- **Controls are tables.** `playr_gui::controls` lists each menu, row menu and button with its action; the window draws them from the tables and the parity test reads the same tables. Controls whose value comes from use, such as a slider or a dragged row, are named in `WITH_VALUES`.

- **The parity test** (`crates/playr-gui/tests/parity.rs`) takes one of each `Action`, with a `match` that has no wildcard so a new action stops it compiling, and checks each is performed by a control, a default key binding, or a named exception: `Search`, `SaveAs` and `RenameTo` go through the search field and the name dialog's model calls, `PlayPlaylist` is a row's `Activate`, and `Map` and `Unmap` are typed.

- **A dragged selection row moves, and so does `:move`.** The session swapped a track with the one N places away. For one place that is a move; for a drag across several rows it is not, so `move_in_selection` now moves the track and the tracks between keep their order. `:move +N` changes to match its description.

- **Only keys a binding used are withheld from widgets.** Withholding every key stopped Escape closing a row's menu.

- **The command bar edits the model's `CommandLine`.** Tab, Shift-Tab and the arrows are taken before the field sees them, and the field locks focus so Tab completes instead of moving on. Completions for the text so far show above the bar.

- **Shift-click on the progress bar marks there** (`MarkAt`); a plain click seeks.

- **Not yet:** the sampler view, CJK fonts, packaging, and a context menu for the sampler's slices.

### Where step 4 differs from the sketch

- **More than the library.** The selection and playlists views are tables too, and the confirmation, name and key and command list dialogs exist in a plain form, because a key can open any of them and the window would otherwise have no way to answer or close it. Step 5 adds their context menus, row dragging, command completion and history, and the parity test.

- **Keys.** A character comes from egui's text event, which heeds Shift and the layout; a named key or a Ctrl or Alt chord comes from its key event. A key a binding takes is removed from the frame's events, so the `:` that opens the command bar is not typed into it. Once a prompt is open, keys go to its field. Bindings pause only while one of the window's text fields has focus: a focused slider still moves with the arrow keys as they also seek.

- **Escape is read before a text field is drawn.** An egui text field takes the key when it gives up focus, so a check after the field never sees it.

- **Shared with the terminal:** tab titles (`View::title`), the confirmation question (`Confirm::question`), the key and command lists (`command::key_rows`, `command_rows`), the meter's scale and zones (`playr_app::meter`), the now-playing label (`model::now_playing`), and `Model::waking`, whose event sink wakes egui.

- **Not yet:** file dialogs and dropped files (`:open` and `:scan` work from the command bar), the sampler view, CJK fonts, and packaging. The Linux window libraries in the CI workflows are egui's usual list, not yet confirmed by a run.

- **Tests:** `crates/playr-gui/tests/window.rs` drives the window headless with `egui_kittest`: tabs, keys, clicks, a confirmation, the search field and the command bar. `tests/keys.rs` covers turning egui key events into bindings.

### Where step 3 differs from the sketch

- **`:open` takes one path.** A path runs to the end of the line, so it can hold spaces without quotes, as a playlist name does. `Action::Open` holds a list, so a GUI's file dialog and dropped files open several at once.

- **Opened tracks join the selection.** They are added to its end and played, and the selection view shows them with the cursor on the first, as `playr <path>` does on an empty selection. Replacing the selection would lose tracks collected for a playlist.

- **A scan can start from an in-memory library.** The terminal tells the session where the library file goes; the scan creates it, and the session moves onto it once the scan finishes. Marks added before that, which were never saved, are lost.

- **Writes during a scan can wait.** A mark or a playlist saved while a scan inserts a batch waits for the inserts, under rusqlite's 5 s busy timeout. Tags are read before the batch's transaction opens, so the wait no longer includes reading files. The library is already in WAL mode.

### Where step 2 differs from the sketch

- **Typed text is shared too.** `Model` holds the whole `Input`, including a search or name being typed and the `:` line, not only which prompt is open. An egui `TextEdit` edits a `String` in place, so the GUI can edit the model's text directly, and both frontends show the same prompt state.

- **Finishing a prompt is a model call.** `answer`, `run_command`, `search_as_typed`, `end_search`, `save_as` and `rename_to` do what the terminal's key handlers did on enter or esc, so a GUI's OK and Cancel buttons do the same.

- **The help list's scroll stays in the terminal.** The model only records that a list is open; the terminal starts its scroll at 0 whenever no list is open.

- **Message expiry is the frontend's call.** `Model::expire_message` runs where the terminal's loop cleared the message before. Expiring inside `refresh` would change when a message disappears in tests that refresh for seconds.

- **The confirmation question stays in the terminal.** `ui::confirm_prompt` ends in "(y/n)", which only fits a key prompt; a dialog words it without.

- **No repaint hook yet.** The model's event sink sends to a channel only. The GUI needs it to call `ctx.request_repaint()` as well; `Model::new` gains that in step 4.

## Open questions

- **Fonts.** egui's default fonts cover Latin, Greek and Cyrillic, not CJK. A terminal shows CJK tags with the terminal's font; the GUI shows boxes unless it loads a font with those glyphs. Bundling Noto Sans CJK adds about 16 MB per binary (estimate); loading a system font needs a font-lookup crate and differs per platform.

- **Native paths.** Both frontends use `~/.local/share/playr` and `~/.config/playr` on every platform, which is unusual on Windows and macOS. Moving to each platform's directories would move existing libraries.

- **Media keys.** GUI users expect the keyboard's play and pause keys, and the macOS and Windows now-playing panels, to work. The terminal has none of this, so under the parity rule it waits, or both frontends gain it.

- **After parity.** Multi-row selection, sorting by column, and dragging a mark each change `Frontend` or `Session`, and are listed as open issues in `docs/architecture.md`.
