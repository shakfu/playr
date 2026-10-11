# Loop library, and a loop per voice

Status: built 2026-10-11, unreleased. Where the build departs from this design is at the end.

Two features:

1. A **loop library**: loops indexed in SQLite, apart from the audio library. Loops come from scanned directories of precut loops, from the sampler's 8 slots, and from the tape's Save loop.

2. **A loop per voice** on the tape: each of the 3 voices can read its own loop instead of voice 1's.

## Decisions taken

From the 2026-10-11 discussion:

| Question | Decision |
|-|-|
| The two libraries | Distinct. A file is a track or a loop, not both. |
| Write head with a loop per voice | Off while any voice reads its own loop. Writing into a chosen loop is later work. |
| Tempo and phase across voices | Manual. No rate is set from a loop's tempo; a load starts at once. |
| The 8 slots | Moved into the loop library, with the slot as a nullable column. |
| Save loop | Adds to the loop library only. A mix recording stays a track: it is not cut to loop. |
| Where the library is shown | A `View` in the terminal and the window. Not the server. |
| Import | A file under a loop root is a loop. The loop scan takes it out of `tracks`. |
| One-shots | An `acid` chunk marking a file one-shot keeps it out. |
| Key | A column. Read from the `acid` chunk or the file name; not detected. |
| Apple Loops | Not read. A TODO item. |

## Schema

New tables, created with `CREATE IF NOT EXISTS` as the schema's others are. `SCHEMA_VERSION` stays 1: a bump would stop an older playr opening the library at all.

```sql
CREATE TABLE IF NOT EXISTS loop_lib (
  id       INTEGER PRIMARY KEY,
  path     TEXT NOT NULL,
  start    INTEGER NOT NULL,  -- source frames, end exclusive
  end      INTEGER NOT NULL,
  rate     INTEGER NOT NULL,  -- the rate start and end count in
  channels INTEGER,
  slot     INTEGER,           -- 1 to 8: the track's F key; NULL for others
  origin   TEXT NOT NULL CHECK (origin IN ('scan', 'slot', 'tape')),
  name     TEXT NOT NULL,
  tags     TEXT NOT NULL DEFAULT '',  -- words, space separated
  bpm      REAL,
  beats    INTEGER,
  bpm_from TEXT,              -- 'tag', 'acid', 'name', 'length', 'user'
  key      TEXT,
  mtime    INTEGER,           -- a scanned file's, as tracks keep them
  size     INTEGER
);
CREATE UNIQUE INDEX IF NOT EXISTS loop_lib_slot ON loop_lib(path, slot) WHERE slot IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS loop_lib_file ON loop_lib(path) WHERE slot IS NULL;
CREATE TABLE IF NOT EXISTS loop_roots (path TEXT PRIMARY KEY);
```

- **A loop is a span of a file.** A scanned or saved loop is the whole file, or the loop its `smpl` chunk names. A slot is a span of a track. Spans keep the track's audio either side for the crossfades. No audio is copied.

- **Search.** An FTS5 index over `name`, `tags` and `key`, kept by triggers, as `tracks_fts` is. `bpm:` and `key:` filter, as `bpm:` does in the library.

- **Slots move on open.** `init` copies rows from `loops` into `loop_lib` as `origin='slot'`, then empties `loops`. An older playr then sees no slots, and a slot it saves moves on the next open. The `loops` table stays, so the older playr still opens the library.

## Scanning loops

`playr loops scan [DIR...]` records `DIR` in `loop_roots` and indexes its audio files. With no `DIR` it rescans the recorded roots. `playr loops roots` lists them; `playr loops rm DIR` forgets one and its loops.

- **The walk is `scan_dir`'s.** Files whose `(mtime, size)` match are skipped. Missing and unavailable roots are counted the same way.

- **Kept apart from tracks.**
  - The loop scan removes `tracks` rows under its root, with their places in playlists, and reports how many.
  - `playr scan` skips files under a loop root.
  - A loop root that holds a track root is refused, so one command cannot empty the audio library.

- **Tags** are the directories between the root and the file: `Pack/Drums/Kicks/x.wav` is tagged `pack drums kicks`.

- **Frames** come from the header (`header_frames`). A file without them is decoded to count them.

### Tempo and key

`bpm = 60 * beats * rate / (end - start)`. The first source that holds a value wins:

1. A BPM tag, as `analysis::bpm_tag` reads it.
2. A WAV `acid` chunk: beats, tempo, root note and the one-shot flag.
3. The file name: a token `120bpm`, or `120` followed by `bpm`, or a bare 2- or 3-digit number from 60 to 200. Key from a token such as `Am`, `C#m`, `Fmaj` or `Bb`. A single letter, such as `A`, is not a key: it names takes too often.
4. Length alone: the power-of-2 beat count whose tempo is nearest 120 BPM on a log scale. Stored with `bpm_from='length'`.

Beat detection is not used: it does not settle on a 2-bar file.

Given a tempo, `beats` is its rounded count over the loop. `:beats N` in the Loops view sets `beats` and `bpm_from='user'`.

The `acid` layout, from memory and to be checked against files from a real pack: `u32` flags (bit 0 one-shot, bit 1 root note set), `u16` root note (MIDI), `u16`, `f32`, `u32` beats, `u16` meter denominator, `u16` meter numerator, `f32` tempo. 24 bytes.

### Rolls for a whole-file loop

