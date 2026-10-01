# Device tests: Dirtywave M8 and OP-XY

Written 2026-10-01. Each test answers an open question in `docs/dev/hardware_samplers.md` by loading a playr export on the device. Nothing here has been run yet. Record each result in the tables, with the device's firmware version, then move the answer into `hardware_samplers.md`.

Claims about the devices are reported, not read in a manual unless marked; the sources are at the end.

## Test material

One source with 8 tones of distinct pitch, so a slice that starts in the wrong place plays the wrong tone. Each tone is 0.5 s; the file is 4 s. `tones` builds the tones in a temporary directory and writes only its file, here into `~/Music/playr/device-tests/`. `-D` turns off dither, which sox adds when writing 16-bit, so each run writes the same bytes:

```sh
tones() {  # tones OUT.wav SOX_FORMAT_OPTIONS...
  out=$1; shift
  tmp=$(mktemp -d)
  for f in 220 247 277 294 330 370 415 440; do
    sox -D -n "$@" "$tmp/$f.wav" synth 0.5 sine $f fade 0.005 0.5 0.01
  done
  sox -D "$tmp"/220.wav "$tmp"/247.wav "$tmp"/277.wav "$tmp"/294.wav \
      "$tmp"/330.wav "$tmp"/370.wav "$tmp"/415.wav "$tmp"/440.wav "$out"
  rm -r "$tmp"
}
mkdir -p ~/Music/playr/device-tests && cd ~/Music/playr/device-tests
tones tones.wav    -r 44100 -b 24 -c 2
tones tones48.wav  -r 48000 -b 24 -c 2
tones tones96.wav  -r 96000 -b 24 -c 2
tones tones16m.wav -r 44100 -b 16 -c 1
```

| file | tests |
|-|-|
| `tones.wav` | the main case: 6 bytes a frame |
| `tones48.wav` | 48 kHz |
| `tones96.wav` | a rate devices may refuse |
| `tones16m.wav` | mono: its export is 24-bit mono, 3 bytes a frame, as playr writes every export 24-bit |

To check a file, measure each segment on one channel; `stat` reads a stereo file at about 0.7 of its pitch:

```sh
for i in 0 1 2 3 4 5 6 7; do
  sox tones.wav -n remix 1 trim $(echo "$i*0.5+0.1" | bc) 0.3 stat 2>&1 | awk '/Rough/ {print $3}'
done
```

Each file reads 219, 246, 276, 294, 329, 369, 414 and 439 Hz.

In playr, play each file and slice it whole into equal parts, outside the sampler view so it writes at once:

```
:slice 8
```

The export is `samples/tones/`: the slice files, `tones.sfz`, and `sliced/tones.wav` with a cue point at each slice's start. For the slice-count test, also run `:slice 40` on `tones.wav`. A second export of one track goes to the next free name, so that one is `samples/tones-2/`, with `sliced/tones-2.wav` and `tones-2.sfz`.

## Dirtywave M8

Reported: the M8 reads WAV cue points as slices since firmware 2.5.0, at most 32. The Sampler's SLICE parameter set to `01 FILE` uses them, and notes from C-1 up play slices 1, 2, and so on. It takes 8, 16 and 24-bit PCM WAV, mono or stereo, with the whole path under 128 characters.

Copy each `sliced/*.wav` to the SD card, in a folder near the root to keep the path short.

| # | question | steps | pass |
|-|-|-|-|
| M1 | Cue positions: frames or bytes | Load `sliced/tones.wav` (8 slices) into a Sampler instrument. Set SLICE to `01 FILE`. Play C-1 to G-1. Open the sample editor. | Notes play 220, 247, ... 440 Hz in order, each from its start. The editor shows markers every 0.5 s. A byte reading plays mostly 220 Hz: each marker sits at 1/6 of its place. |
| M2 | The same at 3 bytes a frame | Repeat M1 with the `tones16m` export. | As M1. A byte reading puts markers at 1/3 of their place. |
| M3 | 48 kHz | Repeat M1 with `tones48`. | As M1. |
| M4 | 96 kHz | Repeat M1 with `tones96`. | Loads and plays at pitch, or is refused. Record which. |
| M5 | More than 32 cue points | Load the 40-slice `sliced/tones-2.wav`. Count the markers in the editor. Play the 33rd note. | Record: 32 markers, 40, none, or an error. |
| M6 | The first marker at frame 0 | In M1, check the first slice. | Slice 1 starts at the file's start, with no empty slice before it. |
| M7 | The slice files | Load `001-tones_S01.wav` as a plain sample. | Plays 247 Hz, 0.5 s. |

