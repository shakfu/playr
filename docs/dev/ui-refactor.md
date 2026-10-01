# Sampler controls in 800 points

Design, written 2026-09-30 against playr 0.14.0. The window opens at its minimum width, 800 points, and the sampler's controls took more height than the waveform at the minimum size. This plan cut them to two rows without dropping an action. It was built the same day; "Built" at the end records where it differs. `docs/dev/gui.md` holds the parity rule and the GUI's earlier design.

## Constraints

- **The parity rule holds.** Every `Action` stays reachable through a control, a key, or a named way in `controls::WITH_VALUES`. Moving an action from a button to a menu is allowed; dropping it is not.

- **Keys do not change.** `keys.toml` and the terminal are untouched, and no `Action` is added.

- **800 x 480 is the design size.** `every_control_fits_the_smallest_window_without_overlap` must pass there. Widening the window is not an option.

- **The vertical budget is the real limit.** egui wraps a row that runs out of width, so a width limit alone turns extra controls into extra rows.

## Before

Measured in `crates/playr-gui/tests/window.rs`, `sized`, sampler view, text transport off. One control row is 21 points; a group gap is 10.

| window | waveform height | under the waveform | transport and bar |
|-|-|-|-|
| 800 x 480 | 116 (24%) | 146: status line, 5 rows, 2 gaps | 116: 4 rows |
| 800 x 720 | 356 (49%) | 146 | 116 |

Under the waveform, in `sampler::show`:

| row | tables and widgets | items |
|-|-|-|
| 1 | `SAMPLER_BAR`, Snap to zero | 9 |
| 2 | `RANGE_BAR`, Fit range, Loop range | 6 |
| 3 | `EDGE_BAR`, Loops 1-8, `LOOP_BAR` | 13 |
| 4 | `SLICE_BAR`, count, Equal slices, Sensitivity, Slice at onsets | 6 |
| 5 | `PLAN_BAR`, Edges, fade text, `WRITE_BAR` | 5 |

The transport drew `MARKS` as its own row, on every view, though Playback already listed them.

Problems:

- **Disabled was used for selected.** The four display buttons disabled the current display, so Envelope looked unavailable while it was showing.

- **10 of 38 items were disabled with no range, plan or loop.** The 8 empty loop slots were enabled but showed nothing.

- **Mouse and key controls duplicated each other.** A drag already set the range, moved either end and moved a mark. The wheel zoomed. Move start, Move end, Earlier and Later repeated the drag for key users.

- **`MARK_BAR` was never drawn.** `61e214f` added it to `controls::TABLES` but no view drew it. `tests/parity.rs` reads the tables, so it counted `PickMark`, `MoveMark`, `SnapMark`, `DeleteMark` and `SetCursor` as reachable by a control. In the window only keys reached them. `WITH_VALUES` named "Cursor earlier and later buttons" for `MoveCursor`, which did not exist either.

- **Two actions shared one label.** `MARKS` had Previous mark for `PrevMark`, which moves the playhead. `MARK_BAR` had Previous mark for `PickMark(false)`, which moves the cursor.

## Decisions

The choice between a button and a menu entry is made per group, not once for the view. The Sampler menu, which replaces the Slice menu, lists every action with its key, so a group can lose its buttons and stay reachable.

| group | placement |
|-|-|
| Zoom in, Zoom out, Whole track | `[+] [-] [<->]`, right of the header row; `<->` is drawn as U+2194, a left-right arrow |
| Track info | `[i]`, beside the zoom buttons |
| Envelope, dB, Spectrogram, Waveform | one drop-down, right of the header row |
| Mark | button under the waveform |
| Audition, Clear range | buttons under the waveform |
| Range in, Range out | Sampler, Range; a drag sets the range by pointer |
| Move start, Move end, Earlier, Later | Sampler, Range; a drag on either end moves it |
| Snap to zero, Fit range, Loop range | Snap, Fit and Loop: buttons that stay pressed while on |
| Loops 1-8 | a lane under the waveform that draws saved loops only, and a Save loop button |
| Clear loops | Sampler, Loops |
| Slice region, Slice at marks, Equal slices, Slice at onsets | one method drop-down, which plans when chosen, and its parameter |
| Previous slice, Next slice, Edges, Write slices, Discard slices | the slice row, only while a plan waits |
| Select previous and next mark, Mark earlier and later, Snap to rise, Delete mark, the cursor | Sampler, Marks; Snap to rise and Delete mark also on a mark's right-click |
| Undo mark, Previous mark, Next mark, Clear marks | Playback, as before; the transport's row is gone |

