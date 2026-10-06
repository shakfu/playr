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

- [ ] **Non-Latin text in the GUI.** egui's default fonts cover Latin, Greek and Cyrillic. Library text in other scripts (tags, titles, paths) renders as boxes. Users with Chinese, Japanese or Korean (CJK) libraries cannot read their tracks in the GUI; the terminal is unaffected. Each script needs a system or bundled font. egui 0.36 shapes text with `harfrust` but has no bidi algorithm, so a title mixing right-to-left and left-to-right text may show out of order (inference).

- [ ] **macOS signing and notarization.** `playr.app` is unsigned, so a downloaded copy is refused until allowed in System Settings. Needs an Apple Developer account and the workflow's secrets.

### Mixer

- [ ] **A headphone meter, and a frame-exact tape Take.** The mixer is built (`docs/dev/mixer.md`, phases 1 to 3). Left: the cue has no meter of its own, and the tape's Take is checked each refresh, about 16 ms, rather than on the frame, which needs a hook in the bus's callback.

## Medium

### DJ decks and tape

- [ ] **A crossfade when a deck's track is replaced.** With strict off, a track picked for a playing deck fades it out over 5 ms, then plays silence while the new track decodes and resamples, which takes seconds (inference). A crossfade needs the new track read before the old one stops, and both in memory. From the review in `docs/dev/261005-review.md`.

- [ ] **Anti-aliasing in the read heads.** The deck and tape heads interpolate with a 4-point Hermite and no anti-alias filter. Above about +16% rate, bright material likely aliases (inference). Measure first: the aliased energy of a sweep at +16%, +50% and the tape's 4x. A fix filters ahead of the head by the rate, or uses a longer windowed-sinc kernel. From the review.

### Playlists

### Playback

- [ ] **Crossfade.** `:crossfade 2` overlaps a track's last 2 s with the next one's first, fading one out as the other fades in. Needs two decoders and a mixer stage in the engine. Tracks at different sample rates cannot share one output stream, so one side must be resampled; see "Gapless across a sample-rate change". A first step without overlap: fade a track's end and the next one's start, with one decoder, reusing the gain ramp at an audition's ends (`Cmd::PlayOnce`). Both change the audio at track boundaries, against gapless playback and exact samples, so both are settings, off by default.

- [ ] **Fine varispeed.** `:speed` parses whole semitones. A semitone is 5.9%, so 120 BPM moves to 127.1 or 113.3 with nothing between. Add `:speed +50c`, and `:tempo 128` on an analysed track to set the ratio that gives 128 BPM. Check first whether the resampler takes an arbitrary ratio.

- [ ] **A key for the EQ.** Every other playback control has one.

### Sampler

- [ ] **Waveform cache.** The sampler keeps one track's `Peaks` and decodes the whole file again on each return to a track. Time `Peaks::read` on a few tracks first; skip this if a read is short. Otherwise keep recent `Arc<Peaks>` in memory, capped at 100 MB and evicting the least recently used. Cap by bytes, not tracks: a 4-minute track is about 17 MB, a 60-minute mix about 250 MB. Check modification time and size on a hit. Optionally read the selection's tracks ahead, so a first visit is fast too. A file cache survives restarts but needs a format, invalidation and cleanup; not worth it for 4 or 5 working tracks.

- [ ] **Preview in the sampler.** Show the waveform of the track highlighted in a list without playing it. The range, marks, `:in`, `:out` and loop all assume the playing track, so decide what each does on a track not playing.

- [ ] **Spectrogram at high sample rates.** The transform is 2048 frames at every rate, so a bin is 21.5 Hz at 44.1 kHz and 94 Hz at 192 kHz, and bass on hi-res files blurs further. Scaling the transform with the rate, 4096 at 96 kHz and 8192 at 192 kHz, keeps about 46 ms and 21 Hz everywhere, at more CPU per read on those files.

