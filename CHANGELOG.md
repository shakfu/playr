# Changelog

Notable changes to playr. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Saved loops: up to 8 a track, kept in the library beside its marks. `:loop N`, on F1 to F8 in the sampler and numbered buttons in the window, saves the range to an empty slot and recalls a full one, making it the range and looping it; `:loop N save`, on shift-F1 to F8 or shift-click, saves over one, `:loop N clear` empties it, and `:loops clear`, or Clear loops in the window, empties them all after asking. One command both saving and recalling, by whether the slot is full, over separate save and load commands: a loop is set up once and recalled many times, so the common case takes one key. Pruning and forgetting a root remove a track's loops with its marks. The table needs no schema version, as `roots` did not. Library API: `db::query::loops`, `save_loop`, `clear_loop`, `clear_loops`; `Session::loops_for`, `save_loop`, `clear_loop`, `loops_to_clear`, `clear_loops`, `LOOP_SLOTS`, `Loops`; `Outcome::LoopSaved`, `LoopCleared`, `LoopsCleared`; `Refusal::NoLoops`; `Task::Loop`; `Confirm::ClearLoops`; `Action::ClearLoops`; `Snapshot::loops`; `Action::LoopSlot`, `SlotOp`; `Message::LoopRecalled`, `NoRangeToSave`.

### Changed

- The window's transport buttons show media symbols (U+23EE, U+23F8, U+23F9, U+23ED); each tooltip names the action and its key, and `transport_text_buttons = true` in `[gui]` shows words as before. Play and From start, a bar beside a play triangle, are painted: U+23F5 is small beside the other glyphs, and no font egui ships has U+29D0 or U+23FD. The window can no longer be narrowed below 800 by 480 points. The ReplayGain menu moved to Playback, ReplayGain, and the gain applied shows beside the format. The marks buttons now have a row of their own.

  At the default 1100 points, the ReplayGain menu was cut off: the row needed 1270 and never wrapped. egui wraps a row only before a widget wider than the space left, and a slider or combo box asks for exactly the space left.

- In the window's sampler, 10-point gaps separate the row of zoom and display controls, the rows that set the range, and the rows that slice it.

- The window's sampler waveform takes the height its controls leave: 36 points more at any window height. A fixed allowance for the controls, larger than they needed, left the rest empty above the transport.

- Subcommands that use no settings, such as `scan` and `search --json`, now read `settings.toml` and print its errors as warnings, then run. Before, a bad file went unnoticed until playr next played. Warn over fail, so a typo in a setting a script never uses does not break the script, and `playr devices` still works to fix `device`.

- In the window, a range's edge shows it can be dragged: the pointer turns to a left-right arrow over it. The reach grows from 5 to 8 points, for marks too, and a drag on an edge picks that edge, so `{` and `}` go on with it. Dragging an edge was already possible, but nothing showed it.

### Fixed

- A Windows path in double quotes in `settings.toml`, as `samples = "C:\Music"`, now gets an error saying to use single quotes or forward slashes. Before, it got toml's message about escapes, or, if every backslash was a valid escape, as in `"C:\new"`, "samples must be an absolute path". A path with a control character in it is now refused, so a half-escaped `"C:\\Music\temp"` is not used with a tab in it.

- A drag on the window's waveform moved what it dragged by as much as the view moved under it. The view moves while a drag is under way when it follows the playhead, and, with Fit range on, when the drag picks the other end to centre on: dragging the start after the end put the start a range's length outside the old range. The view now holds still until the drag ends.

## [0.11.0]

### Added

- Hearing planned slices before they are written. `:audition next|prev`, on `n` and `p` in the sampler and Previous slice and Next slice in the window, plays the next or previous planned slice once, wrapping round at either end. Library API: `Action::AuditionSlice`.

- What an export does at slice edges, against clicks: `slice_edges` in `settings.toml`, `:slice-edges exact|zero|fade`, and an Edges menu beside Write slices. `exact`, the default, cuts as before. `zero` moves each edge to the nearest zero crossing within 10 ms while planning, by the same rule and code as `:snap`, so slices stay exact copies. `fade` fades each slice in over `slice_fade_in` (1 ms) and out over `slice_fade_out` (5 ms, as rtrack fades a tail), and an audition fades the same way, so the choice is heard before export. Changing it plans slices already planned again. A setting over tying edges to the snap toggle: `zero` keeps slices exact but moves edges and finds nothing in silence, and `fade` always works but changes the audio, so which suits depends on the material. `:sli` now matches both this and `:slice`. Library API: `samples::Edges`, `samples::Fades`, `Job::edges`, `Job::fades`; `Session::set_slice_edges`, `slice_edges`, `set_fades`, `fades`, `replan`, `plans_current`; `Settings::slice_edges`, `slice_fades`; `wave::SNAP_WITHIN`, `snap_reach`, `non_negative`, `nearest_crossing`; `Outcome::SliceEdges`; `Action::SetSliceEdges`; `Cmd::PlayOnce` takes the fades.

- Onset slices follow the window's Sensitivity slider as it moves. The region's audio is kept once read, so each step only finds onsets again: 3 ms for 60 s of audio against 19 ms reading it (a WAV, release build, one machine). Values that come while a plan is being made wait for it, and only the last is planned. Library API: `samples::OnsetAudio`, `samples::plan_with`; `Sampler::onsets_wanted`.

- Loop points in `samples.json`. A range cut whole (`:slice region`) while it loops is written with `loop_enabled`, `loop_start` 0 and `loop_end` its length, which rtrack reads, and keeps its edges as set. Library API: `Job::loops`.

- `:fit [on|off]`, on `f`, and the window's Fit range tick box: zoom to the deepest step that shows the range, then centre the view on it rather than the playhead, so zooming keeps the range in view. While it is on, `[` and `]` centre the view on that end, so the end `{` and `}` move stays still while the waveform moves under it. A playhead out of view shows as `<` or `>` at that end of the axis, or an arrow in the window. A toggle over a one-shot zoom, because the view otherwise centres on the playhead again at the next frame. Library API: `Action::Fit`; `Sampler::fit`, `fit_edge`, `centre`; `sampler::zoom_to_fit`; `Layout::with_centre`, `playhead_off`; `Message::Fit`.

- `:restart`, on `R`, and a From start button in the window's transport: play from the start of the sampler's range when one is set, else from the start of the track, from any view and any state. The web page has it too, where there is no range. Library API: `Action::Restart`.

### Changed

- Audition plays the planned slice under the playhead ahead of the range; a range used to hide the slices cut from it. In the sampler, `n` and `p` step through slices rather than tracks: a track change discards the plan in any case, and `:next` and `:prev` still skip.

- Fat LTO moved from the `release` profile to a new `dist` profile, which the release and Linux package workflows build with. A release rebuild after an edit to `playr-core` took 178 s with it and 9 s without. `make release`, `make install` and `make app` build with `dist` and Opus, as the release workflow does, into `target/dist`, so they need cmake. `make install-dev` installs a debug build, stripped: unstripped, the three come to about 870 MB.

- Turning snap on moves the ends of a range already set to the nearest zero crossings. Ends set with snap on already snapped; a range drawn before it did not. A range shorter than the two snaps keeps its ends.

### Fixed

- Audition played a different span, or nothing, when pressed again. It pauses on the end of a region or slice, which is where the next one starts, so the next press played that one; it now plays the same span again while the playhead is there. A range ending at the track's end never paused: the pause needs the decoder at the range's end, which refuses the end of the track, so the track played out and stopped, and every later press found it closed. A one-shot now stops a frame short of the track's end. A press during an audition went on from the playhead; it now starts the span again.

- A seek while stopped did nothing, so after `x`, or once the queue had played out, the playhead could not be placed or a mark set until the track played again. The engine had closed the track and dropped the seek. A seek, an audition or a loop now reopens the track paused at that point; the output is paused before any audio reaches it, so nothing sounds.

## [0.10.0]

### Added

