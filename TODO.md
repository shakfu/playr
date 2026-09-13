# TODO

What is missing, roughly in the order it is worth doing. Items marked **upstream** are not fixable here without replacing a dependency.

## Playback

- [ ] **Shuffle and repeat.** Repeat-one, repeat-all, shuffle. The engine owns play order in `src/audio/mod.rs`, so this is a play-order change there and two keys.

- [ ] **Resume on start.** Remember the last track and position, offer to resume. Needs a small state table, added as schema version 2 (`PRAGMA user_version`).

- [ ] **Gapless across a sample-rate change.** A rate change rebuilds the output stream and leaves a gap. Fixing it means resampling both sides to a common rate, which trades a gap for a conversion. Worth a flag, not a default.

- [ ] **ReplayGain.** Read `REPLAYGAIN_*` and `R128_*` tags and apply track or album gain. Tag reading is already there; this is a gain stage and a preference.

- [ ] **Crossfade.** Needs two decoders and a mixer stage.

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

- [ ] **Rename playlists.** A playlist is edited by adding it to the selection with `a`, changing it, and saving it under the same name. There is no way to rename one short of saving a copy and deleting the original.

- [ ] **Sort and group.** Library order is fixed: album artist, album, disc, track. No way to sort by date added, year or duration.

- [ ] **Field-scoped search.** `artist:evans` and similar. FTS5 supports column filters already; this is query construction.

- [ ] **Watch for changes.** A scan is manual. `notify` could pick up new files, at the cost of a watcher thread.

- [ ] **Multiple roots.** `scan` takes directories but nothing records them, so a rescan means retyping the paths. Pruning only checks the directories scanned, so rows for files deleted under a root that is never rescanned stay. Recorded roots would allow a bare `playr scan` that covers them all.

- [ ] **Keep rows for missing files.** Pruning deletes a row, and the cascade removes it from every playlist. A moved file comes back as a new row in no playlist. Marking rows missing instead needs a schema change.

- [ ] **Relative rows from 0.1.0.** A 0.1.0 scan with a relative path stored relative rows. Rescanning adds absolute duplicates, and pruning never matches the old rows. A one-off cleanup would delete them, and their playlist entries with them.

## Interface

- [ ] **Configuration file.** Nothing is configurable: not keys, not colours, not the seek step, not the default volume.

- [ ] **Album art.** Terminal image protocols exist (kitty, sixel). Embedded art is already parsed by Symphonia. Out of scope until the rest is settled.

- [ ] **Mouse.** No mouse support.

## Testing and packaging

- [ ] **CI.** No workflow. Should run `make test` on both feature settings with `PLAYR_REQUIRE_FFMPEG=1`, so missing ffmpeg fails the run. A runner has no audio device, so only the real-device smoke test skips there.

- [ ] **Packaging.** No release binaries, no crates.io publish, no man page.
