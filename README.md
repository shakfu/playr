# playr

A local music player for active listeners, with a sampler, a tape looper and two DJ decks. It plays from a directory, your library, a saved playlist or search results, and by design contacts no service or cloud.

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

- Marks: `b` marks a moment in a track, `B` undoes the last mark, `{` and `}`, or `,` and `.`, seek between marks, and marks are kept in the library

- Samples: `:slice` writes regions between marks, equal parts or onset slices as lossless WAV files that rtrack loads as a sample bank; planned slices can be heard one by one before they are written, onset slices follow the sensitivity as it changes, and edges can be cut exact, at zero crossings, or faded

- Sampler view: zoom to single frames, nudge the playhead and snap it to zero crossings, and set a range to slice or loop, with its ends moved while it loops and the view held on the range or either end

- In the window, the sampler adds the whole track in a strip above the waveform with the stretch in view framed, a zoom slider, a time axis, and Scrub: a drag plays a moment wherever the pointer moves, then loops the range it set

- Each export also holds an `.sfz` kit and one WAV with a cue point at each slice; with the ConvertWithMoss extension, `:convert` turns an export, the last or any earlier one, into 16 sampler formats, among them MPC, SP-404MK2, OP-XY, Deluge, Logic's Sampler and Kontakt

- Tape looper: three voices read the sampler's range at their own rates and directions, and a write head records them back into it with feedback, so the loop changes each pass; the loop or the mix saves as a sample

- Spectrogram of the playing track in the sampler view, read with its waveform, in the terminal and the window

- The sampler's region shows its peak, loudness in LUFS and stereo correlation

- Varispeed in semitone steps, 0.5x to 2.0x, pitch moving with tempo

- A three-band EQ: bass, mid and treble

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

- An SQLite index, `library.db`, shared by all three programs and the `playr` subcommands: tags and stream properties, playlists, saved searches, marks, loops, hot cues, beat-grid edits and analysis. `:sql` queries it directly

- Recursive scan with tag and stream-property reading

- Rescan skips files whose size and modification time are unchanged

- `playr analyze`, or `:analyze` in the background, decodes each track once and records loudness, tempo and checks: damaged files, FLAC checksums, padded bit depth, likely transcodes and upsampling, and duplicates. It never writes to the files

- Tracks whose files are gone are kept until `playr prune` removes them, so an unplugged drive does not empty playlists

- Scans commit every 500 files, so an interrupted scan keeps its progress

- Full-text search over title, artist, album, album artist and file name, or within one of them with `artist:evans`; any other column by name, as in `year:1955..1965` or `tempo:120..130`; and findings of `playr analyze`, as in `is:damaged`

- Columns and sort order set per program in `settings.toml`, changed for the session with `:columns` and `:sort`; sorting by loudness or tempo needs `playr analyze`

- Search results play directly, in library order

**Interface**

- Five views: library, queue, selection, playlists, and a sampler showing the playing track's waveform or spectrogram; the web page has all but the sampler. The window adds three more: tape, DJ and mix

- Dark and light themes; the terminal takes its colours from its own theme, and honours `NO_COLOR`

- Search filters as you type

- A selection to collect tracks into, edit and save as a playlist; it does not change what plays, and its tracks are marked `+` in the library

- A queue of tracks you choose, played over the library, which resumes after it

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

Only one runs at a time: while one is running, the others, `playr scan` and `playr prune` refuse to start. `playr playlists`, `playr export`, `playr search --json`, `playr roots`, `playr formats` and `playr devices` only read, and run alongside any of them. `playr analyze` runs alongside them too: it writes only its own tables, which none of them caches.

| | `playr` | `playr-gui` | `playr-server` |
|-|-|-|-|
| library, selection and playlists views | yes | yes | yes |
| keys, `:` commands, `?` and `:help` | yes | yes | yes |
| search, marks, playlists, themes | yes | yes | yes |
| playback modes, varispeed, volume, level meter | yes | yes | yes |
| ReplayGain | yes | yes | yes |
| sampler view and `:slice` | yes | yes | no |
| sampler overview, zoom slider and Scrub | no | yes | no |
| `:convert`, an extension | yes | yes | no |
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

`playr-gui` and `playr-server` are not on crates.io yet. `cargo install` builds only the program, without the macOS bundle or the Linux desktop entry. Each builds with Opus, so it needs cmake; see "Opus".

### Building

Building needs Rust 1.89+ and a C compiler; the window, `playr-gui`, needs Rust 1.95+, as egui does. SQLite is vendored and compiled from source, which is what the C compiler is for; no SQLite package has to be installed. On Linux it also needs the ALSA headers, and the window needs the X11 and Wayland development headers. On Debian and Ubuntu:

```sh
sudo apt install libasound2-dev                       # both programs
sudo apt install libxkbcommon-dev libwayland-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev   # the window
```

### Opus

Opus is on by default. It needs libopus, which is vendored and built with **cmake** -- the only part of playr that needs it. To build without it, and without cmake:

```sh
cargo build --release --no-default-features                       # the terminal
cargo build --release -p playr-gui --no-default-features          # the window
```

