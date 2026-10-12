# Tape lanes: every voice's loop on screen

Status: built 2026-10-11, unreleased. Where the build departs from this design is at the end.

## Problem

Since a loop per voice (`loop-library.md`), the Tape tab draws one waveform: the selected voice's loop. A voice on its own loop is seen only by selecting it. `loop-library.md` gives the reason: "At 800 x 592 there is no room for three waveforms." That holds for the current layout, where the waveform takes 146 pt and the strips below take the rest. It does not hold if the strips shrink.

## Proposal: one lane per voice

Each voice gets a lane: a header, its own waveform, and its controls in one row under it. The write head gets a thinner lane on top. Layout at 800 pt:

```
+-----------------------------------------------------------------------------+
| Load range | Take | Record | Save loop | Reset                              |
+-------------------+---------------------------------------------------------+
| [x] Write         | ......[==== write window ====]......|.................. |
| fb 0.90 wear 0.20 | thin 0.00   window 0.0% 100.0%                          |
+-------------------+---------------------------------------------------------+
| [x] Voice 1  P S  | ~~~~~~~[######### window #########]~~|~~~~~~~~~~~~~~~~~ |
| (voice 1's loop)  | ~~~~~~~[##########################]~~|~~~~~~~~~~~~~~~~~ |
| 12.5% .. 62.5%    | Rate  Slew  Lvl  Pan  Send Wear Drive Filt Type Xfade  |
|                   | 1.00  0ms   .80  0.0  .50  .00  .00   1.00 LP   10ms   |
+-------------------+---------------------------------------------------------+
| [x] Voice 2  P S  | (same, its window shaded)                               |
| shares voice 1    |                                                         |
+-------------------+---------------------------------------------------------+
| [ ] Voice 3  P S  | (its own loop's peaks; no write head, no frozen hatch)  |
| kick_120.wav      |                                                         |
+-------------------+---------------------------------------------------------+
```

Height, in points:

| Part | Height |
|-|-|
| Button bar | 24 |
| Write lane: strip 16, one row of fields 22 | 38 |
| Voice lane: waveform 56, labels and fields 32, gap 6 | 94 x 3 = 282 |
| Spacing | ~16 |
| Total | ~360 |

The current tab is 146 for the waveform plus 11 strip rows (~264) plus the bar: about 434. The lanes are shorter and show all three loops. The chrome above the tab was not measured; at the 592 pt minimum the tab gets roughly 440-480 (inference, to check with a screenshot).

Width: a 150 pt header leaves 650 for the waveform. Eleven fields at 59 pt each fit under it.

### What changes

- **Sliders become `DragValue` fields.** A slider plus its field takes ~170 pt; a field alone takes ~55. Fields drag horizontally, take typed values, and keep their `widget_info` names. The rail is lost; the value is still shown.

- **Type becomes one button** cycling LP, HP, BP: 3 toggles do not fit 59 pt.

- **Window % moves to the header**, under the loop's name.

- **The header names the loop**: its file name, or "shares voice 1". This is what the screenshot's per-lane title does.

- **Selection goes.** Each lane drags its own window on its own waveform. `State::selected` and the `Target::Voice(selected)` hit rule are removed.

- **The write head and frozen hatch** are drawn on lanes that read voice 1's loop, which is the only loop the write head writes. While any voice has its own loop, the write lane is dimmed, as `no_effect` already reports.

### Kept

- Every control visible, as now: 33 voice fields and 4 write fields.

- `no_effect` dimming and hover reasons.

- Each lane's peaks from `status.voice_columns(i)`, already one grid per voice. Drawing three grids is 3 x 512 line segments a frame.

## Alternative: three bands, strips unchanged

Replace the single waveform and its 3 lanes (146 pt) with a 12 pt write strip and three 44 pt bands, one per voice (144 pt). The strips below stay as they are.

- About 100 lines changed in `crates/playr-gui/src/tape.rs`, against a rewrite of `rows` and `voice` for the lanes.

- Bands stack top to bottom; strips run left to right. Voice 1's band is above, its controls are on the left. The eye has to map one to the other.

- 44 pt waveforms are 45% of today's 98.

