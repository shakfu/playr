# playr: design plan

A TUI music player. Recursive directory playback, SQLite-backed library,
search-as-playlist, saved playlists. No network, no scrobbling, no cover art
fetching.

## 1. The constraint conflict

"Minimal", "wide variety of formats", and "high quality" are not jointly
satisfiable at the maximum of each. The binding constraint is format coverage.

Symphonia 0.6.1 is the only mature pure-Rust decoder framework. Its shipped
codec list is AAC-LC, ADPCM, ALAC, FLAC, MP1/2/3, PCM, Vorbis; containers AIFF,
CAF, ISO/MP4, MKV/WebM, OGG, Wave.[^1] Absent: **Opus**, WavPack, WMA, Musepack,
APE, DSD, TTA, TAK. Opus is the only one likely to appear in a real library.
The upstream README lists Opus as "in work"; the released crate does not
register an Opus decoder at all.

Three ways to resolve it:

| Stack | Coverage | Cost |
|---|---|---|
| symphonia + cpal | ~90% of a typical library; no Opus | pure Rust, no C deps, full control of the output path |
| libmpv | everything, plus gapless and ReplayGain for free | one large C dep; playr becomes a frontend, not a player |
| ffmpeg-next | everything | heavy C dep, unsafe FFI, build fragility |

**Chosen: symphonia + cpal.** It keeps the "minimal, pure Rust" property, and
the output path is the part that determines quality, so owning it matters.
The decoder is placed behind a trait (section 4) so a libopus shim is additive
rather than a rewrite.

**Alternative framing worth stating:** if format coverage is the actual
priority, playr should be an mpv frontend. That is roughly one fifth the code
and strictly better coverage. The reason to reject it is that it makes the
interesting engineering disappear -- and makes "minimal" a property of playr's
source only, not of what gets installed.

## 2. What "high quality" means here

Operationally, four commitments:

1. **No resampling unless forced.** Open the output stream at the file's native
   sample rate when the device supports it. A 44.1kHz FLAC on a device that can
   do 44.1kHz is sample-for-sample.
2. **Resample well when forced.** `rubato` sinc/FFT resampler, not linear
   interpolation.
3. **f32 throughout.** Decode to f32, mix and attenuate in f32, quantize once at
   the device boundary.
4. **Native PipeWire output.** cpal 0.18 has a `pipewire` backend feature,
   avoiding the ALSA-to-pulse plugin path. `realtime` gives the callback RT
   scheduling priority, which is what prevents underruns.

Volume is applied as a float gain before quantization. No integer scaling.

## 3. Dependencies

Verified against crates.io on 2026-09-12.

| Crate | Version | Role |
|---|---|---|
| symphonia | 0.6.1 | demux + decode, feature `all` |
| cpal | 0.18.2 | output, features `pipewire`, `realtime` |
| rubato | 5.0.0 | resample when device rate != file rate |
| rtrb | 0.4.0 | lock-free SPSC ring, decoder -> audio callback |
| ratatui | 0.30.2 | TUI |
| crossterm | 0.29.0 | terminal backend + input |
| rusqlite | 0.40.2 | library DB, features `bundled`, `fts5` |
| lofty | 0.25.1 | tag reading during scan |
| walkdir | 2.5.0 | recursive scan |

No tokio. Nothing here is async; the concurrency is three long-lived threads.

`rusqlite/bundled` because this machine has no system sqlite3.

## 4. Architecture

Four threads, message-passing, no shared locks on the audio path.

```
  main thread            decoder thread          cpal callback (RT)
  -----------            --------------          ------------------
  ratatui render   --->  PlayerCmd channel
  crossterm input                |
                                 v
                         symphonia decode
                                 |
                         rubato (if needed)
                                 |
                            rtrb producer  ===>  rtrb consumer -> device
                                 |
                         AtomicU64 position  <---  (frames consumed)

  scanner thread: walkdir -> lofty -> sqlite (batched txn) -> progress channel
```

- **Audio callback** never allocates, never locks, never blocks. It pops f32
  frames from the ring, applies gain, writes to the device buffer. On underrun
  it writes silence.
- **Decoder thread** owns the symphonia `FormatReader` and `AudioDecoder`. It
  blocks when the ring is full. It handles seek, track advance, and stream
  reconfiguration when the next track has a different sample rate.
- **Position** is an `AtomicU64` of frames played, written by the callback,
  read by the UI. No mutex on the render path.
- **UI state** is a plain struct owned by the main thread. Player status arrives
  as messages, not shared memory.

Rate changes between tracks require tearing down and rebuilding the cpal stream.
That is a gap in playback. Gapless within an album at a constant rate works;
gapless across a rate change does not, and will not without a resample-to-common
-rate mode. Accept for MVP.