- [ ] **Name an edited plan in the Slice drop-down.** After a planned slice is moved or joined, the drop-down still shows the method that made the plan, such as "At onsets". Only the plan line says "edited". Moving the sensitivity slider then plans again and replaces the edits; undo brings them back. An "Edited" entry in the drop-down would say so before the slider moves.

- [ ] **Keep an edited plan across a track change.** Changing tracks drops the planned slices and clears undo, so starts set by hand are lost for good. Options: confirm before leaving a track with an edited plan, or keep each track's plan until it is written or discarded. `:mark-slices` keeps the starts as marks meanwhile, and `:slice marks` plans them again.

- [ ] **Mark labels and export.** `marks` has `path`, `frame` and `rate`, and no text. A label column would allow notes such as "solo" or "break". Export as an Audacity label file or a cue sheet; `docs/guide-sampler.md` already designs a `marks.jsonl` line. Since slices serve any sampler (decision 3), SFZ output may reach the most samplers (inference, not researched).

- [ ] **OP-XY frame counts at other rates.** ConvertWithMoss 20.3.0 resamples a 48 or 96 kHz slice to 44.1 kHz for the OP-XY, but `patch.json` keeps the old counts: `framecount`, `sample.end` and `loop.end` say 24,000 or 48,000 for a file of 22,050 frames. Every playr export keeps its source's rate, so a 48 kHz track hits it. What the device does with an end past the file is X6 in `docs/dev/device_tests.md`. Fix upstream: the report is drafted in `docs/dev/issues/convertwithmoss-issue.md`. Until then `:convert opxy` could resample to 44.1 kHz first, or warn. Found 2026-10-01.

- [ ] **Which samplers read the `smpl` loop.** A range cut whole while it loops carries its loop in a `smpl` chunk since 2026-10-04, in frames, like the cue points. No device or sampler has read one yet; "Which devices read `smpl` loops" in `docs/dev/hardware_samplers.md` stays open, and `docs/dev/device_tests.md` needs a test for it beside the cue point ones.

- [ ] **Refuse or split past a format's limit.** ConvertWithMoss keeps 24 zones for the OP-XY and drops the rest; `:convert` now says so, after the fact. A table of limits per format would let `:convert` refuse first, or write one preset per 24 slices, as AudioHit splits `.ot` files. The OP-XY's is the only limit known; `docs/dev/hardware_samplers.md` lists the devices' slice limits.

### Library

- [ ] **Deduplicate entries.** Scanning one album from two paths adds it twice. `is:duplicate` lists the copies, and `is:duplicate path:DIR` the copies under one folder. Removing them is manual: delete the duplicate folder, then rescan.

- [ ] **Group, a date added, and play history.** `columns` and `sort` order the library by any column, but there is no grouping, and no date added to sort by: the schema has no `added_at`, and `mtime` is the file's. A column would only be meaningful for tracks added after it. A `plays(path, at)` table would add history; with every column a search field, `added:30d..` and `played:..1y` follow. Album and artist grouping needs a new view in three frontends, and `Frontend` assumes one cursor row per view. A cheaper first step: `:filter album|artist` searches for the cursor row's album or artist. A buyer of music thinks in releases (decision 2), which argues for grouping (inference about users).

- [ ] **Columns on the page.** `[server] columns` is read but not used: the page's rows are a responsive grid, tuned for a phone's two-line rows, and making the columns dynamic means rebuilding that layout. It sorts and shows a fixed set for now.

- [ ] **Calibrate the cutoff heuristics.** The two spectral findings were set against ffmpeg's encoders, not files whose provenance is known; a library of lossless files with known sources would settle them. The tempo confidence is calibrated, against librosa; see `docs/dev/analyze.md`.

- [ ] **Tempo above 170 BPM.** The prior still halves it. The reading records the level above as an alternate, which `bpm:` matches, so such a track is found by the tempo it is heard at, but the number playr shows is the halved one. Choosing between the two needs accent or metrical modelling. Until then `:bpm x2` corrects a track by hand.

