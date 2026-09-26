# A streaming server

Exploration, written 2026-09-27 against playr 0.12.0: what a multi-zone server would require. No decision to build it has been taken. `docs/dev/server.md` describes `playr-server`, which this extends. `docs/architecture.md` describes the crates.

## Goal

One machine holds the library. Several rooms each have a client: a small machine with speakers. Each client plays its own queue, and several people control them from phones at once.

`playr-server` today is one `Model`, one `Player` and one output device. Every page drives the same queue and cursor. See "One view and cursor per server" in `TODO.md`.

Terms used below:

- **Hub:** the process holding the library. It decodes and streams.

- **Client:** a process in a room. It receives audio over the LAN and plays it on its device.

- **Zone:** one client and the queue and playback state playing on it.

- **User:** a person, with their own playlists. A user controls any zone.

## Framing: zones, not users

The request says "different users play different tracks". The unit that holds playback is the speaker, not the person. Two people can share a kitchen. One person can move from the kitchen to the bedroom.

So playback state is per zone: queue, position, volume, mode, speed. Per-user state is what a person owns: playlists, and perhaps the selection. The page picks a zone first, then acts on it.

[Lyrion Music Server](https://lyrion.org), formerly Logitech Media Server, divides state the same way: one queue per player. Not checked against its code.

## Options

| | A: thin client | B: fat client | C: another system's protocol |
|-|-|-|-|
| decodes | hub | client | hub (C1, C2), or the other system (C0) |
| on the wire | PCM | the file's bytes | PCM over slimproto (C1); Snapcast's encoding (C2) |
| speed, loops, ReplayGain | in the hub's `Engine` | in each client's `Engine` | in the hub's `Engine` (C1, C2); lost (C0) |
| client | new: socket to cpal | new: `Engine` plus remote file reading | squeezelite (C1), snapclient (C2) |
| playback state | hub | client | hub |
| new code | one `Backend`, a protocol, a client | a remote `Player`, file serving | one `Backend`, a protocol adapter |

A is described first and in most depth: the later sections refer back to it. The hub's `Engine` already does decoding, gapless, speed, loops, ReplayGain and resampling, so A adds one output `Backend` and keeps all of that in one place. B puts an `Engine` in every client. C keeps A's hub and replaces A's protocol and client with existing ones.

"What any hub needs" covers zones, users and reconnection, which every option but B0 and C0 shares.

## What option A requires

### Where it plugs in

`playr_core::audio::output::Backend` is the seam. `Backend::start` receives the ring's consumer and `Shared`, and `Cpal` is the only real implementation. A `Remote` backend implements the same trait:

- `negotiate` answers from the formats the client reported when it connected.

- `start` spawns a sender thread. It drains the consumer as the client grants credit, and writes frames to the socket.

- The `Engine` and `Player` stay unchanged. Each zone is a `Player::with_backend(Remote)`.

The fields of `Shared` map to the protocol:

| `Shared` field | today | `Remote` |
|-|-|-|
| `frames_out` | frames the cpal callback handed to the device | frames the client reports played |
| `paused` | the callback emits silence | sent to the client; the sender stops sending |
| `flush_requested` / `flush_done` | the callback discards the ring | the sender discards, sends `Flush`; `flush_done` rises on the client's ack |
| volume | applied in `render` | applied in the hub, before sending |

The client reuses `Output::open` with `Cpal`. It writes received frames into the producer.

### Pacing

The client's DAC is the clock. The client grants credit in frames as its buffer empties, and the hub sends no more than granted. A hub that pushed at its own clock would drift against the DAC, so one side would underrun or overflow. The rates differ by crystal tolerance: 100 ppm is 0.36 s an hour (estimate from typical crystal specs, not measured).

Synchronising two zones playing the same track is out of scope. That needs a shared clock, as Snapcast has.

### Latency

- **Pause and seek:** one round trip. Pause stops the client draining and keeps its buffer. Seek flushes both buffers.

- **Starting a track:** the client's buffer fills first. 0.5 s is proposed, on top of the hub's 2 s ring (`BUFFER_SECONDS`).

- **Meters:** `render` measures loudness and peak in the hub, ahead of what plays by the client's buffer depth. Accepted: the meter is a guide. The position is taken from the client, since marks rely on it.

### The protocol

TCP, one connection per client, length-prefixed binary frames.

| from | message | carries |
|-|-|-|
| client | `Hello` | name, token, supported rates, channels and sample formats |
| hub | `Format` | the `Plan` chosen; sent again when a track needs a different one |
| hub | `Audio` | frames in the wire format |
| hub | `Pause`, `Play` | |
| hub | `Flush` | a sequence number |
| client | `Credit` | frames it has room for |
| client | `Played` | frames played, and the last flush acknowledged |
| client | `Lost` | the device failed, mapped to a `DeviceEvent` |

A hand-rolled protocol rather than HTTP chunked audio. HTTP streaming to a stock player such as mpv has no credit, flush or played count. The hub would not know the audible position, and a seek would wait for the player's buffer to play out.

### Bandwidth

| format | Mbit/s |
|-|-|
| 44.1 kHz, 16-bit, stereo | 1.41 |
| the ring's f32 at 44.1 kHz, stereo | 2.82 |
| 96 kHz, 24-bit, stereo | 4.61 |
| 192 kHz, 24-bit, stereo | 9.22 |

Send integers at the client's device depth, never the ring's f32. Four zones of CD audio come to 5.6 Mbit/s, within Wi-Fi's capacity (inference; a weak signal lowers it). FLAC encoding on the wire can come later, if a client on weak Wi-Fi underruns.

## What option B requires

Three variants, by where control lives.

| | B0: a server per room | B1: the engine in the client | B2: the session in the client |
|-|-|-|-|
| control | each room's own page | the hub's page | each room's page |
| library | each room scans a network mount | the hub | the hub, over a store protocol |
| shared playlists | no | yes | yes |
| new code | none | a remote `Player`; file serving | a remote store under `Session` |

### B0: `playr-server` in every room, today

- Each room runs `playr-server`, with the music on a read-only NFS or SMB mount. Each has its own library, lock, token and address.

- It needs no code. Each room scans and analyses the whole library over the network. Playlists, marks and loops differ per room.

- One library file shared over the mount is unsafe: SQLite's locking is unreliable on network filesystems ([SQLite: over a network](https://www.sqlite.org/useovernet.html)).

- It is the benchmark for the other options. What they add over B0 is one address and shared playlists.

### B1: the engine in the client

The hub keeps a `Model` and `Session` per zone, as in A. The client runs the `Engine`, and the hub's `Player` becomes a proxy for it.

- **`Session` uses a small part of `Player`:** `send`, `status`, `position`, `volume`, `loudness`, `take_peak`, `mode`, `queue` and `set_events`. A proxy sends each `Cmd` over the network. It mirrors `Status` and the atomics in `Shared` from the client's reports.

- **`Player` is a struct, not a trait.** `Session::new` takes a `Player`. A proxy needs a trait, or a `Player` whose `mpsc::Sender<Msg>` is replaced by a transport. A needs no core change.

- **`Cmd` must serialise.** Two variants are costly. `Play` carries the queue as paths. `SetGains` carries every library track's gains: 100,000 entries on a large library. Gains could go with each `Play`, for the tracks queued.

- **`Model::refresh` reads `Status` 30 times a second.** The mirror lags by the report interval plus one-way latency. `send` writes `Status::queue` before the engine acts, and the proxy can keep that locally.

- **Position needs no protocol.** The client's `Shared::frames_out` counts frames at its own DAC. There is no drift: the client's engine is paced by its own device.

- **Pause and seek take one round trip,** as in A. The client's engine flushes its own ring, so the protocol needs no flush.

The engine opens a track in two places, and both take a `Path`: `AudioStream::open` (`audio/decode.rs:142`) and, for tag gains, `Gains::from_tags` (`gain.rs:111`, through lofty). Two ways to reach the files:

- **A network mount.** Mount the music on each client at the path the hub scanned, and the queue's paths work unchanged. No code, but a mount per client and matching paths.

- **HTTP range requests.** The hub serves `GET /file/{id}` with `Range`. Symphonia reads any `MediaSource`: `Read + Seek + Send + Sync`, with `is_seekable` and `byte_len`. An HTTP source implements it with a read-ahead buffer. lofty's `Probe` also takes a reader. `Cmd::Play` holds `Vec<PathBuf>`, so either a URL travels as a path or the type changes.

Costs of reading over the network:

- **Reads block the engine thread.** A Wi-Fi stall stalls decoding, and the 2 s ring covers it. A longer stall underruns. In A the same stall hits the socket, and the client's buffer covers it.

- **Opening and seeking cost round trips.** Symphonia issues one or more range requests per seek. An MP4 with its index at the end needs a request at the end on open. A LAN round trip is a few ms (inference).

Bandwidth is the file's bitrate: at most 0.32 Mbit/s for MP3, and about half of PCM for FLAC (inference). It arrives in bursts as the ring refills, not steadily as in A.

CPU moves to the clients. The hub decodes nothing, so a Pi hub serves any number of zones. Each client decodes, and resamples when its device refuses a rate.

### B2: the session in the client

Each room runs a whole `playr-server`, as in B0, but the library lives on the hub. `Session` holds a `rusqlite::Connection` and calls `db::query` throughout: playlists, marks, loops, resume and scans. A remote store means a trait under every one of those calls. It is the largest change of any option.

MPD's satellite setup has B2's shape. A local MPD reads music over NFS or SMB, and passes every database query to the MPD on the file server through its `proxy` database plugin ([MPD user manual](https://mpd.readthedocs.io/en/latest/user.html)). The manual does not say that playlists or stickers are shared.

### B against A

B gives:

- a hub with little CPU;

- no clock drift to handle;

- less bandwidth.

B costs:

- playback state in two processes, and a protocol carrying `Cmd` and `Status` (B1);

- a core change to `Player` (B1) or `Session` (B2);

- a client that is a whole engine, with symphonia and every codec.

## What option C requires

C covers two approaches: replacing playr, or having playr speak another system's protocol.

### C0: replace playr

LMS with squeezelite players, or MPD in each room, does this job today. Nothing is built. playr's library page, marks, loops and speed are lost.

### C1: the hub speaks slimproto, and squeezelite is the client

Slimproto is LMS's player protocol ([Lyrion: SlimProto protocol](https://lyrion.org/reference/slimproto-protocol/); squeezelite's side is [`slimproto.c`](https://github.com/ralph-irving/squeezelite/blob/master/slimproto.c)). From the reference:

- The player connects to the server on TCP 3483. It sends `HELO` with its MAC address and a capabilities string, which lists codecs (`pcm`, `flc`, `mp3`, ...) and `MaxSampleRate`.

- The server sends `strm`. The command byte is `s` start, `p` pause, `u` unpause, `q` stop, `f` flush or `t` status. A start names a format and carries an HTTP request. Format `p` is PCM, with sample size, rate, channels and endianness. The player fetches the audio from the server with that request.

- The player sends `STAT` events: `STMs` track started, `STMt` heartbeat, `STMd` decoder ready, `STMu` underrun, `STMf` flushed, `STMp` paused, `STMr` resumed. Each carries the elapsed seconds and milliseconds, and how full its input and output buffers are.

- `audg` sets the volume.

A's protocol, mapped onto slimproto:

| A | slimproto |
|-|-|
| `Format` | `strm s`, format `p`, with the `Plan`'s rate, size and channels |
| `Audio` | the HTTP response, served from a `Remote` backend |
| `Credit` | TCP backpressure on that response: squeezelite stops reading when its buffer is full |
| `Pause`, `Play` | `strm p`, `strm u`; acknowledged by `STMp`, `STMr` |
| `Flush` | `strm f`, acknowledged by `STMf`; then a new `strm s` and a new HTTP request |
| `Played` | `STAT` elapsed ms, counted from the stream's start |
| `Lost` | `STMo`, `STMu`, or the connection closing |
| volume | applied in the hub, or `audg` |

So C1 is A with a published protocol and an existing client. The hub still decodes, so speed, loops and ReplayGain stay. squeezelite builds for Linux, macOS and Windows. Pi distributions such as piCorePlayer ship it (not checked).

What C1 raises:

- **One format per stream.** The PCM parameters are fixed at `strm s`. A track at another rate needs a new stream, and a gap. The hub can instead resample every track to one rate per zone with the existing resampler. That removes the gap and gives up bit-perfect output.

- **Position in milliseconds.** Marks are frames; a millisecond is 44 frames at 44.1 kHz. The hub interpolates between heartbeats. How often squeezelite sends `STMt` is unchecked.

- **Stream offsets.** Elapsed restarts at each `strm s`. The hub records the track frame at which each stream began.

- **squeezelite's expectations.** The reference describes what LMS sends. squeezelite's source defines what it accepts. Whether it works against a server that offers only port 3483 and the HTTP stream, with no discovery and no LMS CLI, is untested.

### C2: the hub feeds Snapcast

Snapcast ([README](https://github.com/snapcast/snapcast/blob/develop/README.md)):

- snapserver reads PCM from sources: a named pipe, a TCP socket, or a process's stdout.

- It encodes each stream (FLAC by default; PCM, Vorbis or Opus otherwise) and sends it to `snapclient`s over TCP.

- Clients synchronise to the server's clock and play in step. The README gives a typical deviation under 1 ms.

- Clients form groups, and each group plays one stream. A JSON-RPC API over TCP, HTTP or WebSocket sets volume and assigns clients to streams.

Zones map to one Snapcast stream each, fed by a `Remote` backend that writes PCM to a pipe or TCP source. The page assigns a room's client to a zone's stream through JSON-RPC.

C2 gives:

- **Synchronised rooms.** Several clients in one group play one zone in step. A and C1 cannot.

- **Existing, packaged clients.**

- **No drift handling in playr.** Snapcast corrects clients against its own clock.

C2 costs:

- **No flush.** The hub cannot discard what Snapcast has buffered, so pause and seek take effect after the buffer plays out. The server buffer's default is not in the README; 1000 ms is recalled, not checked. While paused, the hub stops writing and Snapcast plays silence.

- **No played position.** The hub knows what it wrote, not what played. It estimates the position as frames written minus the buffer. A mark set by ear lands late by the estimate's error.

- **The hub keeps time.** The source reads at the stream's rate, so a faster writer blocks. `Remote` is paced by the hub's clock, not by a device.

- **One format per stream,** set in snapserver's configuration. The hub resamples every track to it.

- **A second server.** snapserver, its configuration and one stream per zone run beside playr.

## Comparison

| | A | B1 | C1 | C2 |
|-|-|-|-|-|
| core change | none | `Player` behind a trait | none | none |
| client to build | yes | yes, with symphonia | no: squeezelite | no: snapclient |
| protocol | own | own, carrying `Cmd` and `Status` | slimproto | Snapcast source plus JSON-RPC |
| hub CPU per zone | decode, resample | none | decode, resample | decode, resample |
| wire per zone, CD audio | 1.41 Mbit/s | the file's bitrate | 1.41 Mbit/s | Snapcast's codec; FLAC by default |
| pause and seek | one round trip | one round trip | one round trip | after the Snapcast buffer |
| position | counted at the client's DAC | counted at the client's DAC | ms from heartbeats | estimated |
| rate change between tracks | a new `Format` | the client's own | a new stream, or resample | resample |
| rooms in sync | no | no | not explored; LMS has sync groups | yes |

## What any hub needs

These apply to A, B1, C1 and C2: one hub process holding several zones.

### Several zones in one process

The one-process rule in `docs/architecture.md` still holds. What breaks:

- **One `Model` per zone.** `Model` holds a `Session`, and a `Session` holds its own `Vec<Track>` of the whole library. N zones hold N copies. Unmeasured: estimate 50 MB each for 100,000 tracks. The library should move behind an `Arc`, shared by every session.

- **Stale checks.** Each `Session` checks playlist names and marks against its own copy. A playlist saved from one zone is missing in another until reload. Per-user playlists remove most of the conflict. Marks and loops describe a track, not a person, so they stay shared and need one owner.

- **Scans.** One scan at a time, as now. `Event::Scanned` must reach every session, not only the one that started it.

- **SQLite.** Already WAL (`db/mod.rs:110`), with rusqlite's default 5 s busy timeout. N sessions in one process are N connections to one file. That is supported, since each one writes little.

- **The page.** It opens on a zone list. `/events` and every request carry a zone id. The owner thread holds a map of zone ids to `Model`s, or there is one owner thread per zone.

### Users

- **Schema:** a `users` table and a `user` column on `playlists`, plus a migration. Marks, loops, analysis and roots stay global. Resume becomes per zone.

- **Tokens:** one token per user, not one per server. The threat model is unchanged: a household on a trusted LAN. The token identifies a user; it does not isolate one user from another.

- **Clients:** a client token, separate from user tokens. Anyone on the LAN can read the audio stream. Accepted, as plain HTTP already is.

### Dropped connections

A client lost on Wi-Fi appears as a `DeviceEvent`, and today that stops playback. A zone should wait for its client to reconnect and resume at the position the client last played. This is the "Reopening a device that returns" item in `TODO.md`, and it becomes necessary here.

## Order of work, if built

| step | change | proves |
|-|-|-|
| 1 | `Remote` backend and `playr-client`, one zone, `playr-server --output tcp` | the data plane; a hub on a NAS with one speaker in another room |
| 2 | Library behind an `Arc`; events to every session | memory and staleness |
| 3 | Zones: a `Model` per client, zone id on every route, a zone list on the page | several queues at once |
| 4 | Users: schema, tokens, per-user playlists | ownership |
| 5 | Reconnecting clients; mDNS discovery | a household on Wi-Fi |

The table follows A. For C1, step 1 builds slimproto in place of A's protocol, and needs no client. For B1, step 1 is the `Player` proxy and file serving. Step 1 alone is useful. Tests can drive `Remote` against a fake client over loopback, as `fake_player` drives the engine.

## Open questions

- **Why playr and not LMS?** The server page has no sampler. The playback features left are those LMS or MPD already offer. If the answer is "the library, the page and the playlists", C0 is cheaper.

- **Is a phone a client?** Having the page play the audio itself, with `<audio>` over HTTP range requests, needs no install. That is B with the browser as the engine: no speed, loops or ReplayGain.

- **Is the hub a Pi?** In A, C1 and C2, each zone decodes on the hub, and resamples when its device refuses a rate. B moves that to the clients. One Pi 3 zone is already unmeasured (`docs/dev/server.md`).

- **Who controls a zone?** Anyone, or its last user? A household may want the kitchen open to all and a bedroom locked to one user.

- **Is the selection per user or per zone?** Per zone is the current meaning: what plays next. Per user is a shopping list carried between rooms.

- **Is it one crate?** It can be `playr-server` with `--zones`, or a separate `playr-hub`. One crate keeps a single web page. Two keep the single-user server simple.
