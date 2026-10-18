# Tape machine v2: mute, bounce, sound design, effects, compact lanes, resume

Status: decided 2026-10-12. Steps 1 (Mute, Off, Clear), 2 (persist and resume), 3 (fold and drawer), 4 (per-lane EQ), 5 (delay and reverb buses, tape and DJ decks) and 6 (Bounce, both lengths) built 2026-10-12, unreleased: the plan is built. Follows `tape-lanes.md` ("Four lanes"). Decisions are listed at the end.

## Feedback

From use of the four-lane tape:

1. Clear is useful only on the write track.

2. A Mute is wanted: the lane is dimmed and its head keeps moving.

3. Switching a voice off should stop its head and its DSP.

4. Voice 4 serves two roles:

   - (a) a fourth track. With 3-4 loops playing at once, each lane needs precise EQ, filters and effects to carve and shape the sound.

   - (b) a write track. The usual aim is to record what voices 1-3 play into voice 4. The only way now is to solo voice 4 and adjust sends and feedback. That gives an approximation, not a recording.

5. Delay and reverb are needed on the tape and on the DJ decks.

6. The UI should shrink when parts are unused.

7. The playback tracks' settings should persist. The tape should be offered for resume at startup, as the player is.

## Findings

### Why soloing voice 4 is not a recording

The 2026-10-12 session had voices 1-3 sending 0.20 each, voice 4 soloed, Write on, Feedback 0.90:

| Track | Period |
|-|-|
| Write loop | 9.0 s |
| Voice 1 (23.5-75.0% of 9.0 s) | ~4.6 s |
| Voice 2 (8.5-78.0% of ~24.1 s) | ~16.8 s |
| Voice 3 (whole window) | ~20 s |

Two causes:

- **The periods differ.** Each 9 s pass prints a different slice of voices 2 and 3. At feedback 0.90 a pass fades by 10% per pass. The loop never holds a single take of the mix.

- **Sends ignore the mix.** `Voice::render` takes the send after drive and filter, but before level and pan (`crates/playr-looper/src/tape.rs:572-577`). Filter settings reach the print; level and pan do not. Each send is mapped to the write loop's channels: a mono voice reaches both channels of a stereo loop, and a stereo voice is summed into a mono one.

### Voice off

`Voice::render` returns before reading or advancing once the gate reaches 0 and `on` is false (`tape.rs:531-534`). The head stops and the read, filter and send are skipped. Confirmed in use. `Setting::On` leaves the head's position alone, so a voice switched back on resumes where it stopped.

### Clear on voices 1-3

`Looper::clear` replaces the loop with a silent one of the same size (`looper.rs:533`). On a library loop this only destroys it.

### Voice 4's send

Voice 4 can send into the write loop, which it reads. This is a second feedback path at voice 4's rate and window, beside Feedback.

### What the tape holds between runs

Nothing. `TapeState` (`crates/playr-app/src/tape.rs:269`) lives only in the running `Deck`. The player's resume (`Session::resumable`, `Confirm::Resume` in `model.rs:358`) covers the track and queue, not the tape.

## Proposal

### 1. Clear on the write loop only

Clear shows on voice 4, and on any of voices 1-3 while it shares the write loop. There it clears the write loop, as voice 4's Clear does. Other lanes lose Clear; Mute takes its slot.

#### Why voices 1-3 can share the write loop

Share makes a voice read the write loop instead of a loop of its own. The write loop then has up to four read heads. Each has its own rate, window, direction, filter, level and send.

This gives:

- **Several views of one print.** One head plays the whole loop at 1.00; another plays a short window at -0.50.

- **Multi-head feedback.** A sharing voice's send prints back into the loop it reads, offset by its position and rate. Two or three sharing voices with sends above 0 act as a multi-tap tape echo on the print.

- **Processing a print without changing it.** A sharing voice's filter and level change what is heard of the write loop, not the loop.

Clear on a sharing lane is kept because that lane is a view of the write loop.

### 2. Mute

Mute sets a per-voice gain, combined with the existing solo gain `heard`. The head and DSP keep running. The lane is dimmed.