- `playr analyze` decodes each library track once and records loudness, peak, tempo and checks in two new tables, `analysis` and `album_loudness`. The checks are decode errors, FLAC MD5, decoded length against the header, padded bit depth, and the spectral cutoff. `--report` derives findings from the stored measurements, so a threshold can change without decoding again, and adds duplicates by tags and length or by FLAC MD5. A run decodes only tracks whose size, mtime or analyser version changed, and it runs beside a playing playr: it writes only its own tables, which no frontend caches. Files are never written. Design and calibration in `docs/dev/analyze.md`.

  `:analyze [DIR]` runs the same work from inside playr, on a session job, and the window has File, Analyze library. The page may run it over the whole library, not over a directory, as with `:scan`. An analysed track shows its tempo beside the now-playing line, moved by varispeed, and `bpm:128`, `bpm:120..130`, `bpm:140..` or `bpm:..90` searches the recorded tempos, alone or with text.

  `:info` shows one track's measurements in a dialog, in all three interfaces: without it an analysis wrote 22 columns a track and the interface showed one number, the tempo. The row under the cursor in the library and selection views, the playing track elsewhere.

  `analyze_on_scan`, off by default, analyses what a scan inside playr added or found changed: without it the measurements wait for a command nobody runs, and ReplayGain quietly falls back to tags. `playr scan` reads no settings, so it prints how many tracks are waiting instead.

  A tempo records the level above it as an alternate when the track pulses nearly as strongly there and the reading sits below the prior's 120 BPM centre, where halving happens. `bpm:` matches either, so a drum and bass track recorded at 87 is found by `bpm:174`; the number shown stays the one measured. Rows from analyser version 1 have no such column and are analysed again.

  `MIN_CONFIDENCE` is 0.3, chosen against librosa over a 328-track library rather than guessed: at 0.2 a fifth of the tempos kept disagreed with librosa at no simple ratio, against an eighth at 0.3, for 13 points of coverage. The threshold applies when a tempo is read, so changing it reanalyses nothing.

  The lossy-source and upsampling findings are heuristics, checked against ffmpeg's encoders and resampler only. They catch LAME at 192 kbit/s and AAC at 128, not LAME at 320, whose cliff at 20.3 kHz is above the 20 kHz threshold that spares CD masters. Tempo is one BPM a track; a pulse above about 170 BPM reads at half tempo, from the 120 BPM prior.


- ReplayGain: `:replaygain` and the `replaygain` setting, `off` by default, `track`, `album`, or `auto`, which takes album gain in normal and repeat modes and track gain otherwise. Gains come from `playr analyze`, else from `REPLAYGAIN_*` or `R128_*` tags; analysis first, so the library is measured one way. The gain is applied where the engine decodes, not in the audio callback, because the ring spans track boundaries and the callback would change the gain up to 2 s from the boundary. It is capped at the track's peak, never boosts without one, and at exactly 0 dB leaves samples untouched. The bottom line shows it, as `rg -6.2 dB`; the level meter now reads after it.

  Library API: `analysis`, `gain`, `db::analysis`; `Cmd::SetReplayGain`, `Cmd::SetGains`, `Status::gain_db`; `Session::analyze`, `Session::analysed`, `Session::bpm`, `Session::set_replaygain`, `Session::replaygain`; `Event::AnalyzeProgress`, `Event::Analysed`; `Settings::replaygain`, `Settings::analyze_on_scan`; `Action::Analyze`, `Action::SetReplayGain`; `Model::bpm`; `AudioStream::open_verifying`, `finalize`, `skipped`, `lossless`, `bits`, `header_frames`, `md5`.


- Columns and sort order. `columns` and `sort` in `settings.toml` say which columns a track list shows and what it is ordered by, most important key first; a `[terminal]`, `[gui]` or `[server]` table sets them for one program, since a terminal row has less room than a window. `:columns` and `:sort` change them until playr exits, a click on a column heading sorts by it in the window, and the page has a sort control. The columns `tempo`, `loudness` and `peak` come from `playr analyze`, and a track it has not measured sorts last whichever way the column is sorted, so an unanalysed library never fills the top of a sort.

  Sorting is in the session, not in each interface: the list shown is the list played, so ordering had to be one thing rather than three. It is done in Rust rather than in SQL because the measured columns live in `analysis`, keyed by path and counted only while they still describe the file. A search is the library filtered, so it comes back in the same order: `bpm:170..180` sorted by loudness answers "everything near 174, loudest first". The page's rows are fetched by a revision that hashes the list's address and length, and sorting leaves both alone, so the sort keys are hashed with them.

  As shipped the window keeps listing the title first, through a `[gui]` table in the defaults, and the other two take the shared order; no program shows a measured column until asked.

  Library API: `columns` (`Column`, `SortKey`, `Measures`, `Cell`, `cell`, `compare`); `Session::set_sort`, `Session::sort`, `Session::sorted`, `Session::measures`, `Session::measures_of`; `Settings::columns`, `Settings::sort`, `settings::columns_value`, `settings::sort_value`; `config::Program`, `Config::load_for`, `Config::parse_for`; `Action::SetColumns`, `Action::SetSort`; `Model::columns`, `Model::measures`, `Model::measures_map`.

### Changed

- `Status` and `Settings` gained fields, so a struct literal of either no longer compiles; `..Default::default()` does. Building the two from a literal is what a library user does, which is why this is a minor bump rather than a patch.

- One `settings.toml` now serves all three programs. A table another program owns, such as `[gui]` or `[server]`, is passed over by the programs that do not read it, where before any table a program did not name stopped it starting: a `[gui]` table refused to let the terminal run. `settings::FRONTEND_TABLES` holds the names; a name no program owns is still an error, and so is a program's name used for something that is not a table. Only the owner checks a table's contents, so a mistake inside `[gui]` is reported by the window and not by the terminal.

### Fixed

- The Opus header's output gain was ignored. RFC 7845 section 5.1 requires a player to apply the signed Q7.8 dB value at offset 16 of `OpusHead`; playr read only the pre-skip beside it, so a file whose level a tool such as `rsgain` had written there played at the wrong level, and `R128_*_GAIN` tags, which count from that gain, were wrong by the same amount. Encoders write 0, so a file straight from `opusenc` or ffmpeg was unaffected.

## [0.9.1]

### Fixed

- Playback could start with the channels swapped, and stay swapped. The audio callback popped samples one at a time, filling silence while the ring was empty; when the engine pushed during the callback, the next pop succeeded, so the track began wherever the callback had reached, which could be partway through a frame. Every frame after was then one sample off, left in the right channel. This can happen whenever the ring runs empty: at the start of a track, after a seek, or on an underrun. The callback now takes only the whole frames the ring holds when it starts, and leaves anything pushed later for the next callback. Found by `enqueueing_after_the_queue_ends_starts_playback`, which failed about 1 run in 200 under load on macOS CI; `crates/playr-core/tests/render.rs` reproduces the race directly. The swap was not heard, only reproduced against the fake device.

- `a_seek_near_the_end_of_a_track_stays_in_that_track` failed now and then on the Windows runner. It allowed the engine 100 ms to take a seek, and until the engine takes it the position is the one before it. It now waits for the seek, as the other engine tests do.

### Changed

- A `hw:` device that PipeWire holds is now checked to fail as busy, and is recorded so in `docs/dev/device.md`. PipeWire holds only the device it plays to, so another card's `hw:` device plays alongside it. The cpal lookup bug playr works around is drafted as an upstream report in `docs/dev/issues/cpal-issue.md`.

## [0.9.0]

### Added

