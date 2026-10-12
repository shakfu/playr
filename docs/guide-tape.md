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

2. **A lane per voice**, numbered from the bottom: voice 1 at the bottom, voice 4 on top. On the left: the loop it reads, with where it lies in its file; a chevron that folds the lane, on, the voice's name, **Ping**, **M** for mute, **S** for solo; **Eject**, **Clear** on lanes that play the write loop, and in voice 4's lane, **Write**. On the right, its loop. The waveforms take the height the controls leave: about 40 points in the smallest window, more in a taller one.

   - The voice's window shaded, its edges marked, and its head as a line. The pre-roll and post-roll are dimmed.

   - Its crossfade curves at the foot.

   - On voice 4's lane, the write window as a band along the top, which drags as a window does.

   - On a lane reading the write loop, the write head as a line, and what the write head cannot change tinted and hatched: everything outside the write window, or all of it while writing is off. Such a lane is redrawn as the write head changes the loop.

   Under the loop, the voice's main controls in one row: Window, Rate, Level, Pan and Send. **More** opens the lane's drawer, a second row with Slew, Wear, Drive, the EQ (HP, LP, Bell, dB, Q) and Xfade. Voice 4's drawer has a third row for the write head: its window, Feedback, Print wear and Print thin.

   **A folded lane** shows its loop, its header's first row and a strip of its waveform; voice 4 keeps Clear, Eject and Write. The chevron, `>` folded and `v` open, folds and unfolds a lane. An empty lane is folded until a loop is loaded into it, unless you unfold it. With `persist = ["tape"]`, folded lanes and open drawers are kept with the tape.

### Editing windows with the mouse

- Drag near an edge of a voice's window, on its own lane, to move that edge; drag between the edges to move the whole window, keeping its length.

- A drag in the write strip edits the write window.

- An edge dropped within 6 points of the range's edge lands on it exactly.

A control that does nothing as the tape is set, such as a voice's Wear while its Send is 0, is dimmed, and its tooltip says why. You can still set it ahead.

## Voices

Voice 1 is on when a loop loads; the others are off until a loop is loaded into them, and voice 4 also when Write makes its loop. A voice's lane has its on tick box, its name, **Ping**, **M** for mute and **S** for solo, and these controls:

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
| HP | 20 Hz to 20 kHz | 20 Hz | a high-pass, cutting below it; off at 20 Hz |
| LP | 20 Hz to 20 kHz | 20 kHz | a low-pass, cutting above it; off at 20 kHz |
| Bell | 20 Hz to 20 kHz | 1 kHz | the centre of a peaking bell |
| dB | -18 to 18 | 0 | the bell's boost or cut; flat at 0 |
| Q | 0.1 to 10 | 1 | the bell's width: low is wide, high a narrow notch |
| Xfade | 0 to 1000 ms | 10 | the crossfade at each wrap |
| Delay | 0 to 1 | 0 | how much of the voice, as heard, goes to the delay |
| Reverb | 0 to 1 | 0 | how much of the voice, as heard, goes to the reverb |

Turning a voice off stops its head and skips its processing; it sends nothing. Turning it on again starts the head at its window's start, or its end in reverse.

The EQ runs high-pass, low-pass, then bell, after Drive, so it can take off the edge drive adds. HP and LP together pass the band between them. With the drawer open, the lane's header draws the EQ's response: gain from -18 to 18 dB across 20 Hz to 20 kHz, with a line at 0 dB and ticks at 100 Hz, 1 kHz and 10 kHz. Frequencies and Q drag on a log scale, and a frequency can be typed in Hz. In the terminal: `:tape 2 hp 120`, `:tape 2 lp off`, `:tape 2 bell 800`, `:tape 2 bell-gain -6`, `:tape 2 bell-q 4`.

Drive and the EQ shape both what is heard and what is sent. Changes to the rows move over 20 ms, so a drag does not click; Rate moves over Slew.

In the terminal, a window takes a time from the range's start, `1.5`, or a part of the range, `25%`: `:tape 3 window 25% 75%`. Below 0% or past 100% reaches into the pre-roll or post-roll.

### Wrapping, and Ping

At the end of its window a voice wraps to the start, crossfading over Xfade. The leaving head fades out into the audio after the window, so each pass is still exactly one window long; where there is no room, the fade is cut short. Each lane draws its voice's curves.

**Ping** makes the voice turn at its window's edges and play back the other way instead, so nothing jumps and nothing crossfades.

