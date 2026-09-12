# TODO

What is missing, roughly in the order it is worth doing. Items marked
**upstream** are not fixable here without replacing a dependency.

## Playback

- [ ] **Shuffle and repeat.** Repeat-one, repeat-all, shuffle. The queue already
      owns play order, so this is `queue.rs` and two keys.
- [ ] **Resume on start.** Remember the last track and position, offer to resume.
      Needs a small state table in the database.
- [ ] **Gapless across a sample-rate change.** A rate change rebuilds the output
      stream and leaves a gap. Fixing it means resampling both sides to a common
      rate, which trades a gap for a conversion. Worth a flag, not a default.
- [ ] **ReplayGain.** Read `REPLAYGAIN_*` and `R128_*` tags and apply track or
      album gain. Tag reading is already there; this is a gain stage and a
      preference.
- [ ] **Crossfade.** Needs two decoders and a mixer stage.

## Output

- [ ] **Bit-perfect output.** Select a `hw:` ALSA device so PipeWire cannot
      resample behind us. Today the `default` device accepts every rate and may
      convert internally, so "no resampling" means playr does not resample, not
      that nothing does.
- [ ] **Device selection.** No way to choose an output device; it is always the
      default. Needs a CLI flag and a settings view.
- [ ] **Native PipeWire backend.** cpal 0.18 has a `pipewire` feature. Untested
      here; the ALSA path works, so this is a quality experiment, not a fix.

## Formats

- [ ] **WavPack, WMA, Musepack, APE.** No Rust decoders of any maturity. Each
      needs either a C binding or an implementation.
- [ ] **DSD (.dsf, .dff).** Three separate pieces: a container reader, a
      decimation filter to convert 1-bit PDM to PCM, and a decision about DoP.
      DoP also needs bit-perfect output, which does not exist yet, so this is
      blocked on the item above. SACD rips add DST, a fourth piece.
- [ ] **Opus surround.** Multistream Opus is rejected at construction. The
      `opus` crate exposes `MSDecoder`; wiring it up is contained.
- [ ] **AAC gapless.** AAC decodes about 1900 frames long because encoder delay
      and padding are not trimmed. **upstream**, in Symphonia.
- [ ] **Opus in WebM end padding.** 648 frames of padding survive because
      Matroska does not report it as a packet trim. **upstream**.

## Library

- [ ] **Edit the queue.** No way to remove a track or reorder. Needs `d` in the
      queue view and a move binding.
- [ ] **Edit playlists.** Playlists can be saved, loaded and deleted, but not
      changed after the fact except by replacing them wholesale.
- [ ] **Sort and group.** Library order is fixed: album artist, album, disc,
      track. No way to sort by date added, year or duration.
- [ ] **Field-scoped search.** `artist:evans` and similar. FTS5 supports column
      filters already; this is query construction.
- [ ] **Watch for changes.** A scan is manual. `notify` could pick up new files,
      at the cost of a watcher thread.
- [ ] **Multiple roots.** `scan` takes directories but nothing records them, so
      a rescan means retyping the paths.

## Interface

- [ ] **Configuration file.** Nothing is configurable: not keys, not colours,
      not the seek step, not the default volume.
- [ ] **Help view.** The key hints are one line at the bottom and already do not
      fit a narrow terminal.
- [ ] **Album art.** Terminal image protocols exist (kitty, sixel). Embedded art
      is already parsed by Symphonia. Out of scope until the rest is settled.
- [ ] **Mouse.** No mouse support.

## Testing and packaging

- [ ] **Engine tests.** The state machine in `src/audio/mod.rs` is covered only
      through examples driving a real device. The pure parts are extracted and
      tested; the wiring is not.
- [ ] **CI.** No workflow. Should run `make test` on both feature settings, with
      and without ffmpeg present, to prove the format tests really do skip.
- [ ] **Packaging.** No release binaries, no crates.io publish, no man page.
