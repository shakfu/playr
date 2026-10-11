# The tape looper

The tape is a four-track looper with a write head. Each of four voices plays a loop at its own rate, forward or in reverse, over its own part of it. Voice 4's loop is the write loop: with **Write** on, a write head records every voice's send into it, with feedback, so it changes on every pass. Voices 1 to 3 can play it too. You can save the write loop as a sample, or record what you hear.

The window shows it in the **Tape** tab; the terminal has the `:tape` commands only, listed under [Tape](../README.md#tape) in the README. The design is in [`docs/dev/looper-engine.md`](https://github.com/shakfu/playr/blob/main/docs/dev/looper-engine.md).

## Loading a loop

A loop comes from the playing track, so play a track first. Then either:

- **Pick a loop in a lane**: each lane's list searches the [loop library](../README.md#the-loop-library) as you type, as `/` does in the Loops view, and loads the loop picked into that voice. Each row shows where the loop lies in its file, `1:02.3-1:06.1`, so loops cut from one track can be told apart; the lane shows it under the name. `:tape N loop ID` does the same.

- **Set a range** in the sampler view: `i` and `o` at the playhead, or a drag across the waveform in the window. [`guide-sampler.md`](guide-sampler.md) covers ranges. Then **Load range** in the Tape tab, or `:tape load`, loads it into voice 1.

- **Use a saved loop**: `:tape load N` loads loop slot N, 1 to 8, as F1 to F8 saved it.

The load reads the range, plus up to 1 s of the track before it, the **pre-roll**, and 1 s after it, the **post-roll**. The rolls give the crossfades audio to fade into, so the loop repeats exactly the range's length. A saved loop holds only the range. A loop cut to loop, such as a scanned or saved one, takes its rolls from its own other end: its tail before it and its head after it.

The first load starts the tape from the default settings, with voices 2 to 4 empty. A load into a lane later changes that voice alone. **Play tape** starts it and pauses the player; **Stop tape** stops it. The player does not resume on its own; Play on the player afterwards plays both.

**Take**, or `:tape take`, loads the range and hands the player over to the tape, as a deck's Take does. With the player inside the range, voice 1 starts where the player is and the player pauses. With the player before the range, the tape waits and takes over at the range's start. Past the range, it is refused and the loop stays loaded. The take is checked each time the interface refreshes, so it lands within about 16 ms in the window and 33 ms in the server.

## The tab

`6` or `:view tape` opens it. Its keys, under `[keys.tape]`: `space` plays the tape, `x` stops it, `l` loads the range, `t` takes over from the player, `r` resets the loop, `w` saves it and `R` records the mix.

From top to bottom:

1. **Buttons.** Load range, Play tape, Stop tape, Reset, Record, Save loop.

2. **A lane per voice**, numbered from the bottom: voice 1 at the bottom, voice 4 on top. On the left: the loop it reads, with where it lies in its file; on, the voice's name, **Ping**, **S** for solo; **Clear** and **Eject**, and in voice 4's lane, **Write**. On the right, its loop. The waveforms take the height the controls leave: about 40 points in the smallest window, more in a taller one.

   - The voice's window shaded, its edges marked, and its head as a line. The pre-roll and post-roll are dimmed.

   - Its crossfade curves at the foot.

   - On voice 4's lane, the write window as a band along the top, which drags as a window does.

   - On a lane reading the write loop, the write head as a line, and what the write head cannot change tinted and hatched: everything outside the write window, or all of it while writing is off. Such a lane is redrawn as the write head changes the loop.

   Under the loop, the voice's controls in one row, its window first. Voice 4's lane has a second row for the write head: its window, Feedback, Print wear and Print thin.

### Editing windows with the mouse

- Drag near an edge of a voice's window, on its own lane, to move that edge; drag between the edges to move the whole window, keeping its length.

- A drag in the write strip edits the write window.

- An edge dropped within 6 points of the range's edge lands on it exactly.

A control that does nothing as the tape is set, such as a voice's Wear while its Send is 0, is dimmed, and its tooltip says why. You can still set it ahead.

## Voices

Voice 1 is on when a loop loads; the others are off until a loop is loaded into them, and voice 4 also when Write makes its loop. A voice's lane has its on tick box, its name, **Ping** and **S** for solo, and these controls:

| row | range | default | does |
|-|-|-|-|
| Window | start and end | the range | the part of the loop the voice repeats |
| Rate | -4 to 4 | 1 | frames read per frame heard; negative plays in reverse, 0 holds the head still |
| Slew | 0 to 10,000 ms | 20 | how long a rate change takes; long slews bend the pitch, as a tape speeding up or slowing down |
| Level | 0 to 1 | 1 | how loud the voice is heard |
| Pan | -1 to 1 | 0 | left to right |
| Send | 0 to 1 | 0 | how much of the voice the write head records; apart from Level, so a voice can be recorded without being heard |
| Wear | 0 to 1 | 0 | a low-pass on what the voice sends, so it darkens what it prints a little more each pass |
| Drive | 0 to 1 | 0 | saturation on what the voice reads: quiet material up to 12 dB louder, loud material held down |
| Filter | 0 to 1 | 1 | the filter's cutoff, 20 Hz to 20 kHz on a log scale |
| Type | LP, HP, BP | LP | low-pass, high-pass or band-pass. A low-pass at 1 or a high-pass at 0 changes nothing |
| Xfade | 0 to 1000 ms | 10 | the crossfade at each wrap |

Drive and the filter shape both what is heard and what is sent. Changes to the rows move over 20 ms, so a drag does not click; Rate moves over Slew.

In the terminal, a window takes a time from the range's start, `1.5`, or a part of the range, `25%`: `:tape 3 window 25% 75%`. Below 0% or past 100% reaches into the pre-roll or post-roll.

### Wrapping, and Ping

At the end of its window a voice wraps to the start, crossfading over Xfade. The leaving head fades out into the audio after the window, so each pass is still exactly one window long; where there is no room, the fade is cut short. Each lane draws its voice's curves.

**Ping** makes the voice turn at its window's edges and play back the other way instead, so nothing jumps and nothing crossfades.

### Pan and solo

On a stereo loop, pan keeps the near channel and folds the far one into it, so a hard pan keeps both channels' content. A mono loop pans with equal power.

**Solo** on any voice silences the voices without it. What every voice sends to the write head is unchanged, so a soloed voice can be heard while the others go on printing into the loop.

## A loop per voice

Each lane holds a loop or nothing. A loop picked in a lane is loaded into that voice alone: the voice is turned on, and its window becomes the loop's range. Its window and crossfades count in its own loop. In the terminal, `:load N` in the Loops view loads the loop under the cursor into voice N.

Voices 1 to 3 hold loops nothing writes. Voice 4's loop is the write loop: a loop loaded there is written over while Write is on, and **Reset** brings it back as loaded. With Write off, the four lanes are four loops.

Voices 1 to 3 can instead pick **voice 4's loop**, at the top of their list, or `:tape N share`. They then play what the write head prints, at their own rates: a second play head on the same tape. Voice 4 plays it too, at its own rate.

A loop is resampled to the tape's rate as it loads. Rates and starts are not matched across voices: set each voice's rate by hand to line up loops of different tempo.

**Reset** restores every voice's loop as loaded. **Save loop** saves voice 1's.

### Clear and Eject

Each lane has two buttons:

- **Clear**, `:tape N clear`, silences the loop the voice reads. Its length, the windows and the heads stay. A voice reading voice 4's loop clears it. **Reset** brings back the loop as loaded.

- **Eject**, `:tape N eject`, empties the voice: it reads nothing, and its lane says empty. Voice 4's Eject empties the voices reading its loop too, and turns Write off. Once every voice is empty, the tape is gone.

Both do nothing on an empty voice.

## The write head

With voice 4 empty, the first **Write** makes its loop: silent, the length of voice 1's loop, with the same rolls. Voice 1 must then hold a loop. With Write on, the write head moves through its window at the normal rate and rewrites each frame of the write loop as:

the write loop's old content times **Feedback**, plus every voice's Send.

Every voice sends, whatever loop it reads.

| row | range | default | does |
|-|-|-|-|
| Window | start and end | the write loop's range | the part of the write loop that is rewritten; the rest stays as it is |
| Feedback | 0 to 1 | 1 | how much of the old content survives each pass; 0 replaces it with the sends |
| Wear | 0 to 1 | 0 | a low-pass on everything recorded, so the loop darkens each pass |
| Thin | 0 to 1 | 0 | a high-pass on everything recorded, 20 Hz to 2 kHz, so the loop thins each pass |

With Feedback at 1, Wear and Thin at 0, and every Send at 0, writing changes nothing. A soft clip keeps the loop within full scale when the sends and feedback add up past it. It leaves levels under `tape_knee` in `settings.toml`, -6 dBFS by default, untouched.

The voices read before the write head writes, so on the next pass every voice reading the write loop reads what was printed: a reversed or half-speed print is played back at that voice's own rate.

## Some starting points

Voices 2 to 4 start empty.

- **A slow echo.** Voice 1 on a loop, Send 0.6. Write on, which makes voice 4's loop and turns it on. Voice 4 Rate -0.5, Send 0.6; Feedback 0.85. Voice 4 plays back each print reversed and an octave down, and prints that again, so older prints fade under the new ones. Send 0 on both lets the write loop fade.

- **A print that wears out.** After a few passes of the echo, every Send at 0, Feedback 0.9, Wear 0.3. Each pass of the write loop is a little darker and quieter.

- **A tape stop.** Slew 2000, then Rate 0: the voice slows to a halt over 2 s, pitch falling.

- **Layers in time.** Voice 4 on a loop, Write on. Voices 2 and 3 pick voice 4's loop, on short windows inside it at rates 2 and 1.5, sends low. Ping on voice 3 keeps its short window from clicking.

- **Back to the start.** **Reset** restores every voice's loop as loaded, silences the write loop, and puts every head at its window's start, keeping the settings.

## Saving

- **Save loop** writes the write loop as it now is, only the range, without the rolls: `samples/<track>-tape/<track>-tape-loop.wav`, named after voice 4's loop, or voice 1's when Write made it.

- **Record** starts recording what you hear; pressing it again stops and writes `-mix.wav` beside it. If the disk falls behind, dropped blocks are counted and reported.

Both are 32-bit float WAV, in a new directory each time. The loop is added to the loop library, with the beats of the loop it came from; the recording is added to the library. A saved loop can be loaded, sliced or looped again.

## What it does not do

- **Recording an input.** The tape records only its own voices.

- **Remembering a tape.** The settings last until the next load; only saved files are kept.

- **Surviving a lost device.** If the device goes away, the tape closes, a recording under way is finished, and the loop as written is lost. Load it again.

- **The server.** Its web page does not reach the tape.

- **Known clicks.** A voice that wraps again during a crossfade, as when its rate rises mid-fade, can click.
