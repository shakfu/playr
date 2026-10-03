# Mark selection

Status: implemented, both stages. The open questions are at the end.

The sampler edited marks through a cursor, a frame apart from the playhead. It now edits one selected item, a mark, a range end or a planned slice, with one set of action keys for all three. This file records why, and the choices made on the way.

## Problem

- **The cursor is detached from listening.** A mark is placed by ear, at the playhead, with `b`. Editing it means moving a second, silent position onto it with `u` or `i`. Nothing is heard at the cursor.

- **The window did not draw the cursor** until a late fix. `u` and `i` looked dead there.

- **The cursor goes stale.** Only `:cursor off` (`h`) cleared it. Removing the mark under it, or changing track, left it in place. It was a bare frame, not tied to a track as `Range` is, so `y`, `o`, `#` and `delete` could act on a mark of the next track that happened to sit near the old frame.

- **Two "next mark" commands.** `,` `.` seeked the playhead to a mark; `u` `i` moved the cursor to one. `docs/dev/ui-refactor.md:46` records the two sharing one label.

The playhead cannot simply replace the cursor. Every mark edit found the mark within one column of its target (`near`). During playback the playhead leaves that column within one column's time: 358 ms at the whole-track zoom of a 4:40 track, microseconds at full zoom. `,` then `o` would usually find no mark.

Alternative: keep the playhead as the target, and widen `near` while playing to a fixed time, such as the last mark crossed within 500 ms. This needs no selection state and no new drawing. It fails when marks sit closer than the window, and the target changes as playback runs: a second `o` can hit a different mark, or none.

## Keys before

Sampler view unless marked global.

| Target | Set at playhead | Select | Move | Snap | Remove | Hear |
|-|-|-|-|-|-|-|
| Mark | `b` (global) | `u` `i` (cursor) | `y` `o` | `#` | `delete`; `B` last added, `C` all (global) | `,` `.` (global, seek) |
| Range end | `<` `>` | `[` `]` | `{` `}` | none | `backspace` (whole range) | `a` |
| Planned slice | | none | none | none | none | `n` `p` |
| Cursor | | `;` `'` `h` | | | | |

`esc` (`:discard`) discarded the plan, or with none planned cleared the range.

## Options

Marks are captured during playback and edited while paused. Three ways to edit marks, slices and range ends were considered.

1. **The playhead is the target.** No selection. An edit acts on the mark at the playhead.
   - For: no state and no new drawing. It matches how marks are placed.
   - Against: it covers marks only. The playhead identifies the slice it is in, but not which edge, and never a range end.

2. **Each kind is selected with its own keys; one set of action keys acts on the selection.** One item is selected at a time: the one selected last.
   - For: selecting sets the kind, so there is no mode key and no extra keystroke.
   - Against: still modal, with the mode set as a side effect of selecting. Slices need editable plans.

3. **A mode key picks the kind; shared keys then select and act.**
   - For: fewest keys; a new kind costs no new keys.
   - Against: the mode is hidden state. The select keys are shared too, so a wrong mode moves the wrong item with no audio cue. Every kind switch costs a keystroke. `f1`-`f8` and `shift-f1`-`shift-f8` hold loop slots, and F9-F12 are often taken by the OS or terminal (F10 menu and F11 fullscreen in GNOME Terminal).

### Decision

- **Option 2, in two stages:** marks and range ends, then slices.

- **Option 3 is rejected.** It is option 2 plus a keystroke per switch and a hidden mode.

- **Option 1 is not taken for marks.** It would serve marks alone. Range ends and slices need selection state, so marks use the same model.

final remappings:

```text
			select 		deselect 		hear 		move		snap		remove
marks 		{ } 		D				{ } 		< > 		# 			backspace
slice   	, .			D				, . 		< >         #			backspace
rng end		[ ] 		D			  	a   		< > 		#			backspace
```

## Keys

Sampler view unless marked global.

| Kind | Select | Deselect | Hear | Move a column | Snap to onset | Remove |
|-|-|-|-|-|-|-|
| Mark | `{` `}` (global), `b`, click | `D` | `{` `}` | `<` `>` | `#` | `backspace` |
| Slice | `,` `.`, click its start | `D` | `,` `.` | `<` `>`, drag | `#` | `backspace` |
| Range end | `[` `]`, drag | `D` | `a` | `<` `>` | `#` | `backspace` |