Muting a voice:

- removes it from what is heard;

- removes it from a bounce (section 4b);

- leaves its send unchanged, as Solo does. Write can then print a voice that is not heard.

### 3. Off

Off stops the head and DSP, as now. New: switching a voice back on puts its head at its window's start, or at the end in reverse, as `Voice::home` picks.

| | Heard | Head moves | Sends | Bounced |
|-|-|-|-|-|
| On | yes | yes | yes | yes |
| Mute | no | yes | yes | no |
| Off | no | no; back to start on On | no | no |

### 4a. Sound design per lane

Each lane gets a high-pass, a low-pass and one peaking bell (frequency, gain, Q), in series after Drive. They replace the single SVF and its LP/HP/BP toggle. Carving overlapping loops usually needs both cutoffs at once, and a bell to cut one resonance.

Built on `playr_dsp::Svf`, which the voice filter and the DJ isolator already use: two instances for HP and LP, and a bell from its band output. Not Faust: `Voice::render` works one frame at a time, and a `compute(1, ...)` call per frame per voice adds call and loop overhead to three `Svf` ticks (not measured).

That makes 5 fields per lane, against 2 now (Filter, Type). The field row has ~120 pt free (`tape-lanes.md`), so this needs section 6.

Rejected:

- The DJ isolator (`playr-dj/src/fx.rs`): 3 fixed bands, built for kills and sweeps, not carving.

- HP and LP without a bell: no way to cut one resonance.

### 4b. Bounce

Bounce records the mix into a loop. It differs from Write in three ways:

1. **Source:** the stereo mix as heard, after level and pan, of the voices that are on and not muted, plus the effect returns (section 5). Sends are not used.

2. **Feedback 0.**

3. **It stops on its own** at the end of its length.

A bounce's loop is stereo. The write loop currently takes voice 1's channel count.

Two lengths, both offered:

- **(a) Into the write loop, one pass of the write window.** A mix source and a one-shot flag on the write head. Voices with other periods are cut mid-period. Built first.

- **(b) Until stopped, into a new write loop.** Captures whole periods of every voice. The buffer is allocated off the audio thread, at a set maximum length, and trimmed on stop.

Rejected: a bounce through Write's controls (sends at 1, feedback 0). Sends come before level and pan, so the result would still differ from what is heard.

### 5. Delay and reverb

Two shared send buses, one delay and one reverb, in `playr-dsp`, used by the tape and the DJ engine.

- **Sends, not inserts:** one delay and one reverb cost the same CPU for 4 voices or 2 decks.

- **Returns** go to the master mix of their engine, and to the write head (below).

- **Delay time:** the library has no BPM tags. On the tape, time can be a division of a voice's window length (window / n). The DJ engine uses ms, or the analysed BPM where one exists.

- **Reverb:** the Faust library's Dattorro plate, `re.dattorro_rev_default`. See "Faust" below. CLAP hosting stays deferred.

- **Per voice:** two send fields (Delay, Reverb), in the drawer (section 6).

Write and Bounce record the returns. A Bounce takes them as part of the mix. Write adds them to the voices' sends, at a level set by one field, Print FX, in voice 4's write row. A voice reading the write loop and sending to an effect closes a loop: write loop, voice, effect, write head. Feedback below 1 and the write head's soft clip bound it.

#### Faust

The delay and reverb are written in Faust and compiled to Rust with `faust -lang rust`. The Faust libraries supply tested reverbs, fractional delay lines and parameter smoothing (`si.smoo`). A hand-written Dattorro plate is about 150 lines of delicate tuning.

Checked with Faust 2.88 (`brew install faust`) on `re.dattorro_rev_default`:

- **Output:** 365 lines, one struct with `init(sample_rate)`, `set_param` and `compute(count, inputs, outputs)`. No runtime dependency.
- **No imports.** The file expects `F32`, `FaustFloat`, `ParamIndex`, `UI`, `Meta` and the `FaustDsp` trait from the module that includes it. A shim module in `playr-dsp` provides them, rather than the `faust-types` crate.
- **Fixed buffers.** Delay lines are arrays in the struct, 128 to 8192 floats each. The struct is boxed and built off the audio thread, as loops are.
- **Licence per function.** The Dattorro reverb is `LicenseRef-STK-4.3`, which is permissive; `re.vital_rev` is `GPL-3.0-only`. Each `.dsp` is checked against the licences its functions declare in the generated `metadata`.

