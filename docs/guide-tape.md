# The tape looper

The tape plays one loop through three voices at once. Each voice reads the loop at its own rate, forward or in reverse, over its own part of it. A write head can record what the voices play back into the loop, with feedback, so the loop changes on every pass. You can save the loop as a sample, or record what you hear.

The window shows it in the **Tape** tab; the terminal has the `:tape` commands only, listed under [Tape](../README.md#tape) in the README. The design is in [`docs/dev/looper-engine.md`](https://github.com/shakfu/playr/blob/main/docs/dev/looper-engine.md).

## Loading a loop

A loop comes from the playing track, so play a track first. Then either:

- **Pick a loop in a lane**: each lane's list searches the [loop library](../README.md#the-loop-library) as you type, as `/` does in the Loops view, and loads the loop picked into that voice. Each row shows where the loop lies in its file, `1:02.3-1:06.1`, so loops cut from one track can be told apart; the lane shows it under the name. `:tape N loop ID` does the same.

- **Set a range** in the sampler view: `i` and `o` at the playhead, or a drag across the waveform in the window. [`guide-sampler.md`](guide-sampler.md) covers ranges. Then **Load range** in the Tape tab, or `:tape load`, loads it into voice 1.

- **Use a saved loop**: `:tape load N` loads loop slot N, 1 to 8, as F1 to F8 saved it.

The load reads the range, plus up to 1 s of the track before it, the **pre-roll**, and 1 s after it, the **post-roll**. The rolls give the crossfades audio to fade into, so the loop repeats exactly the range's length. A saved loop holds only the range. A loop cut to loop, such as a scanned or saved one, takes its rolls from its own other end: its tail before it and its head after it.

The first load starts the tape from the default settings, with voices 2 and 3 empty. A load into a lane later changes that voice alone. **Play tape** starts it and pauses the player; **Stop tape** stops it. The player does not resume on its own; Play on the player afterwards plays both.

**Take**, or `:tape take`, loads the range and hands the player over to the tape, as a deck's Take does. With the player inside the range, voice 1 starts where the player is and the player pauses. With the player before the range, the tape waits and takes over at the range's start. Past the range, it is refused and the loop stays loaded. The take is checked each time the interface refreshes, so it lands within about 16 ms in the window and 33 ms in the server.

## The tab

`6` or `:view tape` opens it. Its keys, under `[keys.tape]`: `space` plays the tape, `x` stops it, `l` loads the range, `t` takes over from the player, `r` resets the loop, `w` saves it and `R` records the mix.

From top to bottom:

1. **Buttons.** Load range, Play tape, Stop tape, Reset, Record, Save loop.

2. **The write lane.** Write and the write window on the left. On the right, a strip showing the write window on voice 1's loop, with the write head as a line, and Feedback, Wear and Thin under it.

3. **A lane per voice.** On the left: on, the voice's name, **Ping**, **S** for solo, the loop it reads, its window, and **Clear** and **Eject**. On the right, its loop as it now is, redrawn as the write head changes it:

   - The voice's window shaded, its edges marked, and its head as a line. The pre-roll and post-roll are dimmed.

   - Its crossfade curves at the foot.

   - On a lane reading voice 1's loop, the write head as a line, and what the write head cannot change tinted and hatched: everything outside the write window, or the whole loop while writing is off. The write strip labels it **frozen**.

   Under the loop, the voice's controls in one row.

### Editing windows with the mouse

- Drag near an edge of a voice's window, on its own lane, to move that edge; drag between the edges to move the whole window, keeping its length.

- A drag in the write strip edits the write window.

- An edge dropped within 6 points of the range's edge lands on it exactly.

A control that does nothing as the tape is set, such as a voice's Wear while its Send is 0, is dimmed, and its tooltip says why. You can still set it ahead.

## Voices

Voice 1 is on when a loop loads; voices 2 and 3 are off. A voice's lane has its on tick box, its name, **Ping** and **S** for solo, and these controls:

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

Each lane holds a loop of its own, voice 1's loop, or nothing. A loop picked in a lane is loaded into that voice alone: the voice is turned on, and its window becomes the loop's range. Its window and crossfades count in its own loop. In the terminal, `:load N` in the Loops view loads the loop under the cursor into voice N.

Voices 2 and 3 can instead pick **voice 1's loop**, at the top of their list, or `:tape N share`. They then read the loop the write head writes, as heads on one tape do: a load into voice 1 changes theirs too. This is how one voice prints into the loop the others play.

While any voice reads a loop of its own, the write head is held off: its sends would print one loop into another of a different length. Write stays set, and the Tape tab says why it does nothing. An empty voice does not hold it.

A loop is resampled to the tape's rate as it loads. Rates and starts are not matched across voices: set each voice's rate by hand to line up loops of different tempo.

**Reset** restores every voice's loop as loaded. **Save loop** saves voice 1's.

### Clear and Eject

Each lane has two buttons:

- **Clear**, `:tape N clear`, silences the loop the voice reads. Its length, the windows and the heads stay, so the voices play on in silence and the write head can print into it again. A voice reading voice 1's loop clears voice 1's loop, for every voice reading it. **Reset** brings back the loop as loaded.

- **Eject**, `:tape N eject`, empties the voice: it reads nothing, and its lane says empty. Voice 1's Eject empties every voice reading voice 1's loop too. Once every voice is empty, the tape is gone.

Both do nothing on an empty voice.

## The write head

With **Write** off, the default, the loop plays as loaded. With it on, the write head moves through its window at the normal rate and rewrites each frame as:

the loop's old content times **Feedback**, plus every voice's Send.

| row | range | default | does |
|-|-|-|-|
| Window | start and end | the range | the part of the loop that is rewritten; the rest stays as loaded |
| Feedback | 0 to 1 | 1 | how much of the old content survives each pass; 0 replaces it with the sends |
| Wear | 0 to 1 | 0 | a low-pass on everything recorded, so the loop darkens each pass |
| Thin | 0 to 1 | 0 | a high-pass on everything recorded, 20 Hz to 2 kHz, so the loop thins each pass |

With Feedback at 1, Wear and Thin at 0, and every Send at 0, writing changes nothing. A soft clip keeps the loop within full scale when the sends and feedback add up past it. It leaves levels under `tape_knee` in `settings.toml`, -6 dBFS by default, untouched.

The voices read before the write head writes, so on the next pass every voice reading voice 1's loop reads what was printed: a reversed or half-speed voice printed into the loop is played back by the others at their own rates.

## Some starting points

Voices 2 and 3 start empty. For those below that use them, pick **voice 1's loop** in their lists first.

- **A slow echo of itself.** Voice 2 on, Rate -0.5, Send 0.6. Write on, Feedback 0.85. Each pass prints a reversed, octave-down copy into the loop, and older prints fade under the new ones. Send 0 then lets the whole loop fade.

- **A loop that wears out.** Write on, Feedback 0.9, Wear 0.3, every Send at 0. Each pass is a little darker and quieter.

- **A tape stop.** Slew 2000, then Rate 0: the voice slows to a halt over 2 s, pitch falling.

- **Layers in time.** Voice 1 on the whole range, voices 2 and 3 on short windows inside it at rates 2 and 1.5, sends low. Ping on voice 3 keeps its short window from clicking.

- **Back to the start.** **Reset** restores the loop as loaded and puts every head at its window's start, keeping the settings.

## Saving

- **Save loop** writes the loop as it now is, only the range, without the rolls: `samples/<track>-tape/<track>-tape-loop.wav`.

- **Record** starts recording what you hear; pressing it again stops and writes `-mix.wav` beside it. If the disk falls behind, dropped blocks are counted and reported.

Both are 32-bit float WAV, in a new directory each time. The loop is added to the loop library, with the beats of the loop it came from; the recording is added to the library. A saved loop can be loaded, sliced or looped again.

## What it does not do

- **Recording an input.** The tape records only its own voices.

- **Remembering a tape.** The settings last until the next load; only saved files are kept.

- **Surviving a lost device.** If the device goes away, the tape closes, a recording under way is finished, and the loop as written is lost. Load it again.

- **The server.** Its web page does not reach the tape.

- **Known clicks.** A voice that wraps again during a crossfade, as when its rate rises mid-fade, can click.
