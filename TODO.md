# TODO

What is missing, grouped by priority:

- **Critical:** loses data, or blocks the next release.

- **High:** a gap most users meet.

- **Medium:** improves an area that already works.

- **Low:** niche, large for its benefit, or blocked **upstream**, which marks items not fixable here without replacing a dependency.

An item citing "decision N" refers to [docs/dev/decisions.md](docs/dev/decisions.md).

## Critical

### Recording

- [ ] **WAV recording past 4 GiB.** `audio/record.rs` writes through hound, whose RIFF sizes are u32. 32-bit stereo passes 4 GiB after 3h06m at 48 kHz, 46 min at 192 kHz. Release writes a wrapped header; debug panics in the writer thread, so `alive` never drops and `finished()` never returns. Stop or roll over before 4 GiB, or write RF64; lower `alive` from a drop guard. Also in `record.rs`: the writer reads `slots() == 0` before `is_abandoned()`, so frames pushed between the two are lost; after a writer error the bus keeps its record producer, and a later Record frees the old one on the audio thread. From the 0.18.0 review.

## High

### Desktop window

- [ ] **macOS signing and notarization.** `playr.app` is unsigned, so a downloaded copy is refused until allowed in System Settings. Needs an Apple Developer account and the workflow's secrets.

## Medium

### DJ decks and tape

- [ ] **Choose the effects' models.** The delay has 3 models and the reverb 4, on the tape and the decks (`docs/dev/fx-buses.md`). Listen, keep those that sound best, and remove the others and their fields.

### Playlists

### Playback

- [ ] **Resampler latency missing from output marks.** `halt`, `wrap` and `stage_from` count `written + carry`, not the frames `Resample` still holds: up to 1252 output frames (26 ms) at 44.1 to 48 kHz, 2302 (48 ms) at 0.5x (measured). Under varispeed or on a resampled device a one-shot's tail lands after `pause_at`, so the fade-out is cut and the pause clicks; loop wraps and gapless changes are reported early by the same amount. Expose the held latency from `Resample` and add it to the mark. Test with a fake backend that offers only 48 kHz. From the 0.18.0 review.

- [ ] **ReplayGain peak of 0 or NaN removes the cap.** `gain.rs:85` caps at `f32::INFINITY` for a tagged peak that is not positive, so `+40 dB` with peak `0` or `nan` plays 100x and hard-clips. Off by default. Treat such a peak as unknown (cap 1.0) and clamp the gain. From the 0.18.0 review.

### Sampler

- [ ] **Which samplers read the `smpl` loop.** A range cut whole while it loops carries its loop in a `smpl` chunk since 2026-10-04, in frames, like the cue points. No device or sampler has read one yet; "Which devices read `smpl` loops" in `docs/dev/hardware_samplers.md` stays open. `docs/dev/device_tests.md` has the test, M8; run it.

### Library

- [ ] **Library cursor kept by index across reloads.** `Event::Scanned`, `Event::Pruned`, analysis re-sorts and `Confirm::ForgetRoot` replace the track list, but `cursors.library` is neither re-anchored nor clamped. After a rescan adds an album that sorts earlier, Enter plays, or the DJ view loads, a different track. Anchor by path after every reload, as `SetSort` does, then clamp. From the 0.18.0 review.

- [ ] **Analysis panics are not contained.** `analysis::run`'s scoped workers have no `catch_unwind`, so one panicking file fails the run before `update_albums`, stays pending, and panics again on every later `:analyze`; album loudness stops updating. Catch per file and store `Analysis::failed`. `Session::spawn` (`session/jobs.rs`) also has no catch, so a panic in plan, write, export, convert, `sql` or open sends no event and leaves the frontend pending. Whether Symphonia panics on malformed input is inference. From the 0.18.0 review.

- [ ] **Group by album or artist.** `columns` and `sort` order the library by any column, and `added`, `played` and `plays` by history, but there is no grouping. It needs a new view in three frontends, and `Frontend` assumes one cursor row per view. `:filter album|artist` is the first step: it searches for the row's album or artist. A buyer of music thinks in releases (decision 2), which argues for grouping (inference about users).

- [ ] **Calibrate the cutoff heuristics.** The two spectral findings were set against ffmpeg's encoders, not files whose provenance is known; a library of lossless files with known sources would settle them. The tempo confidence is calibrated, against librosa; see `docs/dev/analyze.md`.

- [ ] **Tempo above 170 BPM, untagged.** A fast genre tag now shows the level above a halved reading (`FAST_GENRES`). An untagged track still reads at half; telling 87 from 174 without the tag needs accent or metrical modelling, and labelled tracks to calibrate it. `:bpm x2` corrects one by hand.

- [ ] **A beat grid that follows drift.** The grid is one tempo for the whole track. On the user's library, 23 of the 57 tracks with a clear low-band pulse drift over 35 ms between its first and last quarter, mostly sampled hip hop and music played live (`crates/playr-core/tests/grid_report.rs`; "A beat grid" in `docs/dev/analyze.md`). A loop of a few bars does not notice; slicing a whole track at beats, and DJ decks syncing one, would. Options: beat markers, as Mixxx's variable grids, or a grid per section. Measure on music made to a click first: it may need nothing.

