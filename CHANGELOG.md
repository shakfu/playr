# Changelog

Notable changes to playr. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