Layout:

- `crates/playr-dsp/faust/*.dsp`: the sources, reviewed as code.
- `crates/playr-dsp/src/faust/*.rs`: the generated Rust, committed, never edited by hand.
- `make faust`: regenerates the `.rs` files. Normal builds and CI do not need Faust.

Committed output over a `build.rs` that runs Faust: a `build.rs` makes every build and contributor depend on the Faust compiler.

Faust's `compute` works on blocks. The buses fit this: voices sum their sends per frame into a block buffer, and each bus processes it once per callback. The per-lane EQ does not (section 4a).

### 6. Compact lanes

`tape-lanes.md` rejected a track-count drop-down: hidden lanes could go on playing and sending. Collapsing keeps a lane's state visible without its controls.

- **Collapsed lane:** one header row (On, Mute, Solo, name) and a thin waveform strip. Empty lanes collapse by default; any lane collapses by hand.

- **Two tiers of fields.** The main row has Window, Rate, Level, Pan and Send. A drawer, opened per lane, has HP, LP, the bell, Drive, Wear, Slew, Xfade, and the Delay and Reverb sends.

- **Voice 4** keeps its write row (Write window, Feedback, Print wear, Print thin, Print FX) in its drawer. Write and Bounce stay in its header.

`ROW` and `SEPARATOR` in the lane layout are measured constants. Both need measuring again after this change.

### 7. Persist and resume

On quit, playr saves the playback tracks' settings and sources. The write loop's audio is not saved: the tracks it was printed from can print it again. Bounce and Save loop remain the ways to keep a print.

#### What is saved

| Part | How |
|-|-|
| Voice settings | Every field of voices 1-4, Mute, Solo, Share, effect sends. Text, in `library.db`, beside the player's resume. |
| Voice source | One of: a track path and range; a library loop's path; the write loop; empty. |
| Write row | Write window, Feedback, Print wear, Print thin, Print FX. Settings only, so the print can be made again. |
| Lane layout | Collapsed lanes and open drawers. |
| Playing | Whether the tape was playing at quit. |

Voice 4 resumes as follows:

- **It read a loaded source** (a track range or library loop) and nothing was printed: that source is loaded again.
- **It held a print:** a silent write loop of its saved length, as the first Write makes one (`tape-lanes.md`, "The write loop"). Voices sharing it read that silent loop.

Not saved: whether a print happened. The second case applies whenever Write was ever on, since a print cannot be told from its source without saving its audio.

#### What resume restores

- Every saved part above.
- Each head at its window's start, as Off then On places it (section 3). Exact head positions are not saved.
- Write **off**. A resumed tape that prints at once overwrites the write loop before the user hears it.
- Playing, if it was playing at quit.

#### When it saves

On quit, and when a voice's source changes (load, eject, share). The second covers playr being killed, as the player's resume does at each track change (`Model::remember_position`). Field changes are saved only on quit: a database write per drag is not worth it.

#### Prompts at startup

| Saved at quit | Prompt |
|-|-|
| Player only | The player's, as now |
| Tape only | The tape's |
| Both | The player's, then the tape's |
| Neither | None |

The player is saved when it has a current track, playing or paused (`Model::remember_position`). The tape is saved when any voice holds a source. Both conditions mean "something to resume", not "playing". Answering yes to the tape restores it playing only if it was playing at quit; otherwise stopped.

**Both saved.** Two prompts in sequence fit the current code. `Model::refresh` already asks a deferred question once `input` is `Input::None` (the draft question), so the tape's prompt can wait the same way. Each answer is independent: yes to the tape and no to the player leaves only the tape playing. No new mixing is needed: the tape already plays on the player's stream through `Sources`.

Rejected: one combined prompt. The two are independent; a user may want the tape back without last night's queue.

#### Failure modes