- [ ] **Check the grid's level against a reference.** `HALVED_BELOW`, 80 BPM, was set on one library by counting tracks shown at double; no reference tempo was available, since librosa is not installed. Running librosa's `beat_track` over the same files beside `grid_report.rs` would test the limit, and the phase with a better judge than the low-band fold, which a bassline pulls.

### Interface

- [ ] **Terminal sampler panics on short terminals.** `render.rs:298` sets `rows` to `inner.height - 2`, which is 0 for an inner height of 1 or 2, and `braille_rows` then underflows `dots - 1`; the guard at `:294` catches only 0. Braille is the default display, so a 3-5 row tmux split panics while a track plays. No render test is below 30 columns. Return early on `rows == 0` or `width == 0`, and add a size-sweep test. From the 0.18.0 review.

- [ ] **The window clones the measures map each frame.** `views.rs:108`: `model.measures_map().clone()`, one entry per analysed track, on every frame of Library, Selection or Queue: 12.6 ms at 50k entries against a 33 ms frame (measured). `dj.rs` already borrows. Also the spectrogram image is rebuilt and uploaded each frame, about 1.3 MB at 800x400 (inference); cache it. From the 0.18.0 review.

- [ ] **Window controls unreachable by Tab.** Tab is bound to `view next` and the window cancels egui's focus step, so the Forget buttons, the Columns checkboxes and modal buttons are mouse-only. Options: Tab moves focus while a window or modal is open, or Ctrl+Tab cycles views in the window only, which breaks key parity with the terminal. The routing of keys by input state is also duplicated in `src/ui/mod.rs` and `playr-gui/src/lib.rs`; one `Model` method would serve both. From the 0.18.0 review.

- [ ] **System fonts on Windows.** The window draws other scripts with system fonts, tested on macOS and Linux. The Windows list of files is untried there. Right-to-left text may show out of order: egui 0.36 has no bidi algorithm (inference).

### Server

- [ ] **The page can start an unbounded recording.** `web.rs:99` allows `Mix(_)`, which includes `MixAction::Record`: 32-bit float WAV at 1.4 GB/h, enough to fill a Pi's SD card. Add `Mix(MixAction::Record) => false` ahead of it, with a case in the allow-list test. From the 0.18.0 review.

- [ ] **OSC flood.** `playr-server/src/main.rs:158` queues requests on an unbounded `mpsc::channel()`. OSC is unauthenticated, a bundle expands to about 70 messages, and each runs a full `refresh`, so memory grows and HTTP commands time out (inference). Use a bounded `sync_channel` with `try_send`, or coalesce. Also, the 32 connection slots are taken before the Host or token check and held up to 10 s, so one LAN host can hold them all: cap per peer IP and shorten the head deadline. From the 0.18.0 review.

- [ ] **Tried on a Raspberry Pi.** The arm64 archive's glibc requirement, the ALSA default device from `~/.asoundrc`, the systemd user service with lingering, and the CPU cost of resampling are unchecked on a Pi. `docs/guide-server.md` assumes them.

- [ ] **Tried in TouchOSC.** The generated layout rests on inference for three bindings: a radio sending its segment's index, a received speed scaled back onto the fader, and a dragged fader not moved by the progress sent back. See `docs/dev/server.md`.

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

- [ ] **One view and cursor per server.** Every tab and device drives the same `Model`, so two people browsing move each other's cursor and view. Accepted for one user. A fix gives each page its own view and cursors, which `dispatch`'s one cursor per view does not allow.

- [ ] **Six open pages per browser.** A browser opens six connections to a host at most, and each page's event stream holds one, so with about six tabs of the same server open, a page's requests wait for a free connection and it stalls. Accepted for one user. Sharing one stream between tabs, with a `SharedWorker`, would lift it.

- [ ] **Multi-zone streaming.** `docs/dev/streaming.md` explores it. Test the single-zone server on a Pi and in TouchOSC first.

### Output

- [ ] **Reopening a device that returns.** A device lost during playback stops it, as does unplugging a USB DAC mid-track; playr stays stopped until restarted. Reopening the same device when it reappears, never falling back to another, would suit `playr-server` on a Pi. Undecided.

- [ ] **Native PipeWire backend.** cpal 0.18 has a `pipewire` feature. Untested here; the ALSA path works, so this is a quality experiment, not a fix.

### Formats

- [ ] **Apple Loops metadata.** `playr loops scan` reads tempo, beats and key from a WAV's `acid` chunk, not from Apple Loops' own metadata in AIFF and CAF. Those files are indexed with tempo from the name or the length. Needs a second parser. See `docs/dev/loop-library.md`.

- [ ] **WavPack, WMA, Musepack, APE.** No Rust decoders of any maturity. Each needs either a C binding or an implementation.

- [ ] **DSD (.dsf, .dff).** Three separate pieces: a container reader, a decimation filter to convert 1-bit PDM to PCM, and a decision about DoP. DoP also needs bit-perfect output to a `hw:` device, which `:device` can now confirm. SACD rips add DST, a fourth piece.