| Key | Action |
|-|-|
| `i` `o` | `:in` and `:out`: set the range start or end at the playhead. |
| `u` `r` | Undo and redo. See [Undo](#undo). |
| `esc` | `:discard`: discard the plan. It no longer clears the range. |

- **Hearing on select.** `{` `}` play from the mark to the next, then pause, as `,` `.` play a slice. `a` hears whatever is selected.

- **Stepping.** `{` `}` and `,` `.` step from the selected item of their kind, else from the playhead. An audition leaves the playhead at the next mark or slice, so stepping from the playhead would skip one.

- **Outside the sampler**, `{` `}` seek to the previous or next mark, and so do `,` `.`, as before: slices are edited more than marks, so the sampler gives `,` `.` to slices and the other views keep them for marks. No other view edits marks, so they do not select.

- **With nothing selected**, `backspace` clears the range and `a` hears what it did before. The other action keys refuse with a message.

- **Snap.** `#` moves the selected item to the nearest onset. It is not the zero-crossing snap that `S` toggles. `nearest_onset` takes any frame, so one job serves all three kinds; `Sampler::snapping` records which kind the result moves.

- **Removing a range end** clears the whole range. A range with one end is not used.

- **`esc` stays `:discard`**, paired with `enter`, which writes the plan. It does not deselect: a selected slice belongs to the plan, so a second `esc` would drop the plan and its hand edits.

- **`D` deselects.** It was on trial, since a selection does nothing until an action key is pressed; it stays.

- **Out of view**, the selection shows as an arrow at that edge, as the playhead does: reversed in the terminal, a cell inside the playhead's arrow when both are past the same edge; above the playhead's in the window. Refusing edits until the selection is visible was the alternative; it would block a move of a mark just past the edge.

- **Second move binding**, not taken: `ctrl-left` and `ctrl-right` could also move the selection, so the arrows follow one pattern. macOS switches Spaces on them by default, and tmux forwards them only with extended keys on.

## Undo

`u` undoes the last edit to the playing track's marks, range or planned slices; `r` redoes. Each keeps 100 (`UNDO_DEPTH`); both clear on a track change, and a new edit empties redo.

- **Before and after, not inverses.** `dispatch` records the marks, range, plan and selection before each action and keeps them if the action changed the marks or range or edited the plan. Undo restores the difference, so an edit needs no inverse of its own, and one added later is undoable without more code. Marks are in the library database, so undoing a mark edit is a database write.

- **Plans.** Making or replacing a plan is not an edit, so undo never puts back a cut that a later one replaced. The exception is an edited plan replaced or discarded, whose starts set by hand would otherwise be lost. That step is kept where it happens: `DiscardSlices`, or a new plan landing in `Model`, outside `dispatch`. Undo restores a plan over the same cut, or across such a step, and drops a plan still being made.

- **Selection.** Undo and redo select what was selected in the state they restore, so a mark removed comes back selected.

`B` (`:mark-undo`) still removes the last mark added.

## Stage 1: marks and range ends

The selection is a track and a frame. A track holds at most one mark per frame: `add_mark_within` refuses a mark on the same frame or within `MARK_NEAR`, and `move_mark` refuses `MarkInTheWay`. A move updates the selected frame.

Range ends joined stage 1 because `{` `}`, which moved a range end, became the mark keys. `:edge +N` went with them.

Removed: `Sampler::cursor`; `Action::MoveCursor`, `SetCursor`, `PickMark`, `MoveEdge`; `:cursor`, `:mark-pick`, `:mark-nudge`, `:mark-snap`, `:mark-rm` and their aliases. Their aliases could not stay: `:del-mark` would have resolved to `:remove`, which removes a track in the queue view.

## Stage 2: slices

A slice is selected and edited by its start. Its start is also the end of the slice before, so a move resizes both. Each start stays a frame inside its neighbours; the first stays inside the range or region. Removing a start joins its slice to the one before; the first slice's start cannot go.

### Plans keep hand-set starts

`Job::cuts` holds the starts once a slice is edited, and `plan_with` cuts there instead of where `cut` would. `:slice-edges` plans the job again, so the starts survive it, and zero edges still move them to crossings. A new `:slice`, or a new onset sensitivity, makes a new job and drops them. An edit updates the plan's spans at once rather than planning again: a new plan runs on a thread, and the plan and selection would vanish meanwhile.

Rejected:

- **Edits as edge changes applied after planning.** Zero edges move every edge on a new plan, so a recorded "from" no longer matches.

- **`Cut::At(Vec<u64>)`.** `Cut` derives `Copy`, which a `Vec` drops, and the cut the plan came from would be lost.

### Dragging a slice start on a mark

After `:slice marks` every slice start sits on a mark. While the plan is shown, a click or drag there takes the slice: the plan is what is being reviewed. Discarding the plan frees the mark. Rejected: the mark always winning, which leaves such starts to the keys, and a modifier-drag, which is hidden and unlike the other drags.

### Slices planned at marks

Editing a slice of a `:slice marks` plan moves the plan's start, not the mark. Moving the mark and planning again would make a slice edit behave differently by how the plan was made. Cost: after one edit, `:slice marks` again gives a different plan from the one shown.

## Gaps

- **Zoom-dependent step.** A column is `per_column` frames, so one `>` moves 358 ms at whole-track zoom of a 4:40 track, or a few samples at full zoom.

- **Marks and slices sound the same after `:slice marks`.** `{` and `,` then play the same audio, and `>` moves the mark or the plan's start by which was pressed last. Only the drawing tells them apart: a reversed `|` for a mark, a reversed `+` for a slice.

## Settled after use

- **Select keys stay.** Slices are edited more than marks, so slices keep the unshifted `,` `.` in the sampler. Swapping would have put marks on `,` `.`, select and move on one key.

- **`backspace` with nothing selected clears the range.** A second `backspace` after removing a mark can clear it, but `u` brings it back. It leaves a plan cut from the range: `esc` discards that.

- **`D` stays** as deselect.

## Open questions

- **Hearing a range end.** A pre-roll, playing about 2 s up to the selected end, would let an end edit be heard. Probably not needed.

## Tests

`crates/playr-app/tests/model.rs` covers selection, moves, removal, onset snap, undo, redo and slice edits; `crates/playr-core/tests/samples.rs` planning from hand-set starts; `tests/render.rs` the terminal's drawing; `crates/playr-gui/tests/parity.rs` that the window reaches every action.
