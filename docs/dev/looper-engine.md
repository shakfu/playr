# Looper engine

Status: built, unreleased. Where the build departs from this design, and how each open question was settled, is at the end.

A loop made in the sampler plays through 3 voices at once. Each voice reads the same audio at its own rate and direction, over its own part of the loop. A write head records the voices back into the loop with feedback, so each pass changes the loop's content. The mix plays live, can be recorded, and the loop itself can be saved as a sample. The engine lives in a new crate, `playr-looper`; the window shows it in a Tape tab, and `:tape` commands reach every setting.

## Scope

In:

- 3 voices reading one shared loop buffer.

- Per voice: on or off, rate (negative plays in reverse), loop window (start and end inside the loop), level, pan, send and crossfade time.

- A crossfade at each wrap of a voice's window.

- One write head at rate 1 that records the voices' sends back into a window of the buffer, with feedback and wear.

- Saving the loop buffer, and recording the mix, to the samples directory and the library.

Out, though softcut has them:

- A write head per voice moving at the voice's rate, which needs a resampler on the write side.

- Recording input from a device.

- Filters other than the wear low-pass, the voice-to-voice feedback matrix, fade-shape choices, rec-once, phase quantization, effects, OSC.

## License

softcut-rs is GPL-3, as a port of monome's softcut-lib. playr is MIT. So:

- `playr-looper` does not depend on `softcut` or any crate of softcut-rs.

- The implementation is written from the ideas below, which are general DSP, not from softcut-rs source. Whoever writes it does not read `softcut-rs/softcut/src` or `softcut-rs/softcut-demo/src` while doing so. This design was written from softcut-rs's README alone.

- The ideas used: a read head moving at a fractional rate with interpolation; two heads crossfading with equal-power gain at a loop wrap; one write head recording a mix back into a loop with feedback, as a multi-tap tape delay does; commands to the audio thread over a single-producer single-consumer (SPSC) ring.

This is a working reading of the license, not legal advice.

## Crate

`crates/playr-looper`, depending only on `cpal`, `rtrb` and `hound`, all workspace dependencies already.

It takes audio as interleaved `f32` frames at the device's rate. It neither decodes nor resamples: `playr-core` already does both, and the caller hands the looper a ready buffer. So `playr-looper` does not depend on `playr-core`, and `playr-core` does not depend on it. `playr-app` uses both.

Three layers, so the DSP is testable without a device:

| Layer | Holds | Runs on |
|-|-|-|
| `Tape` | the loop buffer, the voices, the write head and its filters; `process(&mut [f32])` | any thread; tests call it directly |
| `Looper` | the `Tape`, the command ring, the status atomics, the recording ring | the cpal callback |
| `Handle` | the sending end of the command ring, the status reader, the recording writer thread | the caller's thread |

## Output device

Proposed: the looper opens its own cpal output stream on the device the player uses, and `playr-app` pauses the player while the looper plays.

Alternative: mix the looper into the player's `render` callback (`crates/playr-core/src/audio/output.rs:561`). One stream, so no device conflict, and the looper would pass through the EQ, the meter and the volume. Against: it puts a second source in the most timing-sensitive code in playr, and `playr-core` would depend on the looper or grow a generic source hook.

The own-stream design fails when the device is held exclusively. That is the case for a `hw:` ALSA device, which the "Bit-perfect output" TODO wants. With PipeWire, CoreAudio and WASAPI in shared mode, two streams on one device work. The failure is reported as an error; nothing falls back to another device.

The stream opens at the loop's source rate when the device supports it. Otherwise `playr-app` resamples the buffer once, with `playr-core`'s resampler, before handing it over.

## Voice

State: a position in frames (`f64`), and a second head while a crossfade runs.

| Setting | Range | Meaning |
|-|-|-|
| `rate` | -4.0 to 4.0 | Frames advanced per output frame; negative reads backwards. 0 holds the head still |
| `window` | start < end, inside the buffer | The part of the loop the voice repeats |
| `level` | 0 to 1 | What is heard of the voice, before pan |
| `pan` | -1 to 1 | Equal-power pan |
| `send` | 0 to 1 | What is recorded of the voice. Independent of `level`, so a voice can be recorded without being heard |
| `fade` | 0 to 1000 ms | Crossfade time at a wrap |