Without it, Opus files are reported as undecodable and skipped, the same as WMA or DSD. `playr formats` says which build you have.

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
playr export late late.m3u8 # write a playlist as M3U8; no FILE prints it
playr import mix.m3u        # save an M3U file as a playlist
playr analyze               # decode new and changed tracks once; print findings
playr analyze --report      # print what is recorded, decoding nothing
playr formats               # show what this build can decode
playr devices               # list output devices; * marks the default
playr --device ID           # play to that device instead of the default
```

`playr <command> --help` describes each command. A search that starts with `-` goes after `--`, as in `playr search -- -ology`. `--json` prints an array with one object per track, holding every library column, `null` for a missing tag, and `duration_ms` in milliseconds; no match prints `[]` and exits with status 1. Bad arguments exit with status 2.

Without a command it opens the five views on the library. It draws in any terminal, needs no display server, and runs over SSH. [Keys](#keys) lists the bindings, and `?` lists them for the view you are in.

### The desktop window

```sh
playr-gui                        # open the window on the library
playr-gui ~/music/some/album     # play a directory, as playr does
playr-gui --db other.db          # use a different library file
```

It takes the terminal's options and reads the same settings file, so its keys and `:` commands are the terminal's. It has the library, selection, playlists and queue as tables with right-click menus, search, menus for every action, file dialogs, the transport, the level meter, and the sampler view, where a click on the waveform seeks, a shift-click marks, and the mouse wheel zooms. It is dark unless View, Theme or the `theme` setting chooses otherwise. The transport's buttons show media symbols; `transport_text_buttons = true` in the `[gui]` table shows words instead. [docs/dev/gui.md](docs/dev/gui.md) records its design and what is still open.

Run from a terminal, `playr-gui` holds it until the window closes. To start it detached:

```sh
open -a playr                          # macOS, with playr.app installed
open -a playr --args "$PWD/album"      # pass absolute paths; it does not start in $PWD
playr-gui &!                           # zsh; a plain & dies when the terminal closes
playr-gui & disown                     # bash
```

File, Add folder to library scans a directory, as `playr scan` does; File, Rescan library re-scans those folders; File, Library directories lists them, each with a Forget button; File, Remove missing files prunes every recorded folder; and File, Open plays files without adding them; files dropped on the window play too. When the window cannot start, for bad settings or no audio device, it opens a window that says why.

### The server

```sh
playr-server                         # serve on 127.0.0.1:8080, printing the address
playr-server --listen 0.0.0.0:8080   # reach it from other devices
playr-server --db other.db           # use a different library file
```

`playr-server` plays on the machine it runs on, such as a Raspberry Pi with a DAC, and serves a web page that controls it. It takes the terminal's options and reads the same settings file, so the page's keys and `:` commands are the terminal's. The page has the library, selection, playlists and queue views, search, row menus, dialogs, marks, themes and the level meter, without the sampler. It adapts to a phone, a tablet or a desktop browser: click or tap a row to move the cursor, again to play, right-click or `...` for its menu, and shift-click the progress bar to add a mark.

Every request needs a token, printed in the startup address on a terminal and kept in `server.token` beside the library. Opening that address sets a cookie for a year, so each browser needs it once. `--open` serves the page without a token, for a network where every device is trusted; the `Host` and `Origin` checks still refuse a website whose domain resolves to the machine.

The page cannot name a path, so it cannot open, scan or prune one, and it cannot quit the server. Its Rescan library re-scans the directories `playr scan` recorded, and nothing else.

`--osc ADDR:PORT` receives OSC for playback, volume, speed, mode and playlists by index, and `--osc-reply ADDR:PORT` sends the title, position and level back. `playr-server osc-schema` prints every address as JSON, and each release has a TouchOSC layout built from it.

[docs/guide-server.md](docs/guide-server.md) covers access from other devices, a QR code for a phone, running behind a proxy, and running it as a systemd service on a Raspberry Pi.

### The library

One library file serves all three interfaces and the `playr` subcommands. [`docs/guide-library.md`](docs/guide-library.md) explains how scanning, searching, the selection, the queue and playlists fit together, and how to lose nothing.

It lives at `$XDG_DATA_HOME/playr/library.db`, or `~/.local/share/playr/library.db`. Override it with `--db <path>`. Only `playr scan`, or `:scan` inside playr, creates it. Until then the other commands run without a library, and `s` cannot save a playlist. Paths are stored in full, so a scan run from any directory finds the same rows. A path that is not valid UTF-8 is skipped and counted as unreadable.

Rescanning only re-reads files whose size or modification time changed. Each scanned directory is remembered as a root, so `:rescan` (or `:sync`) inside playr, and a bare `playr scan`, cover them again without naming them. A directory holding no audio file does not become a root, and neither does one inside a root: the root above it already covers its files.

`:roots` lists the directories the library covers, and `playr roots` prints them. `:roots rm DIR`, or `playr roots rm DIR`, forgets one: the directory stops being part of the library, and the tracks under it go with it, along with their places in playlists and their marks. Unlike a prune this does not ask the filesystem anything, so it works on a directory that is already gone. `:roots add DIR` is another spelling of `:scan DIR`.

A scan never removes anything itself. It counts the tracks under the scanned directory whose files are gone; inside playr it then asks whether to prune them, and `playr scan` prints the count and the command. `playr prune`, or `:prune`, covers every root; `playr prune DIR` and `:prune DIR` cover one. They remove those tracks, and with them their places in playlists, and the marks of every file under the directory that is gone, whether it was in the library or not. Nothing outside the named directories is touched, so pruning `~/music` leaves an unplugged drive mounted elsewhere alone. Prune after a file is moved or deleted for good, not while a drive under that directory is unplugged.

Playlist entries and marks are the only things in the library that are not read back from the files, so pruning is the one operation that loses work.

`playr export NAME [FILE]` writes a playlist as extended M3U in UTF-8 with absolute paths, which other players read, and which keeps it if the library is lost. It refuses to replace a file unless given `--force`. `playr import FILE...` saves M3U or M3U8 files as playlists, named by `--name`, a `#PLAYLIST:` line or the file name. A relative path is read from the file's folder, and a `file://` URL is decoded. Tracks not in the library are left out and listed; scan their folder first to keep them. A name already taken is refused, so an import never replaces a playlist. Neither is a `:` command, and the web page cannot do either, since both name a file. With `auto_prune = true` a scan inside playr prunes without asking, except when a directory read as empty although the library holds tracks under it: that is what an unmounted drive looks like, so playr asks instead.

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

The tempo is one BPM for the whole track, from the autocorrelation of its onsets. A BPM tag, when present, is used instead, and the report compares the two where a track has both. Where the beat grid was found, its tempo is the one shown, sorted and searched: measured to 0.01 BPM, and at the faster level where the estimate read half, so a house track read at 62 shows 124. `bpm:` still finds a track by its estimate too. When the analysis reads a track an octave off, `:bpm x2` or `:bpm /2` corrects the playing track, up to two octaves either way, and `:bpm reset` puts it back; the window has them under Sampler, Tempo. The correction is kept in the library apart from the analysis, so analysing again keeps it. It also measures the tempo to 0.01 BPM and where the beats fall, for `:slice beats`. A pulse faster than about 170 BPM reads at half tempo, and music without a clear pulse gets none: on an ambient-leaning library expect a tempo for under half the tracks. Of those, about 60% match a second estimator exactly and another 28% at a simple ratio, such as half or three quarters; see `docs/dev/analyze.md`.

### Columns and order

`columns` in `settings.toml` says which columns a track list shows and in which order, and `sort` says what it is ordered by, most important key first: `sort = ["tempo desc", "title"]`. The names are `title`, `artist`, `album_artist`, `album`, `genre`, `disc`, `track`, `year`, `time`, `tempo`, `loudness`, `peak` and `path`; `tempo`, `loudness` and `peak` come from `playr analyze`, and a track it has not measured sorts last whichever way the column is sorted.

A `[terminal]`, `[gui]` or `[server]` table sets them for one program, over the shared keys, since a terminal row has less room than a window. As shipped, the window lists the title first and the other two take the shared order; nothing shows `tempo` or `loudness` until you ask for it. `:columns artist title tempo` and `:sort loudness desc` change them until playr exits. In the window, a click on a column heading sorts by it and a second click turns it around, and View, Columns ticks the columns to show. The page has a sort control; its columns are fixed for now.

One order serves the library, its search results and playback: the list you see is the list that plays, so sorting by tempo and pressing play walks the library in that order. Searching filters and sorting orders, so `bpm:170..180` sorted by loudness answers "everything near 174, loudest first".

`:info` shows what was measured about one track: its format, loudness and peak, the gain ReplayGain would apply, its tempo with how sure playr is of it, where its content stops, how many of its bits it uses, its FLAC checksum, and any findings. It describes the row under the cursor in the library and selection views, and the playing track elsewhere; the window has Track info in a row's menu and on the sampler's button bar, where it describes the playing track. A track that has not been analysed says so.

An analysed track shows its tempo beside the now-playing line, as it sounds: varispeed moves it, so a track at 120 BPM reads 143 BPM at +3 semitones.

### Search

`/` opens the search, and the library view filters as you type, across title, artist, album, album artist and the file name without its folder or extension. The file name is what finds an untagged file, which the list shows by that name. Pressing enter on the results plays them. `playr search` takes the same syntax from the shell.

Every word must match, as the start of a word. Prefix a word with a field to match it in that field alone: `title:`, `artist:`, `album:`, `albumartist:`, or `file:`. Quote words to match them together in order, as in `artist:"bill evans"`; unquoted, a field applies only to the word it is attached to. A prefix that is not one of these fields is searched as text, so `op:1` still finds a title with a colon in it.

Every other column is a field too, by the name `:columns` takes. A number column takes a value or a range: `year:1959`, `year:1955..1965`, `loudness:..-14`, `time:5:00..`. A bare value matches what the list shows, so `time:5:00` matches 5:00.9 but not 4:59.9, and `loudness:-14` matches -14.0. `genre:` and `path:` match anywhere in the text, ignoring case. `tempo:` is `bpm:`, below. `loudness` is integrated loudness in LUFS, and `peak` the sample peak as a linear value from 0 to 1, as the columns show them; both come from `playr analyze`. A value that names nothing, such as `year:soon`, matches nothing.

`bpm:` searches the recorded tempos: `bpm:128` matches within 1 BPM, `bpm:120..130` a range, and `bpm:140..` or `bpm:..90` one end of one. It combines with text, as in `evans bpm:120..130`, and matches nothing for a track playr has not analysed or is unsure of. A track whose tempo was halved, which happens above about 170 BPM, also matches at the tempo it is heard at: one recorded at 87 answers to `bpm:174`.