A whole-file loop has nothing either side for a crossfade to read. The load builds circular rolls: the pre-roll is the loop's tail and the post-roll its head, up to `ROLL` each. For a file cut to loop, that join is the one meant. The rejection of circular rolls under "The write head writes the rolls" in `looper-engine.md` was for a span of a track, whose end does not lead back to its start.

## The Loops view

A list like Playlists. Columns: name, BPM, beats, key, length, channels, origin, tags. `/` searches it. `9` or `:view loops` opens it; the terminal's tab cycle includes it.

| Key | Command | Does |
|-|-|-|
| `enter` | `:load` | Loads the loop into the tape for every voice, as `:tape load` loads a range |
| `!` `@` `#` | `:load N` | Loads it into voice N only |
| | `:beats N` | Sets the loop's beat count |
| | `:key K` | Sets its key |
| | `:remove` | Takes it out of the library; a slot is emptied; no file is deleted |

## The engine: a loop per voice

`Tape` keeps `lp`, voice 1's loop, and gains `own: [Option<Box<Loop>>; VOICES]`. A voice reads its own loop when it has one, else `lp`. Voice 1's own loop is `lp`. A single-loop tape is `own` all `None`, as today.

- **`Cmd::LoadVoice(i, Box<Loop>)`** puts a loop in voice `i`'s place and fits that voice's window to it. Voice 0's replaces `lp`, so voices without their own follow it.
- **`Cmd::Load`** replaces `lp` and returns every `own` loop. A load can return up to 4 values (3 loops and a snapshot), within the return ring's 16.
- **Fades.** A `LoadVoice` while playing fades that voice alone over 5 ms, out and in. The others play on.
- **The write head** is held off while any `own` is `Some`. `Setting::Write(true)` is kept and applies once none is.
- **The peak grid** is one per voice, from the loop it reads.
- **Reset** restores every loop as last loaded. The handle keeps a copy per voice.
- **Snapshot** copies `lp`, as now. An `own` loop is never written, so its copy on the handle is already exact.

A loop for a voice is resampled to the looper's rate on load. A voice load with no tape loaded starts a looper at that loop's rate.

## In the app

- `tape::load_loop(f, id, voice)`, called from the Loops view.
- `Extent` becomes one per voice, so windows, `:tape N window 25% 75%` and the lanes count in that voice's loop.
- The Tape tab draws the selected voice's loop. At 800 x 592 there is no room for three waveforms.
- Saves go to `samples/<name>-tape/`, named after voice 1's loop, as now.

## Tests

- Schema: an old library with `loops` rows opens with them in `loop_lib`; a second open moves nothing twice; a cleared slot stays cleared.
- Scan: tags from directories; a one-shot is skipped; tempo and key from each source, in order; a track under a new loop root leaves `tracks`; `playr scan` skips loop roots; a loop root holding a track root is refused.
- `acid` and `smpl` read from WAVs written by the test.
- Tempo from length: 2 s at 44.1 kHz is 4 beats at 120.
- Engine: voices on their own loops each equal their loop at rate 1; a `LoadVoice` returns the old loop; writing is held while any voice has its own loop; reset restores each.
- Parity: the Loops view's keys and commands in both frontends.

## Steps

1. Schema, slot migration, `loop_lib` queries, and the slots moved onto them.
2. The loop scan, its tempo and key sources, and `playr loops`.
3. The engine's loop per voice.
4. `playr-app`: actions, commands, Save loop to the library, the Loops view's model.
5. The terminal's and the window's Loops view.
6. README, guides, CHANGELOG, `docs/architecture.md`.

`make test` after each.

## Open

- **Writing with a loop per voice.** Option (b): the write head prints every send into voice 1's loop.
- **Apple Loops.** In `TODO.md`.
- **Scanning from inside playr.** `:scan` covers tracks only; loops are scanned from the command line.
- **A slot loop's tempo.** Stored empty. The track's analysed tempo could fill it at save time.

## Where the build departs

- **Keys.** `1` to `3` switch views everywhere, so a voice load is `!` `@` `#`, shift and the digit on a US layout. `:load [N]` in the Loops view, not `:tape N load`.

- **Not a `TapeAction`.** A library load needs the cursor's loop id, which `:tape` cannot name and `command::line` cannot print back. It is `Action::LoopLib(LoopAction::Load(n))`, which calls `tape::load_loop`.

- **No tape loaded.** A load into voice N with no tape loads the loop for every voice. A looper with an empty voice 1 would need every `Extent` user to handle a missing one.

- **Key order.** The name's key wins over the `acid` root note: it carries the mode, `Am`, which the root note does not.

- **Write gate.** The gate closes over 20 ms when a voice gets its own loop while playing, as it does when writing is turned off. Up to 20 ms of writing happens during that ramp; switched at once it would step.

- **`playr loops prune [DIR]`.** Added: rows of deleted loop files otherwise stayed for good.

- **Migration writes only when there is something to move.** Every open running `DELETE FROM loops` took a write lock, and an analysis thread opening the library at the same time failed with `SQLITE_BUSY`.

- **Names.** The Sampler menu's Loops submenu became Loop slots, apart from the Loops view. The window's search field went from 240 to 180 points, so the ninth tab fits 800 points.

- **Not checked.** The `acid` layout is from memory, tested only against files the tests write; check it against a real loop pack. The terminal's Loops view and the window's per-voice Tape drawing were not looked at by hand.
