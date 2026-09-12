# playr

A minimal TUI music player. Plays a directory, a saved playlist, or the results
of a search. Keeps a SQLite index of your library.

It contacts no server, fetches no metadata, scrobbles nothing, and has no
network code in it at all.

![Varispeed on a Boards of Canada record is a good use of an afternoon.](https://raw.githubusercontent.com/shakfu/playr/main/docs/media/playr.png)


## Features

**Playback**

- Plays a file, a directory recursively, a saved playlist, or a search result
- Gapless within a run of tracks that share a sample rate
- Play, pause, next, previous, stop
- Seek by 5 seconds in either direction
- Varispeed in semitone steps, 0.5x to 2.0x, pitch moving with tempo
- Volume as a float gain applied before quantisation
- A file it cannot decode is reported and skipped, never fatal

**Audio quality**

- Opens the output at the file's own sample rate whenever the device allows it,
  so nothing is resampled in the common case
- Sinc resampling when a rate is refused, not linear interpolation
- f32 throughout: decode, mix and gain, quantised once at the device
- Lock-free ring between the decoder and the realtime callback
- Underruns emit silence rather than repeating stale samples

**Library**

- Recursive scan with tag and stream-property reading
- Rescan skips files whose size and modification time are unchanged
- Rows for deleted files are pruned
- Full-text search over title, artist, album and album artist
- Search results play directly as an ad-hoc queue
- Playlists saved to and loaded from the database

**Interface**

- Three views: library, queue, playlists
- Search filters as you type
- Queue cursor follows playback, so the playing track stays on screen
- Now playing shows title, artist, source rate, channels, volume and speed
- Progress bar with elapsed and total time
- Vim and arrow key navigation

## Install

```sh
make build                    # debug
cargo build --release         # release
```

Needs Rust 1.89+, ALSA headers (`libasound2-dev` on Debian and Ubuntu), and a C
compiler. SQLite is vendored and compiled from source, which is what the C
compiler is for; no SQLite package has to be installed.

### Opus

Opus is off by default. It needs libopus, which is vendored and built with
**cmake** -- the only part of playr that needs it. To include it:

```sh
cargo build --release --features opus
```

Without it, Opus files are reported as undecodable and skipped, the same as WMA
or DSD. `playr formats` says which build you have.

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

The library lives at `$XDG_DATA_HOME/playr/library.db`, or
`~/.local/share/playr/library.db`. Override it with `--db <path>`.

Rescanning only re-reads files whose size or modification time changed, and
drops rows whose files are gone.

## Keys

| keys                     | action                                      |
|--------------------------|---------------------------------------------|
| `tab`, `1` `2` `3`       | switch between library, queue and playlists |
| `j` `k`, up/down         | move                                        |
| `g` `G`, home/end        | jump to first or last                       |
| page up/down             | move by ten                                 |
| `enter`                  | play from here; in playlists, load it       |
| `a`                      | add the selection to the queue              |
| `/`                      | search; `esc` clears                        |
| `s`                      | save the queue as a playlist                |
| `d`                      | delete the selected playlist                |
| `space`                  | play or pause                               |
| `n` `p`                  | next or previous track                      |
| `x`                      | stop                                        |
| left/right               | seek back or forward 5 seconds              |
| `[` `]`                  | varispeed down or up, one semitone a press  |
| `\`                      | back to normal speed                        |
| `+` `-`                  | volume                                      |
| `q`                      | quit                                        |

Searching filters as you type, across title, artist, album and album artist.
Pressing enter on the results plays them as an ad-hoc queue.

In the queue, the cursor follows playback, so the playing track stays on screen
through a long album. It moves only when the track changes, so scrolling with
`j`/`k` is not fought while a track plays.

### Varispeed

`[` and `]` change playback speed in semitone steps, and pitch moves with it, as
on a tape machine or a turntable. Twelve presses is exactly an octave, so the
range is 0.5x to 2.0x. The speed shows in the status bar as `1.19x (+3 st)` and
`\` returns to normal.

This is not the pitch-preserving speed change of a podcast app. That is
time-stretching, which needs a phase vocoder; this is a change of resampling
ratio, which is what varispeed means.

## Formats

Decoded: FLAC, ALAC, MP3, MP1, MP2, AAC-LC, Vorbis, PCM and ADPCM, in WAV,
AIFF, CAF, MP4/M4A, MKV/WebM, OGG and raw FLAC containers. Opus as well, when
built with `--features opus`.

Opus is decoded by playr itself, in both OGG and WebM. Symphonia 0.6 demuxes
Opus but ships no decoder, so `src/audio/opus.rs` supplies one on top of libopus
via the `opus` crate and registers it in a custom codec registry. Mono and
stereo only; multistream surround is not handled.

Not decoded: WavPack, WMA, Musepack, APE, DSD, TTA, TAK, and Opus unless the
feature is enabled. playr reports such a file and moves to the next track rather
than stopping. Run `playr formats` for the current list.

## Audio quality

The output stream is opened at the file's own sample rate whenever the device
accepts it, so nothing is resampled in the common case. When a rate is refused,
a sinc resampler converts it rather than linear interpolation. Decoding, mixing
and volume are all f32, quantised once at the device.

This is not bit-perfect output. On a PipeWire system the ALSA `default` device
accepts every rate and may convert internally. Bit-perfect playback would need a
`hw:` device, which playr does not yet select.

Volume is a float gain applied before quantisation.

## Design

`docs/dev/PLAN.md` covers the architecture, the trade-off behind choosing
Symphonia over mpv or ffmpeg, and the assumptions that implementation disproved.

`TODO.md` lists what is missing and what is blocked upstream.

## Tests

```sh
make test
```

`make test` runs the suite twice, with and without the `opus` feature, so
neither build can rot unnoticed. The format and scanner tests generate real
audio with `ffmpeg` when it is present and skip themselves when it is not. The
rendering tests draw into a headless terminal, so they need no audio device.

Opus output was checked against `ffmpeg` by decoding the same file both ways:
identical frame counts and 138.7 dB SNR, with no alignment offset.

## License

MIT. See `LICENSE`.