- **Interpolation.** 4-point cubic Hermite. Linear interpolation is cheaper but dulls the top end at rates far from 1.

- **Wrap.** When the head reaches the window's end (start, in reverse), a second head starts at the other end. The two crossfade with equal-power gain over `fade`. A window shorter than twice `fade` shortens the fade to half the window.

- **Smoothing.** Changes to `rate`, `level`, `pan` and `send` move there over 20 ms, so a slider drag does not click. A changed window takes effect at the next wrap, or at once with a crossfade if the head is outside it.

## Write head

The write head moves through its window at exactly 1 frame per output frame, and wraps to the window's start, so it always lands on a whole frame and needs no interpolation. At each frame, after the voices have read:

```
buffer[w] = clip(dc(lowpass(buffer[w] * feedback + sum(send_i * voice_i))))
```

| Setting | Range | Meaning |
|-|-|-|
| `write` | on or off | Off leaves the buffer unchanged: plain playback |
| `window` | start < end, inside the buffer; the whole buffer by default | The part of the loop that is rewritten. Outside it, the loop stays as loaded |
| `feedback` | 0 to 1 | How much of the old content survives each pass. 0 replaces it with the sends |
| `wear` | 0 to 1 | The low-pass cutoff, from off at 0 to 500 Hz at 1, on a log scale. The filter runs on every pass, so old content darkens with each one |

- **Content changes.** A reversed or half-speed voice is printed into the loop, and on the next pass every voice reads that material at its own rate.

- **Ordering.** Voices read before the write head writes. A voice at rate 1 on the write head's frame hears the loop as it was one pass ago.

- **Runaway gain.** Three sends at 1 with feedback near 1 give a loop gain above 1. `clip` passes samples below 0.5 unchanged and bends larger ones towards 1 with a `tanh` curve, so the buffer stays within full scale.

- **DC.** Repeated writes accumulate any DC offset. `dc` is a one-pole high-pass at 10 Hz.

- **Bypass.** With every send at 0, nothing new enters the loop and it cannot grow, so `dc` and `clip` are bypassed. With wear 0 the low-pass is too. Otherwise `clip` would bend every peak above 0.5 on every pass, and `dc` would thin the low end, even at feedback 1.

- **Window edges.** Inside the window the content changes; outside it does not, so a voice reading across an edge would meet a step. The write blends from the old content to the new over the window's first 10 ms, and back over its last 10 ms. A window shorter than 20 ms gets no blend.

- **Moving the window.** A write head outside the new window jumps to its start. Frames left outside keep what was written there.

- **Reset.** Restores the buffer as loaded, from a copy kept on the `Handle` side, and the heads to their windows' starts.

## Commands and status

- `Cmd` is a `Copy` enum: one variant per setting, plus `Play`, `Stop`, `Reset`, `Record(bool)`, `Snapshot` and `Load(buffer)`. Commands cross an `rtrb` ring of 256, and a full ring returns the command to the sender.

- `Load` moves in a `Box<[f32]>`, which the callback then owns and writes. The callback must not free memory, so the replaced buffer goes back to the `Handle` on a second ring, and is dropped there.

- Status is published once per callback as atomics: each voice's position, the write head's position, and a peak level.

- **Waveform.** The buffer changes every frame, so the window cannot draw it from a copy made at load. The callback keeps peaks for a grid of 512 columns: each frame written raises its column's peak, and the write head resets a column as it enters it. The peaks are published as atomics, so the drawing is at most one pass old.

The callback does not lock or allocate, as the player's `render` does not.

## Saving

Two things can be saved:

- **The loop.** The buffer as it is, exactly the loop's length: the better sample for a hardware sampler.

- **The mix.** What is heard, for as long as it runs.

**The loop.** Copying 46 MB inside one callback would overrun it, so the copy is spread over many callbacks, into a buffer the `Handle` allocates and sends in with `Snapshot`. The copy starts at the write head and runs through the write window at 8 frames per output frame, faster than the head, then wraps to the window's start. Each frame is therefore copied before the head rewrites it in this pass. Frames outside the window are never written, so they are copied last. The copy is the buffer as it was when `Snapshot` arrived, with no seam, and needs no pause in writing.