- Output device selection. `--device ID` on all three programs, or `device` in `settings.toml`, plays to a device other than the default; `playr devices` lists the IDs, which are cpal's `<host>:<id>`, and a bare `hw:2,0` means the default host. Matching is by ID only: on ALSA one card's name labels up to 15 PCMs. A device that is not found stops playr at startup with the list, rather than falling back to the default, which would play through the wrong speakers and hide why output is not bit-perfect. A device another program holds is reported as busy rather than as a failed stream. Design in `docs/dev/device.md`.

  cpal 0.18.2 cannot find some IDs it lists, such as `alsa:sysdefault:CARD=X`: its ALSA lookup appends `,DEV=0` to an ID with a card and no device. playr matches the exact ID first.

  Library API: `output::device`, `output::devices`, `output::by_index`, `output::DeviceInfo`; `OutputError::NotFound`, `OutputError::Busy`; `Settings::device`. `Player::new` takes the device ID; `output::default_device` is gone.

- A spectrogram display in the sampler view, `:display spectrogram`, between dB and Braille in the `w` cycle; the window has a Spectrogram button. It shows level by frequency on a log scale, to separate hits the waveform merges and to show a lossy source's cutoff. It is read in the same pass as the peaks, rather than on demand in a second decode: 2048-point transforms every 512 frames into 128 bands, about 90 ms more per 4-minute track. Both draw in magma: the terminal two rows a cell in half blocks, from the 256-colour entries nearest it, or in shades without colour; the window as one texture, its frequency labels on panels. Magma over a ramp in the waveform's blue: one hue from the ground colour spans too little lightness, and most of a 90 dB range drew as the same mid-blue.

  Below about 340 Hz at 44.1 kHz a band is narrower than a transform bin. Such a band reads the level interpolated between the bins either side of its centre, rather than the nearest bin, which drew runs of identical rows as stepped bands that looked like content. Widening those bands to a bin each was the alternative; it left 20 to 600 Hz 21% of the height rather than 48%. `:display` in the command help now reads `[DISPLAY]`, since the four names widened the usage column and cut other commands' help at 80 columns.

  Library API: `spectrum::Spectrogram`, `Peaks::spectrum`, `Display::Spectrogram`, `Layout::spectrum`, `sampler::SPECTRUM_RANGE_DB`.

## [0.8.1]

### Fixed

- Just after a seek, the position could report where playback was before it, for one pass of the engine. The device discards the pre-seek audio and raises `flush_done` at once, but `frames_out`, `track_start` and `position_offset` still describe that audio until the engine resets them, and a reader took the device's word for it. Anything reading the position in that window acted on the old playhead: `:slice` planned the region around it, `b` marked it, and `,` and `.` stepped from it. The position now holds the seek target until the engine has reset the counters.

- While looping, the reported position ran past the loop's end before returning to its start, by up to one pass of the engine. The audio callback advances the device's frame count; the engine advances the matching track start and offset on its next pass, so a position read between the two was measured from the loop's previous return. The player now applies the returns the device has reached when it is asked for a position. Applying them in the callback would be as exact but would put a queue on the realtime thread.

### Added

- `:rescan`, or `:sync`, re-scans every directory previously given to `:scan` or `playr scan`, so new files are picked up without retyping paths. Each scan records its directory in a `roots` table, and a bare `playr scan`, a bare `playr prune`, `:prune` with no argument and the window's File, Rescan library all cover the recorded ones. A directory holding no audio file is not recorded; nor is one inside a directory already recorded, since the wider one covers its files, and recording a wider one drops the narrower. A scan that finds missing tracks asks before pruning them, or prunes at once with `auto_prune = true` in `settings.toml`. It does not ask over a prompt being typed, and `auto_prune` steps aside when a directory could not be read: an unmounted drive whose mount point survives counts every track under it as missing.

  Library API: `db::add_root`, `db::roots`; `scan::scan_roots`; `Session::rescan`, `Session::roots`; `Action::Rescan`; `Refusal::NoRoots`; `Settings::auto_prune`; `ScanReport::unavailable`. `Event::Scanned`, `Event::Pruned`, their outcomes, `Action::Prune` and `Confirm::Prune` take `Option<PathBuf>`, `None` meaning every recorded root.

- `:roots` lists the directories the library covers, `:roots rm DIR` forgets one, and `:roots add DIR` is another spelling of `:scan DIR`. `playr roots`, `playr roots rm DIR` and `playr roots add DIR` do the same; the window has File, Library directories, with a Forget button per row. Forgetting removes the tracks under the directory, their places in playlists and their marks. Unlike `:prune` it asks the filesystem nothing, so a directory that is already gone can still be forgotten, matched by the path as stored when it no longer resolves. The web page may list the directories but not change them, since the two that change them name a path.

  Library API: `db::forget_root`; `Session::check_forget`, `Session::forget_root`; `Action::ShowRoots`, `Action::ForgetRoot`; `Confirm::ForgetRoot`; `Presentation::RootList`; `Input::Roots`; `Outcome::Forgot`; `Refusal::NotARoot`.

- Media keys and the now-playing panel: the keyboard's play, pause, next and previous keys, MPRIS on Linux as `org.mpris.MediaPlayer2.playr`, and the macOS and Windows panels, through `souvlaki`. Linux uses its `use_zbus` feature, so nothing links libdbus. Both frontends call `Model::attach_media`, which is a no-op where there is no bus or panel, and playr runs as before; on Windows the panel needs a window, so the terminal has none. MPRIS is a local bus, not a network.

- Taking up again where playr left off. The playing track and position go into a one-row `resume` table when playr closes, and again at each track change, so a playr that is killed still leaves the track behind. The next start offers it, unless the command line named tracks to play or the file has gone. Library API: `db::set_resume`, `db::resume`, `db::clear_resume`; `Session::remember`, `Session::resumable`, `Session::forget_resume`, `Session::resume`; `Confirm::Resume`.

- A cursor in the sampler view, apart from the playhead, and mark editing through it. `:cursor` moves it and `h` returns it to the playhead; `:pick next|prev` puts it on a mark, which is how one is picked up. `:nudge-mark` moves that mark a column, `:move-mark TIME` puts it at a time, `:del-mark` removes it wherever it sits in the chain `B` undoes, and `:snap-mark` moves it to the nearest rise in the two seconds either side, read in the background. The window drags a mark along the waveform. A move onto another mark is refused rather than merging the two. Library API: `query::move_mark`, `query::remove_mark`; `samples::nearest_onset`, `samples::SNAP_WINDOW`; `Session::mark_near`, `move_mark`, `remove_mark`, `snap_mark`; `Event::Snapped`; `Sampler::cursor`.

- `:audition`, `a` in the sampler, plays the range, the planned slice the playhead is in, or the region around it, once, and pauses at its end rather than returning to its start as `:loop` does. Playing on continues the track from there. `Cmd::PlayOnce` sets the same loop with `once`, and the engine leaves the decoder at the end rather than seeking back; `Status::looping` leaves a one-shot out, so `:loop` cannot switch off a range that was only auditioned.

### Changed

- `playr-server` drops `--music DIR`. The page's Rescan library re-scans the directories `playr scan` recorded, so the one place a root is named is the library. The button appears once the library has one. A unit file or script passing `--music` must drop it; nothing replaces it.

## [0.8.0]

### Added

- `playr-server`, playr with no screen, for a machine such as a Raspberry Pi with a DAC. Its web page has the window's views, key bindings, `:` command line, dialogs, marks and themes, without the sampler, and adapts to a phone, a tablet or a desktop browser. With `--music DIR` the page can rescan that directory, and no other; it cannot open, scan or prune a path, or quit. With `--osc`, OSC controls playback and plays playlists by index, and `--osc-reply` sends the state back; `playr-server osc-schema` lists the addresses, and `make touchosc` builds a TouchOSC layout from them. See `docs/dev/server.md`. Every web request needs a token, printed at startup, or none with `--open`, for a trusted network or behind a proxy that authenticates. Either way the `Host` must name the machine, so another web site open in the browser cannot reach it. It listens on 127.0.0.1 unless `--listen` says otherwise, and shares the lock of `playr` and `playr-gui`. `playr` and `playr-gui` still have no network code. Release archives include `playr-server`, Linux ones with a systemd user unit, and each release a TouchOSC layout; [docs/server-guide.md](docs/server-guide.md) covers setting it up on a Raspberry Pi.

  ```
  playr-server --listen 0.0.0.0:8080 --open --host pi.lan --music /mnt/music \
    --osc 0.0.0.0:9000 --osc-reply 192.168.1.30:9001
  ```

