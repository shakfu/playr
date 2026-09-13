# playr

A minimal TUI music player. Plays a directory, a saved playlist, or the results of a search. Keeps a SQLite index of your library.

It contacts no server, fetches no metadata, scrobbles nothing, and has no network code in it at all.

![Varispeed on a Boards of Canada record is a good use of an afternoon.](https://raw.githubusercontent.com/shakfu/playr/main/docs/media/playr.png)


## Features

**Playback**

- Plays a file, a directory recursively, a saved playlist, or a search result

- Gapless within a run of tracks that share a sample rate

- Play, pause, next, previous, stop

- Seek by 5 seconds in either direction, or 30 with shift, resuming on the exact sample

- Varispeed in semitone steps, 0.5x to 2.0x, pitch moving with tempo

- Volume as a float gain applied before quantisation

- A file it cannot decode is reported and skipped, never fatal

**Audio quality**

- Opens the output at the file's own sample rate whenever the device allows it, so nothing is resampled in the common case

- Sinc resampling when a rate is refused, not linear interpolation

- f32 throughout: decode, mix and gain, quantised once at the device

- Plays to float, 32-bit, 24-bit and 16-bit integer devices

- Lock-free ring between the decoder and the realtime callback

- Underruns emit silence rather than repeating stale samples

**Library**

- Recursive scan with tag and stream-property reading

- Rescan skips files whose size and modification time are unchanged

- Rows for deleted files under the scanned directory are pruned

- Scans commit every 500 files, so an interrupted scan keeps its progress

- Full-text search over title, artist, album and album artist

- Search results play directly as an ad-hoc queue, in library order

- Playlists saved to and loaded from the database

**Interface**

- Three views: library, queue, playlists

- Search filters as you type

- Queue cursor follows playback, so the playing track stays on screen

- Now playing shows title, artist, source rate, channels, volume and speed

- Progress bar with elapsed and total time

- Skipped files and audio device errors shown in the status line

- Vim and arrow key navigation

- `?` lists every key; the bottom line shows speed, a volume meter and messages

## Install

```sh
make build                    # debug
make release                  # release
make install                  # release, copied to ~/.local/bin
```

Needs Rust 1.89+, ALSA headers (`libasound2-dev` on Debian and Ubuntu), and a C compiler. SQLite is vendored and compiled from source, which is what the C compiler is for; no SQLite package has to be installed.

### Opus

Opus is off by default. It needs libopus, which is vendored and built with **cmake** -- the only part of playr that needs it. To include it:

```sh
cargo build --release --features opus
```

Without it, Opus files are reported as undecodable and skipped, the same as WMA or DSD. `playr formats` says which build you have. `make install` builds without Opus; to install with it, copy `target/release/playr` after the build above.

## Use

```sh
playr scan ~/music          # index a directory
playr                       # browse the library
playr ~/music/some/album    # play a directory, recursively, without indexing
playr search bill evans     # play everything that matches
playr playlist "late night" # play a saved playlist
playr playlists             # list saved playlists
playr formats               # show what this build can decode
```

The library lives at `$XDG_DATA_HOME/playr/library.db`, or `~/.local/share/playr/library.db`. Override it with `--db <path>`. Only `playr scan` creates it. Until then the other commands run without a library, and `s` cannot save a playlist. Paths are stored in full, so a scan run from any directory finds the same rows. A path that is not valid UTF-8 is skipped and counted as unreadable.

Rescanning only re-reads files whose size or modification time changed, and drops rows under the scanned directory whose files are gone. Rows elsewhere are kept, so a scan made while a drive is unmounted does not empty its playlists.

## Keys

| keys                     | action                                      |
|--------------------------|---------------------------------------------|
| `tab`, `1` `2` `3`       | switch between library, queue and playlists |
| `j` `k`, up/down         | move                                        |
| `g` `G`, home/end        | jump to first or last                       |
| page up/down             | move by ten                                 |
| `enter`                  | play from here; in playlists, load it       |
| `a`                      | queue the selection; plays if stopped       |
| `/`                      | search; `esc` clears                        |
| `s`                      | save the queue; asks before overwriting     |
| `d`                      | delete the selected playlist, after `y`     |
| `space`                  | play or pause                               |
| `n` `p`                  | next or previous track                      |
| `x`                      | stop                                        |
| left/right               | seek back or forward 5 seconds              |
| shift left/right         | seek back or forward 30 seconds             |
| `[` `]`                  | varispeed down or up, one semitone a press  |
| `\`                      | back to normal speed                        |
| `+` `-`                  | volume                                      |
| `?`                      | list every key                              |
| `q`                      | quit                                        |

Searching filters as you type, across title, artist, album and album artist. Pressing enter on the results plays them as an ad-hoc queue.

In the queue, the cursor follows playback, so the playing track stays on screen through a long album. It moves only when the track changes, so scrolling with `j`/`k` is not fought while a track plays.

### Varispeed

`[` and `]` change playback speed in semitone steps, and pitch moves with it, as on a tape machine or a turntable. Twelve presses is exactly an octave, so the range is 0.5x to 2.0x. The speed shows in the status bar as `1.19x (+3 st)` and `\` returns to normal.

This is not the pitch-preserving speed change of a podcast app. That is time-stretching, which needs a phase vocoder; this is a change of resampling ratio, which is what varispeed means.

## Formats

Decoded: FLAC, ALAC, MP3, MP1, MP2, AAC-LC, Vorbis, PCM and ADPCM, in WAV, AIFF, CAF, MP4/M4A, MKV/WebM, OGG and raw FLAC containers. Opus as well, when built with `--features opus`.

Opus is decoded by playr itself, in both OGG and WebM. Symphonia 0.6 demuxes Opus but ships no decoder, so `src/audio/opus.rs` supplies one on top of libopus via the `opus` crate and registers it in a custom codec registry. Mono and stereo only; multistream surround is not handled.

Tags are not read from CAF, MKV or WebM files. Those are indexed under their file names, and MKV and WebM files also show no duration.

Not decoded: WavPack, WMA, Musepack, APE, DSD, TTA, TAK, and Opus unless the feature is enabled. playr reports such a file and moves to the next track rather than stopping. Run `playr formats` for the current list.

## Audio quality

The output stream is opened at the file's own sample rate whenever the device accepts it, so nothing is resampled in the common case. When a rate is refused, a sinc resampler converts it rather than linear interpolation. Decoding, mixing and volume are all f32, quantised once at the device. The device format is chosen in the order f32, f64, 32-bit, 24-bit, then 16-bit integer.

This is not bit-perfect output. On a PipeWire system the ALSA `default` device accepts every rate and may convert internally. Bit-perfect playback would need a `hw:` device, which playr does not yet select.

Volume is a float gain applied before quantisation.

## Roadmap

`TODO.md` lists what is missing and what is blocked upstream.

## Tests

```sh
make test
```

`make test` runs the suite twice, with and without the `opus` feature, so neither build can rot unnoticed. The format and scanner tests generate real audio with `ffmpeg` when it is present and skip themselves when it is not. The rendering tests draw into a headless terminal, so they need no audio device. The engine, key-handling and device-failure tests play to a fake output device, so they need no audio device. One smoke test plays to the real default device at zero volume, and skips without one. Set `PLAYR_REQUIRE_FFMPEG=1` or `PLAYR_REQUIRE_DEVICE=1` to fail instead of skip, so a CI run cannot pass by testing nothing.

Opus output was checked against `ffmpeg` by decoding the same file both ways: identical frame counts and 138.7 dB SNR, with no alignment offset.

## License

MIT. See `LICENSE`.