### Pan, mute and solo

On a stereo loop, pan keeps the near channel and folds the far one into it, so a hard pan keeps both channels' content. A mono loop pans with equal power.

**Mute**, `:tape N mute on`, silences what is heard of the voice and dims its lane. Its head goes on, and what it sends to the write head is unchanged.

**Solo** on any voice silences the voices without it. What every voice sends to the write head is unchanged, so a soloed voice can be heard while the others go on printing into the loop. A muted voice stays silent when soloed.

## A loop per voice

Each lane holds a loop or nothing. A loop picked in a lane is loaded into that voice alone: the voice is turned on, and its window becomes the loop's range. Its window and crossfades count in its own loop. In the terminal, `:load N` in the Loops view loads the loop under the cursor into voice N.

Voices 1 to 3 hold loops nothing writes. Voice 4's loop is the write loop: a loop loaded there is written over while Write is on, and **Reset** brings it back as loaded. With Write off, the four lanes are four loops.

Voices 1 to 3 can instead pick **voice 4's loop**, at the top of their list, or `:tape N share`. They then play what the write head prints, at their own rates: a second play head on the same tape. Voice 4 plays it too, at its own rate.

A loop is resampled to the tape's rate as it loads. Rates and starts are not matched across voices: set each voice's rate by hand to line up loops of different tempo.

**Reset** restores every voice's loop as loaded. **Save loop** saves voice 1's.

### Clear and Eject

- **Clear**, `:tape N clear`, silences the write loop. Its length, the windows and the heads stay. It is on voice 4's lane and on lanes that play voice 4's loop; a voice playing a loop of its own has none, since clearing a library loop or a range would only lose it. **Reset** brings back the loop as loaded.

- **Eject**, `:tape N eject`, empties the voice: it reads nothing, and its lane says empty. Voice 4's Eject empties the voices reading its loop too, and turns Write off. Once every voice is empty, the tape is gone.

Both do nothing on an empty voice.

## The write head

With voice 4 empty, the first **Write** makes its loop: silent, the length of the first loop in voices 1 to 3, counting from voice 1, with the same rolls. With Write on, the write head moves through its window at the normal rate and rewrites each frame of the write loop as:

the write loop's old content times **Feedback**, plus every voice's Send.

Every voice sends, whatever loop it reads.

| row | range | default | does |
|-|-|-|-|
| Window | start and end | the write loop's range | the part of the write loop that is rewritten; the rest stays as it is |
| Feedback | 0 to 1 | 1 | how much of the old content survives each pass; 0 replaces it with the sends |
| Wear | 0 to 1 | 0 | a low-pass on everything recorded, so the loop darkens each pass |
| Thin | 0 to 1 | 0 | a high-pass on everything recorded, 20 Hz to 2 kHz, so the loop thins each pass |
| Print FX | 0 to 1 | 0 | how much of the delay and reverb the write head records with the sends |

With Feedback at 1, Wear and Thin at 0, and every Send at 0, writing changes nothing. A soft clip keeps the loop within full scale when the sends and feedback add up past it. It leaves levels under `tape_knee` in `settings.toml`, -6 dBFS by default, untouched.

The voices read before the write head writes, so on the next pass every voice reading the write loop reads what was printed: a reversed or half-speed print is played back at that voice's own rate.

## Delay and reverb

One delay and one reverb serve all four voices. Each voice's drawer has a **Delay** and a **Reverb** send. The sends take the voice after Level, Pan, Mute and Solo: a muted voice has no echo, and fading a voice fades its tail. The Send to the write head is different: it is taken before Level and goes on under Mute.

The **FX** lane, above voice 4, holds the effects' controls. It is folded until opened with its chevron.

| row | range | default | does |
|-|-|-|-|
| Delay | clean, ping-pong, tape | clean | the delay's model |
| Time | 1 to 2000 ms | 375 | the delay; setting it turns Sync off |
| Sync | off, 1/1 to 1/16 | off | sets Time to that part of voice 1's window, as voice 1 plays it, and follows it |
| Feedback | 0 to 0.95 | 0.4 | how much of each echo repeats |
| Tone | 500 Hz to 20 kHz | 6 kHz | a low-pass on the repeats; the first echo is not filtered |
| Wow | 0 to 1 | 0.3 | the tape model's wow and flutter |
| Return | 0 to 1 | 1 | the delay's level in the mix |
| Reverb | freeverb, zita, jp, plate | zita | the reverb's model |
| Size | 0 to 1 | 0.5 | the room's size; Freeverb and JP |
| Decay | 0.1 to 20 s | 2 | the time for the tail to fall 60 dB; not Freeverb |
| Damp | 0 to 1 | 0.3 | how much faster the highs decay |
| Return | 0 to 1 | 1 | the reverb's level in the mix |

