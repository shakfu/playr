# The tape looper

The tape plays one loop through three voices at once. Each voice reads the loop at its own rate, forward or in reverse, over its own part of it. A write head can record what the voices play back into the loop, with feedback, so the loop changes on every pass. You can save the loop as a sample, or record what you hear.

The window shows it in the **Tape** tab; the terminal has the `:tape` commands only, listed under [Tape](../README.md#tape) in the README. The design is in [`docs/dev/looper-engine.md`](https://github.com/shakfu/playr/blob/main/docs/dev/looper-engine.md).

## Loading a loop

A loop comes from the playing track, so play a track first. Then either:

- **Set a range** in the sampler view: `i` and `o` at the playhead, or a drag across the waveform in the window. [`guide-sampler.md`](guide-sampler.md) covers ranges. Then **Load range** in the Tape tab, or `:tape load`.

- **Use a saved loop**: `:tape load N` loads loop slot N, 1 to 8, as F1 to F8 saved it.

The load reads the range, plus up to 1 s of the track before it, the **pre-roll**, and 1 s after it, the **post-roll**. The rolls give the crossfades audio to fade into, so the loop repeats exactly the range's length. A saved loop holds only the range.

A new load replaces the tape and starts from the default settings. **Play tape** starts it and pauses the player; **Stop tape** stops it. The player does not resume on its own.

## The tab

From top to bottom:

1. **Buttons.** Load range, Play tape, Stop tape, Reset, Record, Save loop.

2. **The waveform**, of the loop as it now is, redrawn as the write head changes it:

   - A strip at the top for the **write window**, with the write head as a line.

   - The waveform, with the selected voice's window shaded and its edges marked. The pre-roll and post-roll are dimmed.

   - Under it, a **lane** per voice showing its window, its crossfade curves, and its head.

   - What the write head cannot change is tinted, hatched and labelled **frozen**: everything outside the write window, or the whole loop while writing is off.

3. **A strip per voice, and one for the write head**, side by side.

### Editing windows with the mouse

- Click a voice's name, or its lane, to select it. The waveform then shows and edits its window.

- Drag near an edge of the window to move that edge; drag between the edges to move the whole window, keeping its length.

- A drag in a voice's lane edits that voice; a drag in the top strip edits the write window.

- An edge dropped within 6 points of the range's edge lands on it exactly.

A control that does nothing as the tape is set, such as a voice's Wear while its Send is 0, is dimmed, and its tooltip says why. You can still set it ahead.

## Voices

Voice 1 is on when a loop loads; voices 2 and 3 are off. A voice strip's header has its on tick box, its name, **Ping** and **S** for solo. Its rows:

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

Drive and the filter shape both what is heard and what is sent. Changes to the rows move over 20 ms, so a slider drag does not click; Rate moves over Slew.

In the terminal, a window takes a time from the range's start, `1.5`, or a part of the range, `25%`: `:tape 3 window 25% 75%`. Below 0% or past 100% reaches into the pre-roll or post-roll.

### Wrapping, and Ping

At the end of its window a voice wraps to the start, crossfading over Xfade. The leaving head fades out into the audio after the window, so each pass is still exactly one window long; where there is no room, the fade is cut short. The lanes draw the curves.

**Ping** makes the voice turn at its window's edges and play back the other way instead, so nothing jumps and nothing crossfades.

### Pan and solo

On a stereo loop, pan keeps the near channel and folds the far one into it, so a hard pan keeps both channels' content. A mono loop pans with equal power.

**Solo** on any voice silences the voices without it. What every voice sends to the write head is unchanged, so a soloed voice can be heard while the others go on printing into the loop.

## The write head

With **Write** off, the default, the loop plays as loaded. With it on, the write head moves through its window at the normal rate and rewrites each frame as:

the loop's old content times **Feedback**, plus every voice's Send.

| row | range | default | does |
|-|-|-|-|
| Window | start and end | the range | the part of the loop that is rewritten; the rest stays as loaded |
| Feedback | 0 to 1 | 1 | how much of the old content survives each pass; 0 replaces it with the sends |
| Wear | 0 to 1 | 0 | a low-pass on everything recorded, so the loop darkens each pass |
| Thin | 0 to 1 | 0 | a high-pass on everything recorded, 20 Hz to 2 kHz, so the loop thins each pass |

With Feedback at 1, Wear and Thin at 0, and every Send at 0, writing changes nothing. A soft clip keeps the loop within full scale when the sends and feedback add up past it.

The voices read before the write head writes, so on the next pass every voice reads what was printed: a reversed or half-speed voice printed into the loop is played back by the others at their own rates.

## Some starting points

- **A slow echo of itself.** Voice 2 on, Rate -0.5, Send 0.6. Write on, Feedback 0.85. Each pass prints a reversed, octave-down copy into the loop, and older prints fade under the new ones. Send 0 then lets the whole loop fade.

- **A loop that wears out.** Write on, Feedback 0.9, Wear 0.3, every Send at 0. Each pass is a little darker and quieter.

- **A tape stop.** Slew 2000, then Rate 0: the voice slows to a halt over 2 s, pitch falling.

- **Layers in time.** Voice 1 on the whole range, voices 2 and 3 on short windows inside it at rates 2 and 1.5, sends low. Ping on voice 3 keeps its short window from clicking.

- **Back to the start.** **Reset** restores the loop as loaded and puts every head at its window's start, keeping the settings.

## Saving

- **Save loop** writes the loop as it now is, only the range, without the rolls: `samples/<track>-tape/<track>-tape-loop.wav`.

- **Record** starts recording what you hear; pressing it again stops and writes `-mix.wav` beside it. If the disk falls behind, dropped blocks are counted and reported.

Both are 32-bit float WAV, in a new directory each time, and both are added to the library, so a saved loop can be loaded, sliced or looped again.

## What it does not do

- **Recording an input.** The tape records only its own voices.

- **Remembering a tape.** The settings last until the next load; only saved files are kept.

- **A device held exclusively**, such as an ALSA `hw:` device: the tape opens its own audio stream beside the player's, and that fails there.

- **The server.** Its web page does not reach the tape.

- **Known clicks.** A voice that wraps again during a crossfade, as when its rate rises mid-fade, can click.
