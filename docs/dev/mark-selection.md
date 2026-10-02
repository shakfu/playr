# Mark selection

Status: proposal. Nothing here is implemented.

The sampler edits marks through a cursor, a frame apart from the playhead. This file proposes replacing the cursor with a selected mark (design A), and a variant that also selects planned slices (design B).

## Problem

- **The cursor is detached from listening.** A mark is placed by ear, at the playhead, with `b`. Editing it means moving a second, silent position onto it with `u` or `i`. Nothing is heard at the cursor.

- **The window did not draw the cursor** until the Unreleased fix. `u` and `i` looked dead there.

- **The cursor goes stale.** Only `:cursor off` (`h`) clears it (`dispatch.rs:599`). Removing the mark under it, or changing track, leaves it in place. It is a bare frame, not tied to a track as `Range` is, so `y`, `o`, `#` and `delete` can act on a mark of the next track that happens to sit near the old frame.

- **Two "next mark" commands.** `,` `.` seek the playhead to a mark; `u` `i` move the cursor to one. `docs/dev/ui-refactor.md:46` records the two sharing one label.

The playhead cannot simply replace the cursor. Every mark edit finds the mark within one column of its target (`near`, `dispatch.rs:1231`). During playback the playhead leaves that column within one column's time: 358 ms at the whole-track zoom of a 4:40 track, microseconds at full zoom. `,` then `o` would usually find no mark.

## Current keys

Sampler view unless marked global.

| Target | Select | Move | Snap | Remove | Hear |
|-|-|-|-|-|-|
| Mark | `u` `i` (cursor) | `y` `o` | `#` | `delete` | `,` `.` (global, seek) |
| Range end | `[` `]` | `{` `}` | none | `backspace` (whole range) | `a` |
| Planned slice | none | none | none | none | `n` `p` |
| Cursor | `;` `'` `h` | | | | |

## Design A: a selected mark

The sampler keeps one selected mark, on a track. Editing acts on it, wherever the playhead has moved since.

| Key | Action |
|-|-|
| `,` `.` | Seek to the previous or next mark, as now, and select it. |
| `b` | Add a mark at the playhead and select it. |
| click or drag on a mark (window) | Select it. A drag moves it, as now. |
| `y` `o` | Move the selected mark a column, then seek to it. |
| `#` | Snap the selected mark to the nearest onset, then seek to it. |
| `delete` | Remove the selected mark. Nothing is selected after. |

The selection clears when its mark is removed by any command, including `B` and `C`, and on a track change.

`,` and `.` stay global. Outside the sampler they seek and do not select, since no other view edits marks.

The selected mark is drawn distinct from the others: thicker in the window, a different glyph on the terminal axis. The dashed cursor line and the `#` axis glyph go.

### Removed

- `Sampler::cursor`, `Action::MoveCursor`, `SetCursor`, `PickMark`.

- `:cursor` and `:mark-pick`, and the keys `;` `'` `h` `u` `i`. `left` and `right` already move the playhead a column.

- Sampler, Marks: Cursor earlier, Cursor later, Cursor to playhead.

Five keys come free. Renamed commands kept their old names as working aliases (`CHANGELOG.md:123`). These have no equivalent to alias, so they either refuse with a message naming the replacement or are dropped.

### Seeking after an edit

Seeking after a move or snap lets the result be heard at once. Two cases need a rule:

- **Loop on.** Seeking to a mark outside the looped range leaves the loop. Proposed: do not seek while a loop is on.

- **Playing.** Each press restarts playback from the mark. Several presses in a row stutter. This is the same as pressing `,` repeatedly now.

Alternative: seek only when paused, and audition from the mark when playing. That changes what `a` hears, so it is not proposed here.

## Design B: slices selected with slice keys

Design A, plus planned slices as a second kind of selection. Slices are selected with their own keys. They are moved, snapped and removed with the mark keys.

| Key | Action |
|-|-|
| `n` `p` | Audition the next or previous planned slice, as now, and select it. |
| `y` `o` | Move the selected slice's start edge a column. |
| `#` | Snap the selected slice's start edge to the nearest onset. |
| `delete` | Remove the selected slice's start edge, merging it into the slice before. |

One thing is selected at a time: the mark or slice selected last. `,` `.` `b` select a mark; `n` `p` select a slice. The edit keys act on whichever it is. The selection replaces a mode switch, so there is no mode key and no mode to forget.

A slice's start edge is also the previous slice's end, so moving it resizes both. The first slice's start cannot move before the range or region start. Removing the first slice's start is refused.

### Plans become editable

A plan holds `spans` computed from its `Job` (`samples.rs:282`). Today nothing edits them, and `:slice-edges` replans from the job (`dispatch.rs:523`), so a hand edit would be lost.

Proposed: a new `Cut::At(Vec<u64>)`, slicing at the given frames. The first edit converts the plan's job to `Cut::At` with its current edges. Later replans keep the edits. The plan label shows "edited".

### Slices planned at marks

`:slice marks` cuts at the marks. Its edges and the marks are then the same frames. Two choices:

1. **Edit the plan only.** Marks stay where they were; the plan diverges from them. Simple, and consistent with equal and onset plans.

2. **Edit the mark, then replan.** The plan stays a cut at marks. The edit is kept with the track, as marks are.

Proposed: 1. Choice 2 makes a slice edit behave differently by how the plan was made.

### Range ends

`[` `]` `{` `}` could fold in the same way: `[` `]` select an end, and `y` `o` move it. That frees `{` `}`. Left out of B, since the range is not a slice, and `delete` on an end has no clear meaning.

## Comparison

| | A | B |
|-|-|-|
| Selection kinds | mark | mark, slice |
| Keys removed | 5 | 5 |
| Keys gaining a meaning | `,` `.` `b` select | also `n` `p` select |
| Core change | none | `Cut::At`; plans keep edits |
| Hand-edited slices | no | yes |
| Risk | seeking after an edit while playing | also: an edit key hits a slice when a mark was meant, if `n` `p` were pressed last |

The B risk is the mode error from a modal design, reduced to one rule: the last thing selected. The selected item must be drawn clearly in both frontends for that rule to be visible.

## Questions

- Should editing a mark while a loop plays elsewhere stay possible? A drops it unless seeks are skipped while looping.

- In B, does `delete` on a slice remove its start or end edge? Start is proposed, matching `y` `o`.

- Should `:mark-pick` and `:cursor` stay as refusing aliases, or go?

- Is design C, folding the range ends into the selection, wanted now or later?

## Tests

- Selection follows `,` `.` `b` and a click; clears on removal by `delete`, `B`, `C`, and on a track change.

- `y` `o` `#` act on the selected mark after the playhead has moved on.

- No selection: edit keys refuse with a message.

- B: `n` `p` select a slice; `y` moves the shared edge of two slices; `delete` merges; the first slice's start is refused.

- B: an edited plan survives `:slice-edges`.

- Parity: `crates/playr-gui/tests/parity.rs` still finds every action reachable in the window.