### Changed

- The documentation covers three programs. The README introduces `playr-server` and links its guide, `docs/server-guide.md`, and the "no network code" claim now names `playr` and `playr-gui` only. `docs/architecture.md` and its crate diagram add the server, whose owner thread is half of the daemon the notes left open. `docs/sampler.md` records that OSC for marks would go in `playr-server`, the one program with network code. `TODO.md` lists the server's gaps, among them the Pi and TouchOSC checks, CI browser tests and a `[server]` settings table.

### Fixed

- `:speed` could not set a negative speed. A signed number is relative, so `:speed -3` lowered the speed by three semitones from where it was, yet the error for `:speed 13` gave the range as -12 to 12. `=` now makes a signed speed absolute. It was chosen over reading a bare signed number as absolute, which would change the `(` and `)` bindings and every `[keys]` table that copies them.

  ```
  :speed =-3
  ```

## [0.7.0]

### Added

- Fine movement in the sampler view. There the arrows move the playhead a column, and with shift a tenth of the view, so a step follows zoom; elsewhere they still seek 5 and 30 s. `:snap`, on `S`, moves nudges, marks, seeks and range ends made in the view to the nearest zero crossing within 10 ms, where the channels' mean changes sign. Marks made in the view may be a frame apart, where 500 ms elsewhere blocked close hits. A step that follows zoom was chosen over a fixed `:step` setting, so zooming in is the one way to go finer. Sign bits are kept with the peaks, 1.3 MB for a 4-minute track, so a snap does not decode. The window has a Snap to zero tick box, and its arrows nudge too. Library API: `Action::Nudge`, `Snap`; `Nudge`; `sampler::Scale`, `snap`, `nudge`, `frame_of`, `time_of`, `SNAP_WITHIN`; `Layout::columns`; `Model::set_scale`; `Drawn::scale`; `Message::NoWaveform`, `Snap`; `Peaks::crossing`; `Session::add_mark_within`.

  ```
  :nudge +1    :nudge -10%    :snap on
  ```

- Deeper zoom in the sampler view: to one frame a cell in the terminal, and to 16 points a frame in the window, where the line display draws each frame's channels' mean around a zero line, with a dot per frame. Past the 64 frames a column the peaks hold, the view decodes the frames it shows, and 2 s either side, in the background; the window says "reading frames" until they arrive. Decoding on demand was chosen over finer peaks, which would take 4 to 32 times their memory for every track. A nudge moves at least a frame. Library API: `sampler::DetailRead`; `DETAIL_BELOW`, replacing `MIN_FRAMES_PER_COLUMN`; `DETAIL_MARGIN`; `window` and `Layout::new` take the most columns a frame; `Layout::per_frame`, `detail`, `with_detail`; `Scale::start`, `per_frame`, `shown`, `needs_detail`; `Sampler::detail`; `wave::Detail`; `Session::read_detail`; `Event::Detail`.

- A range to slice in the sampler view, set with `<` and `>` at the playhead, `:range START END`, or a drag across the window's waveform, and cleared with backspace. With both ends set, every cut uses it in place of the region between marks, and `:slice marks` cuts at the marks inside it. It lasts until the track changes and is not saved. Under the window's waveform, buttons set and clear it, and slice the region or range whole, at marks, into a chosen count of equal parts, or at onsets with a sensitivity slider. Library API: `Action::RangeIn`, `RangeOut`, `SetRange`; `sampler::Range`; `Sampler::range`, `range_ends`, `set_range_start`, `set_range_end`; `Layout::with_range`; `Frontend::sampler`, `sampler_mut`; `Message::Range`, `EmptyRange`; `Job::range`; `Session::slice_job`, `plan_slices` and `export` take a range.

- Looping the range in the sampler view. `l` or `:loop` plays it over and over, returning from its end to its start sample-exactly and without a gap, and starts a paused track. Moving either end moves the loop at once; in the window, a drag from a range's edge moves that edge, and a Loop range tick box sets it. `esc` now clears the range when no slices are planned, which ends a loop. The engine loops rather than the frontend seeking at the end, which would leave a gap and miss the end. Library API: `Cmd::Loop`; `Status::looping`; `Action::Loop`; `Message::Loop`, `NoRangeToLoop`.

  ```
  :range 1:02 1:04.5    :loop on
  ```

- Moving one end of the range a column at a time, for setting a loop's ends while it plays, when the playhead will not stay still for `<` and `>`. `[` or `]` picks the start or end, drawn reversed in the terminal and thicker in the window, and `{` `}` move it earlier or later, snapping when snap is on; an end stops a frame short of the other. Picking an end, then moving it, was chosen over a pair of keys for each end, so one pair moves either end. The window has Move start, Move end, Earlier and Later. Library API: `Action::PickEdge`, `MoveEdge`; `sampler::Edge`; `Sampler::edge`; `Message::Edge`, `NoEdge`.

  ```
  :edge end    :edge -4    :edge +10%
  ```

### Changed

- Varispeed moved from `[` and `]` to `(` and `)`, in every view, so the sampler view can use the brackets for the range's ends. `\` still returns to normal speed. A `[keys]` table in `settings.toml` can bind the brackets back outside the sampler view.

### Fixed

- The time on the terminal's progress bar was hard to read. Over the filled part it was the default text colour on the bar, light on cyan in the dark theme; past it, it was the bar's colour, 3.8:1 on white in the light theme. It is now black over the filled part and the default colour past it, since black over the whole label vanishes past the fill on a dark background. It is not bold: some terminals draw bold black as grey. Library API: `Palette::progress_text`.

- On Linux, `make install` installed the desktop entry, but its icon did not show when another app had left an `icon-theme.cache` under `~/.local/share/icons/hicolor`. GTK trusts that cache while `hicolor/` is no newer than it, and adding a file to `256x256/apps/` does not change `hicolor/`. `make install` now rebuilds an existing cache. It does not create one, since a cache that nothing refreshes causes the same failure for the next installer.

- On Linux, playr was not offered to open audio files after `make install`. GIO reads a desktop entry's `MimeType=` only through `~/.local/share/applications/mimeinfo.cache`. `make install` now rebuilds that cache, creating it if needed.

## [0.6.2]

### Added

- Light and dark themes, set by `theme = "system"`, `"light"` or `"dark"` in `settings.toml`, by `:theme` until playr exits, and in the window by View, Theme. `dark` is the default in both, as before. `system` follows the system's appearance in the window. A terminal cannot reliably report its background, so in the terminal `system` and `dark` both use the terminal's own ANSI colours. The terminal's light set uses the 256-colour table over remapped ANSI colours: terminal themes rarely change that table, and ANSI yellow and cyan are unreadable on xterm-style light palettes. Tests hold both light sets and the window's dark set to WCAG contrast: 4.5:1 for text, 3:1 for lines and bars. Library API: `Theme`, `Action::Theme`, `Presentation::Theme`, `Message::Theme`, `Config::theme`, `Model::theme`, `Screen::theme`, `playr::ui::palette`, `playr_gui::palette`.

### Fixed

- With `NO_COLOR` set, the terminal did not highlight the cursor row. crossterm honours `NO_COLOR` by sending an SGR reset in place of each colour, which also clears bold and reverse video. playr now removes colour itself and reverses the cursor row. Library API: `Screen::colour`, `App::set_colour`.

## [0.6.1]

### Changed

- A scan no longer removes tracks whose files are gone. It counts them, and `playr prune DIR`, or `:prune DIR` after a confirmation, removes them with their places in playlists and the marks of every missing file under `DIR`. A scan pruned on its own, so a subfolder on an unplugged drive lost its tracks from every playlist at the next scan of its parent. The window has File, Remove missing files. Library API: `ScanReport::missing` replaces `removed`; `db::missing_under`; `db::prune_missing` returns `Pruned` and removes marks; `Session::check_prune`, `prune` and `pruned`; `Event::Pruned`; `Outcome::PruneStarted` and `Pruned`; `Task::Prune`; `Confirm::Prune`; `Action::Prune`.

