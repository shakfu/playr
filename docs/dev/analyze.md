# Library analysis and ReplayGain

Design, written 2026-09-22 against playr 0.9.1, before any of it was built. All four phases were built the same day, with each open decision taken as recommended; "Built" at the end records where it differs. It adds `playr analyze`, which decodes each file once and records checks, loudness and tempo in the library. ReplayGain applies the loudness at playback. It answers "ReplayGain" under Playback in `TODO.md`. Astra's integrity scanner (`src/main/services/libraryIntegrity.ts` in [Boof2015/astra](https://github.com/Boof2015/astra)) suggested the checks; Astra is GPL-3.0, so nothing is taken from its code.

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

### Tempo, against librosa

No file in the library to hand carries a BPM tag, so the reference is a second
algorithm: librosa's `feature.rhythm.tempo`, over the same 328 tracks (mostly
ambient and electronic, decoded to mono at 22.05 kHz by ffmpeg). "Metrical" is
an answer at a simple ratio of librosa's: half, double, 3:2, 3:4 and so on,
within 4%.

| `MIN_CONFIDENCE` | Tracks kept | Same | Metrical | Unrelated |
|-|-|-|-|-|
| 0.0 | 328 (100%) | 45% | 25% | 30% |
| 0.2 | 196 (60%) | 55% | 26% | 19% |
| **0.3** | **155 (47%)** | **60%** | **28%** | **12%** |
| 0.4 | 129 (39%) | 63% | 27% | 10% |
| 0.5 | 98 (30%) | 65% | 27% | 8% |
| 0.6 | 68 (21%) | 63% | 31% | 6% |

0.3 is the knee: unrelated answers fall from 19% to 12% for 13 points of
coverage, and each step after that costs about 8 points of coverage for 2 of
agreement. Confidence orders the library as it should, from 18% agreement
below 0.1 to 63% above 0.6.

Two things this does not say. librosa is not ground truth: on music with no
pulse both estimators return a number, so "unrelated" counts a disagreement,
not a proven error. And both weight lags towards 120 BPM, so they may share an
octave bias; the 28% metrical share is a floor, and the 60% exact share is
optimistic.

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

## Built

All four phases, 2026-09-22. Where the build differs from the design above:

- **Gains reach the engine as one map, not per queued path.** `Cmd::SetGains` carries gains by path, and the session sends it when ReplayGain turns on and on every reload. `Cmd::Play` keeps its shape, and a library that never uses ReplayGain never loads the map. The engine works out each queue index's factor once and keeps it until the queue or the setting changes, so a mode change or a rescan never steps a playing track's level. `Cmd::SetReplayGain` seeks to the audible position, as a speed change does, so the new gain is heard at once.

- **Tempo confidence is the normalised autocorrelation at the chosen lag**, not the peak over the median. The ratio to the median grew with the number of lags searched: white noise read 1.3 to 1.7, either side of any threshold. The normalised value reads 0.85 to 1.0 for clicks and 0.01 to 0.02 for noise; `MIN_CONFIDENCE` is 0.2. BPM and confidence are both stored, and the threshold applies when read, as for the other findings.

- **The onset curve is smoothed over five hops.** A period of 34.5 hops puts onsets 34 and 35 hops apart in turn, splitting the autocorrelation peak, and twice the period won instead: 150 BPM read as 75.

- **A pulse above 120 * sqrt(2), about 170 BPM, reads at half tempo.** A strict pulse correlates as strongly at twice its period, so the prior decides. `a_pulse_above_170_bpm_reads_at_half_tempo` pins it.

- **The cutoff window scales with the rate**, 500 Hz each side at 44.1 kHz and 1.09 kHz at 96 kHz. The fixed window read ffmpeg's default upsampler, whose transition is wide, as a 15.5 dB fall.

- **More columns.** `rate`, `header_frames`, `lossless`, `bits` and `bpm_tag` make a row's findings computable without `tracks`. `error` also records a decode that stopped partway, and loudness is not stored then, so a partial measurement never sets a gain.