**The mix.** While recording, the callback copies each block of the mix into a ring holding 2 s. A writer thread on the `Handle` side writes it to a WAV. If the writer falls behind, blocks are dropped and counted, rather than the callback waiting. The count is shown when recording stops.

Both are written as 32-bit float, to `samples/<track>-tape-N/`, as exports go under `Session::samples_dir`, and added to the library.

## In playr

- **Source.** The range of the playing track, read through `playr-core` as slices are. Alternatively one of the saved loop slots.

- **Actions.** `Action::Tape(TapeAction)` in `playr-app`, with commands such as:

  ```
  :tape load          :tape play          :tape stop
  :tape 2 rate -0.5   :tape 3 window 25% 75%
  :tape 2 send 0.6    :tape feedback 0.85 :tape wear 0.3
  :tape write off     :tape window 0 50%  :tape save
  :tape rec
  ```

- **Window.** A Tape tab: the loop's waveform with each voice's window and head, the write window and head, one row of controls per voice, a row for the write head, and Play, Stop, Reset, Record and Save. It must fit 800 points wide.

- **Terminal.** Commands only in the first version; see open question 4.

- **Server.** Not in the first version.

- **Player.** Paused when the looper starts. It does not resume on its own.

## Tests

`process` is deterministic, so most tests need no device:

- At rate 1, with writing off, the output equals the window's frames. At rate -1 it equals them reversed.

- A wrap with `fade` set changes no sample by more than the material's own largest step. Without a fade it can.

- With feedback 1, every send 0 and wear 0, a buffer peaking at 0.95 is bit-exact after 10 passes.

- With every send 0 and wear 0, each pass scales the buffer by `feedback`.

- With feedback 0 and one voice at rate -1 and send 1, after one pass the first half of the buffer is the original second half reversed.

- With wear set, energy above the cutoff falls with each pass.

- With 3 sends at 1 and feedback 1, no sample of the buffer exceeds 1 over 100 passes.

- A constant offset written into the buffer decays towards 0.

- A rate change moves over 20 ms, with no step larger than at a steady rate.

- With a write window, frames outside it are bit-exact after 10 passes, at any feedback and send.

- After 10 passes, no step across a window edge is larger than the material's own largest step.

- A snapshot taken while writing equals the buffer at the moment `Snapshot` arrived, with the write window at the buffer's end and at its middle.

- A recording equals the blocks `process` produced, plus the count of dropped blocks.

- `Load` returns the old buffer to the `Handle`.

The cpal layer needs an audio device, which CI runners lack. It is tested by hand, as `make page-test` is.

## Steps

1. The crate: the voices, the write head, `Tape::process`, and the offline tests.

2. The cpal layer, the command ring, the status atomics and the peak grid.

3. The loop snapshot, and recording the mix with its writer thread.

4. `playr-app`: the actions and `:tape` commands, loading from the range, saving to the samples directory and the library.

5. The window's Tape tab, and the parity test.

6. CHANGELOG, README and `docs/architecture.md`.

## Open questions

1. **Device sharing.** Own stream, which fails on an exclusive device, or mixed into the player's callback.

2. **Rate units.** A free ratio, as softcut uses, or semitones as `:speed` uses. Free ratios allow detuned voices; semitones match the rest of playr. A third option: semitones on the Tape tab, any ratio in `:tape N rate`.

3. **Recording format.** 32-bit float avoids clipping, but some hardware samplers do not read it (decision 3). 24-bit with a limiter is the other option.

4. **Terminal.** `View::ALL` is shared by both frontends. Either the terminal draws a text Tape view, or `View` gains a window-only view and the terminal's tab cycling skips it.

5. **Buffer length.** A 60 s stereo loop at 96 kHz takes 46 MB as `f32`, and the snapshot needs a second buffer as large. Cap the loop length, or accept it.

6. **Voice count.** Fixed at 3, or 2 to 4.

7. **Track playback during the tape.** Paused, as proposed, or kept playing on the same device. Keeping it needs the mixed design of question 1.