- [ ] **A beat grid that follows drift.** The grid is one tempo for the whole track. On the user's library, 23 of the 57 tracks with a clear low-band pulse drift over 35 ms between its first and last quarter, mostly sampled hip hop and music played live (`crates/playr-core/tests/grid_report.rs`; "A beat grid" in `docs/dev/analyze.md`). A loop of a few bars does not notice; slicing a whole track at beats, and DJ decks syncing one, would. Options: beat markers, as Mixxx's variable grids, or a grid per section. Measure on music made to a click first: it may need nothing.

- [ ] **Check the grid's level against a reference.** `HALVED_BELOW`, 80 BPM, was set on one library by counting tracks shown at double; no reference tempo was available, since librosa is not installed. Running librosa's `beat_track` over the same files beside `grid_report.rs` would test the limit, and the phase with a better judge than the low-band fold, which a bassline pulls.

- [ ] **Watch for changes.** A scan is manual. `notify` could pick up new files, at the cost of a watcher thread.

- [ ] **Relative rows from 0.1.0.** A 0.1.0 scan with a relative path stored relative rows. Rescanning adds absolute duplicates, and pruning never matches the old rows. A one-off cleanup would delete them, and their playlist entries with them.

### Interface

- [ ] **One list for the selection and the queue.** Both are hand-edited track lists that save as playlists, in two views. Option B in `docs/dev/selection-and-queue.md` merges them. Weigh it with "Multi-row selection", which uses "selection" for rows picked in any view.

- [ ] **Multi-row selection.** Select several rows at once, in the window especially. `dispatch` assumes one cursor row per view, so `Frontend` grows; see `docs/architecture.md`. A cheaper step: `:toggle all` on the listed rows keeps one cursor per view.

- [ ] **Platform directories.** Both interfaces keep the library in `~/.local/share/playr` and settings in `~/.config/playr` on every platform, which is unusual on macOS and Windows. Moving to each platform's directories must move existing libraries.

- [ ] **README as manual.** It is 57 KB and 619 lines; install starts at line 140 and the first command at line 205. Keep the description, install and a ten-line first session. Move the rest to `docs/manual.md`.

- [ ] **Long messages cut off in the terminal.** The bottom line shares its width with the mode and volume. At 100 columns about 58 characters are free, so a ConvertWithMoss warning showed "The preset has 40 regions but the device" and lost "at most 24". The window shows more. Options: wrap onto a second line, or keep the last messages for a `:messages` list.

### Output

- [ ] **Bit-perfect output.** Select a `hw:` ALSA device so PipeWire cannot resample behind us. Today the `default` device accepts every rate and may convert internally, so "no resampling" means playr does not resample, not that nothing does. Negotiation accepts the 32-bit and 24-bit integer formats such devices offer, and `--device` selects one. A `hw:` device PipeWire holds is reported as busy; PipeWire holds only the device it is playing to, and releases an idle card after a few seconds.

### Server

- [ ] **Tried on a Raspberry Pi.** The arm64 archive's glibc requirement, the ALSA default device from `~/.asoundrc`, the systemd user service with lingering, and the CPU cost of resampling are unchecked on a Pi. `docs/guide-server.md` assumes them.

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

### DJ decks and tape

- [ ] **The cue on a 4-channel PipeWire or Pulse sink.** `cue-out 3-4` puts the cue on channels 3 and 4. A sound server advertising 4 or more channels for a stereo device may downmix them into the speakers (inference). Needs a test on such a sink; the fix may be to refuse `3-4` there. The cue's own level is in the Mixer item. From the review.

- [ ] **Commands stuck behind a load.** A `Load` waits in the command ring until the return ring has room, and every command behind it waits too, Pause included, in both engines. It needs the UI to stop draining returns while the ring fills, 16 returns in the looper and 8 in the DJ engine, so it has not been seen. Letting commands pass a waiting `Load` would break their order. From the review.