`is:` finds what `playr analyze` flagged: `is:damaged`, `is:unreadable`, `is:padded`, `is:no-checksum`, `is:wrong-length`, `is:lossy` for a possible lossy source, and `is:upsampled`. `is:duplicate` finds tracks that look like copies of another in the library: the same FLAC checksum, or the same title and artist within 2 seconds, which needs no analysis. `is:unanalysed` finds tracks with no current measurement. A field alone lists every track it admits, and terms combine, so `is:duplicate path:/backup/` lists the copies under one folder.

There is no OR, NOT or exclusion: every term narrows the results. `:sql` answers what the syntax cannot.

#### Saved searches and SQL

`:save-search NAME`, or `:save-search` alone, which asks for a name, keeps the search shown, with the order it is sorted in, and lists it after the playlists. It is run again each time it is opened, so it finds tracks added or analysed since: `:save-search 120-130` after searching `bpm:120..130`. On a saved search, enter shows what it finds in the library view, `a` selects all of it, `e` and `E` queue it, `o` opens it in the search box to change, and `d` deletes it. It shares names with the playlists; to rename one, save it again under the new name and delete the old.

`:sql` lists the tracks a `SELECT` names, as search results, for questions the search syntax cannot ask. The statement must return a column named `path`, and its `ORDER BY` is kept. It reads three views:

- `library`, one row per track: `path`, `title`, `artist`, `album_artist`, `album`, `genre`, `disc`, `track`, `year`, `time` (seconds), `rate`, `channels`, `bits`, `size`, `tempo`, `loudness` (LUFS), `peak` (linear, 0 to 1), and from `playr analyze`: `analysed`, `error`, `lossless`, `cutoff_hz`, `bits_used`, `md5`, `skipped`. The measured columns match the search fields of the same name.

- `playlists`: `playlist`, `position`, `path`, one row per entry.

- `marks`: `path`, `time` (seconds).

```
:sql SELECT path FROM library WHERE tempo BETWEEN 120 AND 130 AND NOT lossless ORDER BY tempo
:sql SELECT path FROM library GROUP BY album HAVING count(*) = 1
```

Only a `SELECT` runs, on its own read-only connection, reading only these views and common text, number, date, aggregate and window functions. It runs on its own thread, so playr keeps responding while it does; a new statement replaces one still running. A statement is stopped after 2 seconds, refused past 100,000 rows, and no value it builds may exceed 1 MB. The web page may send it too. `:save-search` keeps it like any search, and `o` on it opens it on the `:` line to change.

### Media keys and the now-playing panel

The keyboard's play, pause, next and previous keys work while playr runs, and the system's now-playing panel shows the track and takes its buttons: MPRIS on Linux, where playr appears as `org.mpris.MediaPlayer2.playr`, the macOS now-playing panel, and the Windows one. The terminal and the window have them alike. `playr-server` has neither. A machine without a screen has no panel to show and no keyboard to press; its controls are the web page and OSC.

None of it is required. A machine with no session bus or no panel runs playr as before. They belong to the machine playr runs on, so a `playr` reached over SSH has none: the media keys in front of you are routed by your own desktop, to its own players. On Windows the panel attaches to a window, which the terminal has not got, so there it works in `playr-gui` only. MPRIS is a local bus, not a network: playr still contacts nothing.

### Taking up again

playr remembers the track playing and how far into it when it closes, and offers it at the next start: "take up amen.flac again at 1:35?". Only `y` takes it up; anything else starts as playr always did. Nothing is offered when the command line named tracks to play, or when the file has gone. The position is stored in the library when playr closes and again at each track change, so a playr that is killed still leaves the track behind, if not the second. Unless `keep_queue = false`, the queue is stored beside it at each change and offered with it: "take up amen.flac again at 1:35, with its queue of 5 tracks?". The list that was playing is not stored, so playback stops once the queue has played.

## Keys