### Module layout

```
src/
  main.rs        arg parsing, wiring
  db/
    mod.rs       open, migrate
    schema.sql   tables + FTS5
    query.rs     search, playlist CRUD
  scan.rs        walkdir + lofty -> db
  audio/
    mod.rs       Player: public command API
    decode.rs    symphonia wrapper, the swap seam for other decoders
    output.rs    cpal stream construction, rate negotiation
    resample.rs  rubato wrapper
  ui/
    mod.rs       event loop
    views.rs     library / queue / playlists
    widgets.rs   now-playing bar, progress
  queue.rs       play order, repeat, shuffle
```

## 5. Schema

```sql
CREATE TABLE tracks (
  id           INTEGER PRIMARY KEY,
  path         TEXT NOT NULL UNIQUE,
  title        TEXT,
  artist       TEXT,
  album        TEXT,
  album_artist TEXT,
  track_no     INTEGER,
  disc_no      INTEGER,
  year         INTEGER,
  genre        TEXT,
  duration_ms  INTEGER,
  sample_rate  INTEGER,
  channels     INTEGER,
  bit_depth    INTEGER,
  mtime        INTEGER NOT NULL,
  size         INTEGER NOT NULL
);

CREATE VIRTUAL TABLE tracks_fts USING fts5(
  title, artist, album, album_artist,
  content='tracks', content_rowid='id', tokenize='unicode61'
);

CREATE TABLE playlists (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  created_at INTEGER NOT NULL
);

CREATE TABLE playlist_items (
  playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
  position    INTEGER NOT NULL,
  track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
  PRIMARY KEY (playlist_id, position)
);
```

`(mtime, size)` lets rescan skip unchanged files without hashing. FTS5 triggers
keep the index in sync on insert/update/delete.

DB lives at `$XDG_DATA_HOME/playr/library.db`, falling back to
`~/.local/share/playr/library.db`.

## 6. TUI

Three views, switched with Tab. One modal search bar.

```
+----------------------------------------------------------+
| playr            [Library] Queue  Playlists              |
+----------------------------------------------------------+
| Artist          Album              Title           Time   |
| Bill Evans      Sunday at the...   Waltz for Debby 6:56   |
| ...                                                       |
+----------------------------------------------------------+
| > Waltz for Debby -- Bill Evans                           |
| [########--------------] 2:31 / 6:56   FLAC 44.1k/24 -6dB |
+----------------------------------------------------------+
```

Keys: `space` play/pause, `n`/`p` next/prev, `/` search, `Enter` play selection,
`a` append to queue, `s` save queue as playlist, `+`/`-` volume, `[`/`]` seek,
`q` quit. Vim motions `j/k/g/G` in lists.

Search types into FTS5 and replaces the library view with results. `Enter` on
results plays them as an ad-hoc queue -- which is the "search then play" flow,
with no separate concept needed.

## 7. Milestones

1. **M1 skeleton.** Cargo project, Makefile, db module + migrations, `playr scan
   <dir>` CLI writing to SQLite. Tests on schema and rescan skip logic. Done.
2. **M2 audio.** Decode + output, rate negotiation, play/pause/seek/stop.
   This is where format coverage is proven. Done.
3. **M3 TUI.** ratatui loop, library view, now-playing bar, queue. Done.
4. **M4 search + playlists.** FTS5 query, ad-hoc queue from results, save/load.
   Done.
5. **M5 polish.** Shuffle, repeat, resume position, config file. Not started.

MVP is M1-M4, and is built. 42 tests pass.

## 8. Risks

- **Opus absent.** Resolved; see section 11. The seam worked as designed.
- **cpal `pipewire` backend maturity.** It is new. Fallback is the default ALSA
  backend, selected at build time by feature flag.
- **Rate-switch gaps.** Named above; accepted.
- **FTS5 external-content sync.** Triggers are easy to get subtly wrong.
  Covered by tests.

## 9. Corrections found while building

Five assumptions in sections 1-6 turned out to be wrong or incomplete. Each was
caught by a test against real files or real hardware.

**Opus is absent from the release, not merely "in work".** The upstream README
lists Opus and WavPack in its codec table. The shipped 0.6.1 crate registers
neither. This was resolved by supplying a decoder; see section 11.

**Do not apply `packet.trim_start` / `trim_end`.** The demuxers that support
gapless already remove encoder delay and padding. Applying the packet trims on
top double-trims: a 2 second MP3 decoded to 87602 frames instead of 88200, and
Vorbis to 87760. Only AAC, which has no gapless support upstream, decodes long
(90112 frames for 88200) and is left that way.

**ALSA rejects `snd_pcm_pause`.** It returns errno 77 on this PipeWire setup, so
pausing the device is not available. Pause is done in the callback instead: it
emits silence and consumes nothing from the ring. That also avoids a click on
resume.

