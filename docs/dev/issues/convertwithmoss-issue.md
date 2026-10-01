# ConvertWithMoss issue: OP-XY frame counts after resampling

A draft report for [git-moss/ConvertWithMoss](https://github.com/git-moss/ConvertWithMoss/issues), not yet filed. playr has no workaround: `:convert opxy` passes any export not at 44.1 kHz through, and `TODO.md`, "OP-XY frame counts at other rates", tracks it. Test X6 in `docs/dev/device_tests.md` checks what the device does; its result can replace "Effect on the device" before filing. The cause below is read from the source, not confirmed by a build.

Everything below the line is the issue text.

---

## OP-XY: `patch.json` keeps the source's frame counts when samples are resampled to 44.1 kHz

### Summary

Converting to OP-XY resamples any sample that is not 44.1 kHz to 16-bit 44.1 kHz. `patch.json` still gives the frame counts and positions from before resampling. A 0.5 s sample at 48 kHz becomes a file of 22,050 frames, but `framecount` and `sample.end` say 24,000; at 96 kHz they say 48,000. Loop points keep their source values too.

### Version

ConvertWithMoss 20.3.0, macOS. The code below is unchanged on `main` at 31b0f8a.

### To reproduce

```sh
sox -D -n -r 48000 -b 24 -c 2 tone.wav synth 0.5 sine 440
printf '<region> sample=tone.wav key=60\n' > kit.sfz
printf '<region> sample=tone.wav key=60 loop_mode=loop_continuous loop_start=0 loop_end=23999\n' > loop.sfz
ConvertWithMoss -s sfz -d opxy kit.sfz out
ConvertWithMoss -s sfz -d opxy loop.sfz out
```

### Result

| preset | WAV written | `framecount` | `sample.end` | `loop.end` |
|-|-|-|-|-|
| `kit` | 44100 Hz, 16-bit, 22050 frames | 24000 | 24000 | 24000 |
| `loop` | 44100 Hz, 16-bit, 22050 frames | 24000 | 24000 | 23999 |

### Expected

Values counted at 44.1 kHz: 22050, 22050, and a loop end of about 22049. A 44.1 kHz source gives matching values today.

### Likely cause

`OpXyCreator` resamples through `DESTINATION_FORMAT` (16-bit, 44100) in `writeSamples`. `createRegion` then reads `getNumberOfSamples()` and the zone's start, stop and loop positions, which are still at the source rate. The creators that resample call `recalculateSamplePositions` first; `DistingExCreator` does `this.recalculateSamplePositions (multisampleSource, 44100)`. `OpXyCreator` does not. Calling it before `createPatch` may fix the positions. Whether `framecount` then follows is not checked, since it reads the audio metadata.

### Effect on the device

Not tested. Whether the OP-XY clamps a `sample.end` past the end of the file or misplays it is unknown. A looped sample's loop points are off by the rate ratio either way.
