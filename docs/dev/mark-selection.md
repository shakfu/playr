# Mark selection

Status: stage 1 (marks and range ends) is implemented; stage 2 (slices) is proposed.

The sampler edits marks through a cursor, a frame apart from the playhead. This file replaces the cursor with a selected mark, then extends the selection to planned slices and range ends, with one set of action keys for all three.

## Problem

- **The cursor is detached from listening.** A mark is placed by ear, at the playhead, with `b`. Editing it means moving a second, silent position onto it with `u` or `i`. Nothing is heard at the cursor.

- **The window did not draw the cursor** until the Unreleased fix. `u` and `i` looked dead there.

- **The cursor goes stale.** Only `:cursor off` (`h`) clears it (`dispatch.rs:599`). Removing the mark under it, or changing track, leaves it in place. It is a bare frame, not tied to a track as `Range` is, so `y`, `o`, `#` and `delete` can act on a mark of the next track that happens to sit near the old frame.

- **Two "next mark" commands.** `,` `.` seek the playhead to a mark; `u` `i` move the cursor to one. `docs/dev/ui-refactor.md:46` records the two sharing one label.

The playhead cannot simply replace the cursor. Every mark edit finds the mark within one column of its target (`near`, `dispatch.rs:1231`). During playback the playhead leaves that column within one column's time: 358 ms at the whole-track zoom of a 4:40 track, microseconds at full zoom. `,` then `o` would usually find no mark.

Alternative: keep the playhead as the target, and widen `near` while playing to a fixed time, such as the last mark crossed within 500 ms. This needs no selection state and no new drawing. It fails when marks sit closer than the window, and the target changes as playback runs: a second `o` can hit a different mark, or none.

## Current keys

Sampler view unless marked global.

| Target | Set at playhead | Select | Move | Snap | Remove | Hear |
|-|-|-|-|-|-|-|
| Mark | `b` (global) | `u` `i` (cursor) | `y` `o` | `#` | `delete`; `B` last added, `C` all (global) | `,` `.` (global, seek) |
| Range end | `<` `>` | `[` `]` | `{` `}` | none | `backspace` (whole range) | `a` |
| Planned slice | | none | none | none | none | `n` `p` |
| Cursor | | `;` `'` `h` | | | | |

`esc` (`:discard`) discards the plan, or with none planned clears the range (`dispatch.rs:835`).

## Options

Marks are captured during playback and edited while paused. Three ways to edit marks, slices and range ends were considered.

1. **The playhead is the target.** No selection. An edit acts on the mark at the playhead.
   - For: no state and no new drawing. It matches how marks are placed.
   - Against: it covers marks only. The playhead identifies the slice it is in, but not which edge, and never a range end. Slices and range ends would still need a second mechanism.

2. **Each kind is selected with its own keys; one set of action keys acts on the selection.** One item is selected at a time: the one selected last.
   - For: selecting sets the kind, so there is no mode key and no extra keystroke.
   - Against: still modal, with the mode set as a side effect of selecting. Slices need editable plans.

3. **A mode key picks the kind; shared keys then select and act.**
   - For: fewest keys; a new kind costs no new keys.
   - Against: the mode is hidden state, and both frontends must show it at all times. The select keys are shared too, so a wrong mode moves the wrong item with no audio cue. Every kind switch costs a keystroke. `f1`-`f8` and `shift-f1`-`shift-f8` hold loop slots, and F9-F12 are often taken by the OS or terminal (F10 menu and F11 fullscreen in GNOME Terminal).

### Decision

- **Option 2, in two stages.** Stage 1 is design A plus range ends, done regardless. Stage 2 follows if hand-edited slices prove needed.

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
| Slice | `,` `.` | `D` | `,` `.` | `<` `>` | `#` | `backspace` |
| Range end | `[` `]`, click | `D` | `a` | `<` `>` | `#` | `backspace` |