- [ ] **AAC gapless.** AAC decodes about 1900 frames long because encoder delay and padding are not trimmed. **upstream**, in Symphonia.

- [ ] **Opus in WebM end padding.** 648 frames of padding survive because Matroska does not report it as a packet trim. **upstream**.

- [ ] **Opus in WebM seek precision.** Seeks land 1.5 ms early. Matroska timestamps are whole milliseconds, and Symphonia subtracts the 6.5 ms codec delay in those units. The Opus header gives the delay in samples, which would recover part of it.

### Architecture

- [ ] **A DJ tab.** Two decks with beat matching, scoped in `docs/dev/dj-engine.md`. DJing is outside the primary user of decision 2, so a decision comes first. Of its build steps, the grid (2) and the x2 and /2 correction are built; next is step 1, a `playr-dsp` crate holding the looper's read head, ramps and filters, which both would share.

- [ ] **Unmaintained dependencies.** `cargo audit` finds no vulnerabilities, but warns that `ttf-parser`, pulled in by winit, is unmaintained. Recheck after each egui upgrade. **upstream**.

- [ ] **CI gates test less than they claim.** The msrv job checks only `playr`, `playr-core` and `playr-app` on 1.89, though its comment says every crate but `playr-gui`; `playr-gui`'s 1.95 is never checked. `--no-default-features` is never built, so `opus_reports_cleanly_when_the_feature_is_off` never runs. The release `check` job runs `make test` on Linux only, and a tag does not wait for test.yml on macOS or Windows; call test.yml with `workflow_call`. `--locked` is missing from test.yml and the Makefile. From the 0.18.0 review.

- [ ] **Release supply chain.** Actions are pinned by tag, `dtolnay/rust-toolchain@stable` by branch; pin by SHA. Release assets carry only a `SHA256SUMS` from the same workflow: add provenance (`actions/attest-build-provenance`) or a signature. The ffmpeg and nfpm downloads are not checksummed. No `rust-toolchain.toml`, so a new clippy lint can fail `-D warnings` with no code change. From the 0.18.0 review.

- [ ] **Smaller findings, not rechecked since 0.18.0.** From the review of 2026-10-11; each needs checking against the current code first:

  - U16 output truncates (`output.rs:161`); I16 now rounds. Pause and resume are hard cuts mid-waveform.

  - A mid-stream channel-count change may swap L/R under resampling (`decode.rs`; inference).

  - `update_albums` opens a DEFERRED transaction; under WAL a concurrent commit fails it with `SQLITE_BUSY_SNAPSHOT`. Use `TransactionBehavior::Immediate`.

  - `export_marks` writes `<stem><suffix>` with `fs::write`, so two tracks sharing a stem overwrite each other's file.

  - `forget_root` maps any DB error to `NotARoot`; `check_forget` refuses during a scan but not an analysis.

  - Cue sheets keep the `\\?\` prefix on Windows and emit `TRACK 100` and above; `file:///C:/` M3U URLs read as `/C:/...` on Windows.

  - An unwritable lock file leaves playr running unlocked with no warning (`instance.rs`).

  - History joins on `\n` and does not strip newlines on push (`persist.rs`).

  - `save_values` marks values saved even when `Session::set_state` failed.

  - The Origin check is skipped when Origin is absent; under `--open` that allows `text/plain` form CSRF (inference). Require a custom header on POST.

  - `playr_token` is visible to other services on the same host, since cookies are not isolated by port. Document it.

  - `playr-server.service` listens on `0.0.0.0:8080` with no sandboxing directives.

  - The window's system font fallback can load symbol or emoji faces (Apple Symbols, `seguisym.ttf`, `fc-match`'s first pick).

  - `unsafe impl Sync for SyncDecoder` (`opus.rs:105`) relies on opus 0.4's current `&self` methods under a caret requirement; pin `=0.4.x` or use a `Mutex`.

  - `docs/architecture.md` says "current as of 0.12.0"; README.md:7 says `playr` and `playr-gui` have no network code, though both link zbus on Linux and the window opens a browser.

  - Fixed sleeps that may flake on loaded runners: `playr-core/tests/engine.rs`, `playr-app/tests/model.rs`, `playr-core/tests/modes.rs`.

  - `Model::drain_events` is about 250 lines over six subsystems; split it per subsystem, as `tape::poll` and `dj::poll` are.

- [ ] **A playback snapshot in the core.** `Model` in `playr-app` assembles position, levels, marks and the held peak; a frontend that skips `playr-app`, such as a Tauri backend, would repeat it. A `Session::playback()` would serve both. See `docs/architecture.md`.

- [ ] **A `serde` feature on core types.** Step 8 of `docs/architecture.md`, deferred until a Tauri frontend, a daemon or more JSON output needs it. `playr search --json` already fixes a track's field names.

## Not planned

- Cover art, lyrics, online metadata: each needs a service, and playr contacts none.

- More EQ bands: the EQ affects playback only and reaches no other tool. Persist it and stop.