**rubato's streaming API emits its filter delay as leading silence.**
`process_into_buffer` does not trim it, unlike `process_all_into_buffer`. One
second of 44.1kHz resampled to 48kHz produced 48960 frames instead of 48000, the
extra 960 being the delay. `Resample` now discards `output_delay()` output
frames at the start of a stream.

**"No resampling" is not the same as "bit-perfect".** cpal's ALSA `default`
device is PipeWire's plugin, which advertises 1-384000 Hz on every sample
format. Native-rate negotiation therefore always succeeds, and playr never
resamples in practice. PipeWire may still convert internally if its graph rate
differs from the requested rate. Genuine bit-perfect output needs a `hw:` device
and is not implemented.

### A negative offset in unsigned arithmetic

Relative seeks all restarted the track: `]` jumped to 0:05 and `[` to 0:00, from
wherever playback was, on all four bound keys.

A seek reopens the output stream, so the device frame count restarts at zero
while playback is partway into the track. That was carried by storing
`track_start = 0u64.wrapping_sub(offset)`, a deliberately wrapped negative.
The reader then computed `frames_out.saturating_sub(track_start)`, and
`saturating_sub` clamps instead of wrapping back, so the position read zero
forever after any seek. `SeekBy` adds its delta to the current position, so
every relative seek started from zero again.

Position is now `(frames_out - track_start) + position_offset`, with the seek
target carried as an addition rather than a negative. Unsigned types cannot
represent the negative the old code needed, and the clamp hid it. The
calculation is one function, `track_position`, covered by `tests/position.rs`.

### Varispeed

`[` and `]` shift playback speed by a semitone per press, pitch moving with
tempo. Steps are geometric (`2^(1/12)`), so every press is the same musical
interval and twelve of them are exactly an octave; the range is clamped to
0.5x-2.0x. Powers of two were considered and rejected: 2x is a whole octave, so
the useful range would hold three settings.

Speed is a resampling ratio, not a separate stage: the resampler runs from
`source_rate * speed` to the device rate. That meant two changes.

**The resampler had to become adjustable.** rubato's `Fft` resampler has a fixed
ratio -- `as_adjustable` returns `None` -- so it was replaced with `Async` sinc,
which accepts a ratio at construction and can ramp between ratios.

**A speed change re-seeks.** The ring holds up to two seconds already resampled
at the old ratio. Changing the ratio alone leaves that to play out first, so the
key appears dead for a second or more. Re-seeking to the current position
discards it and makes the change immediate, at the cost of the same brief refill
a seek already costs.

Position stays in track time at any speed: `(frames_out - track_start) * speed`,
plus the offset banked at the last seek or speed change. A ten second file plays
in 5.00s at 2x and 19.61s at 0.5x, while still reporting ten seconds of track.

Pitch was verified by Goertzel rather than by ear: at +12 semitones a 440Hz tone
reads 880Hz, at -12 it reads 220Hz, and at +1 it reads 466.16Hz, which is A#4.

## 10. Measured behaviour

Verified on this machine, not inferred:

| Check | Result |
|---|---|
| Formats decoded | WAV, FLAC (44.1k and 96k), AIFF, ALAC, Vorbis, MP3, AAC |
| Frame-exact decode | all of the above except AAC, which runs 1912 frames long |
| Opus | decoded by `src/audio/opus.rs`; frame-exact, 77 dB SNR vs libopus |
| Playback timing | a 2.00s file takes 2.10s wall clock |
| Queue of 5 tracks | 10.11s wall clock, no gaps |
| Rate changes | 44.1k to 96k to 44.1k, each opened at its native rate |
| Resampler accuracy | 440Hz tone stays at 440.0Hz, amplitude 0.5000, by Goertzel |
| Opus, OGG and WebM | frame-exact at 96000 frames, 138.7 dB SNR vs ffmpeg |
| All formats, both profiles | a 2.0s file takes 2.10s; a 4 track queue takes 8.11s |
| Relative seeks | +5 +5 +5 -5 -5 from 0.6s reaches 7.6s, accumulating correctly |
| Varispeed | a 10s file plays in 5.00s at +12 st, 19.61s at -12 st |
| Varispeed pitch | 440Hz reads 880Hz at +12 st, 466.16Hz at +1 st |

## 11. Adding Opus

Symphonia demuxes Opus fully in both OGG and Matroska and stops only at the
decoder: a probe reports codec id `0x1001`, the sample rate, the channel count,
and the packets. Only `make_audio_decoder` fails. That made the change narrow.

