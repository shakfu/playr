# Library analysis and ReplayGain

Design, written 2026-09-22 against playr 0.9.1, before any of it was built. It adds `playr analyze`, which decodes each file once and records checks, loudness and tempo in the library. ReplayGain applies the loudness at playback. It answers "ReplayGain" under Playback in `TODO.md`. Astra's integrity scanner (`src/main/services/libraryIntegrity.ts` in [Boof2015/astra](https://github.com/Boof2015/astra)) suggested the checks; Astra is GPL-3.0, so nothing is taken from its code.

## Constraints

- **playr never modifies the user's music files.** Results go into the library only. No tag is written, so no `mtime` changes and no rescan follows.

- **Off by default.** Without `playr analyze` and a `replaygain` setting, playback is unchanged, bit for bit.

- **No network.** Nothing is looked up.

## Open decisions

Recommendation first.

- **Gain source when both exist:** the library's analysis over tags, or tags over analysis. See "Precedence".

- **Album key:** `album` plus `album_artist`, falling back to the directory when `album_artist` is empty; or `album` plus the directory always.

- **Running alongside a player:** allowed, or refused by the instance lock as `playr scan` is. See "Concurrency".

- **Tracks without gain data under ReplayGain:** played at 0 dB, or at a fixed fallback gain.

## What exists

- **K-weighting.** `audio::meter::k_weighting` has BS.1770-4's filters at any rate. `Meter` computes momentary loudness only; integrated loudness needs gating on top.

- **FLAC MD5.** Symphonia 0.6.1 checks it: `AudioDecoderOptions::verify(true)`, then `finalize().verify_ok` (`symphonia-bundle-flac-0.6.1/src/decoder.rs:300`). No MD5 crate is needed.

- **FFT.** `realfft` is a dependency. `spectrum.rs` computes a 2048-point Hann transform every 512 frames, into 128 log-spaced bands.

- **Onsets.** `samples::onsets` finds discrete hits from an energy envelope, for slicing. A tempo estimate needs a continuous novelty curve, so it reuses the envelope, not the onset list.

- **Tags.** lofty 0.25.1 maps `REPLAYGAIN_{TRACK,ALBUM}_{GAIN,PEAK}` in ID3, Vorbis comments and MP4, `R128_{TRACK,ALBUM}_GAIN`, and `TBPM`/`tmpo` as `ItemKey::IntegerBpm`.

- **Skip logic.** `scan` skips files whose `(mtime, size)` is unchanged and commits every `SCAN_BATCH` (500) files.

- **Gain stage.** `output::render` multiplies by `volume`, clamped to 0..1, in the device callback, after the meter.

## `playr analyze`

```
playr analyze [PATH...]        analyse new and changed files, then print findings
playr analyze --force [PATH...] analyse again regardless of stat and version
playr analyze --report [--json] print stored findings, decoding nothing
```

With no path it covers the recorded roots, as `playr scan` does. It analyses only files already in `tracks`.

### One pass

Each file is decoded once. Every analyser reads the same chunks:

| Analyser | Result | Notes |
|-|-|-|
| Decode | frames decoded, packets skipped | `AudioStream::next_chunk` skips bad packets silently today; it needs a counter |
| FLAC MD5 | `ok`, `bad`, `absent` (all zero in STREAMINFO) | exact |
| Length | decoded frames against the header's `n_frames` | lossless formats only; lossy formats vary by encoder delay |
| Bits used | effective bit depth of an integer source | OR of every sample as an integer; trailing zero bits are padding |
| Cutoff | highest frequency with content, and the slope above it | heuristic, see below |
| Loudness | integrated LUFS, sample peak, a gating histogram | BS.1770-4, EBU R128 gating |
| Tempo | BPM and a confidence | heuristic, see below |

Duplicates need no decode. `--report` finds them from `tracks` (title, artist, duration within 2 s), and from equal FLAC MD5s when STREAMINFO holds them.

### Findings are derived, not stored

The table stores measurements. `--report` applies thresholds to them. A threshold can then change without decoding the library again.

| Finding | Rule |
|-|-|
| unreadable | decode failed at open |
| damaged | packets skipped > 0, or MD5 `bad` |
| no checksum | MD5 `absent` |
| wrong length | lossless, and decoded frames differ from the header |
| padded | bits used < the header's bit depth |
| possible lossy source | lossless, cutoff under 20 kHz, and a steep slope above it |
| possible upsampling | rate above 48 kHz and cutoff under 24 kHz |
| duplicate | as above |

