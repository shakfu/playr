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

  ```sh
  ConvertWithMoss -s sfz -d 1010music SOURCE_FOLDER DESTINATION_FOLDER
  ```

  `-s` and `-d` name the source and destination formats, and `-p KEY=VALUE` sets a format's options.

- **It reports progress for a host program.** `-P`, or `CWM_MACHINE_PROGRESS=1`, writes `CWM_PROGRESS pct=<0..100> phase=<token> detail=<text>` lines to the error output.

- **Licence: LGPL-3.0,** per its `LICENSE` file, not GPL-3.0. playr would run it as a separate program, which places no condition on playr's own licence (inference; not legal advice).

- **It is Java with JavaFX,** shipped as installers: `.deb` for Ubuntu 24.04, `.dmg`, `.exe` ([project page](https://www.mossgrabers.de/Software/ConvertWithMoss/ConvertWithMoss.html), reported). playr could not bundle it; the user installs it.

- **On Linux the `.deb` installs to `/opt/convertwithmoss/bin/ConvertWithMoss`,** which is not on `PATH`. The same executable is the window and the command line. Checked on 2026-09-30 with version 20.3.0.

- **It runs with no display:** with `DISPLAY` and `WAYLAND_DISPLAY` unset, `-V`, `-h` and the conversions under "Test conversion" all ran.

- **Format names for `-s` and `-d`,** as the program lists them: `sfz`, `ableton`, `renoise`, `mc707`, `1010music`, `bento`, `sf2`, `distingex`, and `wav`, `polyendtracker`, `opxy`, `sp404mk2`, `mpc` among the rest.

- **The formats ConvertWithMoss reads or writes.** `[x]` marks a format playr supports: SFZ, which playr writes, and the 16 names in `convertwithmoss::FORMATS`, which `:convert` and Convert to offer and "Test conversion" below wrote. `:convert` also takes any other `-d` name below when typed, untested. A read-only format cannot be a destination. The `-d` names are each writer's prefix in lower case, read from `CLIBackend.java` at commit 31b0f8a (2026-09-27).

  1. [x] 1010music Bento (`preset.xml`) - `-d bento`

  2. [x] 1010music blackbox, tangerine, bitbox (`preset.xml`) - `-d 1010music`

  3. [x] Ableton Sampler (`*.adv`, `*.adg`) - `-d ableton`

  4. [ ] Akai MESA (`*.s3p`) - read only

  5. [x] Akai MPC Keygroups (`*.xpm`) - `-d mpc`

  6. [ ] Akai MPC Projects (`*.xpj`) and Tracks (`*.xty`) - read only

  7. [ ] Akai MPC60 Sets (`*.hfe`, `*.img`, `*.set`) - read only

  8. [ ] Akai MPC500/MPC1000/MPC2500 (`*.PGM`) - read only

  9. [ ] Akai MPC2000/MPC2000XL/MPC3000 programs (`*.hfe`, `*.img`, `*.iso`, `*.PGM`, `*.SND`) - read only

  10. [ ] Akai S900/S950 programs (`*.img`, `*.akai`) - read only

  11. [ ] Akai S1000/S3000 ISO images (`*.iso`) - read only

  12. [ ] Akai S5000/S6000/Z4/Z8/MPC4000 (`*.akp`, `*.akm`) - read only

  13. [ ] Arturia Synclavier V (`*.synx`) - `-d synclavierv`

  14. [ ] Audiomodern Soundbox format (`*.sbpack`) - `-d soundbox`

  15. [ ] Casio FZ-1/FZ-10M/FZ-20M format (`*.img`, `*.hfe`, `*.fzf`, `*.fzv`, `*.fzb`) - `-d casiofz`

  16. [ ] CWITEC TX16Wx (`*.txprog`, `*.txbank`, `*.txperf`) - `-d tx16wx`

  17. [ ] DecentSampler (`*.dspreset`, `*.dslibrary`) - `-d decentsampler`

  18. [ ] disoDSP Bliss (`*.zbp`, `*.zbb`) - `-d zbp`

  19. [ ] Downloadable Sound format (DLS) - read only

  20. [ ] E-mu Emulator II (`*.img`, `*.emuiifd`, `*.hfe`) - `-d eii`; the list says read only, the source has a writer

  21. [ ] E-mu Emulator III/IIIX/ESI (`*.e3b`, `*.e3x`, `*.esi`) - `-d eiii`

  22. [ ] E-mu Emulator IV bank (`*.e4b`, `*.iso`, `*.img`, `*.hda`) - `-d e4b`

  23. [ ] E-mu Emulator X (`*.exb`) - `-d exb`

  24. [x] Elektron Tonverk (`*.emulti`) - `-d emulti`

  25. [ ] Elektron Tonverk preset (`*.tvpst`) - `-d tonverk`

  26. [ ] Ensoniq EPS/EPS16+/ASR-10 (`*.hfe`, `*.img`, `*.gkh`, `*.ede`, `*.eda`, `*.efe`) - read only

  27. [ ] Ensoniq Mirage (`*.hfe`, `*.img`, `*.edm`) - read only

  28. [x] Expert Sleepers disting EX (`*.dexpreset`) - `-d distingex`

  29. [ ] Fairlight CMI (`*.vc`, `*.imd`, `*.img`, `*.hfe`) - `-d cmi3`

  30. [ ] FL Studio DirectWave format (`*.dwp`) - `-d directwave`

  31. [x] ISLA Instruments S2400 (`*.kit`) - `-d s2400`

  32. [ ] Korg KMP/KSF (`*.KMP`) - `-d kmp`

  33. [ ] Korg wavestate/modwave (`*.korgmultisample`) - `-d korgmultisample`

  34. [ ] Kurzweil K2000/K2500/K2600 (`*.krz`, `*.k25`, `*.k26`) - `-d kurzweil`

  35. [x] Logic EXS24 (`*.exs`) - `-d exs24`

  36. [ ] Multisample Format - Bitwig Studio, Presonus Studio One (`*.multisample`) - `-d bitwig`

  37. [x] Native Instruments Kontakt 1-8 (`*.nki`) - `-d nki`; writes Kontakt 1 only

  38. [ ] Native Instruments Maschine 1 Sound (`*.msnd`) - `-d maschine`; which of the two it writes is open

  39. [ ] Native Instruments Maschine 2-3 Sound (`*.mxsnd`) - `-d maschine`; fails without `-pMaschineOutputFormat`, see "Third test conversion"

  40. [ ] Polyend Tracker (PTI) instrument format - `-d polyendtracker`; keeps only the first slice of a kit, see "Second test conversion"

  41. [x] Propellerhead Reason NN-XT (`*.sxt`) - `-d sxt`

  42. [x] Renoise instrument (XRNI) - `-d renoise`

  43. [x] Roland MC-707/MC-101 project (`*.mpj`) - `-d mc707`

  44. [ ] Roland MV-8000/MV-8800 patch (`*.mv0`) - `-d mv8000`

  45. [ ] Roland S-10/S-220/MKS-100 (`*.syx`) - read only

  46. [ ] Roland S-50, S-330, S-550, W-30 (`*.img`, `*.iso`, `*.out`, `*.sdk`) - read only

  47. [ ] Roland S-750, S-770, S-760, DJ-70, DJ-70 MkII, and SP-700 (`*.img`, `*.iso`, `*.out`) - read only

  48. [x] Roland SP-404MK2 (`*.smp`) - `-d sp404mk2`

  49. [ ] Roland ZEN-Core sound format (`*.svz`) - `-d zencore`

  50. [ ] Sample Files: AIFF, FLAC, OGG, NCW, WAV files - `-d wav`

  51. [ ] Sequential Prophet X - `-d prophetx`

  52. [ ] Spectrasonics Omnisphere 3 (`*.prt_omn`, `*.zmap`) - `-d omnisphere`

  53. [x] SFZ (`*.sfz`) - written by playr itself, as the source of every conversion

  54. [x] SoundFont 2 (`*.sf2`) - `-d sf2`

  55. [ ] Synclavier Regen timbre/library (`*.sflc`) - `-d synclavierregen`

  56. [x] Synthstrom Deluge instrument (`*.xml`) - `-d deluge`

  57. [ ] TAL Sampler (`*.talsmpl`) - `-d talsampler`

  58. [x] Teenage Engineering OP-XY multi-sample preset format (`*.preset`) - `-d opxy`

  59. [ ] Waldorf Quantum MkI, MkII / Iridium / Iridium Core (`*.qpat`) - `-d qpat`

  60. [ ] Yamaha YSFC format (read/write: Montage, MODX/MODX+; read, waveforms only: Motif XS, Motif XF, MOXF, Montage M) (`*.ysfc`) - `-d ysfc`

  The source also has four writers this list lacks: E-mu Emax (`-d emax`), E-mu Emulator (`-d ei`), Kurzweil PC3/Forte (`-d pc3`) and Groove Synthesis 3rd Wave (`-d thirdwave`).

### Where playr finds it

The manual says only "locate the ConvertWithMoss executable on your system", and that on Windows the command line is `ConvertWithMossCLI.exe`, in the same folder as `ConvertWithMoss.exe`. Decided 2026-09-30: playr uses the installer's own path by default, with `convert-with-moss.path` under `[extensions]` to override it, and does not search `PATH`. A link such as `convert-wm` is the user's own and playr must not rely on it. When the file is missing, playr names the path it tried.

| platform | default path | status |
|-|-|-|
| Linux | `/opt/convertwithmoss/bin/ConvertWithMoss` | read: where the `.deb` installs |
| macOS | `/Applications/ConvertWithMoss.app/Contents/MacOS/ConvertWithMoss` | read: the path inside the `.app`, checked with 20.3.0. The `.app` sits wherever it was dragged, so another folder needs `convert-with-moss.path` |
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

### Second test conversion

Run on 2026-10-01 with version 20.3.0 on macOS, for the formats added to `FORMATS` that day. The kit was laid out as playr writes one: three 24-bit 44.1 kHz stereo WAV files named as `samples::file_name` names them, and `break.sfz` with one region a slice on keys 36 to 38. A second kit added playr's loop opcodes, `loop_mode=loop_continuous loop_start=0 loop_end=21999`, to each region. Each run exited 0; ConvertWithMoss does whether or not it fails, so each result was read.

| `-d` | files written | slices | loop | audio |
|-|-|-|-|-|
| `mpc` | `break/break.xpm`, `break/*.WAV` | keys 36-38 | written, but the instrument is one-shot; see below | unchanged |
| `sp404mk2` | `break/PADCONF.BIN`, `break/SMPL/BANK1-0N.SMP` | pads 1-3 | open: binary | 16-bit 48 kHz, per its log |
| `opxy` | `break.preset/patch.json`, `break.preset/*.wav` | keys 36-38; the lowest reaches down to 0 | kept | made 16-bit |
| `deluge` | `SYNTHS/break.xml`, `SAMPLES/break/*.wav` | a range a key, transposed to play at pitch | kept | unchanged |
| `emulti` | `break/break.elmulti`, `break/break-000-036-c1.wav` and so on | keys 36-38 | kept, its end scaled to 48 kHz | resampled to 48 kHz |
| `s2400` | `break/break.kit`, `break/*.wav` | pads, by name | in each WAV's `smpl` chunk | made 16-bit |
| `polyendtracker` | `break.pti` | **the first only** | | made 16-bit |

- **The OP-XY keeps frame counts from before resampling.** A 48 or 96 kHz slice is made 44.1 kHz, but `patch.json` keeps its old `framecount`, `sample.end` and `loop.end`: 24,000 or 48,000 for 22,050 frames. At 44.1 kHz they match. Test X6 in `docs/dev/device_tests.md` checks what the device does; `TODO.md` has it, and the upstream report is drafted in `docs/dev/issues/convertwithmoss-issue.md`.

- **The OP-XY takes 24 zones.** A 40-slice kit, converted on 2026-10-01, kept slices 1-24 on keys 36-59. ConvertWithMoss says so on its output, unmarked among the progress: "The preset has 40 regions but the device plays at most 24, the rest is dropped." It exits 0. `:convert` shows such lines in its message since that day; before, a conversion that dropped slices said only "converted to". ConvertWithMoss's source, `OpXyCreator.java`, holds the limit as `MAX_REGIONS`.

- **Polyend Tracker is not offered.** Its log says an instrument holds one sample and the others are ignored, so a kit loses every slice but the first. "Decided" below counted it as covered; it is not. A `.pti` with slice points would need the sliced WAV as its source, not the kit.

- **The MPC's root note is one above each key:** `RootNote` 37 on key 36. It is the format's convention: ConvertWithMoss's reader subtracts 1, with a comment that the root note is "strangely one more" (`MPCModernDetector.java`).

- **An MPC loop is lost to one-shot.** The writer stores the loop (`SliceLoop` 1, `SliceEnd` at its end) but sets `TriggerMode` 0, one-shot, for a zone on one key with no sustain (`MPCKeygroupCreator.java`), which every playr slice is. Its own reader then ignores the loop. That the device plays it through is inference.

- **This settles the loop question in "Built" for the other five:** a looped slice's loop reaches the preset. The SP-404MK2's was read back through ConvertWithMoss, below.

- **None of the results was loaded** on its device.

### Third test conversion

Run on 2026-10-01 with version 20.3.0 on macOS, for software samplers. A host that loads SFZ itself gains nothing from a conversion, so each was looked up first. These are reported, from search results and forum posts, not read in each product's manual:

| host | reads SFZ | `-d` |
|-|-|-|
| Logic's Sampler | no ([Logic Pro Help](https://www.logicprohelp.com/forums/topic/140255-sfz-libraries-in-logic-pro/)) | `exs24` |
| Reason's NN-XT | no; Reason 12 also dropped SF2 ([ReasonTalk](https://forum.reasontalk.com/viewtopic.php?t=7524502)) | `sxt` |
| Kontakt 6 and later | no; Kontakt 5 did ([vi-control](https://vi-control.net/community/threads/is-there-a-good-way-to-port-a-sforzando-sample-set-to-kontakt.163662/)) | `nki` |
| Maschine 2 | no import found | `maschine` |
| Bitwig's Sampler | yes ([Bitwig user guide](https://www.bitwig.com/userguide/latest/browsers/)) | not offered |
| DecentSampler | yes, basic mappings ([Decent Sampler Q&A](https://www.decentsamples.com/qa/11/is-there-an-sfz-to-decent-sampler-format-converter)) | not offered |
| TX16Wx | yes ([KVR](https://www.kvraudio.com/product/tx16wx-software-sampler-by-cwitec)) | not offered |
| TAL-Sampler | yes, with loops ([TAL](https://tal-software.com/products/tal-sampler)) | not offered |
| FL Studio DirectWave | yes ([manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/plugins/DirectWave.htm)) | not offered |

The four that do not were converted from the kits of "Second test conversion". Their files are binary, so each was converted back to SFZ by ConvertWithMoss and read.

| `-d` | files written | slices | loop | audio |
|-|-|-|-|-|
| `exs24` | `break/break.exs`, `break/*.wav` | keys 36-38 | kept | unchanged |
| `sxt` | `break/break.sxt`, `break/*.wav` | keys 36-38 | kept | unchanged |
| `nki` | `break.nki`, `break Samples/*.wav` | keys 36-38 | kept | unchanged |
| `maschine` | nothing | | | |

- **Maschine is not offered.** Without options it says "Version 1 is not supported as an output format" and writes nothing. `-pMaschineOutputFormat=0` or `2` writes `.mxsnd`, and `1` writes `.msnd`; what 0 and 2 differ in is open. Offering it needs `:convert` to pass `-p`.

- **`nki` writes Kontakt 1.** Whether Kontakt 7 or 8 opens it is open.

- **The SP-404MK2's loop survives:** read back, it ends at 23999, its end of 21999 scaled to 48 kHz.

- **A read-back shows what ConvertWithMoss reads,** not what the host does.

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

Loop points and a root note inside the WAV. It is the usual place for a sustain loop (reported; [WAVE File Format](http://midi.teragonaudio.com/tech/wave.htm) documents it, and could not be fetched for this note). Layout from [RecordingBlogs](https://www.recordingblogs.com/wiki/sample-chunk-of-a-wave-file), all fields 32-bit little-endian:

| field | value playr writes |
|-|-|
| manufacturer, product | 0 |
| sample period | nanoseconds a sample: 10^9 / rate, truncated |
| MIDI unity note | 60 |
| pitch fraction, SMPTE format, SMPTE offset | 0 |
| loops, sampler data bytes | 1, 0 |
| loop: ID, type | 1, 0 (forward) |
| loop: start, end | 0, the last frame; the page says "the end sample is also played" |
| loop: fraction, play count | 0, 0 (endless) |

The page counts start and end "in samples". playr writes frames, as for cue points; a byte reading is the same open question as there.

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

`docs/dev/device_tests.md` has tests on the Dirtywave M8 and OP-XY for the first, third, fourth and fifth.

- **Frame index or byte offset in a cue point's sample start.** Check what the M8 and the Blackbox expect.

- **Which devices read `smpl` loops.** A looped range now writes one; see "Built".

- **Bit depth and rate each device accepts.** playr writes 24-bit at the source's rate; ot_utils assumes 16-bit 44.1 kHz mono.

- **Slice limits.** 64 on the Octatrack, 24 on the OP-1, 32 reported for the M8, 24 zones on the OP-XY. An export must refuse or split past the limit. ConvertWithMoss drops the OP-XY's extra zones itself, and `:convert` reports it; see "Second test conversion".

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

- **Not built:** progress from `-P`. Converting an export from an earlier run was added on 2026-10-01: `:convert FORMAT EXPORT`.

The sliced file, on 2026-09-30:

- **Every export writes `sliced/NAME.wav`,** and `sliced/NAME.ot` with `slice_ot_file = true`, off by default; both are named after the export's directory. The WAV runs from the first slice's start to the last one's end, read from the source a second time, so fades on the slice files do not reach it.

- **The `cue ` chunk** follows the audio, after a pad byte when the audio ends on an odd byte, and has a point at each slice's start, the first at frame 0. Both position fields of a point hold the frame.

- **Frames, not bytes.** This settles "Units of sample start" above by what writers in common use do (inference); RecordingBlogs says bytes. It is the first thing to check on an M8 or a blackbox: a byte reading would put each slice at a fraction of where it belongs.

- **The `.ot` file is byte for byte what ot_utils 0.1.5 writes.** Three mono 44.1 kHz files of 1,000, 2,500 and 40,000 frames went through ot_utils, and `sliced::ot_file` given the same slices returned the same 832 bytes. A test holds them. This shows the layout is ot_utils's, not that an Octatrack accepts it.

- **Tempo is fixed at 124,** as ot_utils's default, and the length in bars is counted at it. The track's analysed tempo is not used.

- **More than 64 slices: no `.ot` file.** The WAV and its cue points are still written.

- **The rate and bit depth are the source's,** 24-bit. Nothing is resampled. Which rates an Octatrack plays is open; ot_utils assumes 44.1 kHz.

- **ffprobe, sox and Python's `wave` read the WAV** with no warning. None of them reports cue points, so the chunk was checked by reading its bytes back.

- **Not built:** labels for the cue points, and any check on a device.

The loop in the WAV, on 2026-10-03:

- **A range cut whole while it loops gets a `smpl` chunk** in its slice file and in `sliced/NAME.wav`, after the audio and, in the second, after the `cue ` chunk. Its one loop covers the whole file. Layout in "WAV `smpl` chunk" above.

- **hound still reads both files**, which the tests check; a reader that ignores unknown chunks should too (inference).

- **Not checked on any device or sampler.** "Which devices read `smpl` loops" stays open.