| Key | Action |
|-|-|
| `i` `o` | `:in` and `:out`: set the range start or end at the playhead. |
| `u` | Undo the last edit. See [Undo](#undo). |
| `esc` | `:discard`: discard the plan, as now. With none planned it refuses; it no longer clears the range. |

- **Hearing on select.** `{` `}` audition from the mark to the next, then pause, as `,` `.` audition a slice. `a` hears whatever is selected.

- **Stepping.** `{` `}` and `,` `.` step from the selected item of their kind, else from the playhead. An audition leaves the playhead at the next mark or slice, so stepping from the playhead would skip one.

- **Outside the sampler**, `{` `}` seek to the previous or next mark, as `,` `.` do now. No other view edits marks, so they do not select. `,` `.` have no global binding.

- **With nothing selected**, `backspace` clears the range, as now, and `a` hears what it does now. The other action keys refuse with a message.

- **Snap.** `#` moves the selected item to the nearest onset. It is not the zero-crossing snap that `S` toggles, which still applies when a mark or range end is set, and to slice edges on export (`:slice-edges`). `nearest_onset` (`samples.rs:165`) takes any frame, so one job serves all three kinds.

- **Removing a range end** clears the whole range. A range with one end is not used.

- **`esc` stays `:discard`**, paired with `enter`, which writes the plan. Its range fallback goes, since `backspace` clears the range. `esc` does not deselect: in stage 2 a selected slice belongs to the plan, so a second `esc` would drop the plan and its hand edits, which undo cannot restore.

- **`D` deselects, on trial.** A selection does nothing until an action key is pressed, so deselect may go unused. If it does, `D` is dropped and the selection clears only on removal or a track change.

- **Second move binding.** Option: `ctrl-left` and `ctrl-right` also move the selected item a column. The arrows then follow one pattern: `left` `right` nudge the playhead a column, `shift-left` `shift-right` nudge it 10%, and `ctrl-left` `ctrl-right` move the selection. `Key::parse` already accepts `ctrl-` (`action.rs:337`), and the window passes ctrl through (`playr-gui/src/keys.rs:63`). They are a second binding, not a replacement for `<` `>`: macOS switches Spaces on them by default, and tmux forwards them only with extended keys on.

Keys freed: `y`, `h`, `;`, `'`, `delete`, and `n` `p` in the sampler. `n` `p` then skip tracks in the sampler, as in every other view.

## Undo

`u` undoes the last edit to a mark, slice edge or range end. One history per session, holding edits to the playing track. It clears on a track change, since the range and the plan clear then too.

| Edit | Undone by |
|-|-|
| Mark added (`b`) | removing it |
| Mark moved or snapped | moving it back |
| Mark removed (`backspace`) | adding it again |
| Marks cleared (`C`) | adding them all again |
| Slice edge moved or removed | restoring the plan's previous edits |
| Range end set, moved or cleared | restoring the previous range |

Marks are stored in the library database (`query::add_mark`, `query::move_mark`, `query::remove_mark`), so undoing a mark edit is a database write. An undo that would collide with a mark added since is refused, as `add_mark_within` refuses now.

`B` (`:mark-undo`) removes the last mark added. `u` covers it, so `B` goes or stays as an alias.

## Stage 1: marks and range ends (design A)

The sampler keeps one selected item, on a track: a mark or a range end. Editing acts on it.

Stage 1 takes the whole [Keys](#keys) table except the slice row. That includes `i` `o` for `:in` `:out`, `{` `}` as the global mark keys, `,` `.` unbound, `D` to deselect, and `esc` without its range fallback. `:in` `:out` cannot be split off: `<` `>` move the selection.

Range ends join in stage 1 because `{` `}`, which move a range end now, become the mark keys. `[` `]` select an end, as they pick one now. `<` `>` move it. `#` puts it on an onset, so a loop starts or ends on a transient. No core change is needed.

Undo ships in stage 1 for marks and the range: every row of the [Undo](#undo) table except slice edges, which stage 2 adds.

The selection clears when its mark is removed by any command, including `C` and `u`, on `D`, and on a track change.

The selected mark is drawn distinct from the others: thicker in the window, a different glyph on the terminal axis. The dashed cursor line and the `#` axis glyph go.

The selection is a track and a frame. A track holds at most one mark per frame: `add_mark_within` refuses a mark on the same frame or within `MARK_NEAR` (`session.rs:1346`), and `move_mark` refuses `MarkInTheWay` (`session.rs:1389`). A move updates the selected frame.

### Removed

- `Sampler::cursor`, `Action::MoveCursor`, `SetCursor`, `PickMark`.

- `:cursor` and `:mark-pick`. `left` and `right` already move the playhead a column.

- Sampler, Marks: Cursor earlier, Cursor later, Cursor to playhead.

Renamed commands kept their old names as working aliases (`CHANGELOG.md:95`). These have no equivalent to alias, so they either refuse with a message naming the replacement or are dropped.

### Gaps

- **Off-screen selection.** Resolved: the selection stays, and an arrow at the axis edge points to it, as for the playhead. Refusing edits until it is visible was the alternative; it would block a move of a mark just past the edge.

- **Zoom-dependent step.** A column is `per_column` frames, so one `>` moves a mark 358 ms at whole-track zoom of a 4:40 track, or a few samples at full zoom.

## Stage 2: slices (design B)

Planned slices become a second kind of selection. `,` `.` audition the previous or next planned slice and select it. The action keys act on its start edge.

`#` on an onset plan's edge changes nothing, since the edge is already an onset. It serves equal and mark plans, and edges moved with `<` `>`.

A slice's start edge is also the previous slice's end, so moving it resizes both. The first slice's start cannot move before the range or region start. Removing the first slice's start is refused.

### Plans become editable

A plan holds `spans` computed from its `Job` (`samples.rs:284`). Today nothing edits them, and `:slice-edges` replans from the job (`dispatch.rs:523`), so a hand edit would be lost.

Proposed: store edits as edge changes on the plan, applied after planning. Later replans keep them. The plan label shows "edited".

Alternative: a new `Cut::At(Vec<u64>)`, slicing at given frames, with the first edit converting the job to it. Rejected: `Cut` derives `Copy` (`samples.rs:28`), which a `Vec` drops, and the original job is lost, so edits cannot be undone.

### Slices planned at marks

`:slice marks` cuts at the marks. Its edges and the marks are then the same frames. Two choices:

1. **Edit the plan only.** Marks stay where they were; the plan diverges from them. Simple, and consistent with equal and onset plans.

2. **Edit the mark, then replan.** The plan stays a cut at marks. The edit is kept with the track, as marks are.

Proposed: 1. Choice 2 makes a slice edit behave differently by how the plan was made. Cost: after one edit, re-running `:slice marks` gives a different plan from the one shown.

### Gaps

- **Off-screen slice in the terminal.** The "last selected" rule is visible only if the selected slice is drawn. When it is off screen, nothing shows what `>` will act on.

- **Mode error.** An action key hits a slice when a mark was meant, if `,` `.` were pressed last. The selected item must be drawn clearly in both frontends.

## Questions

- **Select keys for marks and slices.** Marks are the most edited kind, but both their select keys (`{` `}`) and move keys (`<` `>`) need shift, while slices get `,` `.`. Swapping them puts marks on `,` `.`, whose shifted forms on a US layout are `<` `>`: select and move on one key. It also keeps the global `,` `.` as mark seek, so nothing global is rebound.

- **Marks and slices after `:slice marks`.** A mark and a slice edge then sound the same, so `{` and `,` audition the same audio. `>` then moves the mark or the plan edge depending on which was pressed last, and under choice 1 the two diverge silently. Options: draw the selected kind clearly in both frontends, or use choice 2 for mark plans alone, so a slice edit there moves the mark.

- **Hearing a range end.** `[` `]` select without hearing, and `a` hears the whole range. A pre-roll, playing about 2 s up to the selected end, would let an end edit be heard. Probably not needed: there are two ends, so they are not stepped through by ear.

- **`backspace` with nothing selected.** Removing a mark clears the selection, so a second `backspace` clears the range. `u` undoes it. Alternative: refuse `backspace` with nothing selected; the range is still cleared by selecting an end and pressing `backspace`.

- Is redo wanted? Not proposed.

- Should editing a mark while a loop plays elsewhere stay possible?

- In stage 2, does `backspace` on a slice remove its start or end edge? Start is proposed, matching `<` `>`.

- Should `:mark-pick`, `:cursor` and `:mark-undo` stay as refusing aliases, or go?

## Tests

- `{` `}` audition from the mark to the next and select it; `b` and a click select.

- Selection follows `{` `}` `b` and a click; clears on removal by `backspace`, `C`, `u`, on `D`, and on a track change.

- `{` `}` step from the selected mark, not the playhead; outside the sampler they seek.

- `<` `>` `#` act on the selected mark after the playhead has moved on.

- A move updates the selected frame; a move onto another mark is refused and the selection stays.

- No selection: `backspace` clears the range; `<` `>` `#` refuse with a message.

- `i` `o` set the range ends at the playhead.

- `esc` discards the plan and keeps a selected mark or range end; with no plan it refuses and leaves the range.

- Stage 2: `esc` clears a selected slice with its plan.

- `u` reverses each edit in the [Undo](#undo) table, including the database write for marks; the history clears on a track change.

- Stage 2: `,` `.` select a slice; `>` moves the shared edge of two slices; `#` moves it to an onset; `backspace` merges; the first slice's start is refused.

- Stage 2: an edited plan survives `:slice-edges`.

- `[` `]` select an end; `<` `>` `#` move it; `backspace` clears the range.

- Parity: `crates/playr-gui/tests/parity.rs` still finds every action reachable in the window.
