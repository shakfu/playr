# Commands

`:` opens a command line on the bottom row. `:help` shows this list inside playr.

A command works in every view, or only in the view named by its heading. Typed in another view, it is refused with the view it belongs to. The key column gives the default key for the same action. `:map` and `:unmap` change keys until playr exits; [`settings.toml`](../README.md#configuration) changes them for good.

## Arguments

- `TIME` is seconds, `m:ss` or `h:mm:ss`, as in `90`, `1:23`, `1:02:03`.

- A leading `+` or `-` makes a number relative: `:seek +10`, `:volume -5`, `:speed +1`. Without a sign it is absolute. `=` makes a signed speed absolute: `:speed =-3`.

- `NAME` and `QUERY` run to the end of the line, so spaces need no quotes.

- `[ ]` marks an optional argument. Without it, `:search`, `:save` and `:rename` open their prompt.

- `PATH` and `DIR` run to the end of the line; a leading `~` is the home directory, and a relative path is relative to where playr started. `:scan` runs in the background, reports progress on the bottom line, and creates the library if there is none. It keeps tracks whose files are gone and counts them, then asks to prune, or prunes at once if `auto_prune` is set. `:rescan`, or `:sync`, re-scans every directory previously given to `:scan` or `playr scan`. `:roots` lists those directories; `:roots add DIR` is another spelling of `:scan DIR`, and `:roots rm DIR` forgets one, removing the tracks and marks under it after asking. `:prune` removes the tracks of missing files, their places in playlists, and the marks of missing files under `DIR`, after asking; with no directory it covers every directory previously scanned. `:open` adds the files to the end of the selection and plays them.

- `:slice` acts on the playing track and writes to the `samples` directory; `:slice onsets` without `S` uses `onset_sensitivity`. Both are set in [`settings.toml`](../README.md#configuration). See [Samples](../README.md#samples).

## Every view

| command                               | key                     | does                                        |
|---------------------------------------|-------------------------|---------------------------------------------|
| `:help`                               |                         | list these commands                         |
| `:keys`                               | `?`                     | list the keys for this view                 |
| `:quit`                               | `q`                     | quit                                        |
| `:view VIEW`                          | `1` `2` `3` `4`         | library, selection, playlists or sampler    |
| `:next-view`                          | `tab`                   | switch to the next view                     |
| `:down [N]`                           | `j`, down, page down    | move the cursor down N rows, default 1      |
| `:up [N]`                             | `k`, up, page up        | move the cursor up N rows, default 1        |
| `:first`                              | `g`, home               | move the cursor to the first row            |
| `:last`                               | `G`, end                | move the cursor to the last row             |
| `:play`                               | `enter`                 | play the list in view from the cursor       |
| `:search [QUERY]`                     | `/`                     | search the library; no query opens /        |
| `:playlist NAME`                      |                         | play a saved playlist                       |
| `:save [NAME]`                        | `s`                     | save the selection as a playlist            |
| `:scan DIR`                           |                         | add a directory to the library              |
| `:rescan`                             |                         | re-scan directories previously added; `:sync` |
| `:roots [add\|rm DIR]`                |                         | list the directories the library covers      |
| `:prune [DIR]`                        |                         | remove tracks and marks of missing files    |
| `:open PATH`                          |                         | play a file or directory, and select it     |
| `:pause`                              | `space`                 | play or pause                               |
| `:next`                               | `n`                     | next track                                  |
| `:prev`                               | `p`                     | previous track                              |
| `:stop`                               | `x`                     | stop                                        |
| `:seek TIME \| +TIME \| -TIME`        | left, right, with shift | seek to a time, or by one: 1:23, +10        |
| `:volume PERCENT \| +N \| -N`         | `+` `-`                 | set the volume, or change it: 60, +10       |
| `:speed N \| =-N \| +N \| -N`         | `(` `)` `\`             | set varispeed in semitones, or change it    |
| `:mode MODE \| + \| -`                | `m` `M`                 | normal, shuffle, repeat, repeat-one, + or - |
| `:mark [TIME]`                        | `b`                     | mark the playing position, or a time        |
| `:unmark`                             | `B`                     | undo the last mark                          |
| `:delmarks`                           | `C`                     | clear all marks in this track; asks y/n     |
| `:next-mark`                          | `.`                     | seek to the next mark                       |
| `:prev-mark`                          | `,`                     | seek to the previous mark                   |
| `:slice region\|marks\|N\|onsets [S]` |                         | write samples from the region or the track  |
| `:map [VIEW] KEY COMMAND`             |                         | bind a key, in one view or in all           |
| `:unmap [VIEW] KEY`                   |                         | remove a key binding                        |
| `:theme THEME`                        |                         | system, light or dark colours               |

## Library

| command         | key   | does                         |
|-----------------|-------|------------------------------|
| `:toggle`       | `a`   | select or unselect the track |
| `:clear-search` | `esc` | show the whole library again |

## Selection

| command          | key                     | does                                |
|------------------|-------------------------|-------------------------------------|
| `:remove`        | `d`                     | remove the track from the selection |
| `:move +N \| -N` | `J` `K`, shift up, down | move the track N places             |
| `:clear`         | `c`                     | empty the selection; asks y/n       |

## Playlists

| command          | key | does                                       |
|------------------|-----|--------------------------------------------|
| `:add`           | `a` | add the playlist's tracks to the selection |
| `:delete`        | `d` | delete the playlist; asks y/n              |
| `:rename [NAME]` | `r` | rename the playlist                        |

## Sampler

| command                            | key                     | does                                   |
|------------------------------------|-------------------------|----------------------------------------|
| `:zoom + \| - \| all`              | `z` `Z` `0`             | zoom in, out, or to the whole track    |
| `:display [envelope\|db\|braille]` | `w`                     | draw the waveform another way          |
| `:nudge +N \| -N \| +N% \| -N%`     | left, right, with shift | move N columns, or N% of the view      |
| `:snap [on\|off]`                  | `S`                     | snap moves and marks to zero crossings |
| `:in`                              | `<`                     | start the range at the playhead        |
| `:out`                             | `>`                     | end the range at the playhead          |
| `:range [START END]`               | `backspace`             | set the range to slice, or clear it    |
| `:loop [on\|off]`                  | `l`                     | play the range over and over           |
| `:audition`                       | `a`                     | play the range, slice or region once   |
| `:cursor TIME\|+N\|-N\|N%\|off`     | `;` `'` `h`             | move the cursor; `h` returns it to the playhead |
| `:pick next\|prev`                 | `u` `i`                 | move the cursor to a mark              |
| `:nudge-mark +N\|-N\|N%`           | `y` `o`                 | move the mark under the cursor         |
| `:move-mark TIME`                 | drag it                 | move it to a time                      |
| `:snap-mark`                      | `#`                     | move it to the nearest rise            |
| `:del-mark`                       | `delete`                | remove it                              |
| `:edge start\|end \| +N \| -N \| +N%` | `[` `]`, then `{` `}`   | pick a range end, or move it N columns |
| `:write`                           | `enter`                 | write the slices :slice planned        |
| `:discard`                         | `esc`                   | discard planned slices, else the range |

In this view `:slice` plans slices and draws their edges as `+` under the waveform; `:write` writes them. The arrows nudge by a column, or with shift a tenth of the view, so zooming in makes them finer. A range, drawn as `[` and `]`, replaces the region for every cut, and `:slice marks` cuts only at the marks inside it. With snap on, nudges, marks, seeks and range ends made in this view move to the nearest zero crossing within 10 ms. Marks made in this view may be a frame apart; elsewhere they stay 500 ms apart. `l` loops the range; `[` or `]` picks an end, shown reversed, for `{` and `}` to move while it loops. `esc` clears the range once no slices are planned.

## Typing commands

- A command, mode or view can be shortened to a prefix that names only one of those usable in the current view: `:vol 60`, `:mode shuf`.

- Tab completes command names usable in the current view, then the argument of `:edge`, `:loop`, `:mode`, `:snap`, `:theme`, `:view`, `:playlist` and `:rename`. Repeated Tab cycles through the matches; shift-Tab goes back.

- Up recalls earlier lines that start with the typed text; down returns towards it. The history holds 100 lines and lasts until playr exits.

- Enter runs the line, `esc` cancels, and backspace on an empty line closes the prompt.
