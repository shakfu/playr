# TODO

What is missing, grouped by priority:

- **Critical:** loses data, or blocks the next release.

- **High:** a gap most users meet.

- **Medium:** improves an area that already works.

- **Low:** niche, large for its benefit, or blocked **upstream**, which marks items not fixable here without replacing a dependency.

An item citing "decision N" refers to [docs/dev/decisions.md](docs/dev/decisions.md).

## Critical

## High

### Desktop window

- [ ] **CJK fonts.** egui's default fonts cover Latin, Greek and Cyrillic, so Japanese, Chinese and Korean tags show as boxes. Bundling Noto Sans CJK adds about 16 MB a binary (estimate); loading a system font needs a font-lookup crate and differs per platform.

- [ ] **macOS signing and notarization.** `playr.app` is unsigned, so a downloaded copy is refused until allowed in System Settings. Needs an Apple Developer account and the workflow's secrets.

## Medium

### Playback

- [ ] **One span: loop the region, A-B loop in every view.** Region, range, loop and loop slot are four concepts for one span. A region cannot loop. A range is lost at a track change unless saved to a slot. `:loop`, `:range`, `:in` and `:out` are refused outside the sampler view, though they act on playback. Keep the range as the one span, and add `:range region` to set it from the marks around the playhead. Make `:in`, `:out`, `:range`, `:loop` and `:loop N` global; bind `<` and `>` in the sampler only.

- [ ] **Fine varispeed.** `:speed` parses whole semitones. A semitone is 5.9%, so 120 BPM moves to 127.1 or 113.3 with nothing between. Add `:speed +50c`, and `:tempo 128` on an analysed track to set the ratio that gives 128 BPM. Check first whether the resampler takes an arbitrary ratio.

- [ ] **A key for the EQ.** Every other playback control has one.

### Sampler

- [ ] **Waveform cache.** The sampler decodes the whole track each time a new track is shown there. Peaks, spectrogram and loudness are about 16 MB for a 4-minute track and could be kept per file. With it, the sampler could open the cursor row's track without playing it.

- [ ] **Spectrogram at high sample rates.** The transform is 2048 frames at every rate, so a bin is 21.5 Hz at 44.1 kHz and 94 Hz at 192 kHz, and bass on hi-res files blurs further. Scaling the transform with the rate, 4096 at 96 kHz and 8192 at 192 kHz, keeps about 46 ms and 21 Hz everywhere, at more CPU per read on those files.

- [ ] **Tempo-grid slicing.** `:slice beats N` cuts the range every N beats at the analysed tempo, from the range's start or the first mark. Nothing else links analysis to the cutter. Limits: one BPM a track, no downbeat detection, and halved readings above 170 BPM. The user supplies the phase with a mark. Drift over a long range is likely on music not made to a click (inference); onset slicing stays better there. It serves the primary user (decision 2): tempo is the one thing analysis measures that the cutter does not use.

- [ ] **Loop points in the WAV.** A range cut whole while it loops is marked to loop only in `samples.json`, playr's own format, which other samplers are unlikely to read (decision 3; inference). The WAV itself carries no loop. A `smpl` chunk with the loop's start and end is the usual place for one, and many samplers read it (inference, not checked per sampler). A `cue ` chunk could carry slice points and marks the same way.

- [ ] **Mark labels and export.** `marks` has `path`, `frame` and `rate`, and no text. A label column would allow notes such as "solo" or "break". Export as an Audacity label file or a cue sheet; `docs/sampler.md` already designs a `marks.jsonl` line. Since slices serve any sampler (decision 3), SFZ output may reach the most samplers (inference, not researched).

### Library

- [ ] **Deduplicate entries.** Scanning one album from two paths adds it twice. `is:duplicate` lists the copies, and `is:duplicate path:DIR` the copies under one folder. Removing them is manual: delete the duplicate folder, then rescan.

- [ ] **Group, a date added, and play history.** `columns` and `sort` order the library by any column, but there is no grouping, and no date added to sort by: the schema has no `added_at`, and `mtime` is the file's. A column would only be meaningful for tracks added after it. A `plays(path, at)` table would add history; with every column a search field, `added:30d..` and `played:..1y` follow. Album and artist grouping needs a new view in three frontends, and `Frontend` assumes one cursor row per view. A cheaper first step: `:filter album|artist` searches for the cursor row's album or artist. A buyer of music thinks in releases (decision 2), which argues for grouping (inference about users).

- [ ] **Saved searches.** `:save-search NAME` stores the query and sort. The playlists view lists it and runs it on Enter. With field and `is:` search, this gives lists that stay current, such as "120 to 130 BPM" or "damaged files".

- [ ] **Columns on the page.** `[server] columns` is read but not used: the page's rows are a responsive grid, tuned for a phone's two-line rows, and making the columns dynamic means rebuilding that layout. It sorts and shows a fixed set for now.

