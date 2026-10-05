# DJing with playr

playr has two decks and a mixer. Each deck plays a library track at its own rate. A beat grid per track lets one deck follow the other's tempo and phase. The window shows them in the **DJ** tab; the terminal has the `:dj` commands only, listed under [DJ](../README.md#dj) in the README. `:dj` alone lists them all. This page explains the tab and how to mix with it. The design is in [`docs/dev/dj-engine.md`](https://github.com/shakfu/playr/blob/main/docs/dev/dj-engine.md).

## Before you start

- **Analyse the library.** `playr analyze`, or `:analyze`, measures each track's beat grid: its tempo to 0.01 BPM and where a beat falls. A deck that loads a track never analysed analyses it in the background, and the track plays without a grid until that finishes. A track with no clear pulse gets no grid; [Fixing the grid](#fixing-the-grid) shows how to tap one in.

- **Headphones.** Without a second output, the headphone cue shares your one stereo output: the main mix in the left ear, the cued deck in the right. See [Headphone cue](#headphone-cue).

- **The player pauses.** The decks play on their own audio stream, on the player's device. Starting a deck pauses the player, as the tape does. A device held exclusively, such as an ALSA `hw:` device, cannot take a second stream, and the decks fail to open on it.

## The tab

**DJ** in the tab bar opens it, over the library. Keys keep acting in the library; a key that shows another view, such as Tab, leaves the tab.

From top to bottom:

1. **Waveforms.** One per deck, deck A on top. Each shows 3 s either side of the playhead, which is fixed at the centre in red. Both span the same time as heard, so two decks in phase show their beats at the same places.

   - Thin grey lines are the grid's beats. Its bar lines, every fourth beat, are slightly heavier, near-white in the dark theme and near-black in the light one.

   - Purple lines are the track's marks, as the sampler set them.

2. **Overviews.** Under each waveform, the whole track: the cue point in yellow, the marks in purple, the playhead in the text's colour. A click or a drag here seeks. A click within 5 points of a mark lands on it exactly.

3. **Deck strips**, left and right. A deck's title, its tempo as played, its phase meter, and its controls; see [Playing a deck](#playing-a-deck).

4. **Channels and mixer**, in the middle. A channel strip per deck, then the crossfader and the settings that apply to both decks; see [Mixing](#mixing).

5. **The library.** Every track the library view lists, so the search field narrows it. Each row has **A** and **B** to load it onto a deck, and its tempo. A deck's button is lit on the row it holds, and reads **A>** or **B>** on a row waiting as its next track.

At the smallest window, 800 by 592, the library shows a few rows; a taller window shows more.

## Loading a track

Press **A** or **B** on a library row. The track is read into memory, which takes a moment for a long track, and the deck's title shows `reading` until it is ready. A loaded track starts paused, at its start, with its cue point there.

What happens when the deck is already playing depends on **Strict**, next to Quantize in the mixer:

| Strict | a track picked for a playing deck |
|-|-|
| off, as playr starts | replaces the playing track: the deck fades out over 5 ms, loads it, and plays it from its start |
| on | waits as the deck's **Next** track, shown under its title, and loads once the deck stops, by pause or at its end |

Strict off suits listening to one track after another, and correcting a wrong pick by picking again. Strict on suits a set: a mis-click on the deck that is playing to the room cannot cut it. While a track waits, **x** beside it, or `:dj a unqueue`, forgets it.

A library or selection row's right-click menu has **Load to deck A** and **Load to deck B** too. In the terminal, `:dj a load` loads the row under the cursor.

## Playing a deck

A deck strip's rows, from the top:

- **Title**, and the waiting **Next** track, if any.

- **Tempo and phase.** The tempo as played: the grid's tempo times the rate. The phase meter shows where this deck's beat falls against the other deck's, one beat wide: the marker is centred, and green, when the two are in phase within 2% of a beat; right of centre, this deck is ahead.

- **Load, Play or Pause, Cue, Sync.** Load takes the library's cursor row.

- **Rate.** The rate fader, in percent, and its range, **8%**, **16%** or **50%**. The rate is varispeed: pitch moves with tempo, as on a turntable. **-** and **+** bend the rate 4% while held, to nudge the deck's beats later or earlier.

- **Hot** cues **1** to **4**, and **Mark < >**.

- **Loop** of **1** to **32** beats.

- **Jump** **-4**, **-1**, **+1**, **+4** beats, and the **Grid** menu.

### The cue button

**Cue** works as on a CDJ:

- **Paused:** pressing sets the cue point at the head, and plays from it while held. Releasing returns to the cue point and pauses. Pressing **Play** while holding Cue keeps the deck playing on release.

- **Playing:** pressing returns to the cue point and pauses.

**Play** after a pause plays on from where the deck stopped. With **Quantize** on, a cue point set while paused falls on the nearest beat.

### Hot cues

A click on an empty hot cue sets it at the head. A click on a set one, which is lit, jumps there and plays. A right-click clears it. Hot cues are kept with the track in the library, so they come back when it loads again, on either deck.

### Marks

The marks set in the sampler view show on both waveforms. **Mark <** and **Mark >** jump to the previous and next one. Within half a second after a mark, **<** goes to the one before it, as a CDJ's previous does. Marks are read when the track loads; a mark set afterwards shows once the track loads again.

### Loops and jumps

**Loop** repeats that many beats, starting from the head, or with Quantize on from the beat before it. The lit length is the loop playing; clicking it again ends the loop. **Jump** moves the head whole beats, and moves a loop playing with it. Both need a grid; without one they are dimmed.

### Seeking

A click or drag on the overview seeks. With Quantize on and the deck playing, the deck keeps its phase: it lands on the beat nearest the point, as far past it as the head was past its own beat. A seek outside a loop playing ends the loop. In the terminal, `:dj a seek 1:30` or `:dj a seek 25%`.

## Beatmatching

### With sync

1. Play deck A.

2. Load deck B, and press **Sync** on deck B. Deck B takes deck A's tempo and moves into phase with it.

3. Press **Play** on deck B. A synced deck starts in phase.

What sync does:

- It matches tempos at half, the same or double, whichever is nearest. An 87 BPM track syncs to a 174 BPM one at rate 1.

- If the rate it needs is outside the fader's range, it widens the range. Beyond 50% it refuses, and says so.

- While synced, deck B follows deck A's rate fader, though not deck A's nudge.

- A phase lock keeps deck B in phase: if it drifts more than 1% of a beat, a rate trim of up to 5% pulls it back. A nudge on deck B moves the phase the lock holds, so you can set deck B a little ahead or behind on purpose.

- Moving deck B's own rate fader turns sync off. Sync on deck A turns deck B's off, and makes deck A follow.

Both decks need a grid to sync.

### By ear

1. Set **Cue B** in the mixer, so deck B plays in your headphones and not in the room.

2. Play deck B from its cue point on deck A's downbeat.

3. Match tempos with deck B's rate fader, by ear or by the two BPM readings.

4. Hold **-** or **+** on deck B to bring its beats into line, until the phase meter's marker sits centred and green.

**Quantize** helps: with it on, Play and Cue start in phase with the other deck whenever the other deck plays.

## Fixing the grid

A grid is a tempo and the time of one beat; every other beat follows from those two numbers. The analysis finds a beat but not which beat starts the bar, so the bar lines, the heavier ones, may sit on beat 2, 3 or 4. The deck's **Grid** menu corrects it:

| item | does |
|-|-|
| **<**, **>** | moves the grid one beat earlier or later; use it to put the bar lines on the downbeats |
| **-1 ms**, **+1 ms** | moves the grid 1 ms earlier or later, when the lines sit just off the kicks |
| **x2**, **/2** | doubles or halves the tempo, for a track read an octave off; the same correction as `:bpm x2` |
| **Tap** | tap on the beat while the deck plays. On a track with a grid, two or three taps move its beat to the last tap, and four or more set its tempo from the taps too. On a track without one, two taps set both. Taps more than 2 s apart start over |
| **Reset** | the analysed grid again |

Every edit is kept with the track, through later analyses. The tempo shown in the library's tempo column follows it.

If the lines drift off the kicks as the track goes on, the tempo is slightly wrong; that is rare after analysis, since the grid is measured to 0.01 BPM. A track not played to a click, such as a live drummer, drifts from any constant grid; playr has no variable grid.

## Mixing

Each **channel** strip, from the top:

- **Mute.** Silences the deck in the main mix. The headphone cue still hears it, so you can prepare a muted deck.

- **Gain.** The deck's trim, -12 to +12 dB, to level a quiet track with a loud one.

- **High, Mid, Low.** An isolator EQ: above 2.5 kHz, 246 Hz to 2.5 kHz, below 246 Hz, each -24 to +6 dB. **K** kills a band: a tone in the middle of it drops by 40 dB or more. Near 246 Hz and 2.5 kHz the bands overlap, so a kill takes less there. With every band at 0 dB the EQ is out of the signal.

- **Filter.** One knob, -1 to 1: left of centre a low-pass, which closes as you go left; right of centre a high-pass. Near the centre it does nothing.

- **Level.** The channel fader.

The **mixer**:

- **Crossfader.** **A**, **Centre** and **B** glide it to that point over 400 ms; a double click on it glides it to the centre. Dragging moves it at once, and takes over from a glide. In the terminal, `:dj xfade a|b|centre`, or `:dj xfade 0.3` to set it at once.

- **Quantize.** Play, cue, hot cues, loops and seeks fall in phase, as above.

- **Strict.** See [Loading a track](#loading-a-track).

- **Cue Off, A, B.** The deck heard in the headphones.

- **Out Split, 3-4.** Where the headphone cue goes; see below.

- **Curve Smooth, Sharp.** Smooth keeps the level constant across the crossfader's travel, for blends. Sharp holds both decks at full level and fades one out only over the last 5% of the travel, for cuts.

The main mix passes a soft clip, which leaves levels under half of full scale untouched. playr's volume slider does not reach the decks; set their level with the channel faders.

### Headphone cue

| Out | on a stereo device | on a device with 4 channels or more |
|-|-|-|
| **Split** | left: the main mix in mono; right: the cued deck in mono | the same, on channels 1 and 2 |
| **3-4** | refused | channels 1 and 2: the main mix in stereo; channels 3 and 4: the cued deck in stereo |

Split puts the main mix and the cue on one stereo output, so it suits practising on headphones: speakers on that output would play the split too. 3-4 suits an audio interface with two outputs: the room on outputs 1 and 2, headphones on 3 and 4. The decks open a 4-channel stream whenever the device offers one. 3-4 has not been tried on real interfaces yet.

## A first mix

1. Load a track on deck A and play it, crossfader at **A**.

2. Load the next track on deck B. Turn on **Strict** for the rest of the set.

3. Press **Sync** on deck B, then **Cue B**, and listen in the headphones.

4. Pause deck B, put its cue point on a downbeat, and play it on deck A's downbeat; with Quantize on, it starts in phase.

5. Lower deck B's **Low**, or kill it, so two bass lines do not clash.

6. Move the crossfader towards **B**, bringing deck B's low back as deck A's goes.

7. At **B**, pause deck A and load the next track onto it.

## What it does not do

- **Key lock.** A rate change moves the pitch: about a semitone at 6%.

- **Variable tempo.** One tempo per track; a grid drifts on music not played to a click.

- **Downbeats.** Found by you, with the Grid menu's **<** and **>**.

- **More than 2 decks, MIDI controllers, recording the mix.**

- **Remembering the mixer.** Strict, Quantize, the EQ, the faders and the crossfader start afresh each time playr starts. Grid edits and hot cues are kept with the tracks.

- **The server.** Its web page and OSC do not reach the decks.