### Cutoff

Average the magnitude spectrum over about 24 windows spread across the track. Find the highest frequency within some dB of the level below it, then measure how fast the level falls above it. An encoder's lowpass falls tens of dB within a few hundred Hz; a dark master falls slowly. Both thresholds need calibrating on known transcodes before the finding is trusted. It reports "possible" for that reason.

### Loudness

Integrated loudness per BS.1770-4: 400 ms blocks every 100 ms, an absolute gate at -70 LUFS, then a relative gate 10 LU below the mean of the blocks kept.

Album loudness is not the mean of track loudness. It gates the album's pooled blocks. To avoid decoding an album twice, each track stores a histogram: for each 1 LU bin from -70 to +5 LUFS, the block count and the sum of block energy. Pooling histograms gives the album's mean exactly, except for blocks within 1 LU of the relative gate. That is 76 bins of 12 bytes, under 1 KB a track.

Peak is the sample peak. playr quantises samples, so the sample peak is what clips in playr. True peak (4x oversampling, BS.1770 Annex 2) predicts overs in the DAC's reconstruction, costs a 4x upsample per track, and is left out.

At the end of a run, album loudness is recomputed for every album a changed file belongs to, into `album_loudness`.

### Tempo

1. Mono mix. Spectral flux per 512-frame hop: log magnitude in bands, half-wave rectified, summed.

2. Autocorrelate the flux over lags for 40 to 240 BPM.