- Only one of `playr` and `playr-gui` runs at a time. While either is open, the other refuses to start with "playr is already running", and so does `playr scan`; `playr playlists`, `playr search --json` and `playr formats` still run. Each keeps playlists and marks in memory and checks changes against that copy, so two at once could replace each other's playlists without asking, and undo each other's marks. The claim is a per-user lock file, `instance.lock` beside the default library, whichever `--db` is open; the system releases it when playr exits, however it exits. Library API: `playr_app::instance`.

### Fixed

- Answering yes to "clear all marks" cleared the marks of the track playing at the answer, not the track asked about. A gapless track change while the question was open deleted the next track's marks. The question now names the track and carries its path. Library API: `Confirm::ClearMarks { path, count }`; `Session::marks_to_clear` returns the path with the count, and `Session::clear_marks` takes the path.

- Saving a mark or a playlist during a scan could fail with "database is locked". The scan held the write lock while it read tags for up to 500 files, which can take longer than the 5 s busy timeout. Tags are now read before each batch's transaction opens.

- In the sampler view, a slicing started before another finished could replace the newer one's planned slices, and `enter` then wrote the older cut. Its result also cleared "planning slices" while the newer one still ran. A plan is now shown only if it belongs to the latest slicing. Library API: `Sampler::planning` is the planning job's id, and `Frontend::planning` takes it.

- A panic while scanning, such as one in a tag reader, ended the scan with no report, and every later `:scan` was refused as already running. The scan now ends with its error, and another can start.

- Reindexing an older library on open was not atomic. Interrupted between dropping the old search index and refilling the new one, it left an index that later opens took as current, so searches missed every existing track. The migration now runs in one transaction and is redone on the next open.

- A seek to or past the end of a track did nothing, with no message, since the decoder refuses it. It now moves on as the track's end would: to the next track in play order, still paused if playback was, or stops after the last.

- A seek reported the position from before it until the device discarded its buffer, so two quick `:next-mark` presses could both choose the same mark. The position is now the seek's target until then. Changing track in that window also left the discard pending, and it then dropped up to 2 s from the start of the next track.

- `playr-gui` declared Rust 1.89, the workspace's version, but egui 0.36 needs 1.95. It now declares 1.95. CI checks the other crates on 1.89.

- `playr` started the interface with stdout redirected, and drew it into the file or pipe. crossterm opens `/dev/tty` when stdin is not a terminal, so the terminal check passed. playr now exits with an error when stdout is not a terminal. This also stopped `tests/cli.rs` from hanging when run from a terminal.

## [0.6.0]

### Added

- A desktop window, `playr-gui`. It opens with the terminal's options, key bindings and `:` commands, and draws the library, selection and playlists as tables, with search, the transport, marks, volume, speed, mode and the level meter. Menus and a right-click menu on each row reach every action a key does; selection rows drag to a new place; File opens files and folders or adds a folder to the library, and files dropped on the window play; the command bar completes with Tab and recalls with the arrows. The sampler view paints the waveform in the terminal's three displays, with the region, marks, playhead and planned slice edges; a click seeks, a shift-click marks, the mouse wheel zooms, and buttons write or discard planned slices. Release archives carry it beside `playr`: as `playr.app` on macOS, unsigned, with `playr.desktop` and an icon on Linux, and with its icon embedded and no console window on Windows. `make install` installs it beside `playr`, with `playr.app` in `~/Applications` on macOS and a desktop entry on Linux; `cargo install --git` builds it from GitHub. The release workflow, run by hand with no tag, builds and packages every platform without publishing. `docs/dev/gui.md` records the design and what is still open. It is built with egui's own dark style and wraps the same `Model` as the terminal, so a click does what the key does. Library API: `View::ALL` and `View::title`, `Confirm::question`, `command::key_rows` and `command_rows`, `playr_app::meter`, `model::now_playing`, `Model::waking`, `CommandLine::replace`, and `sampler::Layout`, `peaks_of`, `plan_text` and `edges`, which the terminal's sampler view now draws from too.

- `:scan DIR` adds a directory to the library without leaving playr, and `:open PATH` plays a file or directory. A scan runs in the background, counts files on the bottom line, and creates the library if playr started without one; the library view shows the new tracks when it finishes. `:open` adds the tracks to the end of the selection and plays them, as `playr <path>` does, so tracks already collected for a playlist are kept. Both exist so a GUI user who never opens a terminal can build a library and play files, and the terminal has them too so the two frontends do the same things.

  Saving a mark or a playlist during a scan waits while the scan commits its current batch of 500 files, and fails after 5 s. Library API: `Session::scan`, `scanned`, `open` and `set_library_path`; `Event::ScanProgress`, `Scanned` and `Opened`; `scan::scan_into` and `scan::playable`; `App::set_library_path`.

### Changed

- The interface's state is shared, the first step of a GUI frontend (`docs/dev/gui.md`). `playr_app::model::Model` holds what the terminal's `App` held apart from drawing: the session, views and cursors, search results, the list playing, prompts and questions with their typed text, the message, the sampler's state and the per-frame snapshot. It implements `Frontend`, and `App` wraps it, so a GUI built on it does what a key does in the terminal. Nothing a user sees changes.

  Library API: `ui::notice::text` is `playr_app::message::text`, with `fmt_time` and `home_as_tilde`; `ui::Snapshot`, `ui::Input`, `hold_peak` and `PEAK_HOLD` are in `playr_app::model`, and `ui` re-exports `Input` and `Snapshot`; `ui::sampler`'s `Sampler`, `Wave`, `window`, `fmt_frames`, `db_height`, `DB_FLOOR` and `MIN_FRAMES_PER_COLUMN` are in `playr_app::sampler`, and `ui::sampler` keeps the glyph code; `App` no longer implements `Frontend`.

- The terminal's sampler view starts in the Braille display, which shows a waveform's shape most clearly in text; `w` then steps to the envelope and dB displays. The desktop window still starts on the envelope. Library API: `Model::set_display`.

### Fixed

- On Windows, searching matched the folders a track sits in. The search index takes a file's name from its path, and read only `/` as a separator, so a Windows path went in whole and a search for `music` found every track under `C:\Users\...\Music`. A Windows path, one starting with a drive or `\\`, now has its backslashes read as separators, and a library indexed by 0.5.0 or 0.5.1 is reindexed when opened. A backslash in a Unix file name stays part of the name.

- `:move +N` and `:move -N` move the track N places, as documented. They swapped it with the track N places away, so `:move +2` on A, B, C gave C, B, A rather than B, C, A. `J` and `K` move by one place, where the two agree.

## [0.5.1]

### Added

- Release binaries. Pushing a version tag such as `0.6.0` runs `.github/workflows/release.yml`: it checks that the tag matches `Cargo.toml` and that `CHANGELOG.md` has a section for it, runs `make test` on Linux, builds with Opus for Linux (x86_64, arm64), macOS (arm64, x86_64) and Windows (x86_64), and publishes the archives, `SHA256SUMS` and that changelog section as the GitHub release. The workflow can also be run by hand for a tag pushed earlier.

### Fixed

- playr started only when `HOME` was set. The default `samples` directory expands `~`, and without `HOME` the defaults failed to parse, which panicked at startup. Windows does not set `HOME`, so every Windows build would have stopped there. The home directory now comes from the platform, `USERPROFILE` on Windows.

## [0.5.0]

### Added