### Playback

- [ ] **Gapless across a sample-rate change.** A rate change rebuilds the output stream and leaves a gap. Fixing it means resampling both sides to a common rate, which trades a gap for a conversion. Worth a flag, not a default.

- [ ] **Pitch-preserving speed.** The README rules it out by definition. For a musician practising against a loop it outranks most items; for a person who samples, varispeed is the right tool. Needs a phase vocoder or a dependency, and a new engine stage. The primary user samples (decision 2), and time-stretching a sample is a DAW's job on an exact slice, so this stays low.

### Sampler

- [ ] **Live marks for sampling tools.** Slices can be exported as files. Handing a marked passage to a running tool, such as SuperCollider, is not done: `docs/guide-sampler.md` weighs a JSON Lines file against OSC.

- [ ] **ConvertWithMoss options per format.** `:convert` passes none. Maschine writes nothing without `-pMaschineOutputFormat`; 0 and 2 write `.mxsnd`, and what they differ in is open. A fixed table in `convertwithmoss.rs`, not options typed by the user, who would have to learn ConvertWithMoss's names.

- [ ] **Polyend Tracker slices.** ConvertWithMoss keeps only a kit's first slice: a `.pti` instrument holds one sample. The sliced WAV is the right source, as one sample with slice points, which needs a `.pti` writer. [pti-tools](https://github.com/jaap3/pti-tools) builds one (reported).

- [ ] **MPC loops.** ConvertWithMoss writes a looped slice's loop but sets `TriggerMode` 0, one-shot, for any zone on one key without sustain, which every playr slice is, so the loop is ignored (inference from its reader). Only a range sliced whole while it loops is affected. **Upstream** unless ConvertWithMoss has an option for it; not checked.

- [ ] **Kits for exports before 0.15.0.** Those exports have no `.sfz`, so `:convert` refuses them. Their `samples.json` records each slice's frames and loop, enough to write one.

### Interface

- [ ] **Custom colours.** `dark`, `light` and `system` are fixed sets in `playr::ui::palette` and `playr_gui::palette`. A `[colors]` table would need two kinds of value: ANSI or 256-colour indices in the terminal, RGB in the window. Candidate for dropping.

- [ ] **Terminal background detection.** In the terminal, `system` is the ANSI set, as `dark` is. An OSC 11 query at startup would find a light background in most modern terminals, but can stall over ssh and tmux. `COLORFGBG` is cheaper, and only some terminals set it.

- [ ] **Dim text on Solarized Dark.** `dim` and the selected row's background are ANSI bright black, which Solarized Dark sets to its background colour, so dim text disappears there. Not tested here.

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

- [ ] **A DJ tab.** Two decks with beat matching, scoped in `docs/dev/dj-engine.md`. DJing is outside the primary user of decision 2, so a decision comes first. Of its build steps, the grid (2) and the x2 and /2 correction are built; next is step 1, a `playr-dsp` crate holding the looper's read head, ramps and filters, which both would share.

- [ ] **Unmaintained dependencies.** `cargo audit` finds no vulnerabilities, but warns that `derivative`, `instant` and `ttf-parser`, pulled in by egui, are unmaintained. Recheck after each egui upgrade. **upstream**.

- [ ] **A playback snapshot in the core.** `Model` in `playr-app` assembles position, levels, marks and the held peak; a frontend that skips `playr-app`, such as a Tauri backend, would repeat it. A `Session::playback()` would serve both. See `docs/architecture.md`.

- [ ] **A `serde` feature on core types.** Step 8 of `docs/architecture.md`, deferred until a Tauri frontend, a daemon or more JSON output needs it. `playr search --json` already fixes a track's field names.

## Not planned

- Cover art, lyrics, online metadata: each breaks the no-network or minimal claim.

- More EQ bands: the EQ affects playback only and reaches no other tool. Persist it and stop.