## How the open questions were settled

1. Own stream, as proposed.

2. A free ratio, -4 to 4, in `:tape N rate` and on the Tape tab's slider. Semitones on the tab are not built.

3. 32-bit float.

4. Neither option: the Tape tab is the window's own, not a `View`. Keys and commands keep acting in the view under it, and a key that changes the view leaves it. `View::ALL`, the terminal and the server are unchanged.

5. Accepted; no cap.

6. Fixed at 3.

7. Paused.

## Where the build departs

- **`Cmd` is not `Copy`.** `Load` carries a `Box`. The settings are a separate `Copy` enum, `Setting`, inside `Cmd::Set`.

- **No `Cmd::Reset`.** `Handle::reset` sends `Load` with the copy it keeps, and `Load` puts every head at its window's start.

- **More is smoothed.** Voice and write on/off, feedback and wear move over 20 ms as well. A feedback change otherwise leaves a step in the loop itself.

- **Peak grid.** A column takes its new peak when the write head leaves it, not a reset when it enters, so the column does not dip while the head crosses it.

- **Output is stereo.** Loops have 1 or 2 channels; a source with more keeps its first two. A mono loop pans with `sqrt(1 - p)`, `sqrt(1 + p)`. A stereo loop keeps the near channel and pans the far one into it with `cos`, `sin` of `p * pi/2`, so a hard pan keeps both channels' content; a balance law would drop one. Correlated channels panned hard sum to up to twice their level, 3 dB more than the mono law. Both are unity at the centre, so a voice at rate 1 plays its frames exactly.

- **Wear per voice.** Each voice has a low-pass on its send, mapped as the write head's wear. It leaves what is heard alone and builds up each pass the voice records the loop again, so voices can wear the loop at different rates. The write head's wear stays: with every send at 0 it is the only way to darken the loop. `Setting::VoiceWear`, `:tape V wear W`.

- **Pre-roll and post-roll.** A load reads up to 1 s of the track before the range and after it (`ROLL` in `playr_app::tape`), cut where the track ends. `Loop::range` marks the range in the buffer; every window starts as it, and Start and End count from it. Save loop writes the range alone, so the sample is still exactly the loop's length.

- **Where a crossfade reads.** As designed, the leaving head fades out past its window's edge, which keeps each pass exactly one window long. The design read wrapped audio when a window met the buffer's edge, which for a whole-loop window is the new head's own audio, a +3 dB bump at every wrap. `crossfade` now chooses per wrap: past the edge when the post-roll there holds the fade; else the new head starts early in the pre-roll before the other edge, reaching it as the old head reaches the end, so the pass is still one window long; else the fade is cut to the longer of the two, to 0 when there is neither. Nothing is read beyond the buffer. An in-window crossfade was rejected: the overlap shortens each pass by the fade, which moves a loop cut to a bar off tempo.

- **Crossfades are drawn** in the Tape tab's lanes where `crossfade` puts them, with the pre-roll and post-roll dimmed, so what a wrap reads beyond the window shows.

- **Fade cap.** A fade is at most `window / (2 * max(|rate|, 1))` frames, so it ends before the next wrap at rates above 1.

- **A head that has not moved** goes to its window's end when its rate turns negative, so a reversed voice starts at the end, not one frame into the start.

- **Recording** takes blocks only while the tape plays. A snapshot is returned as aborted by a `Load` or a new write window.

- **Returned memory.** The callback leaves a command in the ring until the return ring has room for what it returns, so it never has to drop one.

- **Directories** are `<track>-tape`, `<track>-tape-2` and so on, from `samples::unused_dir`, not `<track>-tape-N` from 1. A save or a recording takes a new one each time.

- **Library.** `Session::add_to_library` adds the one file without recording its directory as a root.

- **A new load starts from the defaults.** It builds a new looper at the new loop's rate.

- **Known limits.** A wrap during a crossfade, as when the rate rises mid-fade, drops the fading head, which can click. The first frame after every send reaches 0 can step by up to about 0.07, as the clipper is bypassed at once. Equal-power crossfades of correlated material can step up to 1.41 times the material's largest step (inference, not measured).
