# The library

playr keeps an index of your music in one SQLite file, the library. It holds what it read from your files, what `playr analyze` measured, and what you made: playlists, marks, loops, saved searches. It never writes to the music files themselves.

The terminal, the window and the server all read the same library. The README has the details: [The library](../README.md#the-library), [Analysis](../README.md#analysis), [Columns and order](../README.md#columns-and-order), [Search](../README.md#search) and [Keys](../README.md#keys). [`cheatsheet.md`](cheatsheet.md) lists every command. This page explains how the parts fit together.

## Building the library

```sh
playr scan ~/music       # index a directory, recursively
playr analyze            # measure loudness, tempo and checks
```

In the window: File, Add folder to library, then File, Analyze library. Inside playr: `:scan ~/music` and `:analyze`.

- **Scanning** reads each file's tags and stream properties: title, artist, album, rate, channels, bit depth, length. It commits every 500 files, so an interrupted scan keeps what it did. Only a scan creates the library; until then playr plays files but cannot save playlists.

- **Roots.** Each scanned directory is remembered as a root. `:rescan`, File, Rescan library, or a bare `playr scan` covers every root again, re-reading only files whose size or modification time changed. `:roots` lists the roots.

- **Analysis** decodes each track once and records its loudness and peak, for ReplayGain; its tempo and beat grid, for the tempo column, `:slice beats` and the DJ decks; and checks such as damaged files and likely transcodes. A later run decodes only what changed. With `analyze_on_scan = true` in `settings.toml`, a scan inside playr analyses what it added.

The library file is `~/.local/share/playr/library.db`, or under `$XDG_DATA_HOME`; `--db PATH` uses another. Only one of the three programs runs at a time.

## Finding tracks

`/` opens the search, and the library filters as you type. Enter plays the results.

| you want | type |
|-|-|
| words anywhere: title, artist, album, album artist, file name | `bill evans` |
| one field | `artist:evans`, `album:"kind of blue"` |
| a range | `year:1955..1965`, `time:5:00..` |
| a tempo | `bpm:120..130`; a track read at half tempo also answers at double |
| loudness or peak | `loudness:..-14`, `peak:0.99..` |
| what analysis flagged | `is:damaged`, `is:lossy`, `is:duplicate`, `is:unanalysed` |

Every term narrows the results; there is no OR or NOT. For anything else, `:sql` lists the tracks a `SELECT` names, as in `:sql SELECT path FROM library GROUP BY album HAVING count(*) = 1`. The README lists the views and columns it reads.

`:save-search NAME` keeps a search, or an `:sql` statement, in the Playlists view. It runs again each time you open it, so it finds tracks added or analysed since.

## Ordering

The list you see is the list that plays. `:sort tempo desc` or a click on a column heading in the window reorders it, and enter then plays in that order. Searching filters and sorting orders, so `bpm:170..180` sorted by loudness plays everything near 174 BPM, loudest first.

`:columns artist title tempo` chooses the columns. `columns` and `sort` in `settings.toml` set them for good, for all programs or, in a `[terminal]`, `[gui]` or `[server]` table, for one. The `tempo`, `loudness` and `peak` columns need `playr analyze`; tracks it has not measured sort last.

## Playing

Enter plays the list you are looking at, from the track under the cursor:

- **In the library**, it plays on through the library. Nothing is queued.

- **On search results, a playlist or the selection**, it plays that list instead, and replaces the queue with it.

**Modes**, on `m` and `M`: normal, shuffle, repeat, repeat one. A mode applies to whatever list is playing. `:stop after` stops at the end of the track; `:stop in 30:00` stops 30 minutes from now.

When playr closes it remembers the track and position, and offers to take it up again at the next start.

## Three places to put tracks

playr keeps three lists apart, each for its own job:

| | holds | made with | plays |
|-|-|-|-|
| **selection** | tracks you are collecting, to save as a playlist | `a` on a track or a playlist | only when you play it |
| **queue** | tracks to play next, over whatever is playing | `e`, `E` to play first, `A` for all listed | at once, then the library resumes |
| **playlists** | saved lists, and saved searches | `s` saves the selection; `s` in the queue view saves it | from the Playlists view |

### The selection

`a` adds the track under the cursor, or removes it if it is marked `+`; on a playlist it adds its tracks. Adding never interrupts what plays. In the selection view, `J` and `K` move a track, `d` removes one, `c` empties it. `s` saves it as a playlist, asking before it replaces one.

As it changes, playr keeps the selection in a playlist called `draft`, so a crash or a quit loses nothing. At the next change after a restart it asks whether to overwrite the old draft, append it, or save it under a name. `draft` in `settings.toml` answers for you.

To change a saved playlist, press `o` on it: the selection becomes the playlist, titled "Editing", and `s` saves it back.

### The queue

`e` queues the track or playlist under the cursor. Over the library, the first track queued plays at once and the rest wait; `E` puts a track first. When the queue has played, the library resumes at the track after the one the queue interrupted; `after_queue = "stop"` stops instead.

The queue view shows what has played, dimmed, then what plays and what waits. Enter jumps to a track; `d` takes one out; `s` saves the whole queue as a playlist, so you can queue, listen, prune and keep. The queue is kept between runs unless `keep_queue = false`.

### Playlists

The Playlists view lists the playlists, then the saved searches. Enter plays one; `a` adds it to the selection; `e` queues it; `r` renames; `d` deletes.

Outside playr, `playr export NAME FILE.m3u8` writes a playlist as M3U8 with absolute paths, for other players or as a backup, and `playr import FILE.m3u` saves one. An import leaves out tracks not in the library and lists them; scan their folder first.

## Looking at one track

`:info`, or Track info in a row's menu, shows what analysis found about one track:

- its format, loudness, peak and the ReplayGain it gets;

- its tempo, and how sure playr is of it;

- where its content stops, which shows a lossy source;

- how many of its bits it uses;

- its FLAC checksum, and any findings.

A tempo read an octave off is fixed with `:bpm x2` or `:bpm /2` on the playing track. The correction is kept apart from the analysis, so analysing again keeps it.

## What the library keeps, and how to lose nothing

| kept in the library | from |
|-|-|
| tracks: tags and stream properties | the files, on each scan |
| loudness, tempo, beat grid, findings | `playr analyze` |
| playlists, saved searches, the draft | you |
| marks and saved loops, by file path | you, in the sampler |
| tempo corrections, DJ grid edits and hot cues | you |
| the queue, and the track to take up again | playr, as it plays |
| remembered settings, with `persist` | playr |

Everything in the first two rows can be rebuilt by scanning and analysing again. The rest is your work, kept by file path.

- **A scan never removes anything.** It counts the tracks under its directory whose files are gone, and asks whether to prune them; `playr scan` prints the count.

- **Pruning** (`playr prune`, `:prune`, File, Remove missing files) removes those tracks, their places in playlists, and their marks, loops, hot cues and edits. It is the one operation that loses your work. Prune after a file is moved or deleted for good, never while a drive is unplugged: an unplugged drive's files look missing. `auto_prune = true` prunes without asking, except when a directory reads as empty though the library holds tracks under it, which is what an unmounted drive looks like.

- **Forgetting a root** (`:roots rm DIR`, or Forget in File, Library directories) removes the root and everything under it, without checking the disk.

- **Moving files** breaks the link, since everything is kept by path. A rescan of the new place adds the tracks afresh, and pruning the old place then removes the old rows with their playlist places, marks and loops. Export the playlists first to keep a record of them.

- **Backups.** `playr export` writes playlists to M3U8. Copying `library.db` while playr is closed keeps everything.