## Not in scope: delay and reverb sends

The screenshot's lanes send to a shared delay and reverb. The looper has no effects: `looper-engine.md` lists them as not done. `Send` prints a voice into the loop through the write head; it is not an effect send. Adding them is engine work: two buses on the audio thread, a send per voice, and their controls. That needs its own design. The lane layout leaves room for two more fields per voice (650 / 13 = 50 pt each, tight).

## Decided

From the 2026-10-11 discussion: lanes; delay and reverb get their own design; no prev and next arrows. A search box with completion per lane, loading a library loop into that voice, is in `TODO.md`.

## Where the build departs

- **Type keeps three toggles**, LP, HP and BP, at 22-25 pt each. They fit, and one click picks any type, where a cycling button needs up to two.

- **Shared lanes name "voice 1's loop"**; the hover gives the name. Voice 1 and a voice on its own loop show the loop's name: `Deck::voice_name`, the source track's name for a range.

- **The frozen hatch is drawn only on lanes reading voice 1's loop.** A voice's own loop is never written, so hatching all of it would mark every such lane permanently.

- **Measured at 800 x 592:** the third lane's fields end at y = 460. The fields end at x = 663, leaving ~120 pt beside them.

## After use (2026-10-11)

- **A loop list per lane** replaces loading from the Loops view. It searches the library as typed with `Session::find_loops`, which leaves the Loops view's own search alone.

- **Lanes are independent.** A voice holds a loop of its own, voice 1's loop, or nothing; Eject empties it rather than falling back to voice 1's. In the engine an empty voice holds `Loop::empty()` as its own, so no new state reaches the audio thread; the write head's hold ignores empty loops. Sharing stays as an explicit choice, since the write head's prints only reach voices reading voice 1's loop.

- **Voices 2 and 3 start empty**, and Load range loads voice 1 alone. Take still starts a new tape.

- **Fixed:** a voice with an empty window and a negative rate computed its first frame as `end - 1` and panicked on the audio thread. A new tape had the bug latent; empty lanes made it reachable. `Window::last` guards the four sites.

## The write loop (2026-10-11)

The write head writes a loop of its own, the write loop, apart from the three lanes. Before, it wrote voice 1's loop, and was held off while any voice read a loop of its own; with independent lanes that was the usual case, so Write looked dead.

- **Made by the first Write**: silent, voice 1's loop's length, channels and rolls. Voice 1 must hold a loop. Eject on the write lane drops it and turns Write off.

- **Heard through a lane** that picks it, as a play head on a tape. No dedicated playback head: one can come later as a monitor level.

- **No hold.** A voice's send is a signal; the write head records it whatever loop the voice reads.

- **Engine.** `TAPE`, equal to `VOICES`, names the write loop wherever a voice index goes: loads, clears, ejects and the peak grids. Voice 0 has an own loop like the others; the old shared `lp` is the write loop.

- **Save loop** saves the write loop, named after voice 1's loop when Write made it. Take still starts a new tape with the range in voice 1.

## Four lanes (2026-10-12)

The separate write lane gave way to a fourth voice, on top, whose loop is the write loop. With Write off, the tape is a four-track looper; with it on, voice 4 is the write track, and plays back what is printed at its own rate. This also gave the write loop a play head of its own: before, it was heard only through a lane that gave up its loop to read it.

- **`WRITER` is voice 4**, and always reads the write loop, `TAPE`. A load into it loads the write loop; its Clear and Eject act on it. Voices 1 to 3 can read it with Share.

- **Rejected: a track-count drop-down** (2-track, 3-track, 4-track, 3 + write). Four-track and three plus write are one layout, told apart by Write; hidden lanes could go on playing and sending.

- **Height.** The waveforms share what the controls leave, between 40 and 96 points: 41 at 800 x 592, 73 at 800 x 720, as measured. A lane's window fields moved into its field row, and the write head's fields into a second row of voice 4's lane. `ROW` and `SEPARATOR` are measured, not derived: a change to the field rows needs them measured again.

- **CPU.** A fourth voice with the sinc kernel: about 13% of a core at x4, scaled from 9.8% for three in `aliasing.md`. Not measured.