These keys are the same in the terminal, the window and the web page, and any of them can be rebound in [`settings.toml`](#configuration). The window and the page take the mouse as well, and the page takes touch.

| keys                     | action                                      |
|--------------------------|---------------------------------------------|
| `tab` shift-tab `1`-`8`  | next, previous view; `2` queue, `5` sampler; in the window, `6` tape, `7` DJ, `8` mix |
| `j` `k`, up/down         | move                                        |
| `g` `G`, home/end        | jump to first or last                       |
| page up/down             | move by ten                                 |
| `enter`                  | play from here; in playlists, play it       |
| `a`                      | select or unselect, then move down; in the queue, select |
| `e` `E` `A`              | queue the track or playlist; first; all listed |
| `/`                      | search; `esc` clears                        |
| `r`                      | rename the selected playlist                |
| `o`                      | edit the selected playlist in the selection |
| `s`                      | save the selection, or the queue in its view; asks before overwriting |
| `d` backspace delete     | remove from selection or queue; `d` deletes a playlist |
| `c`                      | clear the selection or the queue; asks y/n  |
| `J` `K`, shift up/down   | move a track within the selection or queue  |
| `space`                  | play or pause                               |
| `n` `p`                  | next or previous track                      |
| `x`                      | stop                                        |
| `R`                      | play from the range's start, or the track's |
| `m` `M`                  | next or previous playback mode              |
| left/right               | seek back or forward 5 seconds              |
| shift left/right         | seek back or forward 30 seconds             |
| `b`                      | mark the playing position                   |
| `{` `}`, `,` `.`         | seek to the previous or next mark           |
| `B`                      | undo the last mark                          |
| `C`                      | clear all marks in this track; asks y/n     |
| `(` `)`                  | varispeed down or up, one semitone a press  |
| `\`                      | back to normal speed                        |
| `+` `-`                  | volume                                      |
| `:`                      | type a command; see [Commands](#commands)   |
| `?`                      | list the keys for this view                 |
| `q`                      | quit                                        |

[Search](#search) describes what `/` accepts.

The window's Tape, DJ and Mix views have keys of their own, which win there over the ones above: see [Tape](#tape), [DJ](#dj) and [Mixer](#mixer). `?` lists a view's keys.

Enter plays the list you are looking at, from the selected track: the library, search results, the selection, or a playlist. The selection is separate from what plays. It starts empty. In the library, `a` selects the track under the cursor, or unselects it if it is marked `+`, without interrupting playback. On a playlist, `a` adds its tracks, skipping any already selected but keeping the playlist's own repeats. `s` saves the selection as a playlist.

The selection starts empty each run. As it changes, playr saves it to a playlist named `draft`, which the Playlists view lists; an empty selection leaves no draft. No other playlist may be named `draft`. The first time the selection changes in a run, if an earlier run left a draft, playr asks what to do with it: overwrite it with the selection, append it (put its tracks back in the selection, before the new ones), or save it under a name and start a new draft. Anything else leaves the old draft alone until the next change. `draft = "overwrite"` or `"append"` answers without asking, and `draft = "off"` keeps no draft. The draft holds only library tracks, as any playlist does.

To edit a playlist, press `o` on it, or run `:edit`: the selection is replaced with its tracks, after asking if it held any, and titled "Editing" with the playlist's name. `s` then offers that name, and saves over the playlist without asking. Saving, or clearing with `c`, ends the edit. An edit in progress is kept in the draft; taking up the draft, by appending it or by `o` on it, takes up the edit too.

The library plays by itself: enter in it plays on through it, and puts nothing in the queue. The queue holds the tracks you choose. `e` queues the track or playlist under the cursor; the first queued over the library plays at once, and the rest wait in the order queued. `E` puts a track first among those waiting. `A` queues every search result, or the whole selection. Enter on search results, a playlist or the selection replaces the queue with that list, from the track chosen, and says how many tracks were waiting. Once the queue has played, the library resumes at the track after the one the queue interrupted; `after_queue = "stop"` stops instead. Tracks waiting survive enter in the library, and play after the track chosen.

The queue view lists the queued tracks that have played, dimmed, then the track playing if it came from the queue, then the tracks waiting. Enter there jumps to a waiting track, and those skipped count as played; enter on a played track plays it again. `d`, backspace or delete takes a track out; taking out the track playing plays the next. `J` and `K` move a track among the played or the waiting ones. `a` adds a track to the selection. `s` saves every row as a playlist, so tracks can be queued, heard, pruned and kept. The queue and the selection share `:remove`, `:move` and `:clear`; the queue's old names `:dequeue`, `:reorder` and `:queue-clear` still work. `c` empties the queue, after asking; a queued track playing leaves it and plays on as part of the list, which then goes on as after the queue. The next start offers the queue again, with the track playing.

### Playback modes

`m` steps through four modes and `M` steps back. The bottom line names the mode unless it is normal.

| mode       | order                                              | after the last track |
|------------|----------------------------------------------------|----------------------|
| normal     | list order                                         | stop                 |
| shuffle    | every track once per pass, in random order         | reshuffle, go on     |
| repeat     | list order                                         | start again          |
| repeat one | the current track only                             | play it again        |

A mode applies to whatever list is playing: the library, search results, the selection, or a playlist. To shuffle across several playlists, add them to the selection with `a` and play the selection. Shuffle keeps the list on screen in its own order, and `p` goes back through the tracks it has played. Under repeat one, `n` moves on to the next track, which then repeats. Changing mode takes effect from the next track, even if it has already started loading.

`:stop after` stops once the track playing ends, in any mode; again, it plays on. `n` still moves to the next track, which then stops. `:stop in 30:00` stops playback 30 minutes from now, and `:stop in off` cancels it. The bottom line shows either while set. The window's Playback menu and the page's menu have both, with a 30-minute timer.

### Marks

`b` marks the playing position in the current track. Marks show as `^` under the progress bar. `}` seeks to the next mark and `{` to the previous one; within a second after a mark, `{` goes to the one before it, so pressing it twice steps back twice. A mark within half a second of an existing one is not added again, except in the sampler view, where marks may be a frame apart.

Marks form a chain: `B` removes the mark added most recently, then the one before, whatever their positions in the track. `C` clears all of the track's marks; it asks first, and only `y` confirms.

Marks are stored in the library by file path and source frame, so they survive a rescan and stay exact at any playback speed. Without a library file they last until playr exits. A mark lands slightly after the moment you meant, by your reaction time; in the [sampler view](#sampler-view) it can be selected and moved, dragged in the window, or snapped to the nearest rise in the sound.

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
| `:slice beats [N]`  | the region, every N beats at the analysed tempo, 0.125 to 64, so 0.5 cuts eighths; 4 without N |

Each export writes a new directory, named after the track, under `samples` in [`settings.toml`](#configuration), by default `~/Music/playr/samples`. A second export of `amen.flac` goes to `amen-2`. The directory holds:

- `000-amen_S00.wav`, `001-amen_S01.wav`, and so on, one file per slice, in the layout rtrack loads as a sample bank.

- `samples.json`, with the source file and each slice's start and end frame. A range cut whole while it loops (`l`) is marked to loop over the whole slice, so rtrack loads it looping. Its WAV files carry the loop too, in a `smpl` chunk, which other samplers read.

- `amen.sfz`, an [SFZ](https://sfzformat.com/) file that puts each slice on its own key, the first on C1 (36) and up to key 127, which is 92 slices. Samplers that read SFZ load the slices as a kit.

- `sliced/amen.wav`, the same audio as one file, from the first slice's start to the last one's end, with a cue point at each slice's start in a `cue ` chunk. Samplers that slice a file at its cue points are reported to include the Dirtywave M8 and the 1010music blackbox. It is cut exactly, whatever `slice_edges` did to the slice files.

- `sliced/amen.ot`, with `slice_ot_file = true` in `settings.toml`: the slices for an Elektron Octatrack, which reads it beside the WAV of the same name. It is written for up to 64 slices, the most the file holds.

The `sliced` files follow published layouts and match what other tools write; none has been loaded on a device. [docs/dev/hardware_samplers.md](docs/dev/hardware_samplers.md) has the layouts and what is unchecked.

`:convert FORMAT`, or Sampler, Slice, Convert to in the window, turns the slices last written into another sampler's format with [ConvertWithMoss](https://github.com/git-moss/ConvertWithMoss), which must be installed. It is an extension: it runs a program that is not part of playr, so it is off, and left out of `:help`, Tab completion and the window's menu, until `convert-with-moss.enable = true` is set under `[extensions]` in `settings.toml`. The result goes into a directory named after the format inside the export's: `amen/sf2`. `:convert sf2 amen` converts an earlier export instead, named by its directory under `samples`, which Tab completes, or by a path; Sampler, Slice, Convert an export to picks one in a dialog. Exports written before 0.15.0 have no `.sfz` file, so they are refused. Tab completes `1010music`, `ableton`, `bento`, `deluge`, `distingex`, `emulti` (Elektron Tonverk), `exs24` (Logic's Sampler), `mc707`, `mpc` (MPC keygroups), `nki` (Kontakt), `opxy`, `renoise`, `s2400`, `sf2`, `sp404mk2` and `sxt` (Reason's NN-XT); any other name ConvertWithMoss takes for `-d` works too. It resamples where a device needs it. A format already converted is refused, since the slices have not changed. playr runs ConvertWithMoss from where its installer puts it, or from `convert-with-moss.path`; it does not search `PATH`. Until it is there, `:convert` says where it looked and the window's Convert to is disabled.

Slices are read from the source file, so volume and speed do not apply. They are 24-bit WAV at the source's sample rate and channel count; 16- and 24-bit sources are copied bit for bit. Without `S`, `:slice onsets` uses `onset_sensitivity` from `settings.toml`, 0.5 by default. Onset detection is rtrack's: a hit within 50 ms of the region's start stays in the first slice, and each slice starts up to 10 ms before its hit. It reads the region into memory, up to about 23 minutes at 48 kHz, and keeps it: finding onsets again in the same region at another sensitivity reads nothing, which is what lets the window's Sensitivity slider replan the slices as it moves, a few milliseconds a step (60 s of audio, one machine). Export runs in the background, and the bottom line reports when it is done.

An edge that falls where the signal is far from zero clicks when the slice plays. `slice_edges` in `settings.toml`, `:slice-edges`, or the sampler window's Edges menu chooses what an export does about it:

| choice | edges | audio between them |
|-|-|-|
| `exact` (default) | where they fall | an exact copy |
| `zero` | moved to the nearest zero crossing within 10 ms, as `:snap` finds it; the track's own start and end, and an edge whose crossing another edge takes first, stay | an exact copy |
| `fade` | where they fall | faded in over `slice_fade_in` (1 ms) and out over `slice_fade_out` (5 ms), each at most half the slice |

`zero` keeps slices exact and still meeting end to end, but an edge moves up to 10 ms, which can clip the start of a hit, and finds nothing in silence or slow bass. `fade` always removes the click, and softens a hit's attack for the fade's length. A looped range cut whole keeps its edges exact under either, since you set them by ear.

For MP3 and AAC, frame positions follow playr's decoder. Another decoder can count the codec's encoder delay differently and place the same slice up to a few thousand frames away.

[docs/guide-sampler.md](docs/guide-sampler.md) shows how regions and cuts fit together, what `samples.json` holds, and how precise a mark is.

### Sampler view

`5` opens a view of the playing track's waveform, read from the file the first time the view opens for that track. Marks show as `|` under it, the selected mark reversed, the playhead as `^`, and the region between the marks either side of the playhead in the accent colour. The detail line gives the region's times to the millisecond.

| key     | command                 | does                                              |
|---------|-------------------------|---------------------------------------------------|
| `z` `Z` | `:zoom +`, `:zoom -`    | zoom in or out, centred on the playhead or range  |
| `0`     | `:zoom all`             | show the whole track                              |
| `w`     | `:display`              | switch display: Braille, envelope, dB, spectrogram |
| left, right | `:nudge -1`, `:nudge +1` | move the playhead a column                   |
| shift-left, shift-right | `:nudge -10%`, `:nudge +10%` | move it a tenth of the view      |
| `S`     | `:snap`                 | snap to zero crossings, on or off                 |
| `f`     | `:fit`                  | zoom to the range and centre on it, on or off     |
| `\|`    | `:fit on`               | back to the whole range after `[` or `]`          |
| `i` `o` | `:in`, `:out`           | start or end the range at the playhead, or at TIME |
| `l`     | `:loop`                 | play the range, or else the region, over and over, or stop |
| F1-F8   | `:loop 1` ... `:loop 8` | loop a saved loop, or save the range to an empty slot |
| shift-F1-F8 | `:loop N save`      | save the range as loop N, over what it holds      |
| `{` `}` | `:mark-prev`, `:mark-next` | select the previous or next mark, and play it to the next |
| `[` `]` | `:edge start`, `:edge end` | select the range's start or end, shown reversed |
| `<` `>` | `:move -1`, `:move +1`  | move the selected mark or range end a column      |
| `#`     | `:onset`                | move it to the nearest rise in the sound          |
| backspace | `:remove`             | remove the selected mark, join the selected slice to the one before; with an end or nothing selected, clear the range |
| `D`     | `:deselect`             | select nothing                                    |
| `u`     | `:undo`                 | undo the last change to the marks, range or plan  |
| `r`     | `:redo`                 | put back the last change undone                   |
| `a`     | `:audition`             | play the selection, slice, range or region once, then pause |
| `,` `.` | `:audition prev`, `:audition next` | select the previous or next planned slice, and play it once |
| `enter` | `:write`                | write the slices planned                          |
| `esc`   | `:discard`              | discard them                                      |
|         | `:mark-slices`          | mark each slice's start, to keep the cuts         |

- **Displays.** The envelope draws each column as two bars in eighth blocks: its RMS level in the bright colour, inside its peak level in a darker one. The waveform is folded, with negative samples counted by their size, so the bars use the full height. RMS shows loudness, such as a verse against a chorus, where a mastered track's peaks are near full scale everywhere; peak shows where each hit starts. The Braille display, which the view starts with, draws the waveform around a centre line, two dots across and four down a cell, which shows its shape. Both scale to the loudest sample in the track.

- **dB.** The dB display draws the same bars on a scale from -48 dBFS to full scale, not scaled to the track. A linear scale puts RMS 12 dB below full scale a quarter of the way up; this puts it three quarters of the way, which spreads out quiet passages and the level changes between sections. Levels below -48 dB draw nothing.

- **Spectrogram.** `:display spectrogram` draws level by frequency and time: 20 Hz at the bottom to half the sample rate at the top, on a log scale, brighter where louder, down to 90 dB below the loudest level in the track. It separates hits that the waveform merges, such as a kick under a hi-hat, and shows a lossy source's cutoff: an MP3 transcoded to FLAC stops somewhere from 16 to 20 kHz, by bitrate. On the log scale the top 16 to 22 kHz is about 3% of the height, so the cutoff shows in the window but seldom in the terminal. Both draw in magma, black through purple and orange to pale yellow, with the track outside the region dimmed: the terminal two rows a cell from the 256-colour table, the window as one image with 100 Hz, 1 kHz and 10 kHz marked. Each column is a 2048-point transform every 512 frames, 11.6 ms at 44.1 kHz, so at closer zoom neighbouring columns repeat. The transform resolves 21.5 Hz at 44.1 kHz, so below about 340 Hz a row is narrower than that; those rows blend between the neighbouring frequencies it does resolve, and bass shows as a smooth blur, not detail.

- **Measurements.** The region's line under the waveform gives the region's or range's peak in dBFS, its loudness in LUFS as `playr analyze` measures a track, and its channels' correlation: +1 for mono, 0 for one channel alone, and below 0 where they partly cancel when summed to mono. Under 400 ms is too short for LUFS, so the RMS level shows instead. All three are read from the file, before ReplayGain and the volume.

- **Zoom.** Each step halves the time a column shows, down to one frame a cell; the window goes on to 16 points a frame. Down to 64 frames, 1.5 ms at 44.1 kHz, columns start on the 32-frame buckets the peaks are kept in, so a column never shows a neighbour's hit. Closer than that, the view reads the frames it shows, and 2 s either side, in the background; until they arrive, each column shows its bucket's peaks. At a frame a column the window's line display draws each frame's channels' mean around a zero line, with a dot per frame once frames are 4 points apart, so a crossing can be picked out by eye.

- **The selection.** The edit keys act on one selected item, a mark, a range end or a planned slice, wherever the playhead has moved since. `{` and `}` select the mark before or after the selected one, or the playhead with none selected, and play from it to the next mark. `b` and a click on a mark in the window select it too, `[` and `]` select a range end, and `,` and `.` a planned slice, as does a click on its start. While a plan is shown, a click or drag on a slice start that sits on a mark takes the slice; discard the plan to drag the mark. Out of view, it shows as an arrow at that edge: reversed in the terminal, and above the playhead's in the window. The selection goes when its mark, end or slice is removed, on `D`, and on a change of track. With nothing selected the edit keys say so.

- **Editing.** `<` and `>` move the selected mark or range end a column at a time, and `:move-to TIME` puts it at a time; in the window, a mark or slice start is dragged. backspace removes the selected mark, wherever it sits in the chain `B` undoes; with a range end selected it clears the range. A selected slice is edited by its start: `<` and `>` move it, taking the end of the slice before with it, and backspace joins it to the slice before. Each start stays a frame inside its neighbours, and the first stays inside the range or region. The plan shows `edited`, and keeps the starts when `:slice-edges` plans it again; a new `:slice` replaces them. The marks do not move. `#` moves the selection to the nearest rise in the sound, looked for in the two seconds either side: a mark placed by reaction time lands late, and this puts it on the hit. The window around it is read in the background, so it costs the same on a long track as a short one. A move onto another mark is refused rather than merging the two.

- **Undo.** `u` undoes the last change to the playing track's marks, range or planned slices, by any key, command or drag, and again for the one before, and selects what was selected then. Making or replacing a plan is not a change it undoes, unless the plan replaced or discarded was edited. It keeps the last 100 changes, until the track changes. `r` puts back what `u` took, until a new change.

- **Audition.** `a` plays the selected mark up to the next, the selected slice, or the range when an end is selected; with nothing selected, the planned slice the playhead is in, or the range, or the region around it. It plays once, and pauses at the end rather than returning to the start as `l` does. Pressed again, during it or at its end, it plays the same span again from its start. With slices planned, `,` and `.`, or Previous slice and Next slice in the window, select and play the previous or next one, wrapping round at either end, so each can be checked before `enter` writes them. With `slice_edges = "fade"`, an audition fades as the written slice will. Playing on afterwards continues the track from there.

- **Planning.** In this view, `:slice` plans slices instead of writing them, and draws their edges as `+`. Enter writes exactly those slices; esc discards them, and so does a change of track. Outside the view, `:slice` writes at once.

- **Nudging.** Stopped, a seek, a click or a nudge opens the track paused at that point, so it can be placed and marked before playing. The arrows move the playhead a column, and with shift a tenth of the view, so zooming in makes each step finer, down to one frame. Outside this view they seek 5 and 30 s. Pause first to place a point without hearing each step.

- **Snap.** With `:snap on`, shown as `snap` in the title, nudges, marks, seeks and range ends made in this view move to the nearest zero crossing within 10 ms: a frame where the channels' mean changes sign. A nudge snaps only past where it started, so repeated nudges walk from crossing to crossing. Where no crossing is within reach, as in silence, the point stays. Turning snap on moves the ends of a range already set, so a loop drawn first can be snapped after.

- **Range.** `i` and `o` set a range's start and end at the playhead, drawn as `[` and `]`; the window sets one by dragging across the waveform. With both ends set, every cut uses the range in place of the region: `:slice region` cuts it whole, `:slice 8` in equal parts, `:slice onsets` at its onsets, `:slice beats 4` every 4 beats, and `:slice marks` at the marks inside it. The range lasts until cleared or the track changes, and is not saved. In the window, a drag that starts on a range's edge, within 8 points of it, moves that edge and selects it; the pointer turns to a left-right arrow over an edge, mark or slice start that can be dragged, and keeps it while dragging.

- **Fit.** `f`, or the window's Fit button, zooms to the deepest step that shows the range, then centres the view on the range rather than the playhead. Zooming then stays on the range, and the playhead may leave the view; then `<` or `>` at that side of the axis, or an arrow in the window, points to it. It applies once both ends are set, and shows as `fit` in the title. While it is on, `[` and `]` centre the view on that end, keeping the zoom, so `<` and `>` move the end while it stays still on screen; `z` then zooms in on it. `|` returns to the whole range, zoomed to fit and centred.

- **Loop.** `l` plays the range over and over, or with no range sets it to the region and loops that, starting a paused track, and returns from its end to its start without a gap. Moving either end, with `i` or `o`, with `<` or `>` after `[` or `]` selects it, with `:range` or a drag, moves the loop at once; clearing the range, a new track, `l` again or `:loop off` in any view ends it. When the decoder has already read past a new end, the change discards what it read, which can leave a short gap.

- **Saved loops.** Each track keeps up to 8 loops, in the library beside its marks. `:loop N`, on F1 to F8, saves the range to slot N when it is empty; when it holds a loop, it makes that the range and loops it, from a pause or a stop too, and moves a loop already playing at once. `:loop N save`, on shift-F1 to F8, saves over a slot, `:loop N clear` empties it, and `:loops clear` empties them all, after asking. The title lists the slots saved, with `*` on the one the range is. The window has a numbered button for each: a click does what F1 to F8 do, shift-click saves over, and its menu clears it; Clear loops beside them clears them all. Some terminals send shift-F1 as F13; `:map` binds another key if so.

The waveform glyphs are the view's only characters outside ASCII. Marks are placed at the playhead, or in the window at a shift-click.

### Tape

`:tape load` copies the sampler's range of the playing track into a loop, or `:tape load N` copies loop slot N. Three voices read it at once, each at its own rate and over its own window. A write head records what the voices send back into the loop, with feedback, so a reversed or half-speed voice is printed into it and every voice reads that on the next pass. The terminal has the commands; the window has a Tape tab with the same controls. [`docs/guide-tape.md`](docs/guide-tape.md) explains the tab and how the voices and the write head work together.

In the window, `6` opens the Tape view. Its keys are listed in [`docs/guide-tape.md`](docs/guide-tape.md#the-tab).

| command | does |
|-|-|
| `:tape play`, `:tape stop` | play the tape, pausing the player, or stop it |
| `:tape take` | load the range and take over from the player: from where it is inside the range, or at the range's start once it gets there; the player pauses |
| `:tape V on`, `:tape V off` | turn voice 1, 2 or 3 on or off; voice 1 starts on |
| `:tape V rate R` | frames a frame, -4 to 4; negative plays in reverse |
| `:tape V window A B` | the part of the loop the voice repeats |
| `:tape V level L`, `pan P`, `send S` | what is heard of it, where, and what is recorded of it |
| `:tape V wear W` | darken what the voice records, a little more each pass it records the loop again, 0 to 1 |
| `:tape V fade MS` | the crossfade at each wrap, 0 to 1000 ms |
| `:tape V ping on` | turn at the window's edges and play back, instead of wrapping |
| `:tape V slew MS` | how long a rate change takes, 0 to 10000 ms; 20 until set |
| `:tape V drive D` | saturate what the voice reads, heard and sent, 0 to 1 |
| `:tape V filter F`, `filter lp\|hp\|bp` | the voice's filter: its cutoff, 0 to 1 for 20 Hz to 20 kHz, or its type |
| `:tape V solo on` | hear only the soloed voices; what each sends is unchanged |
| `:tape write on` | record the sends into the loop |
| `:tape feedback F` | how much of the loop survives a pass, 0 to 1 |
| `:tape wear W` | darken everything the write head records, each pass, 0 to 1 |
| `:tape thin T` | thin everything the write head records with a high-pass, each pass, 0 to 1 for 20 Hz to 2 kHz |
| `:tape window A B` | the part of the loop that is rewritten |
| `:tape reset` | the loop as loaded |
| `:tape save`, `:tape rec` | save the loop, or start and stop recording the mix |

The load also reads up to a second of the track before the range, the pre-roll, and after it, the post-roll, which the window draws dimmed. Windows start as the range and take a time from its start, `1.5`, or a part of it, `25%`; below 0% or past 100% reaches into the pre-roll or post-roll. At each wrap a voice crossfades, equal power over the fade time: the leaving head fades out into the audio that follows its window, or, where the post-roll is too short, the new head fades in from the pre-roll. Either way the loop repeats every window's length exactly, and a fade with no room either side is cut short. The lanes draw both curves where they read. A voice under Ping turns at its window's edges instead, with no jump and so no crossfade. Drive feeds up to 24 dB into a soft clip and takes half of it back, so quiet material gains up to 12 dB and loud material is held under a quarter of full scale; the filter follows it, so a low-pass can take off the edge drive adds. Both shape what is heard and what is sent. A low-pass at 1 or a high-pass at 0 passes the voice unchanged. In the window, the waveform shows the selected voice's window, chosen by clicking its name or its lane: drag an edge to move it, or between the edges to move the window whole. A drag in a voice's lane edits that voice. The write window has its own strip above the waveform and drags the same way. What the write head cannot change, outside its window or all of the loop while writing is off, is tinted and hatched, and labelled frozen in the strip: it plays as loaded, untouched by feedback and wear. While the write window is the range, the write head also writes the pre-roll and post-roll as the loop's continuation, so a crossfade past the range's edges fades into the loop as it now is, not as loaded. A control that does nothing as the tape is set, such as a voice's Wear while its Send is 0, is dimmed, and its tooltip says why; it can still be set ahead. An edge dropped near the range's edge lands on it. On a stereo loop, pan keeps the near channel and folds the far one into it, so a hard pan keeps both; a mono loop pans with equal power. Writing is off until turned on, and with feedback at 1 and no sends it leaves the loop exactly as it was. A loop saves its range, without pre-roll or post-roll, as `samples/<track>-tape/<track>-tape-loop.wav` and a recording as `-mix.wav`, both 32-bit float, in a new directory each time, and both are added to the library. The tape plays on its own output stream on the player's device, which fails on a device held exclusively, such as an ALSA `hw:` device. Loading again starts from the default settings.

### DJ

Two decks play library tracks at once, each at its own rate. Each track gets a beat grid: its tempo and where a beat falls, from `playr analyze`. A deck loading a track never analysed analyses it first. With grids on both decks, sync matches one deck's tempo to the other's, at half, the same or double, and moves it into phase. A mixer blends the decks to one output. [`docs/guide-dj.md`](docs/guide-dj.md) explains the tab and how to mix with it. The terminal has the commands; the window has a DJ tab with a waveform per deck, centred on the playhead and marked with the grid, and the same controls. A click or drag on a deck's overview, the strip under its waveform, seeks there. The tab lists the library, following the search field, with A and B on each row to load it onto a deck; a library or selection row's menu does the same. A deck shows the track's marks on both its waveforms, and a click near one on the overview lands on it. The grid's edits are under each deck's Grid menu, and A, Centre and B glide the crossfader to that point over 400 ms, as a double click on it glides it to the middle.

In the window, `7` opens the DJ view. It lists the library, with search, and its keys are listed in [`docs/guide-dj.md`](docs/guide-dj.md#the-tab).

| command | does |
|-|-|
| `:dj a load` | load the track under the cursor onto deck A; `b` for deck B. A playing deck fades out, takes it and plays it; with strict on, it keeps it as its next track and loads it once it stops |
| `:dj a take` | load the track the player is playing onto deck A and play it from where the player is, at its speed and ReplayGain; the player pauses. See [the DJ guide](docs/guide-dj.md#taking-over-from-the-player) |
| `:dj strict on\|off` | strict: a track picked for a playing deck waits until the deck stops, so a mis-click cannot cut into a mix. Off until set |
| `:dj a mark next\|prev` | jump to the next or previous mark, as the sampler set them |
| `:dj a unqueue` | forget the next track |
| `:dj a seek TIME\|PCT%` | move the head to a time, `1:30`, or part of the track, `25%` |
| `:dj a mute on\|off` | silence the deck in the main mix; the cue still hears it |
| `:dj a play`, `:dj a pause` | play the deck, pausing the player, or pause it |
| `:dj a cue` | playing: back to the cue point, paused; paused: set the cue point here |
| `:dj a cue down`, `:dj a cue up` | hold CUE: paused, plays from the cue point until released |
| `:dj b sync`, `:dj b sync off` | deck B follows deck A's tempo, and moves into phase once |
| `:dj a rate PCT`, `:dj a range 8\|16\|50` | the rate fader, in percent, and its travel |
| `:dj a nudge +\|-\|off` | bend the rate 4% to move the deck's beats later or earlier by ear |
| `:dj a gain DB`, `:dj a level L`, `:dj xfade X` | trim, -12 to 12 dB; channel fader, 0 to 1; crossfader, 0 for A to 1 for B |
| `:dj xfade a\|b\|centre` | glide the crossfader to deck A's end, deck B's, or the middle, over 400 ms |
| `:dj quantize on` | play and cue start in phase with the other deck |
| `:dj cue a\|b\|off` | split cue: the main mix in the left ear, the deck in the right |
| `:dj a grid x2\|/2` | correct the grid's tempo an octave, as `:bpm` does |
| `:dj a grid <\|>`, `:dj a grid offset MS` | move the grid a beat, to set the downbeat, or by MS |
| `:dj a tap` | tap on the beat: two taps move the grid's beat, four set its tempo |
| `:dj a grid reset` | the analysed grid again |
| `:dj a eq low\|mid\|high DB`, `:dj a kill low\|mid\|high on` | the isolator EQ: a band's gain, -24 to 6 dB, or silence |
| `:dj a filter K` | one knob, -1 to 1: a low-pass left of centre, a high-pass right |
| `:dj a hot N`, `:dj a hot N clear` | hot cue 1 to 4: set it at the head, or jump there and play |
| `:dj a jump BEATS`, `:dj a loop BEATS`, `:dj a loop off` | jump, or loop 0.25 to 32 beats |
| `:dj cue-out split\|3-4` | the cue in the second ear, or in stereo on channels 3 and 4 |
| `:dj curve smooth\|sharp` | the crossfader: constant power, or full level until the last 5% |

Rate is varispeed: pitch moves with tempo, as on a turntable. A synced deck stays in phase: the other deck's rate changes follow, and a small rate trim pulls it back if it drifts. A nudge on it moves the phase it holds. Grid edits and hot cues are kept with the track, across analyses. With quantize on, hot cues and loops fall on beats. A playing deck refuses a new track. The decks play on their own output stream on the player's device, as the tape does.

### Mixer

The master volume, `:volume` or the Volume slider, scales everything playr plays: the player, the tape and the decks. Each of them also has a fader and a mute, and the DJ headphone cue has a fader the master leaves alone.

```
:mix                        every fader, in percent and dB
:mix tape 80                set one: master, player, tape, decks or headphones
:mix decks -10              move one
:mix player mute [on|off]   mute or unmute; alone, toggle
:mix law [db|cubic]         the fader law for this session; alone, toggle
:mix rec                    record the master, or stop recording it
```

- A fader's position maps to a level by its law, which `fader` in `settings.toml` sets. `db`, the default, is even in decibels: each 1% of travel is 0.6 dB, 50% is -30 dB, and 0 is silent. `cubic` is the position cubed, as PulseAudio's volume: 50% is -18 dB. `:mix law` switches it with the faders where they are, to compare the two.
- A fader at the top plays its source at full level. No fader boosts.
- A mute silences a source without stopping it: the decks stay in sync and the tape keeps writing. The tape's recording reads before its fader.
- `:mix rec` records the master, everything playr plays, to a 32-bit float stereo WAV under the samples directory, `master/master.wav` and then `master-2/master-2.wav`, and adds it to the library once stopped. The cue is never in it. Overs above full scale are kept in the file, though the device clips them. While it records, a track at another rate is resampled rather than changing the output's rate.
- `master` in `settings.toml` sets the master at start. `volume`, its name before the mixer, held a level, not a position. It is still read, and plays at the level it always did; a file setting both is an error.
- In the window, the **Mix** tab has a strip for each: a fader, its level in dB, a meter after the fader, and a mute; the master's has Record and the law. The master's meter measures what the device is sent. `8` opens it; `P`, `T`, `D` and `H` mute the player, tape, decks and headphones, `r` records the master and `l` switches the law. The tab is marked `Mix *` while a strip is muted, or once the master has gone over full scale while the tab was hidden. The server plays only the player, so its page and OSC have the master and a mute, `/playr/mute`; `:mix` works from the page's command line.

### Varispeed

`(` and `)` change playback speed in semitone steps, and pitch moves with it, as on a tape machine or a turntable. Twelve presses is exactly an octave, so the range is 0.5x to 2.0x. The speed shows in the status bar as `1.19x (+3 st)` and `\` returns to normal.

This is not the pitch-preserving speed change of a podcast app. That is time-stretching, which needs a phase vocoder; this is a change of resampling ratio, which is what varispeed means.

### EQ

`:eq` cuts or boosts three bands, each -12 to 12 dB: `bass`, a shelf below 100 Hz; `mid`, a wide peak at 1 kHz; and `treble`, a shelf above 10 kHz. `:eq bass 3` sets a band, `:eq bass =-3` sets it below 0, and `:eq treble -2` moves one; `:eq flat` returns all three to 0. In the window, the EQ button beside Mode opens a popup with a slider for each and Flat, which a click elsewhere closes; the web page takes the command. The status bar shows the bands away from 0, as `eq bass +3 treble -2`.

A boost raises its band and leaves the rest, so at full volume a large one can clip a loud track. The volume applies after the EQ, so turning it down makes room: at 50, a 6 dB boost cannot clip. The level meter reads after the EQ and before the volume. `:slice` and the sampler's measurements read the file, so the EQ does not reach them. The EQ starts flat each time playr does, and changes last until it exits.

### ReplayGain

ReplayGain plays each track at one fixed gain that brings it to -18 LUFS, as ReplayGain 2.0 does. It does not compress: the track's dynamics are unchanged. `:replaygain` or the `replaygain` setting chooses it:

- `off`, the default: nothing changes, bit for bit.

- `track`: each track at its own gain.

- `album`: each track at its album's gain, so a quiet track stays quiet beside the others.

- `auto`: album gain in normal and repeat modes, track gain in shuffle and repeat one.

Gains come from `playr analyze`, else from the file's `REPLAYGAIN_*` or `R128_*` tags. The analysis is preferred, so the whole library is measured one way. An album's gain needs every track of the album analysed; until then its tracks take their own. An album is its album tag and album artist, or its album tag and directory when there is no album artist. A track with neither analysis nor tags plays at 0 dB.

A gain never pushes the track's peak past full scale, and with no peak known it never boosts. The bottom line shows the gain applied, as `rg -6.2 dB`. The level meter reads after ReplayGain and before the volume.

### Commands

`:` opens a command line: `:seek 1:23`, `:volume 60`, `:playlist late night`. Every key's action has a command, and commands also take arguments no key can, such as a time or a name. Some commands work only in some views, as `:remove` in the selection and the queue. Tab completes, up recalls earlier lines, and `:help` lists every command. [docs/cheatsheet.md](docs/cheatsheet.md) has the full list.

`:scan ~/music` adds a directory to the library without leaving playr. It runs in the background and counts files on the bottom line; once it finishes, the library view shows the new tracks. `:rescan` or `:sync` re-scans every directory previously added that way, or by `playr scan`. If any tracks are missing, playr asks to prune them, unless `auto_prune` is set. `:prune` (or `:prune ~/music`) does what `playr prune` does, after asking. Saving a playlist or a mark while a scan runs waits for the scan to finish writing its current batch of 500 files, and fails with "database is locked" if that takes more than 5 seconds. `:open ~/music/some/album` plays a file or directory, as `playr <path>` does, and adds its tracks to the end of the selection.

## Configuration

playr reads `$XDG_CONFIG_HOME/playr/settings.toml`, or `~/.config/playr/settings.toml`, when it starts. `--settings <path>` reads another file instead. The file is optional, and it is read on top of the defaults in [`crates/playr-core/src/settings.toml`](crates/playr-core/src/settings.toml) and the default keys in [`crates/playr-app/src/keys.toml`](crates/playr-app/src/keys.toml), so it only needs what it changes. `playr --print-settings` prints both as one valid file, to copy from or save whole; a whole copy stops later default changes from reaching you.

An error in the file stops playr before it plays. Subcommands such as `scan` and `search --json` use no settings; they print the errors as warnings and run.

```toml
master = 60                        # the master fader, percent, 0 to 100; see Mixer
fader = "cubic"                    # how faders map to levels: db or cubic
mode = "shuffle"                   # normal, shuffle, repeat or repeat-one, in full
after_queue = "stop"               # after the queue: resume the library, or stop
speed = -3                         # semitones, -12 to 12
onset_sensitivity = 0.7            # for :slice onsets without a number, 0 to 1
dj_knee = -0.5                     # dBFS where the DJ master's soft clip starts, -24 to -0.1
tape_knee = -3                     # and the tape write head's
samples = "~/Music/playr/samples"  # where :slice writes; on Windows, 'C:\Music'
slice_edges = "zero"               # exact, zero or fade; see Samples
slice_fade_in = 1                  # ms, for slice_edges = "fade", 0 to 100
slice_fade_out = 5
slice_ot_file = true               # an Octatrack .ot file with each export; see Samples
auto_prune = true                  # after a scan, prune missing tracks without asking
analyze_on_scan = true             # after a scan, analyse what it added or changed
columns = ["artist", "album", "title", "time"]   # what a track list shows
sort = ["album_artist", "album", "disc", "track"] # and the order it comes in
device = "alsa:hw:CARD=DAC,DEV=0"  # output device from `playr devices`; "" is the default
replaygain = "auto"                # off, track, album or auto
theme = "light"                    # system, light or dark
keep_queue = false                 # forget the queue on exit
draft = "append"                   # continue the draft; or ask, overwrite, off
persist = ["eq", "volume"]         # session values to remember; see below

[extensions]                       # programs that are not playr's; each off until enabled
convert-with-moss.enable = true    # :convert; see Samples
convert-with-moss.path = "~/apps/ConvertWithMoss"  # if not where its installer puts it

[terminal]                         # or [gui] or [server]: that program only
columns = ["artist", "title", "tempo"]

[keys]                             # every view
right = "seek +10"
shift-right = "seek +60"
ctrl-s = "save"
q = "nop"
"?" = "help"

[keys.selection]                   # one view: library, queue, selection, playlists, sampler, tape, dj or mix
x = "remove"
```

- One file serves all three programs. A table another program owns, such as `[gui]` or `[server]`, is passed over by the ones that do not read it, so the terminal starts on a file written for the window. Only the program that owns a table checks what is inside it. A name no program owns, such as `[colours]`, is still an error.

- Top-level settings come before any table. TOML reads a bare key after a `[table]` header as belonging to that table, so `master = 60` below `[keys]` sets a key binding named `master`, not the master volume.

- Each key's value is a `:` command, as listed in [docs/cheatsheet.md](docs/cheatsheet.md). `"nop"` makes a key do nothing, and `"command"` opens the `:` prompt.

- A key under `[keys.VIEW]` wins in that view over the same key under `[keys]`.

- A key under `[keys]` needs a command that works in every view. `d = "remove"` there is refused, with the table to put it in.

- Keys are named by their character (`j`, `J`), or as `space`, `enter`, `esc`, `tab`, `backtab`, `backspace`, `delete`, `insert`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`, or `f1` to `f12`. Prefix `ctrl-`, `alt-` or `shift-` for a chord; a chord only matches a binding that names it. TOML needs quotes around a key that is not a letter, digit, `-` or `_`, such as `"?"`.

- `ctrl-c` always quits, and the keys inside prompts and help lists cannot be changed.

Any error stops playr before it starts, and every bad setting is listed with its line number. `?` lists the keys as bound in the view you are in. `:map` and `:unmap` change keys until playr exits.

`keep_queue`, on by default, keeps the queue between runs, and offers it with the track that was playing. Turning it off also forgets the queue stored. `draft` chooses what happens to the selection's draft playlist; see Keys, under the selection.

`persist` names session values playr remembers between runs: `eq`, `volume` (the master fader), `mode`, `replaygain`, `theme`, `columns`, `sort`, `history`, the last 100 `:` command lines, and `mix`, the other faders and the mutes. None are remembered unless named. playr stores them in the library, not in this file: `settings.toml` holds only what you write, and playr never changes it. While a name is listed, its remembered value wins over the setting of the same name, which then applies only until something is remembered. Remove the name to go back to the setting. `columns` and `sort` are remembered for each program, as a program's table sets them; the rest are shared. A change is stored once it has held still for half a second, so dragging a slider is one write. Without a library, nothing is remembered.

`theme` sets the colours, `dark` unless set, and `:theme` changes them until playr exits. In the window, `system` follows the system's light or dark appearance. A terminal cannot report its background reliably, so there `system` and `dark` use the terminal's own ANSI colours, which its theme shades, and `light` uses fixed colours for a light background. With `NO_COLOR` set to any value, the terminal draws without colour and reverses the cursor row.

## Formats

Decoded: FLAC, ALAC, MP3, MP1, MP2, AAC-LC, Vorbis, PCM and ADPCM, in WAV, AIFF, CAF, MP4/M4A, MKV/WebM, OGG and raw FLAC containers. Opus as well, unless built with `--no-default-features`.

Opus is decoded by playr itself, in both OGG and WebM. Symphonia 0.6 demuxes Opus but ships no decoder, so `crates/playr-core/src/audio/opus.rs` supplies one on top of libopus via the `opus` crate and registers it in a custom codec registry. Mono and stereo only; multistream surround is not handled.

Tags are not read from CAF, MKV or WebM files. Those are indexed under their file names, and MKV and WebM files also show no duration.

Not decoded: WavPack, WMA, Musepack, APE, DSD, TTA, TAK, and Opus in a build without it. playr reports such a file and moves to the next track rather than stopping. Run `playr formats` for the current list.

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

`make test` runs the suite for every crate in the workspace once, with Opus, the default build. The format and scanner tests generate real audio with `ffmpeg` when it is present and skip themselves when it is not. The rendering tests draw into a headless terminal, and the window's tests drive it headless with `egui_kittest`, so neither needs a display or an audio device. The engine, key-handling and device-failure tests play to a fake output device, so they need no audio device. The tests that ask the machine for its audio devices, and one smoke test that plays to the real default device at zero volume, run only with `PLAYR_DEVICE_TESTS` set, as `make test-devices` sets it: listing devices takes about 14 s a test on macOS. The smoke test skips without a device. Set `PLAYR_REQUIRE_FFMPEG=1` or `PLAYR_REQUIRE_DEVICE=1` to fail instead of skip, so a CI run cannot pass by testing nothing.

`.github/workflows/test.yml` runs the tests on Linux, macOS and Windows on every branch push and pull request, with `PLAYR_REQUIRE_FFMPEG=1` and ffmpeg 9.0 on every runner, and checks formatting and clippy on Linux. Runners have no audio device, so only the real-device smoke test skips there.

`.github/workflows/release.yml` builds and packages the three programs for every platform when a version tag is pushed, builds the TouchOSC layout, and publishes the release. Run by hand from the Actions tab with no tag, it builds and packages the chosen branch and keeps the archives as the run's artifacts without publishing, to try every platform's build before tagging.

`make page-test` drives `playr-server`'s page in Chromium with Playwright, and `make touchosc-test` checks the TouchOSC layout against the server's addresses. They need uv, and the page tests a browser and an audio device, so neither is part of `make test`.

Opus output was checked against `ffmpeg` by decoding the same file both ways: identical frame counts and 138.7 dB SNR, with no alignment offset.

## License

MIT. See `LICENSE`.
