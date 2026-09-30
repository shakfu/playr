# Slice formats hardware samplers read

Notes, started 2026-09-30 against playr 0.14.0. Both exports are built; see "Built" at the end. It records which file formats carry slice points and loops to a hardware sampler, so an export can be chosen. It serves decision 3 in `docs/dev/decisions.md`: exported slices serve any sampler.

Each claim carries its status:

- **read**: taken from source code or a specification read for this note.
- **reported**: a search result or a third party says so; the primary source was not read.
- **open**: not checked.

No claim here was tested on a device.

## What playr writes today

- One 24-bit WAV per slice, at the source's rate and channel count.
- `samples.json`, with each slice's start and end frame in the source. A range cut whole while it loops is marked to loop.

No other sampler reads `samples.json` (inference). The slice files carry no slice or loop data.

## Two ways to carry slices

| | one file per slice | one file with slice points |
|-|-|-|
| what playr has | yes | yes, since 2026-09-30 |
| needs metadata | no | yes: a chunk in the WAV, or a file beside it |
| uses sample slots | one per slice | one for all |

A chain is several sounds joined into one file, with a slice point at each join. Octatrack tools such as [ot_utils](https://github.com/icaroferre/ot_utils) build chains because a device has few sample slots. playr cuts one source, so its slice points already lie in one file: the range, or the track. It needs only the metadata.

## ConvertWithMoss as an export helper

[ConvertWithMoss](https://github.com/git-moss/ConvertWithMoss) converts multi-samples between about 60 formats. Read from its repository at version 20.4.0, last commit 2026-09-27:

- **It has a command line.** Any argument after the executable runs it without the window ([`documentation/README.md`](https://github.com/git-moss/ConvertWithMoss/blob/main/documentation/README.md), "Usage via the command line interface"):

  ```
  ConvertWithMoss -s sfz -d 1010music SOURCE_FOLDER DESTINATION_FOLDER
  ```

  `-s` and `-d` name the source and destination formats, and `-p KEY=VALUE` sets a format's options.

- **It reports progress for a host program.** `-P`, or `CWM_MACHINE_PROGRESS=1`, writes `CWM_PROGRESS pct=<0..100> phase=<token> detail=<text>` lines to the error output.

- **Licence: LGPL-3.0,** per its `LICENSE` file, not GPL-3.0. playr would run it as a separate program, which places no condition on playr's own licence (inference; not legal advice).

- **It is Java with JavaFX,** shipped as installers: `.deb` for Ubuntu 24.04, `.dmg`, `.exe` ([project page](https://www.mossgrabers.de/Software/ConvertWithMoss/ConvertWithMoss.html), reported). playr could not bundle it; the user installs it.

- **On Linux the `.deb` installs to `/opt/convertwithmoss/bin/ConvertWithMoss`,** which is not on `PATH`. The same executable is the window and the command line. Checked on 2026-09-30 with version 20.3.0.

- **It runs with no display:** with `DISPLAY` and `WAYLAND_DISPLAY` unset, `-V`, `-h` and the conversions under "Test conversion" all ran.

- **Format names for `-s` and `-d`,** as the program lists them: `sfz`, `ableton`, `renoise`, `mc707`, `1010music`, `bento`, `sf2`, `distingex`, and `wav`, `polyendtracker`, `opxy`, `sp404mk2`, `mpc` among the rest.

### Where playr finds it

The manual says only "locate the ConvertWithMoss executable on your system", and that on Windows the command line is `ConvertWithMossCLI.exe`, in the same folder as `ConvertWithMoss.exe`. Decided 2026-09-30: playr uses the installer's own path by default, with `convert-with-moss.path` under `[extensions]` to override it, and does not search `PATH`. A link such as `convert-wm` is the user's own and playr must not rely on it. When the file is missing, playr names the path it tried.

| platform | default path | status |
|-|-|-|
| Linux | `/opt/convertwithmoss/bin/ConvertWithMoss` | read: where the `.deb` installs |
| macOS | `/Applications/ConvertWithMoss.app/Contents/MacOS/ConvertWithMoss` | open: a guess from how `.dmg` installers work |
| Windows | `ConvertWithMossCLI.exe` in the install folder | open: the folder is not stated |

### Test conversion

Run on 2026-09-30 with version 20.3.0 and no display. The source was three 24-bit 44.1 kHz stereo WAV files and this `.sfz`:

```
<region> sample=break-01.wav key=36
<region> sample=break-02.wav key=37
<region> sample=break-03.wav key=38
```

Each run was `ConvertWithMoss -s sfz -d NAME SOURCE_FOLDER DESTINATION_FOLDER`, and each exited 0.

| `-d` | files written | audio |
|-|-|-|
| `ableton` | `Presets/Instruments/Sampler/break.adv`, `Samples/Imported/break/*.wav` | unchanged |
| `renoise` | `break.xrni` | inside the file |
| `mc707` | `break.mpj`, 8.4 MB: a whole project | inside the file; made 16-bit |
| `1010music` | `break/preset.xml`, `break/*.wav` | resampled to 48 kHz |
| `bento` | `break/patch.xml`, `break/*.wav`, `break/preview.wav` | resampled to 48 kHz |
| `sf2` | `break.sf2` | inside the file |
| `distingex` | `break.dexpreset`, `break/break_C1_SW24.wav` and so on | made 16-bit, renamed by note |

- **The key mapping survives.** The blackbox preset and the Ableton preset both hold each file on its own key, 36 to 38, with that key as its root.
- **ConvertWithMoss resamples where a device needs it,** so playr can go on writing 24-bit at the source's rate.
- **The name `break` made it choose a drum envelope.** It guesses a category from the name.
- **None of the results was loaded** in the software or on the device it is for.

### It converts key-mapped kits, not slice points

ConvertWithMoss moves zones: a sample with a key range, a root note and a loop. Its formats' documentation mentions slice points only for the Polyend Tracker, where reading turns each slice into a zone. So it would turn playr's slices into a kit with one slice per key. It would not produce one WAV with slice points, which is what the `cue ` chunk and `.ot` carry. Both results are useful, and they are different exports.

### SFZ as the one format playr writes

ConvertWithMoss reads [SFZ](https://sfzformat.com/), a text file listing regions. Its reader takes `sample`, the key opcodes, and `offset` and `end` ([`SfzDetector.java`](https://github.com/git-moss/ConvertWithMoss/blob/main/src/main/java/de/mossgrabers/convertwithmoss/format/sfz/SfzDetector.java), read). playr already writes one WAV per slice, so an `.sfz` beside them needs one line per slice:

```
<region> sample=break-01.wav key=36
<region> sample=break-02.wav key=37
```

Many software samplers load SFZ directly (reported), so the file is useful with no converter. With ConvertWithMoss installed, it becomes the source for every format below.

| wanted format | in ConvertWithMoss | written as |
|-|-|-|
| Ableton Sampler | read and write | `.adv` preset, XML |
| Renoise instrument | read and write | open |
| Roland MC-707 / MC-101 | read and write | a project holding the tones or kits |
| 1010music blackbox / bento | read and write | preset with WAV files |
| SoundFont 2 | read and write | `.sf2` |
| Expert Sleepers disting EX | read and write | `.dexpreset` |

All six are listed without "read only" in [`README-FORMATS.md`](https://github.com/git-moss/ConvertWithMoss/blob/main/documentation/README-FORMATS.md) (read), and "Test conversion" above wrote each of them.

Writing these six in playr instead would mean six writers to build and keep current, several of them binary or undocumented. ConvertWithMoss already tracks them.

## Formats

### WAV `cue ` chunk

Cue points inside the WAV. A file has at most one `cue ` chunk, holding every point ([RecordingBlogs](https://www.recordingblogs.com/wiki/cue-chunk-of-a-wave-file), read).

| field | bytes | meaning |
|-|-|-|
| chunk ID | 4 | `cue `, with the space |
| size | 4 | bytes that follow |
| count | 4 | number of cue points |

Each cue point is 24 bytes:

| field | bytes | meaning |
|-|-|-|
| ID | 4 | unique per point |
| position | 4 | order in a playlist chunk; zero without one |
| data chunk ID | 4 | `data` |
| chunk start | 4 | 0 with one data chunk |
| block start | 4 | 0 for uncompressed audio (open) |
| sample start | 4 | where the point is |

- **Byte order:** little-endian, as all of RIFF (inference; the page does not say).
- **Units of sample start (open).** RecordingBlogs says bytes from the block's start. Writers in common use are reported to store a frame index. A wrong choice puts every point in the wrong place by the frame size, so this must be tested against each device.
- **Labels.** A `LIST` chunk of type `adtl` can name each point (open).
- A cue point has no end. A slice runs to the next point (inference).

### WAV `smpl` chunk

Loop points and a root note inside the WAV. It is the usual place for a sustain loop (reported; [WAVE File Format](http://midi.teragonaudio.com/tech/wave.htm) documents it, and could not be fetched for this note). The layout is open.

### Octatrack `.ot`

A file beside the WAV with the same name: `break.wav` and `break.ot`. Layout read from `generate_ot_file` in [ot_utils `src/lib.rs`](https://github.com/icaroferre/ot_utils/blob/master/src/lib.rs), which credits OctaChainer for it. All numbers are big-endian.

| field | bytes | value ot_utils writes |
|-|-|-|
| header | 23 | `FORM`, 4 zero bytes, `DPS1`, `SMPA`, then `00 00 00 00 00 02 00` |
| tempo | 4 | BPM x 24 |
| trim length | 4 | bars x 25, see below |
| loop length | 4 | the same |
| stretch | 4 | 0 |
| loop | 4 | 0 |
| gain | 2 | 48 |
| quantize | 1 | 255 |
| trim start | 4 | 0 |
| trim end | 4 | total frames |
| loop point | 4 | 0 |
| 64 slices | 12 each | start frame, end frame, loop point; zeros when unused |
| slice count | 4 | |
| checksum | 2 | sum of every byte from offset 16 |

The file is 832 bytes.

- **64 slices at most.** [AudioHit](https://github.com/icaroferre/AudioHit) writes several `.ot` files past that (reported).
- **The slice's second field is its end,** not its length: the code writes `start_point + length`.
- **The slice's loop point is its length** in ot_utils. Whether that means no loop is open.
- **Bars** are computed as `124 * frames / (rate * 60) + 0.5`, truncated, times 25. The constant 124 is used whatever the tempo field holds. Whether that is the device's rule or a shortcut is open.
- **The meaning of stretch, loop, gain 48 and quantize 255 is open.** ot_utils writes constants.
- **ot_utils takes mono 16-bit WAV only** and defaults to 44.1 kHz. That is the library's limit. What the Octatrack itself accepts is open; playr writes 24-bit at the source's rate.
- **Licence.** ot_utils is GPL-3.0 and AudioHit is MIT (both reported). playr would write the format from this layout, not take the code.

### OP-1 and OP-Z drum kit

One AIFF, 16-bit big-endian, with an `APPL` chunk holding JSON: start and end per slice, volume, pan, pitch, play mode. 24 slices at most ([chirashi](https://pkg.go.dev/github.com/g-lok/chirashi), reported). The JSON's field names, the scale of start and end, and the longest kit are open.

### Polyend Tracker `.pti`

An instrument file holding the audio and its slices. [polyend/tracker-lib](https://github.com/polyend/tracker-lib) reads and writes the project files, and [pti-tools](https://github.com/jaap3/pti-tools) builds a sliced instrument (both reported). The layout is open.

## Devices

| device | slices from | status | source |
|-|-|-|-|
| Elektron Octatrack | `.ot` beside the WAV | read, from a writer's code | [ot_utils](https://github.com/icaroferre/ot_utils) |
| Dirtywave M8 | WAV `cue ` points | reported; up to 32 markers, reported | [DigiChain 1.4.6](https://brian3kb.itch.io/digichain/devlog/870054/v146-dirtywave-m8-slice-support) |
| 1010music Blackbox | WAV `cue ` points, read and written | reported; the forum post returned 403 | [WAV tags we support](https://forum.1010music.com/forum/general-topics/19862-wav-tags-we-support) |
| Teenage Engineering OP-1, OP-Z | AIFF with an `APPL` chunk | reported | [chirashi](https://pkg.go.dev/github.com/g-lok/chirashi) |
| Polyend Tracker | `.pti` | reported | [tracker-lib](https://github.com/polyend/tracker-lib) |
| Akai MPC | | open | |
| Roland SP-404MK2 | | open; its app adds markers by hand (reported) | [SP-404MK2 app manual](https://static.roland.com/manuals/sp-404mk2_app/eng/78775991.html) |
| Elektron Digitakt | | open | |

[DigiChain](https://brian3kb.itch.io/digichain) exports for the Octatrack, the OP line, the EP-133 and others (reported). Its source is a second reference for each layout.

## Open questions

- **Frame index or byte offset in a cue point's sample start.** Check what the M8 and the Blackbox expect.
- **Which devices read `smpl` loops.** `TODO.md`, "Loop points in the WAV", depends on it.
- **Bit depth and rate each device accepts.** playr writes 24-bit at the source's rate; ot_utils assumes 16-bit 44.1 kHz mono.
- **Slice limits.** 64 on the Octatrack, 24 on the OP-1, 32 reported for the M8. An export must refuse or split past the limit.
- **Whether the converted kits load.** Each was written without error; none was opened in Live, Renoise or on a device.

- **ConvertWithMoss's install paths on macOS and Windows.**

- **Where the cue points come from.** The plan's edges directly, or marks made from them. See "Planned edges to marks" in `TODO.md`.

## Decided

On 2026-09-30: build both exports. Both are built; see "Built" below.

1. **A kit: `.sfz` beside the slice files.** A few lines of text per export. It serves software samplers as it is, and ConvertWithMoss turns it into the six formats above. playr could offer to run ConvertWithMoss when it is installed.
2. **A sliced loop: one WAV of the range with a `cue ` chunk.** It reaches the M8 and the Blackbox, if the reports hold. An `.ot` file beside the same WAV is 832 bytes of fixed layout and adds the Octatrack.

The OP-1 kit and `.pti` each need a second audio container or a larger format, for one device each; ConvertWithMoss writes the Polyend Tracker and the OP-XY already.

## Built

The kit export, on 2026-09-30:

- **Every export writes `NAME.sfz`,** named after its directory, with the first slice on key 36. Keys stop at 127, so slices past the 92nd get no region.
- **`:convert FORMAT`** runs ConvertWithMoss on that file, into `EXPORT/FORMAT`. The source is the `.sfz` file, not the directory, which also holds earlier conversions.
- **ConvertWithMoss exits 0 when it fails.** A wrong format name and a missing sample both did. playr takes a conversion as failed when the destination is empty, and reports the first line of the error output.
- **A format already converted is refused.** Run twice into one directory, ConvertWithMoss writes `break (2).sf2` beside the first.
- **Checked with the real program:** a kit laid out as playr writes it, with spaces in the file names, converted to `1010music` and `sf2`. Whether the loop opcodes of a looped slice reach the converted preset was not checked.
- **It is an extension, off as shipped.** `:convert` runs a program that is not playr's, so it needs `convert-with-moss.enable = true` under `[extensions]` in `settings.toml`. Until then the command is left out of `:help` and Tab completion, the window's Slice menu has no Convert to, and `:convert` typed anyway says how to enable it. The cheatsheet lists it under Extensions.
- **Enabled, it still needs ConvertWithMoss installed.** Without it the command is refused at once, naming the path tried, and the window's Convert to is disabled with that text as its hover. The path is looked up at each use, so installing takes effect without a restart.
- **While it is off, only the full name finds the command.** A prefix such as `:conv` is an unknown command, and `:co` means `:columns` as it did before; no error lists `convert`. Enabled, `:co` is ambiguous between `columns` and `convert`. A key bound to `convert sf2` in `settings.toml` is accepted either way.
- **Not built:** progress from `-P`, and a way to convert an export from an earlier run.

The sliced file, on 2026-09-30:

- **Every export writes `sliced/NAME.wav`,** and `sliced/NAME.ot` with `slice_ot_file = true`, off by default; both are named after the export's directory. The WAV runs from the first slice's start to the last one's end, read from the source a second time, so fades on the slice files do not reach it.
- **The `cue ` chunk** follows the audio, after a pad byte when the audio ends on an odd byte, and has a point at each slice's start, the first at frame 0. Both position fields of a point hold the frame.
- **Frames, not bytes.** This settles "Units of sample start" above by what writers in common use do (inference); RecordingBlogs says bytes. It is the first thing to check on an M8 or a blackbox: a byte reading would put each slice at a fraction of where it belongs.
- **The `.ot` file is byte for byte what ot_utils 0.1.5 writes.** Three mono 44.1 kHz files of 1,000, 2,500 and 40,000 frames went through ot_utils, and `sliced::ot_file` given the same slices returned the same 832 bytes. A test holds them. This shows the layout is ot_utils's, not that an Octatrack accepts it.
- **Tempo is fixed at 124,** as ot_utils's default, and the length in bars is counted at it. The track's analysed tempo is not used.
- **More than 64 slices: no `.ot` file.** The WAV and its cue points are still written.
- **The rate and bit depth are the source's,** 24-bit. Nothing is resampled. Which rates an Octatrack plays is open; ot_utils assumes 44.1 kHz.
- **ffprobe, sox and Python's `wave` read the WAV** with no warning. None of them reports cue points, so the chunk was checked by reading its bytes back.
- **Not built:** a `smpl` chunk for a looped range, labels for the cue points, and any check on a device.