- Samples. `:slice region` writes the region between the marks either side of the playhead as a WAV file. `:slice marks` cuts the whole track at every mark, `:slice N` cuts the region into N equal parts, and `:slice onsets [S]` cuts it where hits start, with onset detection taken from rtrack. `S` runs from 0 to 1, and without it `onset_sensitivity` in `settings.toml` applies; it is read when a slice runs, so a key bound to `slice onsets` follows the setting. Each export is a new directory under `samples` in `settings.toml`, `~/Music/playr/samples` by default, laid out as rtrack's sample bank: `000-name_S00.wav` and so on, with a `samples.json` recording each slice's source frames.

  Slices are read from the source file, not captured from the output, so volume and varispeed do not reach them. Files are 24-bit at the source's rate and channel count, which copies 16- and 24-bit sources exactly; resampling or mixing to stereo, as rtrack's own render does, would have changed the audio for no gain in rtrack. Writing WAV adds the `hound` crate.

- A sampler view, `4`, showing the playing track's waveform with its marks, playhead and region. Three displays of the same data switch with `w`: a rectified envelope in eighth blocks, with each column's RMS level inside its peak level; the same bars on a dB scale from -48 dBFS, where a linear scale would squeeze RMS into the bottom rows; and a symmetric waveform in Braille, to see its shape. dB is a display rather than a scale for every display because a centred Braille waveform has no readable log scale. RMS is drawn because a mastered track's peaks sit near full scale in almost every column of an overview, so peak alone shows a flat block; RMS still shows its sections. `z` and `Z` zoom around the playhead down to 64 frames a column, and `0` shows the whole track. In this view `:slice` plans slices and draws their edges, and enter writes exactly those slices or esc discards them; elsewhere `:slice` still writes at once.

  Peaks are read from the file on a thread, only while the view is open, as a pyramid of min, max and mean square with 32 frames at its finest scale. A column's peaks are gathered from whole buckets and columns start on bucket boundaries, so a hit never shows in the column before it. The waveform glyphs are the only characters in the interface outside ASCII.

- `playr search --json QUERY` prints the matches as a JSON array instead of playing them: one object per track with every library column, `null` for a missing tag. No match prints `[]` and exits with status 1, as a plain search fails. The objects are built in the terminal crate with `serde_json` rather than derived on `Track`: a `serde` feature on the core's types waits for a frontend that needs it, step 8 of `docs/architecture.md`.

### Changed

- `mode` in `settings.toml` must be one of `normal`, `shuffle`, `repeat` or `repeat-one`, in any case. 0.4.0 also accepted a unique prefix such as `shuf`, which would stop parsing once a new mode shares it.

- The command line is parsed with clap. Each command has its own `--help`; an unknown option or extra argument is an error with status 2; and `--db` and `--settings` may come before or after the command. A search that starts with `-` goes after `--`. The parser was hand-written: `playr scan --help` scanned a directory named `--help`, `playr search rock --db` could not search for `--db`, and `playr formats extra` ignored `extra`. clap over a smaller parser such as `lexopt` because it generates help now, and shell completions and a man page later. Adds the `clap` crate. `help` is not a command, so `playr help` still plays a file named `help`.

- The design notes are user documentation now, with diagrams: `docs/sampler.md` covers marks, regions, cuts, the export format and precision as built, and `docs/architecture.md` the crate split. `make diagrams` renders the d2 sources in `docs/media`. They were in `docs/dev/`, excluded from the crate package.

- `?` lists only the keys that work in the view you are in: the view's own keys, then those for every view that it does not rebind. 0.4.0 listed every view's keys, so a key bound elsewhere, or rebound in this view, still showed. `:help` still lists every command, grouped by view, so commands of other views stay findable. The bottom line's help hint names whichever key opens the list in this view.

- The code is a Cargo workspace of three crates, so an egui or Tauri frontend can reuse everything but the terminal. `playr-core` holds the audio engine, library, scanner, sample export, waveform peaks, session and settings. `playr-app` holds what Rust frontends share about interaction. `playr` is the terminal interface and command line. Neither `playr-core` nor `playr-app` depends on a terminal crate. `docs/architecture.md` records the design and its steps.

  What a frontend gets from `playr-core`:
  - `notice::Notice`: what an operation did, or why it refused, as data. Each frontend words it; the terminal's text is unchanged.

  - `session::Session`: the library, player, selection, playlists and marks, driven with a track, a selection index or a playlist id, never a cursor. Replacing a playlist is refused with `Refusal::WouldReplace` until the frontend asks and saves again with `replace` set.

  - `event::EventSink`: track changes, state changes, playback errors, and the end of each background job, so a frontend with no frame loop, such as a Tauri app, can forward events as they arrive.

  - `settings::Settings`: the file's top-level keys. Tables a frontend names, such as `[keys]`, are handed back to it, so a frontend with no key bindings does not need `playr-app` to read the file.

  `playr-app` holds `Action`, the `:` command language, key bindings over its own `Key` type, the `[keys]` tables, and `dispatch`, which does an action through a `Frontend` trait. A second Rust frontend implements `Frontend` over its own cursors and prompts and gets every key binding and `:` command.

- Library API, following the split:

  - `playr::audio`, `playr::db` and `playr::scan` are `playr_core::audio`, `playr_core::db` and `playr_core::scan`.

  - `playr::ui::action`, `command` and `config` are `playr_app::action`, `command` and `config`. `View` and `Confirm` are defined in `playr_app`.

  - `Config` holds `settings: Settings` and `keys`, in place of `volume`, `mode` and `speed`. `DEFAULT_SETTINGS` is split into `playr_core::settings::DEFAULT_SETTINGS` and `playr_app::config::DEFAULT_KEYS`.

  - `render::draw` takes `&Screen` and writes no state. It returns `Drawn`, the cursors, scroll and zoom it clamped to the frame, for `App::drawn` to store. `Screen` holds `lists: Lists` in place of three `ListState`s, and `help_scroll` by value; `Screen::new` fills in defaults; `App::screen` takes `&self`.

  - New: `App::message` returns the message showing; `ui::key_of` turns a terminal key event into a `Key`; `Mode::NAMES` lists the mode names.

### Fixed

- Search finds untagged files. The index held only title, artist, album and album artist, so a file with no tags had nothing indexed, though the list showed it by its file name. The index now has a `file` column, the file name without folder or extension, which `file:` also searches. A library from an earlier playr has its index rebuilt on first open; the schema version is unchanged, so earlier playr can still open it.

## [0.4.0]

### Added

- Marks. `b` marks the playing position, `B` removes the mark added most recently, `,` and `.` seek to the previous and next mark, and `C` clears the track's marks after asking for `y`. Marks show as `^` under the progress bar. They are stored in the library by file path and source frame, so a rescan that renumbers tracks keeps them and they stay exact at any playback speed. The new table needs no schema version change, so older playr still opens the library.

- `r` in the playlists tab renames the selected playlist, starting from its current name. A name another playlist already has is refused, since taking it would merge or replace that playlist.

- Search terms can be limited to one field: `title:`, `artist:`, `album:` or `albumartist:`, as in `artist:evans`. A quoted value is matched as a phrase, as in `artist:"bill evans"`. A prefix that names no field stays part of the text, so a title such as `Op: 1` is still found. `playr search` accepts the same syntax.

- Playback modes, stepped through with `m` and back with `M`: normal, shuffle, repeat, and repeat one. Shuffle plays every track once per pass in random order, reshuffles at the end of each pass, and never opens a pass with the track that closed the last. It keeps a play order beside the list rather than reordering it, so the list on screen keeps its order and `p` steps back through what played. A mode applies to whatever list is playing. Changing mode rebuilds a next track that has already started loading, so the change takes effect from that track. Under repeat, a list of files that will not play stops after one pass instead of retrying them forever.