**What each result changes in playr:**

- **M1 or M2 fails as a byte reading:** `sliced::cue_chunk` must write bytes for the M8. Other readers may expect frames, so check the 1010music blackbox before changing the default. Record it per device.
- **M5 shows no markers or an error:** an export past 32 slices should warn, or write a second sliced file, as AudioHit does for the Octatrack.
- **M4 is refused:** note it under "Bit depth and rate each device accepts".

## Teenage Engineering OP-XY

Reported: in disk mode the OP-XY shows `presets`, `projects` and `samples` folders. A `.preset` folder copied into `presets` appears in the preset browser. Folders nest only one level deep. It takes WAV and AIFF. Its guide gives no limits on zones, length or rate.

Convert each export, with the `convert-with-moss` extension enabled:

```
:convert opxy tones
```

That writes `samples/tones/opxy/tones.preset/`, holding `patch.json` and the slices made 16-bit. Copy that `.preset` folder into `presets/` on the OP-XY.

For the loop test, make a looped export: in the sampler view, drag a range over the whole track, turn Loop on, choose Range in the Slice drop-down, then Write. In the terminal, in the sampler view: `:range 0 4`, `:loop on`, `:slice region`, `:write`. It goes to the next free name, such as `samples/tones-3/`. Its `.sfz` carries `loop_mode=loop_continuous`; convert it as above, by that name.

| # | question | steps | pass |
|-|-|-|-|
| X1 | The preset loads | Eject, then open the preset browser. | `tones` is listed and loads with no error. |
| X2 | One slice a key, at pitch | Play the keys from MIDI note 36 up. | Each key plays the next tone at its own pitch: 220, 247, ... 440 Hz. Record which on-screen key is note 36. |
| X3 | Keys below the first slice | Play keys below note 36. | Reported in `patch.json`: the first zone reaches down to note 0, so these play slice 1 transposed. Confirm. |
| X4 | A looped slice loops | Load the looped preset: one 4 s slice of all 8 tones, on note 36. Hold that key for 6 s. | The 8 tones play, then start again from 220 Hz at 4 s. Without the loop it stops at 4 s. |
| X5 | Many slices | Convert and load the 40-slice export: `:convert opxy tones-2`. | ConvertWithMoss keeps 24 zones, on notes 36-59, and `:convert` names the 16 it dropped. Check all 24 load and play, and that note 60 up plays nothing new. |
| X6 | 96 kHz source | Load `tones96`, converted with `:convert opxy tones96`. ConvertWithMoss made each slice 44.1 kHz, 22,050 frames, but `patch.json` gives `sample.end` 48,000, the count before resampling. | Record whether each key plays its whole 0.5 s tone at pitch and stops cleanly, or plays past the end, clicks, or is refused. |

**What each result changes in playr:**

- **X2 shows slices off by an octave or more:** `samples::FIRST_KEY` is 36 for C1. Note how the OP-XY names that key, and whether a different first key suits it better.
- **X5 refuses or truncates:** `:convert opxy` should refuse or warn past the limit, as the `.ot` file does past 64.
- **X4 does not loop:** compare `patch.json`'s `loop.enabled` and `loop.end` with the source's `loop_end`.

## Results

| test | firmware | date | result |
|-|-|-|-|
| M1 | | | |
| M2 | | | |
| M3 | | | |
| M4 | | | |
| M5 | | | |
| M6 | | | |
| M7 | | | |
| X1 | | | |
| X2 | | | |
| X3 | | | |
| X4 | | | |
| X5 | | | |
| X6 | | | |

## Sources

- [DirtyWave-M8-Tips](https://github.com/pauley-unsaturated/DirtyWave-M8-Tips): slice markers since 2.5.0, at most 32, SLICE `01 FILE`, notes from C-1.
- [M8 operation manual 6.0.0](https://images.equipboard.com/uploads/item/manual/136211/dirtywave-m8-tracker-model-02-manual.pdf): sample formats and path length, as quoted in a search result; not read here.
- [DigiChain 1.4.6](https://brian3kb.itch.io/digichain/devlog/870054/v146-dirtywave-m8-slice-support): a writer of M8 slice points.
- [OP-XY guide](https://teenage.engineering/guides/op-xy/how-to), section 22.11: disk mode, folders, WAV and AIFF; read.
- [OP Forums](https://op-forums.com/t/op-xy-how-to-load-samples-from-computer/28305): one level of nested folders.
