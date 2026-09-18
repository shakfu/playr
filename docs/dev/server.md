# A server frontend

Design, written 2026-09-17 against playr 0.7.0, before any of it was built; the step sections record what changed. `docs/architecture.md` describes the crates it builds on. `docs/dev/gui.md` holds the parity rule.

## Goal

`playr-server` runs playr without a screen on a small Linux machine, such as a Raspberry Pi with a DAC. Audio plays on that machine's output device. A web page and OSC control it over the LAN.

Decisions taken:

- **Audio:** the machine's own device, through the existing `Player`. No streaming and no new audio code.

- **Users:** one user controlling one machine.

- **Output device:** the ALSA default, set in `~/.asoundrc`. Device selection stays in `TODO.md`.

- **Network code:** only in `playr-server`. `playr` and `playr-gui` keep the README's claim.

- **Parity:** every request goes through `dispatch` (parity rule 1). The web page has the window's features but the sampler, quitting, paths and `:map`; OSC is a remote control.

- **Media keys and the now-playing panel:** not attached. `main` never calls `Model::attach_media`, so the server registers no MPRIS name and no panel. A machine without a screen has no panel to show and no keyboard to press; the page and OSC are its controls. Wiring it is one line after `Model::new`, since `Model::refresh` already drains and publishes -- left out on purpose, not missed.

## What each client can do

| | web | OSC |
|-|-|-|
| pause, next, previous, stop, seek | yes | yes |
| volume, speed, mode | yes | yes |
| now playing, position, state, level meter | yes | sent back |
| play a playlist | yes | by index |
| library, selection and playlists views, with cursor, search and row menus | yes | no |
| edit the selection; save, rename and delete playlists | yes | no |
| marks: add, undo, clear, next, previous; ticks on the progress bar | yes | no |
| key bindings, `:` command line, key and command lists, theme | yes | no |
| re-scan the recorded roots | yes | no |
| messages | yes | no |

Neither client has the sampler, `open`, `scan`, `:map` or quitting.

The page may not name a path. `:rescan`, `:roots` and a bare `:prune` take none, so all three are allowed: they act on the directories `playr scan` recorded and nothing else. `:scan DIR`, `:prune DIR` and `:roots rm DIR` stay refused, so the page lists the library's directories but cannot change them.

## Process

```
playr-server
  main    flags, instance lock, library and player, as in playr-gui's main.rs
  owner   thread owning Model: runs requests from a channel, refreshes, publishes state
  http    the page, requests, state as server-sent events
  osc     addresses to requests; state to the reply address
```

- **`Model`, not `Session`.** `Model` already builds the playback snapshot, applies settings and drains events. Driving `Session` would repeat all three; see "Open issues" in `docs/architecture.md`.

- **One owner, no `Mutex`.** HTTP and OSC threads send requests over `mpsc`. A request of several steps runs as one message, so nothing lands between them: check a row, set the cursor, run a command.

- **The channel does not know who owns `Model`.** A later `--tui` can drain it from the terminal's loop.

## HTTP

| route | does |
|-|-|
| `GET /` | the page: one HTML file with inline script, embedded with `include_str!`; no build step |
| `GET /events` | server-sent events: the screen, as `web::screen` builds it, when it changes |
| `GET /rows?view=&start=&count=` | up to 500 rows of a view |
| `GET /keys`, `GET /help?list=`, `POST /complete` | key bindings, the key or command list, completions and history |
| `POST /key` | a key's name; the owner looks it up in the view shown |
| `POST /command` | a `:` command line, parsed in the view shown |
| `POST /row` | a view, row, the row's key and a command: set the cursor there, then run it |
| `POST /search` | the query as typed, and whether the search is done |
| `POST /answer`, `POST /name`, `POST /close` | answer a question, name a playlist, close a prompt or list |
| `GET /config`, `POST /rescan` | whether the library has a recorded root; re-scan them |

- **Server-sent events over WebSocket.** Plain HTTP in one direction, and `EventSource` reconnects on its own. WebSocket needs another crate, and axum brings tokio into a core built on threads.

- **A synchronous server.** std sockets, a thread per connection, with request heads parsed by `httparse`.

- **Refusals come back.** A key, command or row is checked by `web::allowed` on the owner thread, where the view is known, and a refusal is the response: 400, 403, or 409 for a row that has changed.

## OSC

`rosc` over `std::net::UdpSocket`. Addresses map to `Action` values, not command lines, so a fader's float needs no formatting: `:volume` takes percent.

Received:

| address | argument | action |
|-|-|-|
| `/playr/pause` | none, or non-zero | `TogglePause` |
| `/playr/next` | none, or non-zero | `Next` |
| `/playr/prev` | none, or non-zero | `Prev` |
| `/playr/stop` | none, or non-zero | `Stop` |
| `/playr/progress` | float 0 to 1 | `SeekTo`, as a fraction of the track |
| `/playr/volume` | float 0 to 1 | `SetVolume` |
| `/playr/speed` | int -12 to 12 | `SetSpeed` |
| `/playr/mode` | int 0 to 3, index into `Mode::NAMES` | `SetMode` |
| `/playr/playlist` | int index from 0, oldest playlist first | `PlayPlaylist` |

