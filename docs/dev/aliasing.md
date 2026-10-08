# Read-head error: measured

Measured 2026-10-08, for "Anti-aliasing in the read heads" in `TODO.md`, when the deck and tape heads read with a 4-point cubic Hermite and no filter. "Built" at the end records the windowed sinc that answers it.

## Method

A unit sine at 44.1 kHz, read through `hermite` at a fixed rate for 65,536 output frames, as the heads read. A Blackman-Harris window and an FFT give the output's spectrum. The figure is the largest component other than the intended tone, in dB relative to a unit sine. The intended tone is the input times the rate, or nothing when that passes 22.05 kHz: an ideal band-limited resampler removes it. A scratch program built on `playr-dsp` and `realfft` ran it; it is not in the repository.

## Result

Worst spurious component, dB:

| tone | x0.5 | x1.01 | x1.16 | x1.5 | x4 |
|-|-|-|-|-|-|
| 1 kHz | -105.5 | -91.6 | -91.0 | -103.7 | -96.0 |
| 5 kHz | -50.1 | -46.6 | -47.1 | -50.5 | -94.1 |
| 10 kHz | -27.5 | -26.1 | -26.3 | -27.1 | +0.8 |
| 15 kHz | -15.3 | -14.9 | -14.6 | -1.0 | +0.4 |
| 18 kHz | -10.2 | -10.7 | -10.7 | -2.2 | +0.2 |
| 20 kHz | -7.5 | -8.0 | -3.6 | -3.8 | +0.7 |

Two errors, of different causes:

- **Interpolation error, at every non-integer rate.** The cubic's images: -47 dB at 5 kHz and -26 dB at 10 kHz, the same at x1.01 as at x1.16. It does not start at +16%.
- **Folding, above 22.05 kHz divided by the rate.** At x1.16 only above 19 kHz; at x1.5 above 14.7 kHz; at x4, the tape's top rate, everything above 5.5 kHz folds back at full level.

How loud either is in music depends on its level at those frequencies, typically 30 to 50 dB below its peak at 10 kHz (inference, not measured on the library). Then the interpolation error at 10 kHz lands about 56 to 76 dB below peak, and the tape's folding at x4 about 30 to 50 dB below it.

## What a fix needs

- A filter ahead of the head, cut at 22.05 kHz over the rate, removes folding only. The interpolation error near rate 1 stays.
- A windowed-sinc kernel removes the interpolation error. Removing folding too needs its cutoff scaled by the rate, so its taps grow with the rate: 4 times as many at x4.
- Cost per voice: a 16-tap kernel is 4 times the Hermite's multiplies; scaled for x4, 16 times. The decks run 2 voices, the tape 3 (inference from the code, not profiled).

## Built

2026-10-08: `playr_dsp::interp`, with both kernels; `interp` in `settings.toml` and `:interp` choose one for the decks and the tape together, sinc by default.

- **Sinc:** 32 taps, Kaiser window, beta 8. The cutoff is placed so the window's transition ends near half the stored rate. Above rate 1 the kernel is stretched by the rate, up to 4, the tape's top rate, so its cutoff falls with the rate: up to 128 taps. Read from a table of 512 points a frame, built when an engine is made, so the callback never allocates. The weights are scaled to sum to 1.
- **A whole frame at rate 1** is the stored frame under either kernel, so a deck at 0% plays its buffer exactly.

The same measurement, sinc, worst spur dB / tone level dB, where the 1 kHz level, 0.8 dB, is the measurement's own offset:

| tone | x0.5 | x1.01 | x1.16 | x1.5 | x4 |
|-|-|-|-|-|-|
| 1 kHz | -89.5/0.8 | -90.6/0.8 | -98.0/0.8 | -98.5/0.8 | -96.0/0.5 |
| 5 kHz | -91.3/0.7 | -95.3/0.5 | -97.5/0.6 | -94.4/0.2 | -112.9/-18.6 |
| 10 kHz | -87.3/0.3 | -92.0/0.4 | -92.4/0.0 | -99.2/0.7 | -100.0 |
| 15 kHz | -86.1/0.2 | -94.2/0.8 | -92.1/-1.2 | -87.0 | -97.0 |
| 18 kHz | -84.4/-3.1 | -89.2/-3.6 | -91.1/-32.3 | -92.8 | -101.8 |
| 20 kHz | -83.6/-18.1 | -95.0/-20.8 | -85.6 | -88.7 | -110.0 |

- Every spur is 83 dB down or more, against -26 dB at 10 kHz for the cubic and folding at full level at x4.
- The top octave rolls off: 18 kHz is about 4 dB down at rate 1, against 3 dB for the cubic, and 20 kHz about 21 dB. A longer kernel would move the roll-off up, at a cost in proportion to its taps.
- At x1.16 and x4 a tone whose output would pass half the rate is removed, as intended: 18 kHz at x1.16, 5 kHz at x4.

Cost, on an AMD Ryzen 9 7940HX, release build, share of one core for one second of audio at 48 kHz:

| | Hermite | Sinc |
|-|-|-|
| tape, 3 voices at x1.01 | 0.7% | 3.0% |
| tape, 3 voices at x4 | 0.6% | 9.8% |
| decks, both at +8% | 0.3% | 1.5% |

Hermite stays as the cheaper choice, and for comparison by ear: `i` in the Tape, DJ and Mix views toggles the two.