- **A source file is gone or moved.** Its voice resumes empty, with a notice naming the file. The other voices resume.
- **The device rate changed.** Sources load through the usual load path, which resamples as it does now.
- **The saved state is from an older format.** Fields it lacks take their defaults; it is not rejected.

#### Opt out

A `tape` name for the `persist` setting (`crates/playr-core/src/settings.toml`). Without it, nothing is saved or asked.

## Where the build departs

- **Mute is an M toggle beside S**, not a button in Clear's slot. In Clear's slot, voice 4 and lanes sharing the write loop, which keep Clear, would have had no Mute.
- **Clear on a voice's own loop is refused** from the command line too, with `TapeMessage::OwnLoop`.
- **On while still fading out** (within the 20 ms gate ramp) crossfades the head to its start, over Xfade, instead of stepping.
- **The tape is saved as a `persist` value**, `tape`, so it is saved 0.5 s after any change, on quit and on a window close, not only on quit and source changes. The text is a row per line; paths are kept, not loop ids, so a loop removed from the loop library still resumes from its file.
- **A tape not yet answered for is saved as it was** until the question is answered, so a close before an answer keeps it.
- **A resumed tape that was playing does not pause the player**, where Play does. With both questions answered yes, both play.
- **Windows kept at another rate are scaled** to the device's rate and cut to their loops.
- **A folded voice 4 keeps its button row.** Write makes the write loop of an empty voice 4, so it cannot fold away.
- **Lanes fold by a triangle** at the start of the header's toggle row; **More** at the end of the main row opens the drawer.
- **The EQ is in Hz and dB**, not 0 to 1, with frequencies and Q dragged on a log scale. The bell's Q is scaled by its amplitude, so a cut mirrors a boost. Commands: `hp`, `lp`, `bell`, `bell-gain`, `bell-q`; `filter` is refused with a pointer to them.
- **The layout is in `Deck`**, not GUI state, so the saved tape carries it. An empty lane folds by default only while not folded or unfolded by hand.
- **Bounce (a)** starts where the write head is, not at the window's start, so it begins at once. It fades in over 20 ms and runs 20 ms past its start point fading out, so the join is a crossfade; about a quarter of what the loop held there leaks into the middle of that crossfade. It writes the rolls too while the window is the range, as Write does. A mono write loop gets the mix summed to mono; (b) makes a stereo one. Bounce is beside Record in the tab's bar: beside Write it overflowed the header's column, and on a row of its own it made the open lanes taller than the smallest window.
- **Bounce (b)** records through Record's writer thread to a temporary WAV, and on stop loads it into voice 4 as a loop cut to loop, by the normal load path, over a 115 MB buffer allocated ahead and filled in the callback. It needs no new engine code, and the load swaps the loop off the audio thread. It cannot run while Record does, as they share one recorder. It stops itself 5 minutes after it starts, by the clock, so time paused counts. The WAV is deleted once read, so a resumed tape has a silent write loop of its length, as after a print.

## Order

1. Mute, Off returning to start, Clear on the write loop only. Mute reuses the solo gain.

2. Persist and resume. Built before the fields grow, so each later section adds its fields to the saved state as it lands.

3. Collapse and drawer. Frees room for 4-6.

4. Per-lane HP, LP and bell.

5. Delay and reverb buses in `playr-dsp`, from Faust, tape first, then DJ. Design in `fx-buses.md`.

6. Bounce (a), then (b).

## Decided (2026-10-12)

1. Mute leaves the bounce and keeps its send.

2. Off stops the head; On returns it to the window's start.

3. Clear stays on a lane sharing the write loop.

4. Voice 4's send into its own loop stays.

5. EQ: HP, LP and one bell per lane.

6. Bounce: (a), then (b); both are kept.

7. Write and Bounce record the effect returns.

8. Persist: playback tracks' settings and sources, not the write loop's audio. Voice 4 after a print resumes as a silent write loop.

9. The tape gets its own prompt whenever it was saved, playing or stopped; it follows the player's.

10. Faust output is committed and includes a shim module in `playr-dsp` for the types it expects, not the `faust-types` crate.
