# Delay and reverb buses

Status: built 2026-10-12 on the tape and the DJ decks, unreleased. Step 5 of `tape-machine-v2-design.md`.

## What is decided

From `tape-machine-v2-design.md`, sections 5 and decisions 7 and 10:

- One shared delay and one shared reverb, each a send bus: a send per voice, one return.
- The tape first, then the DJ decks.
- Returns reach the master mix, and the write head and Bounce record them.
- Each bus offers several models, built side by side to be compared by ear. Models that lose are removed after listening.
- Faust models are compiled to Rust. The generated `.rs` is committed, with a shim module in `playr-dsp`; normal builds do not need Faust.
- Rust models may come from `softcut-fx` in [softcut-rs](https://github.com/shakfu/softcut-rs), subject to its licence (see Open).
- Bus controls sit in a foldable FX lane.

## Signal flow (tape)

```
voice i --EQ--> pan, level, mute/solo --+--> mix --------------------------+--> out
                                         |                                  |
                                         +-- x delay send i --> [delay] ----+
                                         +-- x reverb send i -> [reverb] ---+
                                                                |
                         returns x Print FX ----> write head <--+-- voice sends (as now)
```

- **Post-fader sends.** A bus takes the voice as heard: after EQ, pan, level, mute and solo. A muted voice reaches neither bus. This differs from the write-head send, which is pre-level and survives mute (decision 1), because an effect is part of what is heard.
- **Stereo in, stereo out.** The sends are the panned stereo frame, so a voice panned left echoes from the left.
- **Returns** add to the mix before the tape's volume, and so to Record and Bounce.
- **Print FX** (one field in voice 4's write row, 0 to 1) adds the returns, summed to the write loop's channels, to what the write head records. At 0 the write path is as now.
- **Feedback loop.** A voice reading the write loop, sending to a bus, printed back: loop, voice, bus, write head, loop. Feedback below 1 and the write head's soft clip bound it, as they bound the voices' sends now.

## Per-frame or per-block

`Tape::process` runs frame by frame: each frame, every voice renders, then the write head writes. Faust's `-os -ec` options generate a `frame()` for one frame and a `control()` run only when a parameter changes, so the effects run inside that loop. A return printed this frame is written this frame.

The alternative was 32-frame chunks of Faust's block `compute`. It delays the print path by up to 32 frames (0.7 ms at 48 kHz) for a voice reading close behind the write head.

Measured 2026-10-12 on an Apple M1, release build, 48 kHz, 20 s of input, two runs:

| Model | `frame()` ns | `compute(32)` ns a frame | Overhead, % of a core |
|-|-|-|-|
| Ping-pong | 19-20 | 12-13 | 0.035 |
| Tape | 30 | 28-29 | 0.007 |
| Zita | 45-46 | 41 | 0.022 |
| JP | 205-211 | 203-236 | 0.01 or below |
| Plate | 36-41 | 12-13 | 0.134 |

Every overhead is below the 0.5% set before measuring: per-frame it is. JP costs about 1% of a core by itself.

Faust 2.88.0's one-sample Rust code types its outputs `f64`, so the shim sets `FaustFloat` to `f64`; the DSP inside stays `f32`. `-it` puts the tables (oscillators, `ma.primes`) in the struct: without it they are `static RwLock`s locked every frame.

## The delays

Three models, chosen to sound different. Two implementations of one algorithm would compare code, not sound.

| Model | Source | Character |
|-|-|-|
| Clean | `softcut-fx` `Delay`, ported: linear-interpolated ring, time glides over 50 ms | digital repeats |
| Ping-pong | Faust `de.fdelay`, `si.smoo` on its time, cross-fed channels | each repeat crosses sides |
| Tape | Faust: `de.fdelay` read moved by wow (0.5% at 0.6 Hz), flutter (0.1% at 9 Hz) and slow noise; `aa.softclipQuadratic1` in the loop | repeats drift in pitch and saturate |

In every model Tone filters the feedback path only: the first echo is bright, each repeat darker. Faust's `~` delays its loop by a frame, so the Faust lines are a frame short and their outputs a frame late; every echo lands on Time.

Fields shared by every model:

| Field | Range | Default | Does |
|-|-|-|-|
| Model | clean, ping-pong, tape | clean | the algorithm |
| Time | 1 to 2000 ms | 375 | the delay |
| Sync | off, 1/1, 1/2, 1/4, 1/8, 3/16, 1/16 | off | sets Time to that part of voice 1's window |
| Feedback | 0 to 0.95 | 0.4 | how much of each echo repeats |
| Tone | 500 Hz to 20 kHz | 6 kHz | a low-pass in the feedback path |
| Return | 0 to 1 | 1 | the bus's level into the mix |

Tape adds Wow (0 to 1). `softcut-fx` sets damping as 0 to 1, not Hz; the wrapper maps Tone to its one-pole coefficient.

- **Sync** uses voice 1's window: tracks often have no tempo tag, and a loop's window is the one musical length the tape always has. A window change while synced glides the time, which bends the pitch of repeats in flight.
- **Buffer.** 2 s at the highest device rate. Faust sizes delay lines at compile time, so the `.dsp` takes the maximum as a constant: 2 s x 192 kHz = 384,000 frames, 1.5 MB per channel. `softcut-fx` sizes its ring from the rate at construction. All models are boxed and built off the audio thread, in `playr_looper::new`, as the sinc table is.

## The reverbs

| Model | Source | Licence | Rate |
|-|-|-|-|
| Freeverb | `softcut-fx` `Reverb`, ported (Jezar, public domain); 8 combs, 4 allpasses | MIT (author's grant) | scales: lengths x rate / 44.1 kHz |
| Zita | Faust `re.zita_rev1_stereo`; FDN, two-band decay | libraries' LGPL with its exception for generated code (inference: no per-function licence declared) | scales: lengths from `ma.SR` |
| JP | Faust `re.jpverb`; hall with modulation | MIT | does not scale: lengths are `ma.primes(size * k)` samples |
| Plate | Faust `re.dattorro_rev` | `LicenseRef-STK-4.3` (MIT-style) | does not scale: fixed sample counts |

Late-tail decay measured 2026-10-12 (Schroeder integral, -25 to -55 dB, damping 0), against settings of 1 and 3 s:

| Model | 44.1 kHz | 48 kHz | 96 kHz |
|-|-|-|-|
| Zita | 0.99, 2.97 | 0.99, 2.96 | 0.96, 2.96 |
| Plate | within 10% after calibration | same | same |
| JP | 2.3-2.5, 5.7-6.8 | 2.0-2.5, 5.3-6.5 | 0.9-1.4, 2.6-3.2 |

- Zita's early reflections fall faster than its tail: from -5 dB its T20 reads 0.73 s for 1 s.
- Plate: tuned for 29.8 kHz (Dattorro's paper), so it sounds smaller as the rate rises. Its decay is set from the rate: the tank applies the decay gain every 5,397 samples, calibrated to 5,800 against the measured tail.
- JP: lines are `ma.primes(size * k)` samples, not scaled by the rate. The wrapper scales Size by rate / 48 kHz, capped at 5 by its 8192-sample lines. Its decay is approximate, as its documentation says, and the error depends on rate and size: no fixed correction fits. The test checks only that a longer setting gives a longer tail.
- Zita adds `1e-20` in its loop against subnormals, so it never outputs exact zero; the bus's bypass keeps the dry tape bit-exact.

`re.greyhole` (MIT) is left out: it is an echo effect more than a room. `re.vital_rev` and `re.kb_rom_rev1` declare GPL-3.0-only and are excluded. The generated `metadata` names only `maths.lib`'s licence, not each function's: the licences above are from the `declare ... license` lines in Faust 2.88.0's `reverbs.lib`, to recheck when Faust is upgraded.

| Field | Range | Default | Freeverb | Zita | JP | Plate |
|-|-|-|-|-|-|-|
| Model | | Zita | | | | |
| Size | 0 to 1 | 0.5 | size | - | size 0.5 to 5 | - |
| Decay | 0.1 to 20 s | 2 | - | t60 mid | t60 | decay |
| Damping | 0 to 1 | 0.3 | damp | HF t60 ratio | damp | damping |
| Return | 0 to 1 | 1 | | | | |

A field a model lacks is shown disabled for it. Freeverb has no decay time: Size sets its comb feedback.

## Comparing models

- Every model is built at start; only the selected one runs.
- Switching model clears the old tail and starts the new one silent.
- The test harness renders one impulse and one loop through every model at 44.1, 48 and 96 kHz, for listening and for the decay tests.
- After listening, the losing models and their fields are deleted, with a CHANGELOG line naming the winner and why.

## Code layout

- `crates/playr-dsp/faust/pingpong.dsp`, `tape_delay.dsp`, `zita.dsp`, `jp.dsp`, `plate.dsp`: the sources, reviewed as code.
- `crates/playr-dsp/src/faust/mod.rs`: the shim (`F32`, `FaustFloat`, `ParamIndex`, `UI`, `Meta`, the `FaustDsp` trait), `#![allow(...)]` for the generated names, and a small wrapper per bus that names its parameters.
- `crates/playr-dsp/src/faust/*.rs`, one per `.dsp`: generated, never edited.
- `crates/playr-dsp/src/fx.rs`: `Delay` and `Reverb`, each an enum over its models, with one set of fields.
- `make faust`: regenerates them. The command and Faust's version go in the generated header, which Faust writes.

The generated code calls `remainderf` and `rintf` through `unsafe extern "C"` from libm. Both are in the platform's C library on macOS, Linux and Windows (inference for Windows: MSVC's CRT exports both).

## Engine

- `Tape` holds `Box<Delay>` and `Box<Reverb>`, built in `Tape::new`.
- `softcut-fx` processes frame by frame already; per-frame vs chunked applies to the Faust models.
- New settings: `Setting::DelaySend(usize, f32)`, `ReverbSend(usize, f32)`, `Delay(DelayParam, f32)`, `Reverb(ReverbParam, f32)`, `PrintFx(f32)`.
- **Idle buses.** A bus with every send at 0 and a return below -120 dB for 1 s is skipped, and its state cleared; output is then bit-exact with today's (the tests that compare voices to their source keep passing).
- **No allocation** in the callback: `tests/alloc.rs` runs with both buses active.

## Controls

- **Per voice**, in the drawer: Delay and Reverb sends, 0 to 1.
- **The buses:** an FX lane above voice 4, folded by default like an empty lane, with the delay's fields in one row and the reverb's in a second. It folds with the same chevron; `Layout` gains its slot.
- **Commands:** `:tape V delay S`, `:tape V reverb S`; `:tape delay model clean|ping-pong|tape|time MS|sync DIV|feedback F|tone HZ|wow W|return R`; `:tape reverb model freeverb|zita|jp|plate|size S|decay SEC|damp D|return R`; `:tape print-fx P`.
- **Saved tape:** a `delay` and a `reverb` row, and the sends and Print FX in the voice and write rows.

## DJ decks

Built 2026-10-12, on the same `playr_dsp::fx::Buses` as the tape.

- **Sends after the crossfader.** A send per deck takes the deck as it reaches the main mix: after trim, EQ, filter, fader, crossfader and mute. This note first put them before the crossfader. That would send a deck crossfaded out but with its fader up, as one is while cued, so its reverb would reach the main mix. After the crossfader, a fader cut leaves the tail ringing: an echo out.
- **Returns** join the main mix before the master's soft clip, not the headphone cue: the cue hears the deck dry.
- **Sync** is a part of a bar of 4 beats at the played tempo of the deck heard most: playing, with a grid, and the larger of its fader times its side of the crossfader. The app resends the time when it moves by more than 0.5 ms, as rates move with sync and nudges.
- **Controls:** Delay and Reverb sends in each channel strip; the fields in an FX window, as the tab has no room for them at 800 by 592. The fields are the tape's, from one module in each of `playr-app` and `playr-gui`.
- **Not kept** between runs, as the rest of the mixer is not.

## Tests

- **Delay:** an impulse returns at Time, within a frame, at Feedback^k on repeat k; Tone darkens each repeat; Sync tracks a window change; ping-pong alternates sides.
- **Reverb:** an impulse decays to -60 dB within 20% of Decay; Return 0 is silent.
- **Routing:** a muted voice reaches neither bus; Print FX 0 leaves the write loop as now; Print FX 1 prints the returns.
- **Bypass:** sends at 0 give output bit-exact with the dry tape.
- **Allocation:** none in the callback.
- **Generated code:** `make faust` on a clean tree leaves no diff (CI cannot run it without Faust; checked by hand when a `.dsp` changes).

Faust 2.88.0 bugs in its Rust output, worked around:

- `-os` code types its output slots `f64` whatever `FaustFloat` is: the shim sets `FaustFloat = f64`.
- `-ftz 2` negates a float's bit pattern as an integer (`-x.to_bits()`): it panics on `-0.0` in debug and flushes wrong values in release. `make faust` uses `-ftz 1`.

## Decisions (2026-10-12)

1. Per-frame Faust (`-os -ec`), by measurement above.
2. Several models per bus, compared by ear; losers removed.
3. Bus controls in a foldable FX lane above voice 4.
4. Sends post-fader (recommended; awaiting confirmation).

## Open

None. `softcut-fx`'s `Delay` and `Reverb` are ported into `playr-dsp/src/fx.rs` under MIT, by the author's grant (2026-10-12); its `Saturation`, from monome softcut (GPL-3.0), is not used.