Sent to `--osc-reply HOST:PORT`:

| address | argument | when |
|-|-|-|
| `/playr/title`, `/playr/artist` | string | track change |
| `/playr/state` | int: 0 stopped, 1 playing, 2 paused | state change |
| `/playr/progress` | float 0 to 1 | each refresh while playing, about 30 times a second |
| `/playr/time` | string, `1:23 / 4:56` | with progress |
| `/playr/volume`, `/playr/speed`, `/playr/mode` | as received | on change |
| `/playr/level` | float 0 to 1, over -40 to 0 dB as the meter bar | with progress |

- **Triggers ignore a zero argument.** A TouchOSC button sends on press and on release; acting on both would pause and resume at once.

- **Seeks are coalesced.** A fader drag sends dozens of positions. The owner runs only the last seek received since its previous refresh.

- **`/playr/progress`, not `/playr/position`.** `docs/sampler.md` proposes `/playr/position` for marks, in frames.

- **`playr-server osc-schema`** prints both tables as JSON. A py2tosc script generates the TouchOSC layout from it. A test checks that each address maps to an action.

## Security

- **Listening.** HTTP binds to `127.0.0.1` unless `--listen` names an address. OSC is off unless `--osc` names one. A Pi's service file names both.

- **Token.** Generated on first start, kept in `server.token` beside the library with mode 0600, printed at startup. `GET /?token=` sets a cookie; every other route requires it.

- **`--open`.** No token: anyone who can reach the address controls playr. Chosen for a home network over a login page, since a phone cannot easily take a 64-character address, and a proxy in front can add authentication where it is needed. The token stays the default, so `--listen 0.0.0.0` alone does not expose playr.

- **`Host` and `Origin` checks.** Without them, a web page open in the user's browser can send requests, directly or through DNS rebinding. They apply with `--open` too. An `https` origin is accepted, for a proxy serving TLS; its host name needs `--host`.

- **Allow list.** `web::allowed` refuses quitting, the sampler, `:map`, and every command that takes a path.

- **Names.** `Host` must be an IP address, `localhost`, a `.local` name, or a name given with `--host`. A domain an attacker owns can resolve to the Pi, but its name is not on that list.

- **OSC has no authentication.** Anyone on the LAN can change playback, and nothing more.

- **Plain HTTP.** Anyone capturing LAN traffic can read the token. Acceptable on a home WPA2 network.

## Instance lock and scanning

- The server claims the lock in `playr_app::instance`. While it runs, `playr`, `playr scan` and `playr prune` refuse to start.

- To use the terminal over SSH, stop the service first.

- The page's rescan button appears when the library has a recorded root, and re-scans those roots. The directories come from `playr scan DIR`, never from a flag or a setting: a directory named in the server's configuration would be a second place roots live.

## Raspberry Pi

- **Binary:** the Linux arm64 archive needs glibc 2.35 or later. Raspberry Pi OS Bookworm has 2.36 (unchecked). No 32-bit build.

- **Audio:** Pi OS Lite plays through ALSA with no sound server (unchecked). The service user joins the `audio` group.

- **User:** the library and settings are under the service user's home. Run `playr` over SSH as that user, or it opens another library and another lock.

- **Name:** avahi on Pi OS resolves `raspberrypi.local`.

- **CPU:** sinc resampling runs only when the DAC refuses a rate. Its cost on a Pi 3 is unmeasured.

## Steps

| step | change | state |
|-|-|-|
| 1 | Crate, flags, lock, owner thread, tested with `fake_player` | done |
| 2 | HTTP: page, events, transport commands; token, `Host` and `Origin` checks, allow list | done |
| 3 | Search, play, playlists, marks, rescan | done |
| 4 | OSC: received and sent addresses, coalesced seeks, `osc-schema` | done |
| 5 | TouchOSC layout from `osc-schema` with py2tosc | done |
| 6 | Packaging: a systemd unit, `playr-server` in the release archives | done |
| 7 | The web page as a frontend: the window's features without the sampler | done |

### Where step 1 differs from the sketch

- **No wake on events.** `Model::waking` could wake the owner when an event arrives, but the waking sender lives inside the model, so the channel would never close. The owner refreshes every 250 ms when nothing plays instead, and every 33 ms while a track plays.

- **Only `--db` and `--settings`.** `--listen`, `--music` and `--osc-reply` arrive with the steps that use them.

### Where step 2 differs from the sketch

- **`httparse` over `tiny_http`.** `tiny_http`'s last release was 0.12.0, on 2022-10-06. `httparse` is hyper's request parser, maintained, with no dependencies. The server around it reads at most an 8 KiB head and a 4 KiB body, takes 10 s for either, and holds 32 connections.

- **Clients parse and check.** `/command` parses in the HTTP thread, so a parse error or a refusal is the response to the request that caused it: 400 or 403 with the reason. `Request` holds only an `Action`.

