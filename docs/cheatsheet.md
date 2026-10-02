# Commands

`:` opens a command line on the bottom row. `:help` shows this list inside playr.

A command works in every view, or only in the view named by its heading. Typed in another view, it is refused with the view it belongs to. The key column gives the default key for the same action. `:map` and `:unmap` change keys until playr exits; [`settings.toml`](../README.md#configuration) changes them for good.

## Arguments

- `TIME` is seconds, `m:ss` or `h:mm:ss`, as in `90`, `1:23`, `1:02:03`.

- A leading `+` or `-` makes a number relative: `:seek +10`, `:volume -5`, `:speed +1`. Without a sign it is absolute. `=` makes a signed `:volume`, `:speed` or `:eq` value absolute: `:speed =-3`.

- `NAME` and `QUERY` run to the end of the line, so spaces need no quotes.

- `[ ]` marks an optional argument. Without it, `:search`, `:save`, `:save-search` and `:rename` open their prompt.

- `PATH` and `DIR` run to the end of the line; a leading `~` is the home directory, and a relative path is relative to where playr started. `:scan` runs in the background, reports progress on the bottom line, and creates the library if there is none. It keeps tracks whose files are gone and counts them, then asks to prune, or prunes at once if `auto_prune` is set. `:rescan`, or `:sync`, re-scans every directory previously given to `:scan` or `playr scan`. `:roots` lists those directories; `:roots add DIR` is another spelling of `:scan DIR`, and `:roots rm DIR` forgets one, removing the tracks and marks under it after asking. `:prune` removes the tracks of missing files, their places in playlists, and the marks of missing files under `DIR`, after asking; with no directory it covers every directory previously scanned. `:open` adds the files to the end of the selection and plays them.

- Renamed commands keep their old names, so bindings written for them still work: `:next-view` and `:prev-view` are `:view next` and `:view prev`; `:unmark`, `:delmarks`, `:next-mark`, `:prev-mark` and `:move-mark` are `:mark-undo`, `:mark-clear`, `:mark-next`, `:mark-prev` and `:mark-move`; `:clear-search` is `:search-clear`; in the queue, `:dequeue`, `:reorder` and `:queue-clear` are `:remove`, `:move` and `:clear`.

- `:slice` acts on the playing track and writes to the `samples` directory; `:slice onsets` without `S` uses `onset_sensitivity`. Both are set in [`settings.toml`](../README.md#configuration). See [Samples](../README.md#samples).

## Search

What `/`, `:search` and `playr search` accept. [Search](../README.md#search) has the details.

| form | example | matches |
|-|-|-|
| words | `bill evans` | every word, as the start of a word, in title, artist, album, album artist or file name |
| field | `artist:evans`, `file:take2` | one word in `title`, `artist`, `album`, `albumartist` or `file` |
| quoted | `artist:"bill evans"` | the words together, in order |
| column | `genre:jazz`, `path:/backup/` | text anywhere in that column, ignoring case |
| number | `year:1959`, `year:1955..1965`, `time:5:00..` | a value as shown, or a range; either end may be left open |
| measured | `loudness:..-14`, `peak:0.99..` | LUFS, and peak from 0 to 1, from `playr analyze` |
| tempo | `bpm:128`, `bpm:120..130` | within 1 BPM, or a range; also at double a halved tempo |
| finding | `is:damaged`, `is:lossy`, `is:duplicate` | what `playr analyze` flagged; also `unreadable`, `padded`, `no-checksum`, `wrong-length`, `upsampled`, `unanalysed` |

Terms combine, and each narrows the results; there is no OR or NOT. `:sql SELECT path ...` answers the rest, and `:save-search` keeps either.

## Every view

| command                               | key                     | does                                        |
|---------------------------------------|-------------------------|---------------------------------------------|
| `:help`                               |                         | list these commands                         |
| `:keys`                               | `?`                     | list the keys for this view                 |
| `:quit`                               | `q`                     | quit                                        |
| `:view VIEW \| next \| prev`          | `1` to `5`, `tab`, shift-tab | a view by its tab's name, or next or prev |
| `:down [N]`                           | `j`, down, page down    | move the cursor down N rows, default 1      |
| `:up [N]`                             | `k`, up, page up        | move the cursor up N rows, default 1        |
| `:first`                              | `g`, home               | move the cursor to the first row            |
| `:last`                               | `G`, end                | move the cursor to the last row             |
| `:play`                               | `enter`                 | play the list in view from the cursor       |
| `:enqueue [next \| all]`              | `e` `E` `A`             | queue the row, first, or every row          |
| `:search [QUERY]`                     | `/`                     | search the library; no query opens /        |
| `:playlist NAME`                      |                         | play a saved playlist                       |
| `:save [NAME]`                        | `s`                     | save the selection as a playlist            |
| `:save-search [NAME]`                 |                         | keep the search shown, to run again         |
| `:sql SELECT path ...`                |                         | list the tracks a query names               |
| `:scan DIR`                           |                         | add a directory to the library              |
| `:rescan`                             |                         | re-scan directories previously added; `:sync` |
| `:roots [add\|rm DIR]`                |                         | list the directories the library covers      |
| `:columns NAME...`                    |                         | which columns a track list shows, in order  |
| `:sort KEY[ desc]... \| off`          |                         | sort every track list by these columns      |
| `:info`                               |                         | what analysis measured about this track     |
| `:analyze [DIR]`                      |                         | measure loudness, tempo and file health     |
| `:prune [DIR]`                        |                         | remove tracks and marks of missing files    |
| `:open PATH`                          |                         | play a file or directory, and select it     |
| `:pause`                              | `space`                 | play or pause                               |
| `:next`                               | `n`                     | next track                                  |
| `:prev`                               | `p`                     | previous track                              |
| `:stop [after \| in TIME \| in off]`   | `x`                     | now, after this track, or in TIME           |
| `:restart`                            | `R`                     | play from the range's start, or the track's |
| `:seek TIME \| +TIME \| -TIME`        | left, right, with shift | seek to a time, or by one: 1:23, +10        |
| `:volume PERCENT \| +N \| -N`         | `+` `-`                 | set the volume, or change it: 60, +10       |
| `:speed N \| =-N \| +N \| -N`         | `(` `)` `\`             | set varispeed in semitones, or change it    |
| `:eq BAND =N \| +N \| -N \| flat`     |                         | bass, mid or treble, -12 to 12 dB           |
| `:mode MODE \| + \| -`                | `m` `M`                 | normal, shuffle, repeat, repeat-one, + or - |
| `:replaygain SETTING`                 |                         | level by loudness: off, track, album, auto  |
| `:slice-edges exact\|zero\|fade`       |                         | slice edges: exact, at zeros, or faded      |
| `:mark [TIME]`                        | `b`                     | mark the playing position, or a time        |
| `:mark-undo`                          | `B`                     | undo the last mark                          |
| `:mark-clear`                         | `C`                     | clear all marks in this track; asks y/n     |
| `:mark-next`                          | `}`                     | next mark: seek, or select in the sampler   |
| `:mark-prev`                          | `{`                     | prev mark: seek, or select in the sampler   |
| `:undo`                               | `u` in the sampler      | undo the last edit to marks or the range    |
| `:slice region\|marks\|N\|onsets [S]` |                         | write samples from the region or the track  |
| `:loop off`                           |                         | stop looping                                |
| `:map [VIEW] KEY COMMAND`             |                         | bind a key, in one view or in all           |
| `:unmap [VIEW] KEY`                   |                         | remove a key binding                        |
| `:theme THEME`                        |                         | system, light or dark colours               |

## Library

| command         | key   | does                         |
|-----------------|-------|------------------------------|
| `:toggle`       | `a`   | select or unselect the track |
| `:search-clear` | `esc` | show the whole library again |

## Selection

| command          | key                     | does                                |
|------------------|-------------------------|-------------------------------------|
| `:remove`        | `d` backspace delete    | remove the track from the selection |
| `:move +N \| -N` | `J` `K`, shift up, down | move the track N places             |
| `:clear`         | `c`                     | empty the selection; asks y/n       |

## Queue

| command             | key                     | does                             |
|---------------------|-------------------------|----------------------------------|
| `:remove`           | `d` backspace delete    | take the track out of the queue  |
| `:move +N \| -N`    | `J` `K`, shift up, down | move the track N places          |
| `:clear`            | `c`                     | empty the queue, played too; asks y/n |
| `:add`              | `a`                     | add the track to the selection   |
| `:save [NAME]`      | `s`                     | save the queue as a playlist     |

## Playlists

| command          | key | does                                       |
|------------------|-----|--------------------------------------------|
| `:add`           | `a` | add the playlist's tracks to the selection |
| `:delete`        | `d` | delete the playlist; asks y/n              |
| `:edit`          | `o` | edit the playlist in the selection         |
| `:rename [NAME]` | `r` | rename the playlist                        |

## Sampler

| command                            | key                     | does                                   |
|------------------------------------|-------------------------|----------------------------------------|
| `:zoom + \| - \| all`              | `z` `Z` `0`             | zoom in, out, or to the whole track    |
| `:display [DISPLAY]`               | `w`                     | envelope, db, braille or spectrogram   |
| `:nudge +N \| -N \| +N% \| -N%`     | left, right, with shift | move N columns, or N% of the view      |
| `:snap [on\|off]`                  | `S`                     | snap moves and marks to zero crossings |
| `:fit [on\|off]`                   | `f`; `\|` is `:fit on`  | zoom to the range and keep it centred  |
| `:in`                              | `i`                     | start the range at the playhead        |
| `:out`                             | `o`                     | end the range at the playhead          |
| `:range [START END]`               |                         | set the range to slice, or clear it    |
| `:loop [on\|off] \| N [save\|clear]` | `l`; F1-F8, with shift  | loop the range or region, or recall or save |
| `:loops clear`                    |                         | clear this track's loops; asks y/n     |
| `:audition [next\|prev]`          | `a`, `,` `.`            | play the selection, a slice, the range or region once; step slices |
| `:scrub TIME`                     | drag, with Scrub on     | play a moment from a time              |
| `:select TIME`                    | click a mark            | select the mark at a time              |
| `:edge start\|end`                 | `[` `]`                 | select a range end                     |
| `:deselect`                       | `D`                     | select nothing                         |
| `:move +N\|-N\|N%`                 | `<` `>`                 | move the selected mark or range end    |
| `:mark-move TIME`                 | drag a mark             | move the selected mark to a time       |
| `:onset`                          | `#`                     | move the selection to the nearest rise |
| `:remove`                         | `backspace`             | remove the selected mark, or the range |
| `:write`                           | `enter`                 | write the slices :slice planned        |
| `:discard`                         | `esc`                   | discard the planned slices             |

In this view `:slice` plans slices and draws their edges as `+` under the waveform; `:write` writes them. The arrows nudge by a column, or with shift a tenth of the view, so zooming in makes them finer. A range, drawn as `[` and `]`, replaces the region for every cut, and `:slice marks` cuts only at the marks inside it. With snap on, nudges, marks, seeks and range ends made in this view move to the nearest zero crossing within 10 ms; turning snap on moves the ends of a range already set. With `:fit on`, the view centres on the range rather than the playhead, so zooming keeps the range in view; `[` or `]` then centres it on that end, and `|` on the whole range again. Marks made in this view may be a frame apart; elsewhere they stay 500 ms apart. `l` loops the range; a selected end moves the loop with it.

The edit keys act on one selected item: a mark, selected by `{` `}`, `b` or a click, or a range end, selected by `[` `]`. A selected end is shown reversed. `{` `}` play from the mark to the next, and step from the selected mark rather than the playhead. With nothing selected, `backspace` clears the range and the other edit keys refuse. `u` undoes the last change to the marks or the range, as far back as the track started playing.

## Extensions

An extension runs a program that is not part of playr. Each is off, and its command left out of `:help` and of Tab completion, until enabled under `[extensions]` in [`settings.toml`](../README.md#configuration).

| command           | enable with                        | does                                    |
|-------------------|------------------------------------|-----------------------------------------|
| `:convert FORMAT [EXPORT]` | `convert-with-moss.enable = true`  | last export, or EXPORT, via ConvertWithMoss |

## Typing commands

- A command, mode or view can be shortened to a prefix that names only one of those usable in the current view: `:vol 60`, `:mode shuf`.

- Tab completes command names usable in the current view, then the argument of `:edge`, `:fit`, `:loop`, `:loops`, `:mode`, `:replaygain`, `:slice-edges`, `:snap`, `:theme`, `:view`, `:playlist` and `:rename`. Repeated Tab cycles through the matches; shift-Tab goes back.

- Up recalls earlier lines that start with the typed text; down returns towards it. The history holds 100 lines and lasts until playr exits.

- Enter runs the line, `esc` cancels, and backspace on an empty line closes the prompt.
