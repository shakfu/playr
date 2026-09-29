# Sampler controls in 800 points

Design, written 2026-09-30 against playr 0.14.0, before any of it was built. The window now opens at its minimum width, 800 points. The sampler's controls do not fit it well: they take more height than the waveform at the minimum size. This plan cuts them to two rows without dropping an action. `docs/dev/gui.md` holds the parity rule and the GUI's earlier design.

## Constraints

- **The parity rule holds.** Every `Action` stays reachable through a control, a key, or a named way in `controls::WITH_VALUES`. Moving an action from a button to a menu is allowed; dropping it is not.

- **Keys do not change.** `keys.toml` and the terminal are untouched. No phase adds an `Action`, except where an open decision says so.

- **800 x 480 is the design size.** Every phase must pass `every_control_fits_the_smallest_window_without_overlap` there. Widening the window is not an option.

- **The vertical budget is the real limit.** egui wraps a row that runs out of width, so a width limit alone turns extra controls into extra rows.

## Measured

`crates/playr-gui/tests/window.rs`, `sized`, sampler view, text transport off:

| window | waveform height | under the waveform | transport and bar |
|-|-|-|-|
| 800 x 480 | 116 (24%) | 146: status line, 5 rows, 2 gaps | 116: 4 rows |
| 800 x 720 | 356 (49%) | 146 | 116 |

One control row is 21 points; a group gap is 10.

## What exists

Under the waveform, in `sampler::show`:

| row | tables and widgets | items |
|-|-|-|
| 1 | `SAMPLER_BAR`, Snap to zero | 9 |
| 2 | `RANGE_BAR`, Fit range, Loop range | 6 |
| 3 | `EDGE_BAR`, Loops 1-8, `LOOP_BAR` | 13 |
| 4 | `SLICE_BAR`, count, Equal slices, Sensitivity, Slice at onsets | 6 |
| 5 | `PLAN_BAR`, Edges, fade text, `WRITE_BAR` | 5 |

The transport adds `MARKS` as its own row, on every view.

Problems, from the code and the screenshot of 2026-09-30:

- **Disabled is used for selected.** The four display buttons disable the current display. Envelope looks unavailable while it is showing.

- **10 of 38 items are disabled with no range, plan or loop:** the current display, Clear range, Earlier, Later, Loop range, Clear loops, Previous slice, Next slice, Write slices, Discard slices. The 8 empty loop slots are enabled but show nothing.

- **Mouse and key controls duplicate each other.** A drag already sets the range, moves either end and moves a mark (`EDGE_REACH`). The wheel zooms. Move start, Move end, Earlier and Later repeat the drag for key users.

- **`MARK_BAR` is never drawn.** `61e214f` added it to `controls::TABLES` but no view draws it. `tests/parity.rs` reads the tables, so it counts `PickMark`, `MoveMark`, `SnapMark`, `DeleteMark` and `SetCursor` as reachable by a control. In the window they are reachable only by key. `WITH_VALUES` names "Cursor earlier and later buttons" for `MoveCursor`; they do not exist either.

- **Two actions share one label.** `MARKS` has Previous mark for `PrevMark`, which moves the playhead. `MARK_BAR` has Previous mark for `PickMark(false)`, which moves the cursor.

- **The Slice menu repeats three slice buttons.**

## Approach

Menus make every action reachable, with its key shown. Buttons are then kept only for actions used often. The waveform takes the edits it can do by pointer.

Target, at 800 points:

```
Coldcut - Rubaiyat.mp3  0:00-5:55  one column 454 ms        [-] [+] [All] [Envelope v]
+------------------------------------------------------------------------------------+
|                                    waveform                                          |
+------------------------------------------------------------------------------------+
 [1 0:12-0:20] [3 1:02-1:10]                                 loop lane, when any saved
region 0:00.000-5:55.578 (355.579 s)  marks 0  peak 1.3 dBFS  -10.1 LUFS  corr +0.92
[Mark] [Audition] [Loop] [Save loop] [Clear range]  |  [Snap] [Fit]
Slice [Onsets v] ---o--- 0.50 [Plan]  |  [<] [>] Edges [exact v] [Write] [Discard]
```

- **Header row:** zoom and a display combo box move to its right side, in space it already has. Track info moves to the Sampler menu.

- **Toggles:** Snap, Fit and Loop become `toggle_value` buttons, as EQ is in the transport. They show state without a check box's label width.

