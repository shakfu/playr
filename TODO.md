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

- [ ] **macOS signing and notarization.** `playr.app` is unsigned, so a downloaded copy is refused until allowed in System Settings. Needs an Apple Developer account and the workflow's secrets.

## Medium

### DJ decks and tape

- [ ] **Choose the effects' models.** The delay has 3 models and the reverb 4, on the tape and the decks (`docs/dev/fx-buses.md`). Listen, keep those that sound best, and remove the others and their fields.

- [ ] **Modulation on the tape.** Sources that move a setting over time, for the voices, the EQ and the effects. Open questions:
  - **Sources:** an LFO (sine, triangle, square, sample and hold), synced to voice 1's window as delay Sync is, or free in Hz; an envelope follower on a voice; random walk.
  - **Destinations:** voice rate, window start, pan, level, sends; HP, LP and bell frequency, bell gain; delay time, feedback, tone; reverb damp; Print FX.
  - **Where it runs:** on the audio thread at a control rate, such as every 32 frames, with each value smoothed. A Faust parameter recomputes its coefficients in `control()` on each change; its cost at that rate is to measure.
  - **Controls:** a depth per field, or a small matrix of source to destination. The lanes have little room; the FX lane or a drawer of its own may hold it.
  - **Kept:** in the saved tape, with the settings.

### Playlists

### Playback

- [ ] **Crossfade across sample rates.** `:crossfade` overlaps tracks that share a rate and channel count; others join gaplessly. Overlapping them needs one side resampled to the other's rate; see "Gapless across a sample-rate change".

### Sampler

- [ ] **Which samplers read the `smpl` loop.** A range cut whole while it loops carries its loop in a `smpl` chunk since 2026-10-04, in frames, like the cue points. No device or sampler has read one yet; "Which devices read `smpl` loops" in `docs/dev/hardware_samplers.md` stays open. `docs/dev/device_tests.md` has the test, M8; run it.

### Library

- [ ] **Group, a date added, and play history.** `columns` and `sort` order the library by any column, but there is no grouping, and no date added to sort by: the schema has no `added_at`, and `mtime` is the file's. A column would only be meaningful for tracks added after it. A `plays(path, at)` table would add history; with every column a search field, `added:30d..` and `played:..1y` follow. Album and artist grouping needs a new view in three frontends, and `Frontend` assumes one cursor row per view. `:filter album|artist` is the first step: it searches for the row's album or artist. A buyer of music thinks in releases (decision 2), which argues for grouping (inference about users).

- [ ] **Calibrate the cutoff heuristics.** The two spectral findings were set against ffmpeg's encoders, not files whose provenance is known; a library of lossless files with known sources would settle them. The tempo confidence is calibrated, against librosa; see `docs/dev/analyze.md`.

- [ ] **Tempo above 170 BPM.** The prior still halves it. The reading records the level above as an alternate, which `bpm:` matches, so such a track is found by the tempo it is heard at, but the number playr shows is the halved one. Choosing between the two needs accent or metrical modelling. Until then `:bpm x2` corrects a track by hand.

- [ ] **A beat grid that follows drift.** The grid is one tempo for the whole track. On the user's library, 23 of the 57 tracks with a clear low-band pulse drift over 35 ms between its first and last quarter, mostly sampled hip hop and music played live (`crates/playr-core/tests/grid_report.rs`; "A beat grid" in `docs/dev/analyze.md`). A loop of a few bars does not notice; slicing a whole track at beats, and DJ decks syncing one, would. Options: beat markers, as Mixxx's variable grids, or a grid per section. Measure on music made to a click first: it may need nothing.

- [ ] **Check the grid's level against a reference.** `HALVED_BELOW`, 80 BPM, was set on one library by counting tracks shown at double; no reference tempo was available, since librosa is not installed. Running librosa's `beat_track` over the same files beside `grid_report.rs` would test the limit, and the phase with a better judge than the low-band fold, which a bassline pulls.

### Interface

- [ ] **System fonts on Windows.** The window draws other scripts with system fonts, tested on macOS and Linux. The Windows list of files is untried there. Right-to-left text may show out of order: egui 0.36 has no bidi algorithm (inference).