- **The window and the page have a ReplayGain menu.** The terminal shows the gain in its bottom line; all three show it while ReplayGain is on.

- **`:analyze` has its own lock, not the scan's.** `Session::analysing` is separate from `Session::scanning`, so an analysis and a scan may run together: an analysis writes only its own tables. One analysis runs at a time. It reports every file, not every hundredth as a scan does, since a file takes about half a second rather than a millisecond.

- **The tempo shown is the tempo heard.** `Model::bpm` multiplies by the varispeed ratio, so 120 BPM reads 143 at +3 semitones. It is read on each track change, not each frame.

- **A scan can analyse what it added.** `analyze_on_scan`, off by default, starts an analysis of the scanned directory once `Event::Scanned` lands and the scan added something. The rows the scan wrote are exactly the ones an analysis then finds out of date, so it needs no list of its own. `playr scan` loads no settings, so it prints the number waiting instead.

- **`bpm:` is not an FTS column.** `search` takes the term out, then joins `analysis` for the range; with no text term it matches on tempo alone. The join qualifies the columns, since `analysis` has a `path`, an `mtime` and a `size` of its own. A bare `bpm:128` matches within 1 BPM, and a range that parses as nothing matches nothing rather than everything.

### Built later

- **The Opus header's output gain is applied** (RFC 7845 section 5.1), so `R128_*_GAIN` tags, which count from it, no longer need it added back. `crates/playr-core/tests/decode.rs` patches the field in an encoded file and redoes the Ogg page checksum, since no encoder to hand writes one.

- **`:info` shows a track's measurements**, as a dialog rather than a fifth view. A view needs rows and a cursor of its own, and a per-track panel has neither: its content would follow another view's cursor. Browsing by measurement is a different feature, and belongs in the library view's columns and sorting, which `TODO.md` still holds. The rows are worded in `playr_app::message::info_rows`, which the report shares, so a finding reads the same in both.

- **A tempo keeps the level above it**, as `bpm_alt`, when the track pulses nearly as strongly there and the reading is below the prior's centre. Only above: a pulse correlates at every multiple of its period, so the slower readings are always available and recording them would match searches for tempos the track never plays. Only below the centre: that is where the prior halves. `bpm:` matches either level; what playr shows is unchanged. The column is analyser version 2, so version 1 rows are measured again.

### Calibration

ffmpeg 9.0.2 on pink noise, 30 s, 44.1 kHz unless marked. Cutoff and fall as `playr analyze --report --json` gives them.

| File | Cutoff | Fall | Finding |
|-|-|-|-|
| FLAC, genuine | 13.2 kHz | 0.8 dB | none |
| FLAC from MP3, LAME 128 kbit/s | 16.8 kHz | 57.6 dB | possible lossy source |
| FLAC from MP3, LAME 192 kbit/s | 18.9 kHz | 61.5 dB | possible lossy source |
| FLAC from MP3, LAME 320 kbit/s | 20.3 kHz | 67.7 dB | none: above 20 kHz |
| FLAC from AAC, ffmpeg 128 kbit/s | 17.3 kHz | 75.7 dB | possible lossy source |
| FLAC, 96 kHz, genuine | 10.3 kHz | 1.2 dB | none |
| FLAC, 44.1 upsampled to 96 kHz by ffmpeg's default resampler | 25.1 kHz | 26.5 dB | possible upsampling |

A genuine file's "cutoff" is wherever its largest small fall happens to be; the fall, not the frequency, says there is none. The upsampled file clears `STEEP_DB` by 1.5 dB, so a gentler resampler would not be flagged. LAME at 320 kbit/s is missed on purpose: a threshold above 20.3 kHz would reach CD masters whose anti-alias filters start near 20 kHz. None of this is checked against real recordings.

### Cost

A release build on an Apple M1 analyses 14.3 of those 30 s stereo files a second on one worker: about 430 s of audio a second per core. By extrapolation, not measurement, 50,000 four-minute tracks take about an hour on seven workers.