Other decisions:

- **Save loop** picks the first empty slot in the window. Adding `loop save` with no slot to `playr-app` would give the terminal the same; it is not done.

- **`PickMark` is labelled "Select previous mark" and "Select next mark".** `PrevMark` and `NextMark` keep their labels.

## Alternative considered

Split the sampler into Listen and Slice modes, each with its own one or two rows. Each mode would be simpler. Range, loops and audition belong to both modes, though, so they would be repeated or would need a switch. Rejected; it could be revisited if two rows prove too tight.

## Built

```
name.wav  0:00.000-0:12.000  one column 12 ms              [+] [-] [<->] [i] [Envelope v]
+------------------------------------------------------------------------------------+
|                                    waveform                                        |
+------------------------------------------------------------------------------------+
              [1              ]                                  saved loops, if any
range 0:03.072-0:06.144 (3.072 s)  marks 0  peak 0.0 dBFS  rms 0.0 dBFS  corr +1.00
[Mark] [Audition] [Clear range] [Loop] [Save loop]  |  [Snap] [Fit]

Slice [At onsets v] ---o--- 0.50 Sensitivity  |  [<] [>] [exact v] Edges [Write] [Discard]
```

| window | waveform height | under the waveform | transport and bar |
|-|-|-|-|
| 800 x 480 | 210 (44%) | 73: status line, 2 rows, 1 gap | 95: 3 rows |
| 800 x 720 | 450 (63%) | 73 | 95 |

A saved loop adds the lane's 16 points.

Where it differs from the decisions, or adds to them:

- **The loop lane draws a loop only where it is in view.** A loop outside the frames shown has no band; Sampler, Loops and the F keys still reach it. A band is at least 16 points wide, so two short loops close together can overlap.

- **Range starts here and Range ends here** keep the other end while it is on the right side of the pointer. Otherwise they use the track's end or its start.

- **The Slice drop-down shows the plan that waits,** or None. It reads the plan's cut from the model, so a plan made with `:slice` shows too, with its count or sensitivity. Choosing a method plans with it, again if it is the one shown; choosing None discards. A changed count or sensitivity plans again. A first build had a Plan button beside the drop-down; it was dropped because a second step added nothing, and its hover text had to explain a Write button not yet shown. Region shows as Range while a range is set.

- **Edges is also under Sampler, Slice.** Outside the sampler view a slice is written at once, with no plan to show the drop-down. The fade times are the drop-down's hover text.

- **Outside the sampler view, the Sampler menu enables only slicing and Edges.**

- **Loop is enabled with no range,** as `l` then loops the region around the playhead. The old tick box was disabled.

- **Only the Sampler menu shows keys.** The other menus are unchanged.

- **A long file name keeps its full width in the header;** the times and the scale beside it truncate first.

## Tests

- `the_sampler_s_controls_leave_the_waveform_200_points_of_the_smallest_window` is the budget: at 800 x 480, with the slice row at its widest, nothing overlaps and the waveform is at least 200 points high.

- `a_saved_loop_shows_under_the_stretch_it_spans_and_a_click_loops_it` and `the_waveform_s_menu_acts_on_the_point_right_clicked` cover the lane and the waveform's menu.

- `the_sampler_menu_holds_every_table_no_button_draws` finds every label of `RANGE_MENU`, `MARK_MENU` and `LOOP_MENU` in the menu.

## Not done

- **`tests/parity.rs` still reads `controls::TABLES`,** not what is drawn. A table that no view draws would pass again, as `MARK_BAR` did. The menu test above covers three tables only.

- **The tab and the menu are both named Sampler.** A screen reader hears two controls with one name, and tests pick the higher one.

- **Right-click is hard to discover.** Nothing in the view names the waveform's menu; the Sampler menu repeats most of it.

## Later: overview, 2026-10-01

The overview strip above the waveform takes 21 points. At 800 x 480 that left the waveform 189 points, under the 200-point budget. The minimum height rose to 504 rather than hiding the strip at whole-track zoom, which would resize the waveform on the first zoom step. The design size is now 800 x 504; the tests above check it there.