The models:

- **Clean**: plain repeats.
- **Ping-pong**: each repeat crosses to the other side.
- **Tape**: the repeats drift in pitch and saturate as they build.
- **Freeverb**: a bright, dense room.
- **Zita**: a smooth room; Decay is close to exact.
- **JP**: a modulated hall; its decay runs about twice the setting at 48 kHz.
- **Plate**: a plate; it sounds smaller at 96 kHz than at 48 kHz.

Switching a model starts it silent. The models are here to be compared; those that sound worse will be removed.

**Print FX** on voice 4's write row records the returns into the write loop. With it up, an echo is printed into the loop and then played back by the voices reading it, which can send it to the delay again.

In the terminal: `:tape 1 delay 0.5`, `:tape delay model tape`, `:tape delay sync 3/16`, `:tape reverb decay 4`, `:tape print-fx 0.5`.

## Some starting points

Voices 2 to 4 start empty.

- **A slow echo.** Voice 1 on a loop, Send 0.6. Write on, which makes voice 4's loop and turns it on. Voice 4 Rate -0.5, Send 0.6; Feedback 0.85. Voice 4 plays back each print reversed and an octave down, and prints that again, so older prints fade under the new ones. Send 0 on both lets the write loop fade.

- **A print that wears out.** After a few passes of the echo, every Send at 0, Feedback 0.9, Wear 0.3. Each pass of the write loop is a little darker and quieter.

- **A tape stop.** Slew 2000, then Rate 0: the voice slows to a halt over 2 s, pitch falling.

- **Layers in time.** Voice 4 on a loop, Write on. Voices 2 and 3 pick voice 4's loop, on short windows inside it at rates 2 and 1.5, sends low. Ping on voice 3 keeps its short window from clicking.

- **A dub echo.** Voice 1 Delay 0.5. Delay model tape, Sync 3/16, Feedback 0.7, Tone 2000. Each repeat is darker and drifts; Feedback 0.9 makes it build up and saturate.

- **Printed space.** Voice 1 Reverb 0.6, Send 0.5; Write on, Print FX 0.5, Feedback 0.8. Each pass prints the room into the loop, so it washes out over several passes.

- **Back to the start.** **Reset** restores every voice's loop as loaded, silences the write loop, and puts every head at its window's start, keeping the settings.

## Saving

- **Save loop** writes the write loop as it now is, only the range, without the rolls: `samples/<track>-tape/<track>-tape-loop.wav`, named after voice 4's loop, or voice 1's when Write made it.

- **Record** starts recording what you hear; pressing it again stops and writes `-mix.wav` beside it. If the disk falls behind, dropped blocks are counted and reported.

Both are 32-bit float WAV, in a new directory each time. The loop is added to the loop library, with the beats of the loop it came from; the recording is added to the library. A saved loop can be loaded, sliced or looped again.

## Keeping the tape between runs

With `persist = ["tape"]` in `settings.toml`, playr keeps the tape in the library: every voice's settings, the effects' and the file each loop was read from. At the next start it asks `take up the tape again, with its 3 loops?`, after the player's question if there is one. On yes, each loop is read again from its file and the settings are put back.

- **The write loop's audio is not kept.** A write loop that was written into or cleared comes back silent, its length. The voices that printed it can print it again; **Save loop** keeps a print. A loop loaded into voice 4 and not written into comes back from its file.
- **Write comes back off**, so nothing is printed over before you hear it. The tape plays if it was playing.
- **Heads start at their windows' starts.**
- **A file that is gone** leaves its voice empty, and the message names it.

The tape is kept half a second after a change, and when playr closes.

## What it does not do

- **Recording an input.** The tape records only its own voices.

- **Keeping the write loop's audio.** See above; Save loop keeps it as a file.

- **Surviving a lost device.** If the device goes away, the tape closes, a recording under way is finished, and the loop as written is lost. Load it again.

- **The server.** Its web page does not reach the tape.

- **Known clicks.** A voice that wraps again during a crossfade, as when its rate rises mid-fade, can click.
