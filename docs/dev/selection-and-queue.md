# Selection and queue

Status: option A adopted and done, unreleased; option B deferred. `TODO.md` links here from "Selection and queue overlap". The sections up to "Questions" describe the state before A.

Since 0.13.0, `:save` works in both the Selection and Queue views. The two lists now do much the same job. This file compares them and weighs four ways to resolve the overlap.

## What each list is

| | Selection | Queue |
|-|-|-|
| Purpose | tracks to keep, not yet heard | tracks to hear soon, then maybe keep |
| Filled by | `a` in the library or playlists, files on the command line | `e`, `E`, `A` |
| Affects playback | only when played with enter | yes: spliced after the track playing |
| Rows | tracks, in the order added | played, playing, waiting |
| Held in | `Session::selection` | `Session::played`, plus the player's list for playing and waiting rows |
| Kept between runs | no | yes, in `resume_queue`, offered at start |
| Saved with | `:save` | `:save` in its view |

## Overlap

- Both are ordered track lists the listener edits by hand.

- Both have remove, move and clear. The same three `Action`s serve both, under different command names: `remove`/`dequeue`, `move`/`reorder`, `clear`/`queue-clear`.

- Both save as a playlist, through one `Session::save_tracks`.

- Both play: enter on the selection replaces the queue with it.

- Each has its own view, empty-state text, row menu in the window and the page, and toolbar. `View::Selection` appears at 32 sites in the three frontends and `playr-app`, `View::Queue` at 42.

## Differences that matter

1. The selection never changes what plays by itself. The queue does, the moment a track is added.

2. The queue records what was heard. The selection does not.

3. The queue survives a restart. The selection does not. A selection built over an evening is lost on exit.

4. The library marks selected rows with `+`. Queued rows are not marked.

## Options

### A. Keep both, close the gaps

- Keep the selection between runs, as the queue is kept.

- Bind `a` in the Queue view to add the row to the selection. Today a track moves from selection to queue (`e`, `A`), but not back.

- Give the shared actions one name in both views: `:remove`, `:move`, `:clear`. Pre-1.0 renames need the owner's approval (decision 4).

Cost: small. Each item is independent. Risk: the two views stay. A new user still has to learn which list does what.

### B. One list, each row marked

A single working list, whose rows are each played, playing, waiting or kept. `e` adds a waiting row, `a` a kept row. Playback plays waiting rows and skips kept ones. `:save` saves every row.

Cost: large. One view and one set of commands go, and each of the 74 sites above changes. The player's list would hold only playing and waiting rows. Mapping view rows to player indices while kept rows sit between waiting ones adds bookkeeping to every remove and move. Risk: if kept rows may not sit between waiting ones, the view is two sections: kept below, the queue above. That is option A in one view.

### C. Queue only

Remove the selection. `a` queues without playing: it adds a row marked "not yet", as in B.

This is option B with the same costs. It also removes "play the selection", which is how the primary user shuffles across playlists today (README, Playback modes).

### D. Selection only; the queue plays through it

The selection gains a play position. `e` appends to it and moves playback there.

Cost: medium. It breaks difference 1: an edit to the selection would change what plays. That is the property the selection was built for. Rejected unless that property is dropped deliberately.

## Recommendation

Option A now. It fixes the observed loss (difference 3) and the one-way movement, and it is cheap.

Revisit B together with two related TODO items that also change the selection's role:

- "Editing a playlist" makes the selection a playlist editor. Done since: `:edit`, with the edit kept in the draft.

- "Multi-row selection" uses "selection" for a different idea: rows picked in any view.

If both land, the name "selection" means two things. Settling A, the playlist editor and multi-row picking together avoids renaming twice.

## Questions for the owner, answered

1. Is "play the selection" in use? Yes: the selection must play as a playlist does. Enter on it already does. This rules out option C.

2. Should a kept selection be offered at start, or restored silently? Keep both the queue and the selection between runs, and let the user turn either off. First restored silently; later replaced by the draft playlist below, which is visible. The queue is still offered, as it changes what plays.

3. Is the rename to one set of command names approved (decision 4)? Yes.

## What A changed

- `keep_queue`, on by default. A boolean over a `persist` name: a user's `persist` list replaces the default, so naming `eq` alone would have turned it off.
- The selection starts empty each run and is saved as it changes to the reserved playlist `draft`. A draft left by an earlier run is settled at the first change: overwrite, append or save as a name, asked by default, or set by `draft`. This replaced restoring the selection silently, which could carry a stale selection into a new playlist unseen.
- The selection is stored in a `selection` table at each change.
- `a` in the Queue view adds the track to the selection.
- The queue's commands are `:remove`, `:move` and `:clear`; the old names are aliases.