3. Weight lags with a log-Gaussian prior centred on 120 BPM, one octave wide, as in [Ellis 2007](https://www.ee.columbia.edu/~dpwe/pubs/Ellis07-beattrack.pdf).

4. Refine the peak by parabolic interpolation. At a 512-frame hop, 120 BPM is a lag of 43 hops, and one hop is 2.8 BPM.

5. Confidence is the peak's height over the median of the curve. Below a threshold, BPM is stored as NULL.

Known failure modes:

- **Octave errors.** Half or double the tempo. The prior reduces these, and does not remove them.

- **Tempo changes.** One number per track. A live recording's drift averages out.

- **No pulse.** Ambient music yields a low confidence, so NULL.

A `TBPM` tag, where present, is preferred over the estimate: it is often set by hand. `--report` compares the two on tracks with both, which measures accuracy on the user's own library.

Uses: BPM times the varispeed ratio in now playing, and later a `bpm:` search field. The search parser matches text only; a numeric range is separate work.

### Schema

New tables, created with `CREATE TABLE IF NOT EXISTS` like `roots` and `resume`, so `SCHEMA_VERSION` stays 1. Keyed by `path`, like `marks`, so a rescan that renumbers tracks keeps them.

```sql
CREATE TABLE IF NOT EXISTS analysis (
  path        TEXT PRIMARY KEY,
  mtime       INTEGER NOT NULL,  -- the file's, when analysed
  size        INTEGER NOT NULL,
  version     INTEGER NOT NULL,  -- analyser version; older rows are redone
  error       TEXT,              -- open failed; the columns below are NULL
  frames      INTEGER,
  skipped     INTEGER,           -- packets that failed to decode
  md5         TEXT,              -- 'ok', 'bad', 'absent'; NULL: not FLAC
  md5_hex     TEXT,              -- STREAMINFO's, for duplicates
  bits_used   INTEGER,           -- NULL: not an integer source
  cutoff_hz   INTEGER,
  cutoff_db   REAL,              -- fall in the octave above the cutoff
  loudness    REAL,              -- integrated, LUFS; NULL: silent
  peak        REAL,              -- sample peak, linear
  histogram   BLOB,              -- 76 x (u32 count, f64 energy)
  bpm         REAL,
  bpm_conf    REAL
);

CREATE TABLE IF NOT EXISTS album_loudness (
  album_key TEXT PRIMARY KEY,
  loudness  REAL NOT NULL,       -- LUFS, gated over the album's blocks
  peak      REAL NOT NULL,
  tracks    INTEGER NOT NULL     -- tracks pooled; less than the album has: stale
);
```

A row is current when `mtime`, `size` and `version` match. `playr prune` deletes rows whose files are gone, as it does `marks`.

### Concurrency

The instance lock exists because a running playr caches playlists and marks and checks changes against its copy (`playr-app/src/instance.rs`). No frontend caches `analysis`. So `playr analyze` can run beside a player, writing only its two tables. WAL allows it, and rusqlite waits 5 s on a busy database by default.

Analysis can take hours, so refusing to play during it is the larger cost. The one cost of allowing it is CPU: on a Raspberry Pi, decoding on every core could starve the player's decoder. Workers default to one fewer than `available_parallelism`. Results reach the database from one thread, committed every 500 files, so an interrupted run keeps its progress.

A `:analyze` command, running in the background as `:slice` does, can come later.

### Cost

Unmeasured. Per file, the cost is a full decode plus a few FFTs a second. The FFT work is less than what the sampler's spectrogram does. The first run should report files per second so the README can quote a figure.

## ReplayGain

### Setting

```toml
# off, track, album or auto. auto uses album gain in normal and repeat modes,
# track gain in shuffle and repeat-one.
replaygain = "off"
```

`:replaygain auto` changes it for the session. The bottom line shows the gain applied, as it shows speed.

`auto` follows the mode rather than detecting whether an album plays in order. In normal mode, a playlist mixing albums gets each track's album gain, which is still a valid normalisation.

### Gain

- Reference level: -18 LUFS, per the [ReplayGain 2.0 spec](https://wiki.hydrogenaud.io/index.php?title=ReplayGain_2.0_specification). Gain in dB is `-18 - loudness`.

- Clipping: the linear gain is capped at `1 / peak`, so a boosted track never exceeds full scale. No limiter.

- Album gain needs every track of the album analysed (`tracks` in `album_loudness` equal to the album's count). Otherwise the track's own gain is used.

- A track with no data plays at 0 dB.

### Precedence

Recommended: library analysis, then tags.

- **Analysis first** puts the whole library on one measurement. Tags may come from ReplayGain 1.0 scanners, which used a different loudness model and an 89 dB reference. Mixed sources put tracks a few dB apart.

- **Tags first** respects gains the user set deliberately. It also needs no analysis run.

Tags are read by lofty when the engine opens a track, not at scan. Reading them at scan would need new `tracks` columns, and `scan` skips unchanged files, so an existing library would never fill them.

Opus: `R128_*_GAIN` is Q7.8 dB relative to -23 LUFS ([RFC 7845 section 5.2.1](https://www.rfc-editor.org/rfc/rfc7845#section-5.2.1)), so add 5 dB to reach -18. It is relative to the header's output gain, which RFC 7845 section 5.1 says a player must apply. `opus.rs` reads pre-skip from `OpusHead` and not the output gain, bytes 16 and 17. That is a separate, existing gap; most files carry 0.

### Where the gain applies

In the engine, where decoded samples are converted and appended to the ring (`Engine::convert_and_carry`). Not in the callback. The ring holds up to 2 s and spans track boundaries, so a gain the callback read from `Shared` would change up to 2 s away from the boundary it belongs to.

The engine needs each queued track's data. `Cmd::Play` and `Cmd::Enqueue` carry one entry per path:

```rust
pub struct Entry {
    pub path: PathBuf,
    pub track: Option<Gain>,   // from `analysis`
    pub album: Option<Gain>,   // from `album_loudness`
}
pub struct Gain { pub db: f32, pub peak: f32 }
```

The frontend fills them with one query for the queue. When both are `None`, the engine falls back to tags. The engine has no database connection, and this keeps it that way.

`volume` keeps its 0..1 clamp. ReplayGain is a separate factor, and the only one that can exceed 1.

### Consequences

- **The meter moves after ReplayGain.** It stays before `volume`. Under ReplayGain, loudness reads near -18 LUFS on every track. The comment on `render` changes from "describes the recording" to "describes what plays, before the volume".

- **Slices are unaffected.** `:slice` reads the source file, so exported WAVs stay exact copies.

- **Not bit-perfect when on.** Any gain other than 0 dB changes the samples. A gain of exactly 0 dB skips the multiply.

## Phases

1. `analysis` table, `playr analyze` and `--report` with the decode, MD5, length and bits-used checks. No heuristics.

2. Loudness, `album_loudness`, and ReplayGain from analysis and tags.

3. Cutoff and tempo, each after calibration against known files.

4. `:analyze` in the frontends; BPM in now playing; `bpm:` in search.

## Not included

- **Writing tags.** Excluded by the constraints.

- **True peak.** See "Loudness".

- **A PCM hash for duplicates** across formats. Needs a hash crate; `DefaultHasher` is not stable across Rust releases.

- **Clipping and DC-offset detection.** Cheap in the same pass. No finding needs them yet.