- **`--host NAME`.** A name on the LAN other than a `.local` one, such as `pi.lan`, is accepted only when named.

- **Unread requests are drained.** After refusing a request it has not read whole, the server shuts its side and reads what is left for up to 1 s. Closing with bytes unread resets the connection, and the client can lose the response.

### Where step 3 differs from the sketch

- **`POST /search`, not `GET /search?q=`.** The query is the body, as text, so the server decodes no URL.

- **`/play` carries the track's id.** The owner searches again before playing. A scan between the two can move rows, so a row whose track is not that id is not played, and the page is told to search again.

- **`/library` in place of `/playlists`.** It also says whether the library has a recorded root, so the page shows the Rescan button only then.

- **Playlists play by name.** The page sends `playlist NAME` to `/command`. OSC's index needs step 4.

- **Marks as a count.** The state has how many marks the track has, not their times.

### Where step 4 differs from the sketch

- **OSC is off by default.** `--osc ADDR:PORT` turns on receiving, and `--osc-reply ADDR:PORT` sending. Either works without the other.

- **Playlists by creation order.** The index counts playlists by id, oldest first. Ids follow creation, and replacing or renaming a playlist keeps its id, so only a delete moves a button to another playlist.

- **Seeks merge in the owner.** When the owner takes a seek, it also takes the seeks queued behind it and runs the last. A seek stops 100 ms short of the end: a seek to the end moves to the next track, so a fader at its top would skip tracks.

- **Values are sent when they change.** Every value is also sent again each 2 s, so a layout opened later shows the state. Progress, time and level change each refresh while a track plays, about 30 times a second.

- **Lenient arguments.** A number may be an int or a float, and speed is rounded, so a fader can send either. A trigger also takes a bool.

### Step 5

- **Where.** `packaging/touchosc/layout.py`, beside the addresses it follows. `make touchosc` writes `target/playr.tosc`; the layout is built, not kept in the repository.

- **Kept in step.** The script refuses to write a layout whose controls and the schema's addresses differ in either direction. `make touchosc-test` checks that and py2tosc's validation. It is not part of `make test`, which would then need uv and PyPI.

- **Bindings copy the editor's.** Each binding takes a shape py2tosc's corpus of editor-written layouts contains: buttons send `x` as a float, labels take `text`, radios send `x` as an integer. Playlist buttons send a float constant on press only.

- **Not yet tried in TouchOSC.** Three things rest on inference: that a radio's `x` is its segment's index, that a received speed is scaled back onto the fader, and that a fader being dragged is not moved by the progress sent back.


### Step 6: packaging

- **Every archive.** `playr-server` is in all five, not only Linux: it costs one more binary on runners that build the same crates, and a Mac or a Windows machine can serve too.

- **A user service, not a system one.** It runs as the user who owns the library, so `playr` over SSH as that user finds the same library and lock. A system service would need its own user and home, and SSH as that user.

- **Flags in the unit.** A `[server]` table in `settings.toml` would stop `playr` and `playr-gui` starting; see `TODO.md`. `systemctl --user edit --full` changes them.

- **The TouchOSC layout is a release asset.** One Linux build writes it from its own `osc-schema`, so a Pi without Rust or uv has it.

- **No `playr-server url`.** `--open` serves a trusted network without a token, and the guide builds the address over SSH from `server.token`.

- **The user's guide.** `docs/server-guide.md`, shipped in every archive.

### Step 7: the web page as a frontend

Steps 2 and 3 built a remote control. A frontend with the window's features replaced it, since a remote control fell short of what a Pi without a screen needs. Step 6, packaging, follows it.

- **The page draws the model.** The owner pushes the screen: view, counts, cursors, the prompt or question open, message, theme and playback. The page fetches rows a page of 200 at a time and draws only those in view, so a library of 100,000 tracks costs a screenful.

- **Keys go by name.** The page asks for the bindings once, takes the keys bound, and sends each key's name. The owner looks it up as `playr-gui` does, since some targets, such as `command`, are keys and not `:` lines.

- **Rows are named.** A tap sends the row's path, or a playlist's id, with its index. The owner refuses with 409 if the row now holds another, so a scan cannot make a tap act on the wrong track.

- **Dialogs follow the model.** The page opens the confirmation, name prompt, key list or command line when the model's input says so, and closes them the same way.

- **Three layouts.** Over 1000 px, the terminal's columns. From 640 to 1000 px, no album column. Under 640 px, two-line rows, 44 px targets, row menus, and volume, speed and mode behind More.

- **Style.** The terminal's colour roles in both themes, its monospace text, `+` for a selected track and `>` for the cursor.

- **Tests.** `tests/web.rs` and `tests/owner.rs` cover the JSON and requests on a model. `make page-test` drives the page in Chromium with Playwright: keys, rows, dialogs, search, the command line, marks, and the three widths. It is not part of `make test`, which would then need uv, a browser and an audio device.
## Open questions

- **Settings.** A `[server]` table stops `playr` and `playr-gui` from starting; see `TODO.md`. Flags only until that is fixed.