- [ ] **Advanced search in SQL.** A `:query SELECT path FROM tracks JOIN analysis ...` whose rows become the library listing, as a search's do, for questions the search syntax cannot ask. It must run on a read-only connection and be refused unless it is a `SELECT`: playlists and marks are the only things in the library that cannot be read back from the files. Saved queries would then be smart playlists. Field search, as in `year:1955..1965` or `is:damaged`, covers most of its uses without SQL.

- [ ] **Calibrate the cutoff heuristics.** The two spectral findings were set against ffmpeg's encoders, not files whose provenance is known; a library of lossless files with known sources would settle them. The tempo confidence is calibrated, against librosa; see `docs/dev/analyze.md`.

- [ ] **Tempo above 170 BPM.** The prior still halves it. The reading records the level above as an alternate, which `bpm:` matches, so such a track is found by the tempo it is heard at, but the number playr shows is the halved one. Choosing between the two needs accent or metrical modelling.

- [ ] **Watch for changes.** A scan is manual. `notify` could pick up new files, at the cost of a watcher thread.

- [ ] **Relative rows from 0.1.0.** A 0.1.0 scan with a relative path stored relative rows. Rescanning adds absolute duplicates, and pruning never matches the old rows. A one-off cleanup would delete them, and their playlist entries with them.

### Interface

- [ ] **One list for the selection and the queue.** Both are hand-edited track lists that save as playlists, in two views. Option B in `docs/dev/selection-and-queue.md` merges them. Weigh it with "Multi-row selection", which uses "selection" for rows picked in any view.

- [ ] **Multi-row selection.** Select several rows at once, in the window especially. `dispatch` assumes one cursor row per view, so `Frontend` grows; see `docs/architecture.md`. A cheaper step: `:toggle all` on the listed rows keeps one cursor per view.

- [ ] **Platform directories.** Both interfaces keep the library in `~/.local/share/playr` and settings in `~/.config/playr` on every platform, which is unusual on macOS and Windows. Moving to each platform's directories must move existing libraries.

- [ ] **README as manual.** It is 57 KB and 619 lines; install starts at line 140 and the first command at line 205. Keep the description, install and a ten-line first session. Move the rest to `docs/manual.md`.

### Output

- [ ] **Bit-perfect output.** Select a `hw:` ALSA device so PipeWire cannot resample behind us. Today the `default` device accepts every rate and may convert internally, so "no resampling" means playr does not resample, not that nothing does. Negotiation accepts the 32-bit and 24-bit integer formats such devices offer, and `--device` selects one. A `hw:` device PipeWire holds is reported as busy; PipeWire holds only the device it is playing to, and releases an idle card after a few seconds.

### Server

- [ ] **Tried on a Raspberry Pi.** The arm64 archive's glibc requirement, the ALSA default device from `~/.asoundrc`, the systemd user service with lingering, and the CPU cost of resampling are unchecked on a Pi. `docs/server-guide.md` assumes them.

- [ ] **Tried in TouchOSC.** The generated layout rests on inference for three bindings: a radio sending its segment's index, a received speed scaled back onto the fader, and a dragged fader not moved by the progress sent back. See `docs/dev/server.md`.

- [ ] **Browser tests in CI.** `make page-test` needs an audio device, which runners lack. A null output device for tests would let the release and test workflows run it.

- [ ] **Reordering the selection by dragging.** The web page moves a selected track with its row menu; the window also drags rows.

- [ ] **A hostname for `--osc-reply`.** It takes an IP address, so the tablet needs a fixed one. Resolving a name at startup would allow `ipad.local`.

- [ ] **The terminal alongside the server.** The lock stops `playr` while `playr-server` runs. A `--tui` mode would drain the server's request channel in the terminal's loop, so both control one playback.

- [ ] **A `[server]` table in `settings.toml`.** The server's settings are flags in its systemd unit. No longer blocked: `FRONTEND_TABLES` already names `server`, so the table is passed over by the other two programs. It needs the flags given settings equivalents and a precedence rule, flag over file.

- [ ] **Marks on the page.** The page shows marks as a count, so a mark made on a phone does not appear on the progress bar (inference).

### Formats

- [ ] **Opus surround.** Multistream Opus is rejected at construction. The `opus` crate exposes `MSDecoder`; wiring it up is contained.

- [ ] **Tags and duration for CAF, MKV and WebM.** lofty cannot parse these, so they are indexed under their file names, and Symphonia's Matroska reader reports no duration. Symphonia's own metadata might supply the tags; untested. Duration would otherwise mean decoding each file at scan time.

### Packaging

- [ ] **Man page and shell completions.** The command line is parsed with clap, so `clap_mangen` and `clap_complete` can generate them.

- [ ] **Publishing to crates.io.** Manual, with `cargo publish --workspace`. `playr-gui` and `playr-server` have `publish = false`; decide whether they go to crates.io, and whether the release workflow publishes.