- **Loop lane:** saved loops show as labelled bands under the waveform. A click recalls one; its context menu saves the range over it or clears it. Empty slots draw nothing. Save loop saves the range to the first empty slot.

- **Waveform context menu**, at the pointer: Mark here, Range starts here, Range ends here, Clear range. On a mark: Delete mark, Snap to rise.

- **Slice row:** one method combo box (region or range, marks, equal, onsets), its parameter (count or sensitivity), and Plan. Sensitivity still replans as it moves. The review controls on the right appear only while a plan is pending. The left side does not move when they do.

- **Fade times** move from a label into the Edges combo box's hover text. The label would overflow the slice row.

- **Transport:** the `MARKS` row moves to Playback, Marks. Shift-click on the progress bar still marks.

- **Sampler menu**, replacing the Slice menu, drawn from tables with `items()`: Display, Zoom, Range, Edges, Marks and cursor, Loops 1-8 (use, save, clear), Slice, Snap, Fit, Loop, Track info. Each item shows its key.

Expected height at 800 x 480: 3 control rows and 1 gap removed (73 points), the marks row removed (21), and the loop lane added (14). The waveform grows from 116 to about 196. This is an estimate; phase 0 records the real value.

## Open decisions

Recommendation first.

- **Where parity lives:** in the Sampler menu, with buttons only for common actions. The alternative is a button for every action, which is today's design and the cause of the crowding.

- **Loop slots:** a lane that draws saved loops only; or one "Loop [3 v]" combo box; or keep 8 buttons. The lane costs 14 points of height when any loop is saved, and nothing otherwise.

- **Save loop:** the window picks the first empty slot; or add `loop save` with no slot to `playr-app`, so the terminal gets it too. The second follows the parity rule more closely.

- **Review controls:** shown only while a plan is pending, in a fixed place; or always shown and disabled, as now.

- **Marks row:** to Playback, Marks; or kept in the transport for views other than the sampler.

- **Labels:** "Select previous mark" and "Select next mark" for `PickMark`; or rename `PrevMark` and `NextMark` to "Seek to previous mark" and "Seek to next mark".

## Alternative considered

Split the sampler into Listen and Slice modes, each with its own one or two rows. Each mode would be simpler. Range, loops and audition belong to both modes, though, so they would be repeated or would need a switch. Rejected for now; it could be revisited if two rows prove too tight.

## Phases

Each phase ends with `make test` and a screenshot at 800 x 480 and 800 x 720.

0. **Guard.** Add a window test: at 800 x 480 the sampler's waveform is at least 116 points tall. Raise the number with each phase. Add a test that every label in every table in `TABLES` is drawn in some view or menu. It fails on `MARK_BAR` today.

1. **Sampler menu.** Replace the Slice menu. Draw every sampler table in it, `MARK_BAR` included. Fix the duplicate labels. The test from phase 0 now passes.

2. **Header and toggles.** Zoom and a display combo box go to the header row. Snap, Fit and Loop become toggles. Track info goes to the menu.

3. **Waveform.** Add the context menu and the loop lane. Remove `EDGE_BAR`, the loop slot buttons and `LOOP_BAR` from under the waveform.

4. **Slice row.** Put the method combo box, parameter and Plan with the review controls in one row.

5. **Transport.** Move `MARKS` to Playback, Marks.

Phases 1 and 5 each stand alone. Phases 2 to 4 depend on 1.

## Tests to update

- `the_waveform_takes_the_height_the_controls_leave` finds the bottom through "Discard slices", which phase 4 hides with no plan pending.

- `a_drag_sets_the_range_and_the_bar_slices_it` and `slices_planned_in_the_sampler_are_written_with_a_button` click slice buttons by label.

- `the_spectrogram_button_paints_one_texture_the_size_of_the_waveform` clicks the Spectrogram button, which becomes a combo box entry.

- `tests/parity.rs` should check that controls are drawn, not only that they are listed in `TABLES`.

## Risks

- **Right-click is hard to discover.** The waveform's hover text should name it, and the menu repeats each action.

- **Combo boxes take two clicks where buttons take one.** This affects display and slice method, which are chosen less often than they are used.

- **egui does not wrap before a slider or combo box.** The slice row holds two combo boxes and a slider. It fits by estimate only; phase 4 must measure it with a plan pending and fades on.