`src/audio/opus.rs` implements Symphonia's `AudioDecoder` and
`RegisterableAudioDecoder` over libopus, reached through the `opus` crate.
`decode.rs` now builds its own `CodecRegistry` -- Symphonia's enabled codecs plus
this one -- instead of calling `get_codecs()`. Nothing else changed, and WebM
with an Opus payload started working at the same time.

Two details cost more than the wiring:

**Pre-skip is the decoder's job.** Opus declares an encoder delay in its
OpusHead header, and no container reports it as a packet trim: for a 2 second
file the OGG demuxer sets `trim_end=648` on the last packet but `trim_start=0`
on the first, while `Track::delay` carries 312 that nothing applies. The decoder
parses OpusHead and drops those 312 frames itself. Without it every Opus track
opens with 6.5ms of encoder warm-up and runs long. `reset()` clears the counter,
since a seek means the stream start is behind us.

**Where trimming belongs.** Section 9 recorded that re-applying `packet.trim_*`
double-trims, but not why. The reason is that Symphonia's *decoders* call
`buf.trim(packet.trim_start, packet.trim_end)` -- `symphonia-codec-vorbis`
line 324, `symphonia-bundle-mp3` line 131. Demuxers set the fields; decoders
apply them. The Opus decoder does the same, which is why it composes correctly
with the rest of the pipeline.

Verified against libopus by decoding the same file with ffmpeg: 96000 frames
from both, max sample difference 1.7e-05, 77.0 dB SNR, no alignment offset. An
earlier run needed a 312 frame offset to align, which is how the missing
pre-skip was found.

Not covered: multistream (surround) Opus, rejected at construction. WebM-Opus
keeps 648 frames of end padding, because Matroska does not report it and
Symphonia does not surface the block duration that would.

**A slow decoder exposed an unbounded `pump`.** With Opus as the only queued
track, a debug build showed the player stuck: no status, no response to
commands. `pump` filled the whole two second ring in a single call, and until it
returned the run loop could neither publish status nor read a command. FLAC
decodes fast enough that the stall was invisible; Opus, several times slower,
made it last about a second. `pump` now stops at an 8ms deadline and lets the
run loop take a turn. This was a pre-existing flaw in the engine, not something
Opus introduced.

## 12. Choosing the Opus decoder

Opus was first wired to `opus-decoder`, a pure-Rust implementation, to keep the
project free of C dependencies. That was replaced with libopus.

**`audiopus` cannot build here.** It is the better-known binding, but version
0.2.0 depends on `audiopus_sys` 0.1.8, whose build script shells out to
`autoreconf`. This machine has neither autotools nor libopus development
headers, and `libopus.so` has no linker symlink, so both the vendored and the
dynamic path fail. `audiopus_sys` 0.2.2 builds via cmake, but the safe wrapper
does not depend on it.

**The `opus` crate was used instead.** It is the other safe libopus binding,
more widely used than `audiopus`, and it vendors and builds libopus through
cmake with no system packages. Same C library, so it satisfies the reason for
moving off a pure-Rust decoder.

Accuracy against ffmpeg decoding the same file:

| Decoder | Max sample difference | SNR |
|---|---|---|
| libopus via `opus` | 3.7e-08 | 138.7 dB |
| pure Rust `opus-decoder` | 1.7e-05 | 77.0 dB |

Both produce 96000 frames with no alignment offset, so both handle pre-skip
correctly. The 138.7 dB figure is float32 rounding: ffmpeg uses libopus too, so
this is the same decoder compared against itself.

Opus is therefore behind the `opus` cargo feature, **off by default**. It is the
only part of playr that pulls cmake, so the default build needs no cmake at all
and `--features opus` adds it. A C compiler is required either way, because
`rusqlite`'s bundled SQLite compiles from C source and always has; the feature
controls cmake and libopus only. `make test` and `make clippy` run both
configurations, so the opt-in path cannot rot unnoticed.

Off rather than on because the default build should not levy a build dependency
for a codec most libraries do not contain. The cost is that Opus users must find
one flag, which `playr formats` names when it reports an Opus file it cannot
decode.

Costs of the change:

- A C compiler and cmake are now build requirements. Building libopus adds about
  7 seconds to a clean build.
- `opus::Decoder` declares `Send` but not `Sync`, while `AudioDecoder` requires
  both. A newtype asserts `Sync`: every method that changes decoder state takes
  `&mut self`, and the one `&self` method passes a `const` handle to libopus and
  only parses the packet, so a shared reference cannot reach mutable state.
- Decoding got faster. Under a debug build the pure-Rust decoder ran near
  real time, which is what exposed the unbounded `pump` described above.

[^1]: Read from the shipped `symphonia-0.6.1/src/lib.rs` `register_enabled_codecs`
and `register_enabled_formats`, not the upstream README, which describes
unreleased work.