## Low

### Playback

- [ ] **Gapless across a sample-rate change.** A rate change rebuilds the output stream and leaves a gap. Fixing it means resampling both sides to a common rate, which trades a gap for a conversion. Worth a flag, not a default.

- [ ] **Crossfade.** Needs two decoders and a mixer stage. Candidate for dropping: it conflicts with gapless playback and the exact-sample claims.

- [ ] **Pitch-preserving speed.** The README rules it out by definition. For a musician practising against a loop it outranks most items; for a person who samples, varispeed is the right tool. Needs a phase vocoder or a dependency, and a new engine stage. The primary user samples (decision 2), and time-stretching a sample is a DAW's job on an exact slice, so this stays low.

### Sampler

- [ ] **Live marks for sampling tools.** Slices can be exported as files. Handing a marked passage to a running tool, such as SuperCollider, is not done: `docs/sampler.md` weighs a JSON Lines file against OSC.

### Interface

- [ ] **Custom colours.** `dark`, `light` and `system` are fixed sets in `playr::ui::palette` and `playr_gui::palette`. A `[colors]` table would need two kinds of value: ANSI or 256-colour indices in the terminal, RGB in the window. Candidate for dropping.

- [ ] **Terminal background detection.** In the terminal, `system` is the ANSI set, as `dark` is. An OSC 11 query at startup would find a light background in most modern terminals, but can stall over ssh and tmux. `COLORFGBG` is cheaper, and only some terminals set it.

- [ ] **Dim text on Solarized Dark.** `dim` and the selected row's background are ANSI bright black, which Solarized Dark sets to its background colour, so dim text disappears there. Not tested here.

- [ ] **Show when slices are planned.** `enter` plays in three views and writes files in the sampler view when slices are planned. An indicator while slices are planned would reduce the risk.

### Server

- [ ] **One view and cursor per server.** Every tab and device drives the same `Model`, so two people browsing move each other's cursor and view. Accepted for one user. A fix gives each page its own view and cursors, which `dispatch`'s one cursor per view does not allow; see "Multi-row selection" under Interface.

- [ ] **Six open pages per browser.** A browser opens six connections to a host at most, and each page's event stream holds one, so with about six tabs of the same server open, a page's requests wait for a free connection and it stalls. Accepted for one user. Sharing one stream between tabs, with a `SharedWorker`, would lift it.

- [ ] **Multi-zone streaming.** `docs/dev/streaming.md` explores it. Test the single-zone server on a Pi and in TouchOSC first.

### Output

- [ ] **Reopening a device that returns.** A device lost during playback stops it, as does unplugging a USB DAC mid-track; playr stays stopped until restarted. Reopening the same device when it reappears, never falling back to another, would suit `playr-server` on a Pi. Undecided.

- [ ] **Native PipeWire backend.** cpal 0.18 has a `pipewire` feature. Untested here; the ALSA path works, so this is a quality experiment, not a fix.

### Formats

- [ ] **WavPack, WMA, Musepack, APE.** No Rust decoders of any maturity. Each needs either a C binding or an implementation.

- [ ] **DSD (.dsf, .dff).** Three separate pieces: a container reader, a decimation filter to convert 1-bit PDM to PCM, and a decision about DoP. DoP also needs bit-perfect output, which does not exist yet, so this is blocked on that item. SACD rips add DST, a fourth piece.

- [ ] **AAC gapless.** AAC decodes about 1900 frames long because encoder delay and padding are not trimmed. **upstream**, in Symphonia.

- [ ] **Opus in WebM end padding.** 648 frames of padding survive because Matroska does not report it as a packet trim. **upstream**.

- [ ] **Opus in WebM seek precision.** Seeks land 1.5 ms early. Matroska timestamps are whole milliseconds, and Symphonia subtracts the 6.5 ms codec delay in those units. The Opus header gives the delay in samples, which would recover part of it.

### Architecture

- [ ] **Unmaintained dependencies.** `cargo audit` finds no vulnerabilities, but warns that `derivative`, `instant` and `ttf-parser`, pulled in by egui, are unmaintained. Recheck after each egui upgrade. **upstream**.

- [ ] **A playback snapshot in the core.** `Model` in `playr-app` assembles position, levels, marks and the held peak; a frontend that skips `playr-app`, such as a Tauri backend, would repeat it. A `Session::playback()` would serve both. See `docs/architecture.md`.

- [ ] **A `serde` feature on core types.** Step 8 of `docs/architecture.md`, deferred until a Tauri frontend, a daemon or more JSON output needs it. `playr search --json` already fixes a track's field names.

## Not planned

- Cover art, lyrics, online metadata: each breaks the no-network or minimal claim.

- More EQ bands: the EQ affects playback only and reaches no other tool. Persist it and stop.
