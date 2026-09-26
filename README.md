# playr

A minimal music player which plays a directory, a saved playlist, or the results of a search. Keeps a SQLite index of your library.

When you download playr, you get three implementations: `playr`, a terminal app, `playr-gui`, a desktop gui app, and `playr-server`, a server for a machine without a screen, controlled from a web page or Open Sound Control (OSC).

None of the three contact external services or download any metadata and images. Indeed, `playr` and `playr-gui` have no network code at all. `playr-server` runs on your local network and communicates only with the browsers and OSC apps you connect to it.

![Varispeed on a Boards of Canada record is a good use of an afternoon.](https://raw.githubusercontent.com/shakfu/playr/main/docs/media/playr.png)


## Features

**Playback**

- Plays a file, a directory recursively, a saved playlist, or a search result

- Gapless within a run of tracks that share a sample rate

- Play, pause, next, previous, stop, and restart from the start of the track or the sampler's range

- Four playback modes on one key: normal, shuffle, repeat, repeat one

- Seek by 5 seconds in either direction, or 30 with shift, resuming on the exact sample

- Marks: `b` marks a moment in a track, `B` undoes the last mark, `,` and `.` seek between marks, and marks are kept in the library

- Samples: `:slice` writes regions between marks, equal parts or onset slices as lossless WAV files that rtrack loads as a sample bank; planned slices can be heard one by one before they are written, onset slices follow the sensitivity as it changes, and edges can be cut exact, at zero crossings, or faded

- Sampler view: zoom to single frames, nudge the playhead and snap it to zero crossings, and set a range to slice or loop, with its ends moved while it loops and the view held on the range or either end

- Spectrogram of the playing track in the sampler view, read with its waveform, in the terminal and the window

- Varispeed in semitone steps, 0.5x to 2.0x, pitch moving with tempo

- Volume as a float gain applied before quantisation

- ReplayGain by track, by album, or by album only while tracks play in order, from `playr analyze` or the files' tags; off by default

- A file it cannot decode is reported and skipped, never fatal

**Audio quality**

- Opens the output at the file's own sample rate whenever the device allows it, so nothing is resampled in the common case

- Sinc resampling when a rate is refused, not linear interpolation

- f32 throughout: decode, mix and gain, quantised once at the device

- Plays to float, 32-bit, 24-bit and 16-bit integer devices

- Plays to the default output device, or one chosen by ID, such as an ALSA `hw:` device

- Lock-free ring between the decoder and the realtime callback

- Underruns emit silence rather than repeating stale samples

**Library**

- Recursive scan with tag and stream-property reading

- Rescan skips files whose size and modification time are unchanged

- `playr analyze`, or `:analyze` in the background, decodes each track once and records loudness, tempo and checks: damaged files, FLAC checksums, padded bit depth, likely transcodes and upsampling, and duplicates. It never writes to the files

- Tracks whose files are gone are kept until `playr prune` removes them, so an unplugged drive does not empty playlists

- Scans commit every 500 files, so an interrupted scan keeps its progress

- Full-text search over title, artist, album, album artist and file name, or within one of them with `artist:evans`, and over analysed tempos with `bpm:120..130`

- Columns and sort order set per program in `settings.toml`, changed for the session with `:columns` and `:sort`; sorting by loudness or tempo needs `playr analyze`

- Search results play directly, in library order

- Playlists saved to and loaded from the database

**Interface**

- Four views: library, selection, playlists, and a sampler showing the playing track's waveform or spectrogram; the web page has the first three

- Dark and light themes; the terminal takes its colours from its own theme, and honours `NO_COLOR`

- Search filters as you type

- A selection to collect tracks into, edit and save as a playlist; it does not change what plays, and its tracks are marked `+` in the library

- Now playing shows title, artist, source rate and channels

- Progress bar with elapsed and total time

- Skipped files and audio device errors shown in the status line

- Vim and arrow key navigation; the window and the web page take the mouse as well, and the page takes touch

- Vim-style `:` commands for every action, with Tab completion and a history; they also take arguments such as `:seek 1:23` or `:playlist late night`

- `?` lists the keys for the view you are in; the bottom line shows messages, speed, volume, and a level meter

- The level meter reads momentary loudness in LUFS (ITU-R BS.1770, 400 ms) and holds the sample peak for 1.5 s. It measures the recording before the volume setting

- The meter bar runs from -40 dB to full scale and fills green below -18 dB, yellow to -6 dB, and red above; the peak marker `|` takes the colour of where it sits. Red on the bar means near the top, which is normal for loud masters. The peak number turns red only at full scale, where the recording clips

## Interfaces

playr is three programs over one core. All read the same library and the same `settings.toml`, and all take the same keys and `:` commands.

- **`playr`**, the terminal interface, drawn with ratatui. It needs no display server, so it runs over SSH.

- **`playr-gui`**, a desktop window, drawn with egui. Tables with right-click menus, menus for every action, file dialogs, and files dropped on the window.

- **`playr-server`**, for a machine without a screen, such as a Raspberry Pi with a DAC. It plays on that machine, and a web page or OSC controls it. The only program with network code.

Only one runs at a time: while one is running, the others, `playr scan` and `playr prune` refuse to start. `playr playlists`, `playr search --json`, `playr roots`, `playr formats` and `playr devices` only read, and run alongside any of them. `playr analyze` runs alongside them too: it writes only its own tables, which none of them caches.

| | `playr` | `playr-gui` | `playr-server` |
|-|-|-|-|
| library, selection and playlists views | yes | yes | yes |
| keys, `:` commands, `?` and `:help` | yes | yes | yes |
| search, marks, playlists, themes | yes | yes | yes |
| playback modes, varispeed, volume, level meter | yes | yes | yes |
| ReplayGain | yes | yes | yes |
| sampler view and `:slice` | yes | yes | no |
| mouse | no | yes | yes, and touch |
| media keys and the now-playing panel | yes* | yes | no |
| opens, scans or prunes a path it is given | yes | yes | no |
| re-scans the directories already recorded | yes | yes | yes |
| `:map` and `:unmap` | yes | yes | no |
| quits from the interface | yes | yes | no |
| network code | none | none | HTTP, and OSC with `--osc` |

\* Not on Windows, where the panel attaches to a window and the terminal has none.

The page reaches playr over a network, so it refuses the sampler, any command naming a path, `:map` and quitting. [docs/dev/server.md](docs/dev/server.md) holds the allow list.

## Install

### Release archives

Each [GitHub release](https://github.com/shakfu/playr/releases) has prebuilt archives, with Opus, for:

- Linux: x86_64 and arm64, glibc 2.35 or later

- macOS: arm64 and x86_64, 11.0 or later

- Windows: x86_64

Each archive holds the three programs, with `SHA256SUMS` in the release for checking them, and the release has a TouchOSC layout for `playr-server`. Put `playr` and `playr-server` on your `PATH`; the window needs one more step on macOS and Linux:

- **macOS:** the window is `playr.app`; move it to `Applications`. It is not signed or notarized, so macOS refuses to open it once downloaded with a browser; allow it under System Settings, Privacy & Security, or run `xattr -dr com.apple.quarantine playr.app`.

- **Linux:** copy `playr-gui` onto your `PATH`, `playr.desktop` to `~/.local/share/applications`, and `playr.png` to `~/.local/share/icons/hicolor/256x256/apps`, and playr is listed among your applications.

- **Windows:** run `playr-gui.exe`; it opens without a console window.

### From a clone

```sh
make install    # the three programs, and the window as an application
make install-dev  # the same, as a debug build
make gui        # run the window without installing
make app        # macOS: build target/dist/playr.app
```

`make install` builds `playr`, `playr-gui` and `playr-server` as they ship, with the `dist` profile, and copies them to `~/.local/bin`. On macOS it also puts `playr.app` in `~/Applications`; on Linux it adds `playr.desktop` and its icon under `~/.local/share`. `make install-dev` does the same with a debug build, stripped, quicker to build and slower to run.

### With cargo

```sh
cargo install playr                                             # the terminal, from crates.io
cargo install --git https://github.com/shakfu/playr playr-gui    # the window, from GitHub
cargo install --git https://github.com/shakfu/playr playr-server # the server, from GitHub
```

`playr-gui` and `playr-server` are not on crates.io yet. `cargo install` builds only the program, without the macOS bundle or the Linux desktop entry. Add `--features opus` to either for Opus.

### Building

Building needs Rust 1.89+ and a C compiler. SQLite is vendored and compiled from source, which is what the C compiler is for; no SQLite package has to be installed. On Linux it also needs the ALSA headers, and the window needs the X11 and Wayland development headers. On Debian and Ubuntu:

```sh
sudo apt install libasound2-dev                       # both programs
sudo apt install libxkbcommon-dev libwayland-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev   # the window
```

### Opus

Opus is off by default. It needs libopus, which is vendored and built with **cmake** -- the only part of playr that needs it. To include it:

```sh
cargo build --release --features opus                                        # the terminal
cargo build --release -p playr -p playr-gui --features playr/opus,playr-gui/opus   # both
```

Without it, Opus files are reported as undecodable and skipped, the same as WMA or DSD. `playr formats` says which build you have. `make install` and `make app` build with Opus, so they need cmake; `make install-dev` builds without it.

## Use

```sh
playr                                # the terminal
playr-gui                            # the desktop window
playr-server                         # the web page; prints the address to open
```

Each takes the same options, reads the same library and the same settings, and answers the same keys. [Interfaces](#interfaces) compares what each one has.

### The terminal

```sh
playr scan ~/music          # index a directory
playr prune ~/music         # remove tracks and marks of files gone from it
playr                       # browse the library
playr ~/music/some/album    # play a directory, recursively, without indexing
playr search bill evans     # play everything that matches
playr search album:blue     # match the album only
playr search --json evans   # print the matches as JSON instead of playing them
playr playlist "late night" # play a saved playlist
playr playlists             # list saved playlists
playr analyze               # decode new and changed tracks once; print findings
playr analyze --report      # print what is recorded, decoding nothing
playr formats               # show what this build can decode
playr devices               # list output devices; * marks the default
playr --device ID           # play to that device instead of the default
```

`playr <command> --help` describes each command. A search that starts with `-` goes after `--`, as in `playr search -- -ology`. `--json` prints an array with one object per track, holding every library column, `null` for a missing tag, and `duration_ms` in milliseconds; no match prints `[]` and exits with status 1. Bad arguments exit with status 2.

Without a command it opens the four views on the library. It draws in any terminal, needs no display server, and runs over SSH. [Keys](#keys) lists the bindings, and `?` lists them for the view you are in.

### The desktop window

```sh
playr-gui                        # open the window on the library
playr-gui ~/music/some/album     # play a directory, as playr does
playr-gui --db other.db          # use a different library file
```

It takes the terminal's options and reads the same settings file, so its keys and `:` commands are the terminal's. It has the library, selection and playlists as tables with right-click menus, search, menus for every action, file dialogs, the transport, the level meter, and the sampler view, where a click on the waveform seeks, a shift-click marks, and the mouse wheel zooms. It is dark unless View, Theme or the `theme` setting chooses otherwise. The transport's buttons show media symbols; `transport_text_buttons = true` in the `[gui]` table shows words instead. [docs/dev/gui.md](docs/dev/gui.md) records its design and what is still open.

File, Add folder to library scans a directory, as `playr scan` does; File, Rescan library re-scans those folders; File, Library directories lists them, each with a Forget button; File, Remove missing files prunes every recorded folder; and File, Open plays files without adding them; files dropped on the window play too. When the window cannot start, for bad settings or no audio device, it opens a window that says why.

### The server

```sh
playr-server                         # serve on 127.0.0.1:8080, printing the address
playr-server --listen 0.0.0.0:8080   # reach it from other devices
playr-server --db other.db           # use a different library file
```

`playr-server` plays on the machine it runs on, such as a Raspberry Pi with a DAC, and serves a web page that controls it. It takes the terminal's options and reads the same settings file, so the page's keys and `:` commands are the terminal's. The page has the library, selection and playlists views, search, row menus, dialogs, marks, themes and the level meter, without the sampler. It adapts to a phone, a tablet or a desktop browser: click or tap a row to move the cursor, again to play, right-click or `...` for its menu, and shift-click the progress bar to add a mark.

Every request needs a token, printed in the startup address and kept in `server.token` beside the library. Opening that address sets a cookie for a year, so each browser needs it once. `--open` serves the page without a token, for a network where every device is trusted; the `Host` and `Origin` checks still refuse a website whose domain resolves to the machine.

The page cannot name a path, so it cannot open, scan or prune one, and it cannot quit the server. Its Rescan library re-scans the directories `playr scan` recorded, and nothing else.

`--osc ADDR:PORT` receives OSC for playback, volume, speed, mode and playlists by index, and `--osc-reply ADDR:PORT` sends the title, position and level back. `playr-server osc-schema` prints every address as JSON, and each release has a TouchOSC layout built from it.

[docs/server-guide.md](docs/server-guide.md) covers access from other devices, a QR code for a phone, running behind a proxy, and running it as a systemd service on a Raspberry Pi.

### The library

One library file serves all three interfaces and the `playr` subcommands.

It lives at `$XDG_DATA_HOME/playr/library.db`, or `~/.local/share/playr/library.db`. Override it with `--db <path>`. Only `playr scan`, or `:scan` inside playr, creates it. Until then the other commands run without a library, and `s` cannot save a playlist. Paths are stored in full, so a scan run from any directory finds the same rows. A path that is not valid UTF-8 is skipped and counted as unreadable.

Rescanning only re-reads files whose size or modification time changed. Each scanned directory is remembered as a root, so `:rescan` (or `:sync`) inside playr, and a bare `playr scan`, cover them again without naming them. A directory holding no audio file does not become a root, and neither does one inside a root: the root above it already covers its files.

`:roots` lists the directories the library covers, and `playr roots` prints them. `:roots rm DIR`, or `playr roots rm DIR`, forgets one: the directory stops being part of the library, and the tracks under it go with it, along with their places in playlists and their marks. Unlike a prune this does not ask the filesystem anything, so it works on a directory that is already gone. `:roots add DIR` is another spelling of `:scan DIR`.

A scan never removes anything itself. It counts the tracks under the scanned directory whose files are gone; inside playr it then asks whether to prune them, and `playr scan` prints the count and the command. `playr prune`, or `:prune`, covers every root; `playr prune DIR` and `:prune DIR` cover one. They remove those tracks, and with them their places in playlists, and the marks of every file under the directory that is gone, whether it was in the library or not. Nothing outside the named directories is touched, so pruning `~/music` leaves an unplugged drive mounted elsewhere alone. Prune after a file is moved or deleted for good, not while a drive under that directory is unplugged.

Playlist entries and marks are the only things in the library that are not read back from the files, so pruning is the one operation that loses work. With `auto_prune = true` a scan inside playr prunes without asking, except when a directory read as empty although the library holds tracks under it: that is what an unmounted drive looks like, so playr asks instead.

### Analysis

`playr analyze` decodes each library track once and stores what it measured in the library: loudness and peak for ReplayGain, tempo, and the checks below. It then prints the findings. `:analyze` does the same from inside playr, in the background, over the whole library or one directory; the window has File, Analyze library. With `analyze_on_scan`, a scan inside playr analyses what it added or found changed, so gains and tempos are there without asking; `playr scan` says how many tracks are waiting instead, since it uses no settings. A later run decodes only tracks added or changed since; `--force` decodes them all again. Paths limit it to the tracks under them. `--report` prints what is stored without decoding, and `--json` prints it as JSON. `--jobs N` sets how many files decode at once; the default leaves one processor free.

It only reads the files. It runs beside a playing playr, which picks up the new gains at its next start or rescan. An interrupted run keeps every batch of 500 files it finished.

| Finding | Meaning |
|-|-|
| unreadable | the file failed to decode, or stopped partway |
| damaged | packets failed to decode, or a FLAC file's audio does not match its MD5 |
| no checksum | a FLAC file with no MD5 to check |
| wrong length | a lossless file that decodes to a length its header does not give |
| padded | samples wider than the bits they use, such as 16 bits in a 24-bit file |
| possible lossy source | a lossless file whose content stops in a cliff below 20 kHz, as an MP3 or AAC encoder leaves |
| possible upsampling | a file above 48 kHz whose content stops in a cliff below 26 kHz |
| duplicates | the same title and artist within 2 s of each other, or the same FLAC MD5 |

The two "possible" findings are heuristics, tested against ffmpeg's encoders and resampler rather than a library of known files. They catch LAME at 192 kbit/s and below, and not at 320, whose cliff sits at 20.3 kHz. A dark recording falls gradually and is not flagged.

The tempo is one BPM for the whole track, from the autocorrelation of its onsets. A BPM tag, when present, is used instead, and the report compares the two where a track has both. A pulse faster than about 170 BPM reads at half tempo, and music without a clear pulse gets none: on an ambient-leaning library expect a tempo for under half the tracks. Of those, about 60% match a second estimator exactly and another 28% at a simple ratio, such as half or three quarters; see `docs/dev/analyze.md`.

### Columns and order

`columns` in `settings.toml` says which columns a track list shows and in which order, and `sort` says what it is ordered by, most important key first: `sort = ["tempo desc", "title"]`. The names are `title`, `artist`, `album_artist`, `album`, `disc`, `track`, `year`, `time`, `tempo`, `loudness`, `peak` and `path`; the last three and `tempo` come from `playr analyze`, and a track it has not measured sorts last whichever way the column is sorted.

A `[terminal]`, `[gui]` or `[server]` table sets them for one program, over the shared keys, since a terminal row has less room than a window. As shipped, the window lists the title first and the other two take the shared order; nothing shows `tempo` or `loudness` until you ask for it. `:columns artist title tempo` and `:sort loudness desc` change them until playr exits. In the window, a click on a column heading sorts by it and a second click turns it around, and View, Columns ticks the columns to show. The page has a sort control; its columns are fixed for now.

One order serves the library, its search results and playback: the list you see is the list that plays, so sorting by tempo and pressing play walks the library in that order. Searching filters and sorting orders, so `bpm:170..180` sorted by loudness answers "everything near 174, loudest first".

`:info` shows what was measured about one track: its format, loudness and peak, the gain ReplayGain would apply, its tempo with how sure playr is of it, where its content stops, how many of its bits it uses, its FLAC checksum, and any findings. It describes the row under the cursor in the library and selection views, and the playing track elsewhere; the window has Track info in a row's menu and on the sampler's button bar, where it describes the playing track. A track that has not been analysed says so.

An analysed track shows its tempo beside the now-playing line, as it sounds: varispeed moves it, so a track at 120 BPM reads 143 BPM at +3 semitones. `bpm:` searches the recorded tempos: `bpm:128` matches within 1 BPM, `bpm:120..130` a range, and `bpm:140..` or `bpm:..90` one end of one. It combines with text, as in `evans bpm:120..130`, and matches nothing for a track playr has not analysed or is unsure of. A track whose tempo was halved, which happens above about 170 BPM, also matches at the tempo it is heard at: one recorded at 87 answers to `bpm:174`.

### Media keys and the now-playing panel

The keyboard's play, pause, next and previous keys work while playr runs, and the system's now-playing panel shows the track and takes its buttons: MPRIS on Linux, where playr appears as `org.mpris.MediaPlayer2.playr`, the macOS now-playing panel, and the Windows one. The terminal and the window have them alike. `playr-server` has neither. A machine without a screen has no panel to show and no keyboard to press; its controls are the web page and OSC.

None of it is required. A machine with no session bus or no panel runs playr as before. They belong to the machine playr runs on, so a `playr` reached over SSH has none: the media keys in front of you are routed by your own desktop, to its own players. On Windows the panel attaches to a window, which the terminal has not got, so there it works in `playr-gui` only. MPRIS is a local bus, not a network: playr still contacts nothing.

### Taking up again

playr remembers the track playing and how far into it when it closes, and offers it at the next start: "take up amen.flac again at 1:35?". Only `y` takes it up; anything else starts as playr always did. Nothing is offered when the command line named tracks to play, or when the file has gone. The position is stored in the library when playr closes and again at each track change, so a playr that is killed still leaves the track behind, if not the second.

## Keys

These keys are the same in the terminal, the window and the web page, and any of them can be rebound in [`settings.toml`](#configuration). The window and the page take the mouse as well, and the page takes touch.

| keys                     | action                                      |
|--------------------------|---------------------------------------------|
| `tab`, `1` `2` `3` `4`   | switch view; `4` is the sampler             |
| `j` `k`, up/down         | move                                        |
| `g` `G`, home/end        | jump to first or last                       |
| page up/down             | move by ten                                 |
| `enter`                  | play from here; in playlists, play it       |
| `a`                      | select or unselect, then move down          |
| `/`                      | search; `esc` clears                        |
| `r`                      | rename the selected playlist                |
| `s`                      | save the selection; asks before overwriting |
| `d`                      | remove from selection; delete a playlist    |
| `J` `K`, shift up/down   | move a track within the selection           |
| `c`                      | clear the selection; asks y/n               |
| `space`                  | play or pause                               |
| `n` `p`                  | next or previous track                      |
| `x`                      | stop                                        |
| `R`                      | play from the range's start, or the track's |
| `m` `M`                  | next or previous playback mode              |
| left/right               | seek back or forward 5 seconds              |
| shift left/right         | seek back or forward 30 seconds             |
| `b`                      | mark the playing position                   |
| `,` `.`                  | seek to the previous or next mark           |
| `B`                      | undo the last mark                          |
| `C`                      | clear all marks in this track; asks y/n     |
| `(` `)`                  | varispeed down or up, one semitone a press  |
| `\`                      | back to normal speed                        |
| `+` `-`                  | volume                                      |
| `:`                      | type a command; see [Commands](#commands)   |
| `?`                      | list the keys for this view                 |
| `q`                      | quit                                        |

Searching filters as you type, across title, artist, album, album artist and the file name without its folder or extension. The file name is what finds an untagged file, which the list shows by that name. Pressing enter on the results plays them.

Every word must match, as the start of a word. Prefix a word with a field to match it in that field alone: `title:`, `artist:`, `album:`, `albumartist:`, or `file:`. Quote words to match them together in order, as in `artist:"bill evans"`; unquoted, a field applies only to the word it is attached to. A prefix that is not one of these fields is searched as text, so `op:1` still finds a title with a colon in it.

Enter plays the list you are looking at, from the selected track: the library, search results, the selection, or a playlist. The selection is separate from what plays. It starts empty. In the library, `a` selects the track under the cursor, or unselects it if it is marked `+`, without interrupting playback. On a playlist, `a` adds its tracks, skipping any already selected but keeping the playlist's own repeats. `s` saves the selection as a playlist. To edit a playlist, add it to the selection with `a`, change it, and save it under the same name.

### Playback modes

`m` steps through four modes and `M` steps back. The bottom line names the mode unless it is normal.

| mode       | order                                              | after the last track |
|------------|----------------------------------------------------|----------------------|
| normal     | list order                                         | stop                 |
| shuffle    | every track once per pass, in random order         | reshuffle, go on     |
| repeat     | list order                                         | start again          |
| repeat one | the current track only                             | play it again        |

A mode applies to whatever list is playing: the library, search results, the selection, or a playlist. To shuffle across several playlists, add them to the selection with `a` and play the selection. Shuffle keeps the list on screen in its own order, and `p` goes back through the tracks it has played. Under repeat one, `n` moves on to the next track, which then repeats. Changing mode takes effect from the next track, even if it has already started loading.

### Marks

`b` marks the playing position in the current track. Marks show as `^` under the progress bar. `.` seeks to the next mark and `,` to the previous one; within a second after a mark, `,` goes to the one before it, so pressing it twice steps back twice. A mark within half a second of an existing one is not added again, except in the sampler view, where marks may be a frame apart.

Marks form a chain: `B` removes the mark added most recently, then the one before, whatever their positions in the track. `C` clears all of the track's marks; it asks first, and only `y` confirms.

Marks are stored in the library by file path and source frame, so they survive a rescan and stay exact at any playback speed. Without a library file they last until playr exits. A mark lands slightly after the moment you meant, by your reaction time; in the [sampler view](#sampler-view) it can be picked up with the cursor and moved, dragged in the window, or snapped to the nearest rise in the sound.

### Samples

![Spectrogram in sampler view of the gui.](https://raw.githubusercontent.com/shakfu/playr/main/docs/media/gui-spectrogram.png)

`:slice` writes parts of the playing track as WAV files, for rtrack or any sampler. Marks set the regions. The region is the span between the marks either side of the playhead, from the start of the track or to its end where there is no mark on that side. In the [sampler view](#sampler-view) a range can replace it.

![Slicing a Boards of Canada record is a good use of an afternoon.](https://raw.githubusercontent.com/shakfu/playr/main/docs/media/waveform.png)


| command             | writes                                                               |
|---------------------|----------------------------------------------------------------------|
| `:slice region`     | the region                                                           |
| `:slice marks`      | the whole track, cut at every mark                                   |
| `:slice N`          | the region in N equal parts, 2 to 256                                |
| `:slice onsets [S]` | the region, cut where hits start; `S` from 0 to 1, higher finds more |

Each export writes a new directory, named after the track, under `samples` in [`settings.toml`](#configuration), by default `~/Music/playr/samples`. A second export of `amen.flac` goes to `amen-2`. The directory holds:

- `000-amen_S00.wav`, `001-amen_S01.wav`, and so on, one file per slice, in the layout rtrack loads as a sample bank.

- `samples.json`, with the source file and each slice's start and end frame. A range cut whole while it loops (`l`) is marked to loop over the whole slice, so rtrack loads it looping.

Slices are read from the source file, so volume and speed do not apply. They are 24-bit WAV at the source's sample rate and channel count; 16- and 24-bit sources are copied bit for bit. Without `S`, `:slice onsets` uses `onset_sensitivity` from `settings.toml`, 0.5 by default. Onset detection is rtrack's: a hit within 50 ms of the region's start stays in the first slice, and each slice starts up to 10 ms before its hit. It reads the region into memory, up to about 23 minutes at 48 kHz, and keeps it: finding onsets again in the same region at another sensitivity reads nothing, which is what lets the window's Sensitivity slider replan the slices as it moves, a few milliseconds a step (60 s of audio, one machine). Export runs in the background, and the bottom line reports when it is done.

An edge that falls where the signal is far from zero clicks when the slice plays. `slice_edges` in `settings.toml`, `:slice-edges`, or the sampler window's Edges menu chooses what an export does about it:

| choice | edges | audio between them |
|-|-|-|
| `exact` (default) | where they fall | an exact copy |
| `zero` | moved to the nearest zero crossing within 10 ms, as `:snap` finds it; the track's own start and end, and an edge whose crossing another edge takes first, stay | an exact copy |
| `fade` | where they fall | faded in over `slice_fade_in` (1 ms) and out over `slice_fade_out` (5 ms), each at most half the slice |

`zero` keeps slices exact and still meeting end to end, but an edge moves up to 10 ms, which can clip the start of a hit, and finds nothing in silence or slow bass. `fade` always removes the click, and softens a hit's attack for the fade's length. A looped range cut whole keeps its edges exact under either, since you set them by ear.

For MP3 and AAC, frame positions follow playr's decoder. Another decoder can count the codec's encoder delay differently and place the same slice up to a few thousand frames away.

[docs/sampler.md](docs/sampler.md) shows how regions and cuts fit together, what `samples.json` holds, and how precise a mark is.

### Sampler view

`4` opens a view of the playing track's waveform, read from the file the first time the view opens for that track. Marks show as `|` under it, the playhead as `^`, the cursor as `#`, and the region between the marks either side of the playhead in the accent colour. The detail line gives the region's times to the millisecond.

| key     | command                 | does                                              |
|---------|-------------------------|---------------------------------------------------|
| `z` `Z` | `:zoom +`, `:zoom -`    | zoom in or out, centred on the playhead or range  |
| `0`     | `:zoom all`             | show the whole track                              |
| `w`     | `:display`              | switch display: Braille, envelope, dB, spectrogram |
| left, right | `:nudge -1`, `:nudge +1` | move the playhead a column                   |
| shift-left, shift-right | `:nudge -10%`, `:nudge +10%` | move it a tenth of the view      |
| `S`     | `:snap`                 | snap to zero crossings, on or off                 |
| `f`     | `:fit`                  | zoom to the range and centre on it, on or off     |
| `<` `>` | `:in`, `:out`           | start or end the range at the playhead            |
| backspace | `:range`              | clear the range; `:range 1:02 1:04.5` sets one    |
| `l`     | `:loop`                 | play the range over and over, or stop             |
| F1-F8   | `:loop 1` ... `:loop 8` | loop a saved loop, or save the range to an empty slot |
| shift-F1-F8 | `:loop N save`      | save the range as loop N, over what it holds      |
| `[` `]` | `:edge start`, `:edge end` | choose the range end to move, shown reversed   |
| `{` `}` | `:edge -1`, `:edge +1`  | move that end a column earlier or later           |
| `a`     | `:audition`             | play the slice, range or region once, then pause  |
| `n` `p` | `:audition next`, `:audition prev` | play the next or previous planned slice once |
| `;` `'` | `:cursor -1`, `:cursor +1` | move the cursor a column                       |
| `h`     | `:cursor off`           | return the cursor to the playhead                 |
| `u` `i` | `:pick prev`, `:pick next` | move the cursor to a mark                      |
| `y` `o` | `:nudge-mark -1`, `:nudge-mark +1` | move the mark under the cursor         |
| `#`     | `:snap-mark`            | move it to the nearest rise in the sound          |
| delete  | `:del-mark`             | remove it                                         |
| `enter` | `:write`                | write the slices planned                          |
| `esc`   | `:discard`              | discard them, or with none planned, clear the range |

- **Displays.** The envelope draws each column as two bars in eighth blocks: its RMS level in the bright colour, inside its peak level in a darker one. The waveform is folded, with negative samples counted by their size, so the bars use the full height. RMS shows loudness, such as a verse against a chorus, where a mastered track's peaks are near full scale everywhere; peak shows where each hit starts. The Braille display, which the view starts with, draws the waveform around a centre line, two dots across and four down a cell, which shows its shape. Both scale to the loudest sample in the track.

- **dB.** The dB display draws the same bars on a scale from -48 dBFS to full scale, not scaled to the track. A linear scale puts RMS 12 dB below full scale a quarter of the way up; this puts it three quarters of the way, which spreads out quiet passages and the level changes between sections. Levels below -48 dB draw nothing.

- **Spectrogram.** `:display spectrogram` draws level by frequency and time: 20 Hz at the bottom to half the sample rate at the top, on a log scale, brighter where louder, down to 90 dB below the loudest level in the track. It separates hits that the waveform merges, such as a kick under a hi-hat, and shows a lossy source's cutoff: an MP3 transcoded to FLAC stops somewhere from 16 to 20 kHz, by bitrate. On the log scale the top 16 to 22 kHz is about 3% of the height, so the cutoff shows in the window but seldom in the terminal. Both draw in magma, black through purple and orange to pale yellow, with the track outside the region dimmed: the terminal two rows a cell from the 256-colour table, the window as one image with 100 Hz, 1 kHz and 10 kHz marked. Each column is a 2048-point transform every 512 frames, 11.6 ms at 44.1 kHz, so at closer zoom neighbouring columns repeat. The transform resolves 21.5 Hz at 44.1 kHz, so below about 340 Hz a row is narrower than that; those rows blend between the neighbouring frequencies it does resolve, and bass shows as a smooth blur, not detail.

- **Zoom.** Each step halves the time a column shows, down to one frame a cell; the window goes on to 16 points a frame. Down to 64 frames, 1.5 ms at 44.1 kHz, columns start on the 32-frame buckets the peaks are kept in, so a column never shows a neighbour's hit. Closer than that, the view reads the frames it shows, and 2 s either side, in the background; until they arrive, each column shows its bucket's peaks. At a frame a column the window's line display draws each frame's channels' mean around a zero line, with a dot per frame once frames are 4 points apart, so a crossing can be picked out by eye.

- **The cursor.** The cursor is a second position, apart from the playhead, and it is what the mark keys act on. It starts on the playhead and follows it until moved; `h` returns it. `u` and `i` put it on the mark before or after it, which is how a mark is picked up: every mark key acts on the mark the cursor is on, within a column of the view, and says so when there is none. In the window, a mark is dragged along the waveform instead.

- **Editing a mark.** `y` and `o` move the picked mark a column at a time, `:move-mark TIME` puts it at a time, and delete removes it, wherever it sits in the chain `B` undoes. `#` moves it to the nearest rise in the sound, looked for in the two seconds either side: a mark placed by reaction time lands late, and this puts it on the hit. The window around it is read in the background, so it costs the same on a long track as a short one. A move onto another mark is refused rather than merging the two.

- **Audition.** `a` plays the planned slice the playhead is in, or the range, or the region around it, once, and pauses at its end rather than returning to its start as `l` does. Pressed again, during it or at its end, it plays the same span again from its start. With slices planned, `n` and `p`, or Previous slice and Next slice in the window, play the next or previous one, wrapping round at either end, so each can be checked before `enter` writes them; in this view they no longer skip tracks, which `:next` and `:prev` still do. With `slice_edges = "fade"`, an audition fades as the written slice will. Playing on afterwards continues the track from there.

- **Planning.** In this view, `:slice` plans slices instead of writing them, and draws their edges as `+`. Enter writes exactly those slices; esc discards them, and so does a change of track. Outside the view, `:slice` writes at once.

- **Nudging.** Stopped, a seek, a click or a nudge opens the track paused at that point, so it can be placed and marked before playing. The arrows move the playhead a column, and with shift a tenth of the view, so zooming in makes each step finer, down to one frame. Outside this view they seek 5 and 30 s. Pause first to place a point without hearing each step.

- **Snap.** With `:snap on`, shown as `snap` in the title, nudges, marks, seeks and range ends made in this view move to the nearest zero crossing within 10 ms: a frame where the channels' mean changes sign. A nudge snaps only past where it started, so repeated nudges walk from crossing to crossing. Where no crossing is within reach, as in silence, the point stays. Turning snap on moves the ends of a range already set, so a loop drawn first can be snapped after.

- **Range.** `<` and `>` set a range's start and end at the playhead, drawn as `[` and `]`; the window sets one by dragging across the waveform. With both ends set, every cut uses the range in place of the region: `:slice region` cuts it whole, `:slice 8` in equal parts, `:slice onsets` at its onsets, and `:slice marks` at the marks inside it. The range lasts until cleared or the track changes, and is not saved. In the window, a drag that starts on a range's edge, within 8 points of it, moves that edge and picks it for `{` and `}`; the pointer turns to a left-right arrow over an edge that can be dragged.

- **Fit.** `f`, or the window's Fit range tick box, zooms to the deepest step that shows the range, then centres the view on the range rather than the playhead. Zooming then stays on the range, and the playhead may leave the view; then `<` or `>` at that side of the axis, or an arrow in the window, points to it. It applies once both ends are set, and shows as `fit` in the title. While it is on, `[` and `]` centre the view on that end, keeping the zoom, so `{` and `}` move the end while it stays still on screen; `z` then zooms in on it. `f` off and on again returns to the whole range.

- **Loop.** `l` plays the range over and over, starting a paused track, and returns from its end to its start without a gap. Moving either end, with `<` or `>`, with `{` or `}` after `[` or `]` picks it, with `:range` or a drag, moves the loop at once; clearing the range, a new track or `l` again ends it. When the decoder has already read past a new end, the change discards what it read, which can leave a short gap.

- **Saved loops.** Each track keeps up to 8 loops, in the library beside its marks. `:loop N`, on F1 to F8, saves the range to slot N when it is empty; when it holds a loop, it makes that the range and loops it, from a pause or a stop too, and moves a loop already playing at once. `:loop N save`, on shift-F1 to F8, saves over a slot, `:loop N clear` empties it, and `:loops clear` empties them all, after asking. The title lists the slots saved, with `*` on the one the range is. The window has a numbered button for each: a click does what F1 to F8 do, shift-click saves over, and its menu clears it; Clear loops beside them clears them all. Some terminals send shift-F1 as F13; `:map` binds another key if so.

The waveform glyphs are the view's only characters outside ASCII. Marks are placed at the playhead, or in the window at a shift-click.

### Varispeed

`(` and `)` change playback speed in semitone steps, and pitch moves with it, as on a tape machine or a turntable. Twelve presses is exactly an octave, so the range is 0.5x to 2.0x. The speed shows in the status bar as `1.19x (+3 st)` and `\` returns to normal.

This is not the pitch-preserving speed change of a podcast app. That is time-stretching, which needs a phase vocoder; this is a change of resampling ratio, which is what varispeed means.

### ReplayGain

ReplayGain plays each track at one fixed gain that brings it to -18 LUFS, as ReplayGain 2.0 does. It does not compress: the track's dynamics are unchanged. `:replaygain` or the `replaygain` setting chooses it:

- `off`, the default: nothing changes, bit for bit.
- `track`: each track at its own gain.
- `album`: each track at its album's gain, so a quiet track stays quiet beside the others.
- `auto`: album gain in normal and repeat modes, track gain in shuffle and repeat one.

Gains come from `playr analyze`, else from the file's `REPLAYGAIN_*` or `R128_*` tags. The analysis is preferred, so the whole library is measured one way. An album's gain needs every track of the album analysed; until then its tracks take their own. An album is its album tag and album artist, or its album tag and directory when there is no album artist. A track with neither analysis nor tags plays at 0 dB.

A gain never pushes the track's peak past full scale, and with no peak known it never boosts. The bottom line shows the gain applied, as `rg -6.2 dB`. The level meter reads after ReplayGain and before the volume.

### Commands

`:` opens a command line: `:seek 1:23`, `:volume 60`, `:playlist late night`. Every key's action has a command, and commands also take arguments no key can, such as a time or a name. Some commands work only in one view, as `:remove` in the selection. Tab completes, up recalls earlier lines, and `:help` lists every command. [docs/cheatsheet.md](docs/cheatsheet.md) has the full list.

`:scan ~/music` adds a directory to the library without leaving playr. It runs in the background and counts files on the bottom line; once it finishes, the library view shows the new tracks. `:rescan` or `:sync` re-scans every directory previously added that way, or by `playr scan`. If any tracks are missing, playr asks to prune them, unless `auto_prune` is set. `:prune` (or `:prune ~/music`) does what `playr prune` does, after asking. Saving a playlist or a mark while a scan runs waits for the scan to finish writing its current batch of 500 files, and fails with "database is locked" if that takes more than 5 seconds. `:open ~/music/some/album` plays a file or directory, as `playr <path>` does, and adds its tracks to the end of the selection.

## Configuration

playr reads `$XDG_CONFIG_HOME/playr/settings.toml`, or `~/.config/playr/settings.toml`, when it starts. `--settings <path>` reads another file instead. The file is optional, and it is read on top of the defaults in [`crates/playr-core/src/settings.toml`](crates/playr-core/src/settings.toml) and the default keys in [`crates/playr-app/src/keys.toml`](crates/playr-app/src/keys.toml), so it only needs what it changes. Copying either defaults file whole, or both into one, is also valid.

An error in the file stops playr before it plays. Subcommands such as `scan` and `search --json` use no settings; they print the errors as warnings and run.

```toml
volume = 60                        # percent, 0 to 100
mode = "shuffle"                   # normal, shuffle, repeat or repeat-one, in full
speed = -3                         # semitones, -12 to 12
onset_sensitivity = 0.7            # for :slice onsets without a number, 0 to 1
samples = "~/Music/playr/samples"  # where :slice writes; on Windows, 'C:\Music'
slice_edges = "zero"               # exact, zero or fade; see Samples
slice_fade_in = 1                  # ms, for slice_edges = "fade", 0 to 100
slice_fade_out = 5
auto_prune = true                  # after a scan, prune missing tracks without asking
analyze_on_scan = true             # after a scan, analyse what it added or changed
columns = ["artist", "album", "title", "time"]   # what a track list shows
sort = ["album_artist", "album", "disc", "track"] # and the order it comes in
device = "alsa:hw:CARD=DAC,DEV=0"  # output device from `playr devices`; "" is the default
replaygain = "auto"                # off, track, album or auto
theme = "light"                    # system, light or dark

[terminal]                         # or [gui] or [server]: that program only
columns = ["artist", "title", "tempo"]

[keys]                             # every view
right = "seek +10"
shift-right = "seek +60"
ctrl-s = "save"
q = "nop"
"?" = "help"

[keys.selection]                   # one view: library, selection, playlists or sampler
x = "remove"
```

- One file serves all three programs. A table another program owns, such as `[gui]` or `[server]`, is passed over by the ones that do not read it, so the terminal starts on a file written for the window. Only the program that owns a table checks what is inside it. A name no program owns, such as `[colours]`, is still an error.

- Top-level settings come before any table. TOML reads a bare key after a `[table]` header as belonging to that table, so `volume = 60` below `[keys]` sets a key binding named `volume`, not the volume.

- Each key's value is a `:` command, as listed in [docs/cheatsheet.md](docs/cheatsheet.md). `"nop"` makes a key do nothing, and `"command"` opens the `:` prompt.

- A key under `[keys.VIEW]` wins in that view over the same key under `[keys]`.

- A key under `[keys]` needs a command that works in every view. `d = "remove"` there is refused, with the table to put it in.

- Keys are named by their character (`j`, `J`), or as `space`, `enter`, `esc`, `tab`, `backtab`, `backspace`, `delete`, `insert`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`, or `f1` to `f12`. Prefix `ctrl-`, `alt-` or `shift-` for a chord; a chord only matches a binding that names it. TOML needs quotes around a key that is not a letter, digit, `-` or `_`, such as `"?"`.

- `ctrl-c` always quits, and the keys inside prompts and help lists cannot be changed.

Any error stops playr before it starts, and every bad setting is listed with its line number. `?` lists the keys as bound in the view you are in. `:map` and `:unmap` change keys until playr exits.

`theme` sets the colours, `dark` unless set, and `:theme` changes them until playr exits. In the window, `system` follows the system's light or dark appearance. A terminal cannot report its background reliably, so there `system` and `dark` use the terminal's own ANSI colours, which its theme shades, and `light` uses fixed colours for a light background. With `NO_COLOR` set to any value, the terminal draws without colour and reverses the cursor row.

## Formats

Decoded: FLAC, ALAC, MP3, MP1, MP2, AAC-LC, Vorbis, PCM and ADPCM, in WAV, AIFF, CAF, MP4/M4A, MKV/WebM, OGG and raw FLAC containers. Opus as well, when built with `--features opus`.

Opus is decoded by playr itself, in both OGG and WebM. Symphonia 0.6 demuxes Opus but ships no decoder, so `crates/playr-core/src/audio/opus.rs` supplies one on top of libopus via the `opus` crate and registers it in a custom codec registry. Mono and stereo only; multistream surround is not handled.

Tags are not read from CAF, MKV or WebM files. Those are indexed under their file names, and MKV and WebM files also show no duration.

Not decoded: WavPack, WMA, Musepack, APE, DSD, TTA, TAK, and Opus unless the feature is enabled. playr reports such a file and moves to the next track rather than stopping. Run `playr formats` for the current list.

## Audio quality

The output stream is opened at the file's own sample rate whenever the device accepts it, so nothing is resampled in the common case. When a rate is refused, a sinc resampler converts it rather than linear interpolation. Decoding, mixing and volume are all f32, quantised once at the device. The device format is chosen in the order f32, f64, 32-bit, 24-bit, then 16-bit integer.

This is not bit-perfect output by default. On a PipeWire system the ALSA `default` device accepts every rate and may convert internally. A `hw:` device avoids that: `--device` or the `device` setting selects one by the ID `playr devices` prints. It offers only the card's own rates and formats, and cannot be opened while PipeWire or PulseAudio holds it. `--device` overrides the setting in all three programs. A device that does not exist stops playr at startup with the list, rather than playing somewhere else.

Volume is a float gain applied before quantisation. ReplayGain, when on, is a second gain beside it, and the only one that can exceed 1.

## Roadmap

`TODO.md` lists what is missing and what is blocked upstream.

## Design

[docs/architecture.md](docs/architecture.md) describes the split. `playr-core` holds audio, library, samples and session, with no presentation dependency; `playr-app` holds keys, commands and interface state. The terminal, the window and the server are three frontends over that pair, each supplying only its own presentation, and a fourth, such as a Tauri app, would too. [docs/dev/gui.md](docs/dev/gui.md) and [docs/dev/server.md](docs/dev/server.md) record the window's and the server's designs.

## Tests

```sh
make test
```

`make test` runs the suite for all five crates in the workspace, `playr-core`, `playr-app`, `playr`, `playr-gui` and `playr-server`, twice: with and without the `opus` feature, so neither build can rot unnoticed. The format and scanner tests generate real audio with `ffmpeg` when it is present and skip themselves when it is not. The rendering tests draw into a headless terminal, and the window's tests drive it headless with `egui_kittest`, so neither needs a display or an audio device. The engine, key-handling and device-failure tests play to a fake output device, so they need no audio device. One smoke test plays to the real default device at zero volume, and skips without one. Set `PLAYR_REQUIRE_FFMPEG=1` or `PLAYR_REQUIRE_DEVICE=1` to fail instead of skip, so a CI run cannot pass by testing nothing.

`.github/workflows/test.yml` runs both builds' tests on Linux, macOS and Windows on every branch push and pull request, with `PLAYR_REQUIRE_FFMPEG=1` and ffmpeg 9.0 on every runner, and checks formatting and clippy on Linux. Runners have no audio device, so only the real-device smoke test skips there.

`.github/workflows/release.yml` builds and packages the three programs for every platform when a version tag is pushed, builds the TouchOSC layout, and publishes the release. Run by hand from the Actions tab with no tag, it builds and packages the chosen branch and keeps the archives as the run's artifacts without publishing, to try every platform's build before tagging.

`make page-test` drives `playr-server`'s page in Chromium with Playwright, and `make touchosc-test` checks the TouchOSC layout against the server's addresses. They need uv, and the page tests a browser and an audio device, so neither is part of `make test`.

Opus output was checked against `ffmpeg` by decoding the same file both ways: identical frame counts and 138.7 dB SNR, with no alignment offset.

## License

MIT. See `LICENSE`.
