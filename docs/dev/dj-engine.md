# DJ engine

Status: built, through tier 1; see "Build order". Claims marked (inference) were not checked against a source or a test.

Two decks play library tracks at once, each at its own rate. A beat grid per track lets one deck follow the other's tempo and phase. A small mixer blends them to one output. The engine lives in a new crate, `playr-dj`; the window shows it in a DJ tab, and `:dj` commands reach every setting.

## Fit with playr

Decision 2 names the primary user: a buyer of music who listens and samples. DJing is not in it. This tab widens playr's scope, so it needs its own decision before it is built.

Two things argue for it:

- It plays the user's own files, offline. "Contacts nothing" holds.
- The beat grid it needs also serves the sampler: `:slice beats N` takes its phase from a mark, which a grid would supply.

Against: two decks, a mixer and a sync engine roughly double the real-time audio code playr maintains (inference, from the size of `playr-looper`).

## Scope

The first version beat-matches two tracks and mixes them. Tiers follow mainstream software; the split is a judgement, not a standard.

Tier 0, the first version:

- Per deck: load a library track, play/pause, a CDJ-style cue, a rate fader, a temporary nudge, the effective BPM, and a beat-phase indicator.
- A constant-tempo beat grid per track: BPM and the time of one beat. Analysed once, stored in `library.db`, correctable by hand.
- Sync: tempo sync, with half and double tempo matched, and a one-shot phase alignment on sync.
- Quantize: play and cue land in phase with the other deck.
- Mixer: per-deck gain, channel faders, a constant-power crossfader.
- Headphone cue as split cue: main in one ear, the cued deck in the other, on one stereo output. Without it, a mix cannot be checked by ear before the crowd hears it (inference).

Tier 1, after the first version works:

- A 3-band isolator EQ per deck, with kills.
- A one-knob filter per deck: low-pass left of centre, high-pass right.
- Hot cues (4 per deck), beat-jump, and an auto loop of N beats.
- A continuous phase lock, so a synced deck stays in phase without a re-sync.
- Cue on channels 3 and 4 of a multichannel device.
- A crossfader curve setting.

Out:

- Key lock (tempo change without pitch change). See "Key lock".
- Variable-tempo grids, for live drummers. Mixxx and rekordbox default to constant grids for drum-machine music and offer variable grids as an option ([Mixxx beat detection](https://manual.mixxx.org/2.5/ro/chapters/preferences/beat_detection)).
- Downbeat detection. Downbeat accuracy trails beat accuracy and varies across datasets ([Beat This! paper](https://arxiv.org/pdf/2106.08685)). The user marks the first downbeat instead.
- Key detection, effects, slip mode, scratching, more than 2 decks, controllers and MIDI, a booth output, recording the mix (the looper's recorder could be reused later).

## License

Mixxx is GPL-2 ([repository](https://github.com/mixxxdj/mixxx)). Its beat tracker, qm-dsp, is GPL-2 or later ([qm-dsp in Ardour](https://git.ardour.org/ardour/ardour/src/commit/3bf7c4ef49f4c271512f3d3eeb4b83df76f78649/libs/qm-dsp)). Rubber Band is GPL-2 or commercial ([license](https://breakfastquay.com/rubberband/license.html)). aubio-rs is GPL-3 ([lib.rs](https://lib.rs/crates/aubio-rs)). playr is MIT. So, as for the looper:

- `playr-dj` depends on none of them.
- The ideas used are general and published: beat distance as a fraction of a beat, leader and follower, a proportional phase trim with a deadband, half and double tempo matching, a grid as one anchor plus a tempo. They are written from papers and manuals, not from Mixxx's source. The Mixxx constants quoted below are starting points for tuning, not code.
- librosa is ISC ([repository](https://github.com/librosa/librosa)); its beat tracker implements Ellis (2007) and may be read.

This is a working reading of the licenses, not legal advice.

## Crate

`crates/playr-dj`, depending on `cpal` and `rtrb`, both workspace dependencies. Like `playr-looper` it neither decodes nor resamples: `playr-app` hands it ready buffers at the device's rate. It does not depend on `playr-core`.

The same three layers as the looper, so the DSP is testable without a device:

| Layer | Holds | Runs on |
|-|-|-|
| `Mixer` | two `Deck`s, the sync state, the mixer, the cue bus; `process(&mut [f32])` | any thread; tests call it directly |
| `Engine` | the `Mixer`, the command ring, the status atomics | the cpal callback |
| `Handle` | the sending end of the ring, the status reader | the caller's thread |

A `Deck` holds a whole track in memory as interleaved `f32` at the device's rate, a read position as `f64` frames, a rate, a cue point and a grid. In-memory tracks give instant cue, seek and beat-jump with no read-ahead logic. The cost is 92 MB per deck for 4 minutes of stereo at 48 kHz. A chunked streaming reader is the alternative if that proves too much (open question 1).

### Shared DSP

Built as step 1; see "Build order". `playr-looper` already had the pieces a deck needs: a fractional read head with cubic Hermite interpolation (`Loop::read`), linear ramps (`Ramp`), a state-variable filter (`Svf`) and one-pole filters. All are private. Rather than copy them, move them to a small `playr-dsp` crate that both depend on (open question 2).

## Output device

As for the looper: `playr-dj` opens its own cpal stream on the player's device, and `playr-app` pauses the player while a deck plays. Two shared-mode streams on one device already work for the looper. An exclusively held device, such as an ALSA `hw:` device, fails, as it does for the looper.

The cue bus needs the main mix and the cued deck on separate channels:

1. **Split cue**, the first version: on a stereo device, left carries the main mix summed to mono and right the cued deck summed to mono. The sides swap with a setting; Mixxx and mixers disagree on which is which ([Mixxx manual](https://manual.mixxx.org/2.4/en/chapters/user_interface), [DJM-450 layout](https://virtualdj.com/manuals/hardware/pioneer/djm450/layout/hp.html)).
2. **Channels 3 and 4**, tier 1: a device that offers 4 output channels takes the main mix on 1 and 2 and the cue on 3 and 4. One device has one clock, so the two outputs cannot drift. cpal gives a channel count, not routing; the engine writes 4-channel interleaved frames ([cpal](https://docs.rs/cpal/latest/cpal/)). This is untested on real interfaces.
3. **Two devices**, out: their clocks drift, and Mixxx resamples the second output to cope ([Mixxx blueprint](https://blueprints.launchpad.net/mixxx/+spec/better-soundcard-sync)). On macOS an Aggregate Device with drift correction does this in the OS ([Apple](https://support.apple.com/en-ae/102171)).

## Beat grid

A grid is a tempo `bpm: f64` and an anchor `t0`, the time in seconds of one beat, ideally a downbeat. Beat `n` is at `t0 + n * 60 / bpm`. A constant grid suits drum-machine music, which is what DJs mostly mix (inference).

### Analysis

`playr-core` already estimates tempo (`analysis/tempo.rs`): spectral flux, then autocorrelation with a log-Gaussian prior at 120 BPM after Ellis (2007). It returns BPM, a confidence and a doubled alternate, but no phase, and it discards its novelty curve after `finish()`. `estimate(novelty, fps)` is public. The grid analysis extends it:

1. **Tempo.** As now. Then refine it to 0.01 BPM by searching near the estimate for the period whose comb over the novelty curve scores highest. Why 0.01: a grid wrong by `d` BPM slips `d` beats a minute. At 128 BPM, 0.01 BPM is about 5 ms a minute and 23 ms over 5 minutes, near audible flam (inference, from the arithmetic).
2. **Phase.** The comb's best offset gives `t0` to one hop, 11.6 ms. Refine it with `samples::onsets`, which works at a 2.5 ms hop: take the median offset of onsets that lie near a grid beat.
3. **Downbeat.** Not analysed. `t0` is a beat; the user shifts it by whole beats to a downbeat.

Ellis's dynamic-programming beat tracker ([paper](https://hajim.rochester.edu/ece/sites/zduan/teaching/ece472/reading/Ellis_2007.pdf), [FMP notebook](https://audiolabs-erlangen.de/resources/MIR/FMP/C6/C6S3_BeatTracking.html)) is the alternative to steps 1 and 2: it finds individual beats, and a least-squares line through them gives the grid. It handles a weak or late first beat better but costs more code. Build the comb first and add DP if the tests below show it is needed (open question 3).

Known limits, carried over from the tempo estimate (`docs/dev/analyze.md`):

- A pulse above about 170 BPM reads at half tempo. The deck offers x2 and /2.
- Of tracks above the confidence threshold, 60% match librosa, 28% sit at a metrical ratio and 12% are unrelated. The library tested was mostly ambient and electronic. DJ material should score better, being drum-led (inference, untested).
- A track with no clear pulse gets no grid. It plays, but sync and quantize are off for it.

### Storage

New columns in the `analysis` table: `grid_bpm REAL`, `grid_t0 REAL` and `grid_edited INTEGER`. The analyser version rises to 3. `playr analyze` and `:analyze` fill them; a deck loading an unanalysed track analyses it on the load thread, which adds a few seconds to the first load (inference).

A grid edited by hand is kept across re-analysis, as `bpm_tag` overrides `bpm` today. Hand edits:

- x2 and /2. Built 2026-10-04 as `:bpm x2|/2|reset`, in `tempo_fix`; a deck would use the same.
- Shift `t0` by one beat either way, to set the downbeat.
- Nudge `t0` by 1 ms steps.
- Tap tempo, for a track the analysis misses.

## Transport

Per deck:

- **Rate.** A fader over +/-8%, +/-16% or +/-50%, the ranges Serato offers ([Serato forum](https://serato.com/forum/discussion/943445)). Rate is varispeed: pitch moves with tempo, as on a turntable, and as `:speed` does for the player. The read head moves `rate` frames a frame and interpolates.
- **Nudge.** While held, rate is multiplied by 1.04 or 0.96 (inference: a typical bend). With phase lock on (tier 1), a nudge instead shifts a user phase offset, which the lock keeps. Mixxx keeps such an offset ([bpmcontrol.cpp](https://github.com/mixxxdj/mixxx/blob/main/src/engine/controls/bpmcontrol.cpp)).
- **Cue**, as a CDJ does ([Digital DJ Tips](https://www.digitaldjtips.com/gated-vs-normal-hot-cues/)):
  - paused: CUE sets the cue point here;
  - held from the cue point: plays, and returns there and pauses on release;
  - playing: CUE returns to the cue point and pauses.
- **Play** from the cue point, or from where it paused.

Every gain and the rate change over one block, not at once, so nothing clicks; the looper's `Ramp` does this already.

## Sync

With two decks, the leader is the other deck: pressing SYNC on deck B makes B follow A. There is no election; Mixxx needs one for 4 decks and a master clock ([enginesync.cpp](https://github.com/mixxxdj/mixxx/blob/main/src/engine/sync/enginesync.cpp)).

Per deck, with native tempo `bpm_n`, rate `r` and position `p` in track seconds:

- beat period `T = 60 / bpm_n`, in track time;
- effective tempo `bpm_e = bpm_n * r`;
- phase `phi = frac((p - t0) / T)`, in [0, 1).

**Tempo sync** sets the follower's rate to `r_F = bpm_e,L / (m * bpm_n,F)`. `m` is 0.5, 1 or 2, whichever brings `m * bpm_n,F` nearest the leader's tempo in log terms, so a 87 BPM track can follow a 174 BPM one. If `r_F` lies outside the fader's range, the range widens to the next one, or sync refuses if none fits.

**Phase sync** happens once, on SYNC. The phase error is `e = wrap(phi_L - phi_F)`, in [-0.5, 0.5) beats. The follower moves by `e * T_F` track seconds, at most half a beat, with a 5 ms declick crossfade. While the follower's rate then equals the leader's, the two stay aligned as long as both grids are right.

**Quantize**, when on, applies the same correction to play and cue: the deck starts at once, at the position in phase with the leader.

**Phase lock**, tier 1: a proportional trim keeps the follower in phase continuously. `r_F = base * (1 + clamp(k * e, +/-c))`, with Mixxx's tuning as a start: a deadband of 0.01 beat, `k` = 0.7, `c` = 5% ([bpmcontrol.cpp](https://github.com/mixxxdj/mixxx/blob/main/src/engine/controls/bpmcontrol.cpp)). It also hides grid errors smaller than the trim can absorb. The trim moves pitch slightly, since rate is varispeed.

Position stays exact over a long mix: the deck holds it in `f64` frames and computes phase from it each block, rather than adding up per-block changes.

Traktor's master clock, which both decks follow, is the alternative ([Traktor manual](https://www.native-instruments.com/ni-tech-manuals/traktor-pro-manual/en/global-concepts)). It extends to MIDI clock and Ableton Link, but with two decks a deck-to-deck leader is simpler (inference).

## Mixer

Per deck: read, rate, then gain (a trim, +/-12 dB), the EQ and filter (tier 1), then the channel fader. Then the crossfader blends the decks.

- **Crossfader**: constant power, `gA = cos(x * pi/2)`, `gB = sin(x * pi/2)` for `x` from 0 to 1. Two different tracks are uncorrelated, so their summed power stays constant; a linear fade dips at the centre ([Sound On Sound](https://www.soundonsound.com/node/4920442)). Tier 1 adds a sharp curve that cuts only near the ends.
- **EQ**, tier 1: Linkwitz-Riley crossovers at 246 Hz and 2.5 kHz, Mixxx's defaults ([Mixxx equalizers](https://manual.mixxx.org/2.4/it/chapters/preferences/equalizers)). Each band is scaled, then the bands are summed; a kill sets a band's gain to 0. The bands sum flat in level, with an all-pass phase shift. The EQ is bypassed at unity, so a flat deck stays exact.
- **Filter**, tier 1: the looper's state-variable filter, low-pass left of centre and high-pass right, bypassed in a deadband around the centre.
- **Master**: the sum, through the looper's soft clip, then playr's volume.

`playr-core`'s player EQ (RBJ shelves at 100 Hz, 1 kHz and 10 kHz) is a listening EQ with no kill. The decks do not reuse it.

## Key lock

Out of the first version. Varispeed changes pitch by about a semitone at a 6% rate change (inference: 2^(1/12) = 1.059). DJs mixing far-apart tempos turn key lock on, so the deck's read stage sits behind a trait, so a time-stretcher can replace it later. Candidates:

| Option | License | Rust | Note |
|-|-|-|-|
| signalsmith-stretch | MIT, C++ | `signalsmith-stretch` 0.1.3 (bindgen), `ssstretch` 0.1.0 (cxx) ([docs.rs](https://docs.rs/signalsmith-stretch)) | 120 ms blocks at its default preset ([source](https://github.com/Signalsmith-Audio/signalsmith-stretch)); needs a C++ compiler, no system library |
| `timestretch` | MIT, pure Rust | 0.15.0 ([docs.rs](https://docs.rs/crate/timestretch/latest)) | claims a DJ focus, 12.7 ms key-lock latency and no allocation; unverified |
| Bungee | MPL-2.0 | `bungee-rs` 0.1.1 | file-level copyleft, usable in an MIT work ([MPL FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/)) |
| SoundTouch | LGPL-2.1 | none mature | static linking into a Rust binary is awkward |

A stretcher adds latency, about 100 ms at signalsmith's default (inference, from its block size). The deck would offset its displayed position by it and feed the stretcher ahead after a seek, so cue stays immediate.

## In playr-app

Mirrors the tape:

- `crates/playr-app/src/dj.rs`: a `Decks` type like `tape::Deck`. It loads a track on a thread, decoding with `samples::read_frames` and resampling once with `playr-core`'s `Resample` to the device's rate. It reads or makes the grid, then hands the buffer to the engine.
- `Action::Dj(DjAction)`, `Frontend::dj`, `Model::decks`.
- Loading a track pauses nothing; playing a deck pauses the player, as `:tape play` does.

Commands, a sketch:

```
:dj a load              the cursor row onto deck A
:dj a play|cue|sync     :dj b rate 2.5        :dj b nudge +|-
:dj a range 8|16|50     :dj quantize on|off   :dj cue a|b|off
:dj a gain G            :dj a level L         :dj xfade X
:dj a grid x2|/2|<|>    :dj a grid offset MS  :dj a tap
```

The terminal gets the commands only, as for the tape. A text DJ view is possible later, but sync by ear needs the cue bus, not a screen.

## DJ tab

Window-only, opened as the Tape tab is, and fitting 800 by 592:

- **Waveforms.** Two zoomed waveforms, one per deck, stacked so their beat grids line up, with the playhead fixed at the centre. Under them, an overview per deck with the cue point. `wave::Peaks::from_interleaved` builds these from the deck's buffer, so no second decode is needed.
- **Decks.** A strip per deck, as on the Tape tab: play, cue, sync, a rate slider with its range, nudge, the BPM, and a phase meter showing the deck's beat against the leader's.
- **Mixer.** Gain and channel faders between the decks, the crossfader under them, and the cue buttons.
- **Grid.** Grid edits sit by the deck's waveform: x2, /2, shift a beat, nudge, tap.

Loading a deck: a library row's context menu gets "Load to deck A" and "Load to deck B".

## Tests

Offline, on `Mixer::process`, as the looper's are:

- At rate 1 a deck plays its buffer exactly; at rate r it covers r frames a frame.
- After tempo sync, the effective tempos match to 1e-9. After phase sync, the beats of two click tracks at different tempos coincide to within 1 frame, and still do 5 minutes later.
- Half and double: an 87 BPM track syncs to a 174 BPM leader at rate 1.
- Quantized play starts in phase.
- The cue state machine, step by step.
- The crossfader keeps the summed power of two uncorrelated noises within 0.1 dB across its travel.
- The EQ at unity is exact; a kill takes its band down by at least 40 dB.
- Split cue puts the mono main on one side and the mono cue on the other.

Analysis, on synthetic click tracks at known tempos and offsets:

- 128.00 BPM with the first beat at 0.137 s: the grid is within 0.01 BPM and 3 ms.
- The same with noise and a kick, hat and bass pattern.
- 174 BPM reads as 87 with 174 offered as the alternate; x2 corrects it.

On real music, a report like `playr analyze --report`: the grid against librosa's `beat_track` on the user's library. That library is all lossy AAC with no BPM tags, which is fine for this.

## Build order

1. `playr-dsp`: move the looper's read head, `Ramp`, `Svf` and one-pole filters into it. The looper's tests still pass. Built 2026-10-04, after step 6, as `Ramp` (over `f32` and `f64`), `Svf` (tuned in Hz, any Q, with its all-pass), `one_pole`, `clip` and the Hermite kernel. Each read head stays in its crate: the looper's wraps at the loop's ends, the deck's reads silence past the track's.
2. Grid analysis in `playr-core`, its columns and its tests. Built 2026-10-04 as the comb search in "Analysis", with the onset refinement replaced by a 2.9 ms envelope in the same pass, at the faster level where the estimate was halved. `:slice beats N` takes its tempo from it, not its phase. See "A beat grid" in `docs/dev/analyze.md`.
3. `playr-dj`: `Deck` (load, play, rate, cue), the mixer, `Engine` and `Handle`, the cpal stream, split cue. Offline tests.
4. Sync, phase sync and quantize, with their tests. Steps 3 and 4 built 2026-10-04, before step 1, with these departures:
   - `playr-dj` has its own `Ramp`, read head and soft clip. Its read head gives silence outside the track; the looper's wraps. The filters that should stay one are tier 1, so the move to `playr-dsp` waits for tier 1.
   - Tempo sync stays on: the follower tracks the leader's fader, not its nudge, until its own fader moves. One deck is synced at a time. Phase sync is still one-shot.
   - Rate and gain changes take 10 ms, not one block. Sync and quantize align the heads as if every rate ramp had finished. Aligning the current heads left a 512-frame ramp's half-sum, several frames, as phase error.
   - A playing deck refuses a load.
   - With quantize on, a cue point set while paused falls on the deck's own nearest beat.
5. `playr-app`: `Decks`, `DjAction`, the `:dj` commands, and the parity test.
6. The DJ tab, with its fit test at 800 by 592. Steps 5 and 6 built 2026-10-04, with these departures:
   - Hand edits go in a `grid_edit` table beside `tempo_fix`, not in `grid_*` columns of `analysis`, whose rows analysis replaces. x2 and /2 write `tempo_fix`, so the tempo column follows. `Session::grid` gives the grid a deck uses: the edit, else the analysis's, times the octave fix.
   - A track never analysed starts a background analysis on load, as `:slice beats` does, rather than analysing on the load thread. The deck plays meanwhile and takes the grid when it lands.
   - Tap: two taps keep the grid's tempo and put a beat at the last; four or more set the tempo from the mean interval, in track time.
   - The app takes a track as loaded once the engine accepts it. Checking the status first missed a deck started since the last callback.
   - Added: `pause`, `cue down|up` for a held CUE, `sync off`, `grid reset`. Load reads the cursor row of the library or the selection.
7. Tier 1, item by item. Built 2026-10-04, with these departures:
   - The EQ's crossovers are Linkwitz-Riley 8th order, not 4th. With LR4, a mid kill leaves the low and high bands about 40 dB down each at the band's centre, about 34 dB summed, short of the 40 dB the tests ask. The low band passes the high crossover's all-pass, so the bands sum flat. While every band is at unity the EQ does not run; moving one fades it in over 10 ms.
   - Hot cues are kept with the track in a `hot_cues` table, in seconds, so they hold at any device rate.
   - A synced deck that starts, by Play or a hot cue, starts in phase, as with quantize on. The lock then holds the phase it starts at, or where a nudge leaves it.
   - Beat-jump moves a loop playing with the head. Loops take 1/4 to 32 beats, from the beat before the head with quantize on.
   - The sharp curve holds both decks at full level and fades one out over the last 5% of the travel.
   - The stream takes 4 channels or more where the device offers them, so `:dj cue-out 3-4` needs no reopening. On a stereo device it is refused.

Steps 1 and 2 are useful on their own, so the work can stop after either.

## Open questions

1. **Memory.** Whole tracks in memory cost 92 MB per deck for 4 minutes at 48 kHz, more for long mixes. Store `f32`, store `i16` to halve it, or stream in chunks. Proposed: `f32` at first, measure, then decide.
2. **Shared DSP.** A `playr-dsp` crate, or `playr-dj` depending on `playr-looper` for its primitives, or a copy. Proposed: `playr-dsp`. A copy forks two filters that should stay one.
3. **Grid method.** A comb search first, or Ellis's DP tracker from the start. Proposed: the comb, with DP added if the analysis tests or the library report show a need.
4. **Headphone cue in tier 0.** Proposed: yes, as split cue. Without it, tier 0 can beat-match only by sync, never by ear.
5. **Key lock.** Out, behind a trait. When it comes in: `timestretch` if its claims survive a benchmark, else `signalsmith-stretch`, which needs a C++ compiler in the build.
6. **Where the decks play.** Their own stream, pausing the player, as the looper does; or mixed into the player's callback. Proposed: their own stream. The player's 2 s ring would add 2 s to every cue and nudge.
7. **Decision 2.** Record DJing as a secondary use, so later trade-offs know where it ranks.