- Vim-style `:` commands. Every key's action has one, and commands also take what a key cannot: a time, a percentage, or a name. `docs/cheatsheet.md` lists them all, and `:help` inside playr. A leading `+` or `-` makes a number relative.

  ```
  :seek 1:23      :seek -30       :volume 60      :speed +1
  :mode shuffle   :mark 2:05      :playlist late night
  ```

  Commands that act on one view's rows work only there: `:remove`, `:move` and `:clear` in the selection; `:toggle` and `:clear-search` in the library; `:add`, `:delete` and `:rename` in playlists. In another view they are refused with the view they belong to, since they would act on a cursor that is not on screen. A command can be shortened to any prefix unique among those usable in the current view. Tab completes those names and some arguments, and up and down recall earlier lines; the history holds 100 lines for the session.

  Keys and commands both resolve to one `Action` type, performed in one place, so a key and its command cannot drift apart. Library API: `ui::action`, `ui::command` and `ui::config` are new; `App::perform` runs an action; `App::configured` takes a `ui::config::Config`; `Screen` has `keys` and `help_scroll`; `ui::KEYS` is gone; and `Cmd::SetSpeed` sets an absolute speed.

- A settings file, `~/.config/playr/settings.toml`, or the path given with `--settings`. It sets the starting volume, mode and speed, and binds keys. Each key's value is a `:` command, checked by the same parser as the prompt, so a binding cannot mean something its command does not.

  ```toml
  volume = 60
  mode = "shuffle"

  [keys]
  right = "seek +10"
  q = "nop"

  [keys.selection]
  x = "remove"
  ```

  A key under `[keys.VIEW]` wins in that view. The defaults are a settings file too, `src/ui/settings.toml`, read by the same code; a user file applies on top of it. TOML over a file of `:` commands because editors highlight and check it, and it gives colours a natural table later; the cost is the `toml` crate. Any bad setting stops playr before it starts, with every error listed by line. Starting anyway and showing the first error was the alternative, but a skipped binding is easy to miss. `:map` and `:unmap` change keys while playr runs.

### Changed

- `?` lists the keys as bound, grouped by view, with the command each key runs. It showed a fixed list before, which a remapped key would contradict. The bottom line's help hint names whichever key opens it.

- A key with Ctrl or Alt held, or Shift on a key that is not a character, only runs a binding that names that chord. Before, most chords ran the plain key's action, so Ctrl-M cycled the mode as `M` does.

- The key and command lists scroll with `j` and `k`; any other key closes them. The key list already ran past the bottom of a 24-row terminal.

- `esc` clears a search only in the library, where the results are shown. `a` in the selection and `d` in the library did nothing and are now unbound.

## [0.3.3]

### Changed

- Library rows mark tracks in the selection with `+`, beside the `>` for the playing track, so the selection is visible without leaving the library. The mark uses the existing two-character gutter, so no column loses width. `a` on a marked track now unselects it; before, it reported the track as already selected, and removing it meant switching to the selection tab. `a` on a playlist still only adds.

## [0.3.2]

### Changed

- The Queue view is now Selection: a list to collect tracks into and save as a playlist, separate from what plays. It starts empty. `a` adds a track, or a whole playlist, without touching playback; enter in the library plays the library from the selected track and leaves the selection alone. Before, enter replaced the queue with the whole list in view, so a queue built with `a` was lost to one keypress. In the selection, `d` removes a track, `J` and `K` or shift with up and down move it, `c` clears the list after `y`, and enter plays it. `playr <path>` selects the files as well as playing them. `a` moves the cursor down after adding, so a run of tracks takes one key each. A track already in the selection is skipped; a playlist added with `a` keeps its own repeats. Library API: `ui::View::Queue` is `View::Selection`, `App::with_queue` is `App::with_selection`, and `Screen` has `selection` and `playing` where it had `queue`.

- The level meter's bar is coloured by position, as on an LED meter: green below -18 dB, the EBU R68 alignment level; yellow to -6 dB; red above. The peak marker takes its zone's colour. Colouring by the current reading instead would show most commercial masters, which sit above broadcast and streaming loudness targets, as a warning.

### Removed

- The cursor in the former queue view followed playback. The selection is not what plays, so it no longer does.

## [0.3.1]

### Added

- A level meter on the bottom line, while playing: momentary loudness in LUFS per ITU-R BS.1770-4, over 400 ms, with the sample peak held for 1.5 s on the same bar. The bar takes the free width of the line, and a message borrows it while it shows. It is measured as audio leaves playr, so it matches what is heard rather than the decoder, which runs ahead. It reads the recording before the volume setting. On a 1 kHz test tone it agrees with ffmpeg's `ebur128` filter to 0.1 LU.

### Changed

- Panes no longer repeat the view name and count that the tabs show. The library pane has a title only while searching: the prompt as you type, then a reminder that `esc` clears the results.

### Fixed

- Pressing `+` or `-` quickly changed the volume by one step, however many times the key was pressed. Each press added to the volume shown in the last frame, which updates five times a second.

## [0.3.0]

### Added

- Shift with the left or right arrow seeks 30 seconds; the arrows alone still seek 5.

- `?` opens a list of every key. The bottom line, which held key hints cut off at about 80 columns, now shows messages on the left, and the speed, a volume meter and `? help` on the right. Volume and speed moved there from the now-playing line.

- `PLAYR_REQUIRE_FFMPEG=1` and `PLAYR_REQUIRE_DEVICE=1` make a test fail where it would skip for want of ffmpeg or an audio device. The engine and key-handling tests now play to a fake device, so they run without one.

### Changed

- Seeking and changing speed no longer close and reopen the audio device, which left a gap and could click. The output callback discards the buffered audio instead, and the device is reopened only if it has not done so within a second, as when it has stopped calling back.

- The queue exists once. The interface kept its own list of tracks beside the engine's and checked only that the lengths matched. `Status::queue` is now the queue: `Player::send` writes it before the engine acts, and the interface builds its rows from it. Library API: `Cmd::Jump` plays a queued track without replacing the queue, `Player::queue` returns it, and `App::with_queue` starts playback itself.

- Under varispeed, or on a device that refuses the source rate, consecutive tracks that share a source format now run through one resampler. Each track change used to start a new one. The join then differed from continuous resampling by up to -10 dB peak relative to the signal, over about 2 ms, and shifted the next track by up to a frame.

- A stopped player reports no source format.

- Library API: `audio::convert::Converter` is public, and `App::quitting` reports a quit key. The engine plays to an `output::Backend`, the trait a device implements, and `Player::with_backend` accepts one; `Player::new` uses `output::Cpal` on the default device. Device errors arrive as `output::DeviceEvent`, and `Output::open` takes the backend and a `Sender<DeviceEvent>`. `output::render` is the output callback's body. Tests use these to play to a fake device that fails on demand.

### Removed

- The `examples/` programs `devices`, `playtest` and `probe`. Nothing documented or used them, and the tests now cover what `playtest` and `probe` checked by hand.

### Fixed

- Seeking in an Opus file landed 312 frames (6.5 ms) early, and the audio after it was wrong for about 200 ms: -6 dB relative to the signal in the first 10 ms. OGG timestamps include the Opus pre-skip, which the decoder removes, and a decoder reset at the target needs time to settle. Seeks now correct for the pre-skip and decode 320 ms before the target, and match decoding from the start. The 80 ms that RFC 7845 gives as a minimum still left the start 26 dB off. Opus in WebM still lands 1.5 ms early, because Matroska timestamps are whole milliseconds.

- The first 10 ms after seeking in an AAC file were 20 dB off, because the first frame after a reset overlaps the one before it. Seeks now decode one frame early.

- A lost audio device left playback `Playing` with the position frozen, because device errors were only reported. It now stops with a message. A reroute to another device, which cpal reports as an error, is no longer shown. This follows cpal's error kinds; it was not checked by unplugging a device.

- A source whose channel count differed from the device's, such as a mono file on a stereo device, was resampled as if it had the device's channel count. Adjacent samples were treated as one frame, which distorted everything above a few kHz: at +1 semitone, an 8 kHz tone came out with an error of -14 dB. Playback at the file's own rate and normal speed was unaffected.

- A long run of unplayable files held every command until it ended, so quitting could wait seconds, or minutes on a slow disk. Stop, play, next, previous and quit now interrupt it.

- `p` could not step back past an unplayable track: it skipped forward onto the current track again. It now steps back to the nearest playable track, and restarts the current one if there is none.