- [ ] **One list for the selection and the queue.** Both are hand-edited track lists that save as playlists, in two views. Option B in `docs/dev/selection-and-queue.md` merges them. Weigh it with "Multi-row selection", which uses "selection" for rows picked in any view.

- [ ] **Multi-row selection.** Select several rows at once, in the window especially. `dispatch` assumes one cursor row per view, so `Frontend` grows; see `docs/architecture.md`. `:toggle all` is the first step: it selects or unselects every listed row, keeping one cursor per view.

### Output

- [ ] **Bit-perfect output.** Select a `hw:` ALSA device so PipeWire cannot resample behind us. Today the `default` device accepts every rate and may convert internally, so "no resampling" means playr does not resample, not that nothing does. Negotiation accepts the 32-bit and 24-bit integer formats such devices offer, and `--device` selects one. A `hw:` device PipeWire holds is reported as busy; PipeWire holds only the device it is playing to, and releases an idle card after a few seconds.

### Server

- [ ] **Tried on a Raspberry Pi.** The arm64 archive's glibc requirement, the ALSA default device from `~/.asoundrc`, the systemd user service with lingering, and the CPU cost of resampling are unchecked on a Pi. `docs/guide-server.md` assumes them.

- [ ] **Tried in TouchOSC.** The generated layout rests on inference for three bindings: a radio sending its segment's index, a received speed scaled back onto the fader, and a dragged fader not moved by the progress sent back. See `docs/dev/server.md`.

- [ ] **The terminal alongside the server.** The lock stops `playr` while `playr-server` runs. A `--tui` mode would drain the server's request channel in the terminal's loop, so both control one playback.

### Formats

- [ ] **Titles of MKV and WebM files from ffmpeg.** ffmpeg writes a file's title into the segment's Info and a stream's into its track entry, neither of which Symphonia 0.6 exposes; such files show their file names. **upstream**, unless playr reads the EBML itself.

### Packaging

- [ ] **Publishing to crates.io.** Manual, with `cargo publish --workspace`. `playr-gui` and `playr-server` have `publish = false`; decide whether they go to crates.io, and whether the release workflow publishes.

## Low

### DJ decks and tape

- [ ] **The cue on a 4-channel PipeWire or Pulse sink.** `cue-out 3-4` puts the cue on channels 3 and 4. A sound server advertising 4 or more channels for a stereo device may downmix them into the speakers (inference). Needs a test on such a sink; the fix may be to refuse `3-4` there. The cue's own level is in the Mixer item. From the review.

- [ ] **Commands stuck behind a load.** A `Load` waits in the command ring until the return ring has room, and every command behind it waits too, Pause included, in both engines. It needs the UI to stop draining returns while the ring fills, 16 returns in the looper and 8 in the DJ engine, so it has not been seen. Letting commands pass a waiting `Load` would break their order. From the review.

### Playback

- [ ] **Gapless across a sample-rate change.** A rate change rebuilds the output stream and leaves a gap. Fixing it means resampling both sides to a common rate, which trades a gap for a conversion. Worth a flag, not a default.

- [ ] **Pitch-preserving speed.** The README rules it out by definition. For a musician practising against a loop it outranks most items; for a person who samples, varispeed is the right tool. Needs a phase vocoder or a dependency, and a new engine stage. The primary user samples (decision 2), and time-stretching a sample is a DAW's job on an exact slice, so this stays low.

### Sampler

- [ ] **File the OP-XY frame count issue.** `:convert opxy` corrects ConvertWithMoss 20.3.0's counts for 48 and 96 kHz exports itself. The report is drafted in `docs/dev/issues/convertwithmoss-issue.md`.

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

- [ ] **Apple Loops metadata.** `playr loops scan` reads tempo, beats and key from a WAV's `acid` chunk, not from Apple Loops' own metadata in AIFF and CAF. Those files are indexed with tempo from the name or the length. Needs a second parser. See `docs/dev/loop-library.md`.

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

- Cover art, lyrics, online metadata: each needs a service, and playr contacts none.

- More EQ bands: the EQ affects playback only and reaches no other tool. Persist it and stop.
