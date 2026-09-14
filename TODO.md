# TODO

What is missing, roughly in the order it is worth doing. Items marked **upstream** are not fixable here without replacing a dependency.

## Playback

- [ ] **Resume on start.** Remember the last track and position, offer to resume. Needs a small state table. A new table is compatible with older playr, so like `marks` it needs no `PRAGMA user_version` bump.

- [ ] **A-B loop between marks.** Loop the stretch between the marks either side of the playing position, for practice with varispeed. Marks are already source frames; the loop is an engine change.

- [ ] **Live marks for sampling tools.** Slices can be exported as files. Handing a marked passage to a running tool, such as SuperCollider, is not done: `docs/sampler.md` weighs a JSON Lines file against OSC.

- [ ] **Sampler: mark editing.** A cursor in the sampler view, independent of the playhead; select a mark and nudge it by a column, a millisecond or a frame; snap to a zero crossing or the nearest onset; delete one mark. `MARK_NEAR` refuses marks within 500 ms of each other, which is too coarse at the sampler's zoom.

- [ ] **Sampler: audition and loop.** Play the region, or the slice under the cursor, once or looped. Needs sample-accurate looping in the engine, without reopening the device.

- [ ] **Sampler: loop points.** Write loop points to `samples.json` as `loop_start`, `loop_end` and `loop_enabled`, which rtrack reads.

- [ ] **Waveform cache.** The sampler decodes the whole track each time a new track is shown there. Peaks are about 8 MB for a 4-minute track and could be kept per file.

- [ ] **Slice edges.** Slices start and end on whatever sample falls there. rtrack snaps loop points to zero crossings; the same for slice edges, or a short fade, would remove clicks at the cost of a slice that is no longer an exact copy.

- [ ] **Gapless across a sample-rate change.** A rate change rebuilds the output stream and leaves a gap. Fixing it means resampling both sides to a common rate, which trades a gap for a conversion. Worth a flag, not a default.

- [ ] **ReplayGain.** Read `REPLAYGAIN_*` and `R128_*` tags and apply track or album gain. Tag reading is already there; this is a gain stage and a preference.

- [ ] **Crossfade.** Needs two decoders and a mixer stage.

## Sampler

- [ ] The arrow movement which works well in the play mode, is not granular enough in sampler mode. It should change to snapping-to-zero in sampler mode.


## Output

- [ ] **Bit-perfect output.** Select a `hw:` ALSA device so PipeWire cannot resample behind us. Today the `default` device accepts every rate and may convert internally, so "no resampling" means playr does not resample, not that nothing does. Negotiation already accepts the 32-bit and 24-bit integer formats such devices offer; choosing the device is what remains.

- [ ] **Device selection.** No way to choose an output device; it is always the default. Needs a CLI flag and a settings view.

- [ ] **Native PipeWire backend.** cpal 0.18 has a `pipewire` feature. Untested here; the ALSA path works, so this is a quality experiment, not a fix.

## Formats

- [ ] **WavPack, WMA, Musepack, APE.** No Rust decoders of any maturity. Each needs either a C binding or an implementation.

- [ ] **DSD (.dsf, .dff).** Three separate pieces: a container reader, a decimation filter to convert 1-bit PDM to PCM, and a decision about DoP. DoP also needs bit-perfect output, which does not exist yet, so this is blocked on the item above. SACD rips add DST, a fourth piece.

- [ ] **Opus surround.** Multistream Opus is rejected at construction. The `opus` crate exposes `MSDecoder`; wiring it up is contained.

- [ ] **AAC gapless.** AAC decodes about 1900 frames long because encoder delay and padding are not trimmed. **upstream**, in Symphonia.

- [ ] **Opus in WebM end padding.** 648 frames of padding survive because Matroska does not report it as a packet trim. **upstream**.

- [ ] **Opus in WebM seek precision.** Seeks land 1.5 ms early. Matroska timestamps are whole milliseconds, and Symphonia subtracts the 6.5 ms codec delay in those units. The Opus header gives the delay in samples, which would recover part of it.

- [ ] **Tags and duration for CAF, MKV and WebM.** lofty cannot parse these, so they are indexed under their file names, and Symphonia's Matroska reader reports no duration. Symphonia's own metadata might supply the tags; untested. Duration would otherwise mean decoding each file at scan time.

## Library

- [ ] **Sort and group.** Library order is fixed: album artist, album, disc, track. No way to sort by date added, year or duration.

- [ ] **Watch for changes.** A scan is manual. `notify` could pick up new files, at the cost of a watcher thread.

- [ ] **Multiple roots.** `scan` takes directories but nothing records them, so a rescan means retyping the paths. Pruning only checks the directories scanned, so rows for files deleted under a root that is never rescanned stay. Recorded roots would allow a bare `playr scan` that covers them all.

- [ ] **Keep rows for missing files.** Pruning deletes a row, and the cascade removes it from every playlist. A moved file comes back as a new row in no playlist. Marking rows missing instead needs a schema change.

- [ ] **Relative rows from 0.1.0.** A 0.1.0 scan with a relative path stored relative rows. Rescanning adds absolute duplicates, and pruning never matches the old rows. A one-off cleanup would delete them, and their playlist entries with them.

## Interface

- [ ] **Colours.** Not configurable. `render.rs` uses 16 colours directly; they need named roles, such as accent and dim, before a `[colors]` table in `settings.toml` could set them.

- [ ] **Command history across sessions.** `:` history is kept in memory and lost on exit. A table in the library would keep it.

## Testing and packaging

- [ ] **Packaging.** No man page and no shell completions. The command line is parsed with clap, so `clap_mangen` and `clap_complete` can generate them. Release binaries are built by `.github/workflows/release.yml`; crates.io publishing is still manual, with `cargo publish --workspace`.