- A failed format change between tracks left the previous track's state, so play restarted that track. The engine now stops cleanly at the track that failed.

- An unknown option such as `--verbose` opened the interface. It is now an error.

- A library from a newer playr opened as if it were current, and the older schema was applied to it. It is now refused.

- A directory the scan could not list was skipped without a count. It now counts as unreadable, and `playr <path>` warns about it.

- Saving a queue as a playlist dropped tracks that are not in the library, and reported only the smaller count. The message now says how many were left out.

- When several playback errors arrived between frames, only the last was shown. It now says how many more there were.

- Playing an empty queue left the engine `Playing`. Nothing in the interface sends one; the library API could.

- Ctrl-C in the search or save prompt typed `c` instead of quitting. Other Ctrl and Alt chords were typed too, and Ctrl-Y confirmed a deletion.

- A seek or speed change in the last second or two of a track acted on the next track. The decoder reads ahead and had already opened it, so the next track played from the current track's position under its title, then again from its start. In the last track of the queue the seek was ignored. A seek now reopens the track being heard.

- A resampled track, under varispeed or on a device that refused its rate, ended on the wrong frame. Depending on its length it lost up to 6 ms of its end, which was still inside the filter, or gained up to 40 ms of padding silence, which a gapless change played as a dropout. It now ends at exactly its length times the ratio.

- Pruning matched the scanned directory ignoring ASCII case, through SQL `LIKE`. On a case-sensitive file system, scanning `/mnt/music` could delete the rows and playlist entries of an unplugged drive mounted at `/mnt/Music`. The match is now exact.

## [0.2.0]

### Changed

- Only `playr scan` creates the library file. Every other command used to create an empty one, including playing a file. Without a library, `s` refuses to save a playlist rather than keep it in memory and lose it on exit.

- Modification times are stored in nanoseconds, and the library records its schema version in `PRAGMA user_version`. The first rescan after upgrading re-reads every file once.

- Library API: `Status::queue` is an `Arc<[PathBuf]>`, `Output::open` takes a `Sender<String>` for stream errors, and `output::choose` and `App::on_key` are public.

### Removed

- `AudioStream::time_base`, `Shared::starved` and `Resample::output_frames_max`, which nothing read.

### Fixed

- `playr scan` with a relative path stored relative rows. A later scan from another directory found none of those files and deleted the rows, with their playlist entries. Scan roots and paths passed to `playr` are now canonicalized. Relative rows written by 0.1.0 are not migrated.

- A scan pruned every row whose file was missing, not only rows under the scanned directory. Scanning one directory while a drive was unmounted deleted that drive's tracks and emptied its playlists. Pruning is now limited to each scanned root, and a root that does not resolve prunes nothing. Marking rows missing instead of deleting them would also survive moved files, but needs a schema change.

- A long run of undecodable files, such as an Opus library in a build without `opus`, aborted playr with a stack overflow. The engine skipped a bad file by recursing, and about 2,000 in a row overflowed a debug build. Skipping is now a loop.

- Varispeed reset to normal speed on every track change, while the status bar kept the shifted speed and the position ran fast by the same ratio. Three of the four places that build a resampler checked only the sample rate.

- The engine copied every queued path into the status up to 200 times a second, on the thread that refills the audio buffer. The status now shares the engine's queue.

- The library and queue views formatted every track on every frame. Only the rows on screen are drawn now.

- An interrupted scan kept nothing, because the whole scan was one transaction. Scans now commit every 500 files.

- `d` deleted a playlist and `s` overwrote one without asking. Both now wait for `y`, and any other key cancels.

- A path that is not valid UTF-8 was stored lossily, never played, and was re-read on every rescan. The scanner now counts it as unreadable, and `playr <path>` skips it with a warning. Storing paths as bytes would keep such files playable, but changes the schema.

- A seek resumed up to one packet before its target, 546 frames in a FLAC test, while the position showed the target. Symphonia's accurate seek lands on an earlier packet and leaves the caller to discard up to the target.

- A seek whose output failed to reopen left the engine playing with no output and no message. It now reports the error and stops.

- Moving to the next track skipped some failures silently: a first packet that would not decode, an unknown stream format, a refused output format, and a decode error mid-track. They are now reported as they are at playback start.

- Devices offering only 32-bit or 24-bit integer, or 64-bit float, output could not play. Negotiation accepted only f32, i16 and u16, and used the default config without checking its format.

- Enqueueing started playback only into an empty queue. Tracks added after the queue had played out, or while its last track played from the buffer, did not play.

- `playr playlist NAME` matched case-insensitively first, so with `Late` and `late` saved it always chose one. An exact name now wins, and a case-insensitive match is used only when it is unique.

- CAF and Matroska files were never indexed, because the tag reader cannot parse them. They are now indexed untagged, with stream properties from Symphonia. Matroska rows have no duration: Symphonia reports none, and the alternative was decoding each file in full at scan time.

- A same-size rewrite within one second was skipped by rescan, because modification times were whole seconds.

- Columns were sized by character count, so a row of CJK text overflowed and lost its duration. They are now sized in terminal cells.

- Audio device errors were printed to stderr, over the interface. They now appear in the status line.

- `make install` printed an empty path, and created a file named `bin` when `~/.local/bin` did not exist.

## [0.1.0]

First release.

### Added

- Playback of FLAC, ALAC, MP3, MP1, MP2, AAC-LC, Vorbis, PCM and ADPCM, in WAV, AIFF, CAF, MP4/M4A, MKV/WebM, OGG and raw FLAC containers.

- Opus playback in OGG and WebM, behind the `opus` cargo feature. Symphonia 0.6 demuxes Opus but registers no decoder, so `src/audio/opus.rs` supplies one over libopus and installs it in a custom codec registry. The feature is off by default because libopus is built with cmake, which nothing else here needs.

- SQLite library with recursive scanning. Tags and stream properties come from lofty. A rescan skips files whose size and modification time are unchanged, so re-indexing an unchanged library opens no files. Rows for deleted files are pruned.

- Full-text search over title, artist, album and album artist, using an FTS5 external-content index kept in sync by triggers. Results play directly as an ad-hoc queue, which is the whole of the search-then-play flow; no separate concept was needed.

- Playlists saved to and loaded from the database, order preserved.

- TUI with library, queue and playlist views, a search bar, and a now-playing bar showing source rate, channels, volume, speed and progress.

- Varispeed on `[` and `]`, in semitone steps from 0.5x to 2.0x, pitch moving with tempo. Steps are geometric, so each press is the same musical interval and twelve of them are exactly an octave. Powers of two were considered and rejected: 2x is a whole octave, which leaves three usable settings.

- Seeking on the arrow keys, five seconds a press.

- CLI: `scan`, `search`, `playlist`, `playlists`, `formats`, plus playing paths directly and `--db` to point at another library.

### Audio path

- The output stream opens at the file's own sample rate whenever the device accepts it, so the common case is not resampled at all. This is not bit-perfect: on PipeWire the ALSA `default` device accepts every rate and may convert internally.

- Sinc resampling when a rate is refused. Linear interpolation folds audible aliasing into the passband, which would defeat decoding losslessly.

- f32 from decoder to device, quantised once. Volume is a float gain applied before quantisation.

- Decoding runs on its own thread and feeds a lock-free ring; the realtime callback touches only atomics. An underrun emits silence rather than repeating stale samples, which would click.

- Pausing is done in the callback, not with `Stream::pause`. ALSA rejects `snd_pcm_pause` with errno 77 on a PipeWire setup, and stopping the device risks a click on resume.

### Known limits

- Gapless playback holds within a run of tracks at one sample rate. A rate change tears down and rebuilds the output stream, which leaves a gap.

- AAC decodes about 1900 frames long. Symphonia has no gapless support for it.

- Opus surround (multistream) is rejected rather than decoded.

- Opus in WebM keeps 648 frames of end padding, because Matroska does not report it and Symphonia does not expose the block duration that would.

