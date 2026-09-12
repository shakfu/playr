# Changelog

Notable changes to playr. Format follows [Keep a Changelog][kac], versioning
follows [Semantic Versioning][semver].

[kac]: https://keepachangelog.com/en/1.1.0/
[semver]: https://semver.org/spec/v2.0.0.html

## [0.1.0] - 2026-09-12

First release.

### Added

- Playback of FLAC, ALAC, MP3, MP1, MP2, AAC-LC, Vorbis, PCM and ADPCM, in WAV,
  AIFF, CAF, MP4/M4A, MKV/WebM, OGG and raw FLAC containers.
- Opus playback in OGG and WebM, behind the `opus` cargo feature. Symphonia 0.6
  demuxes Opus but registers no decoder, so `src/audio/opus.rs` supplies one over
  libopus and installs it in a custom codec registry. The feature is off by
  default because libopus is built with cmake, which nothing else here needs.
- SQLite library with recursive scanning. Tags and stream properties come from
  lofty. A rescan skips files whose size and modification time are unchanged, so
  re-indexing an unchanged library opens no files. Rows for deleted files are
  pruned.
- Full-text search over title, artist, album and album artist, using an FTS5
  external-content index kept in sync by triggers. Results play directly as an
  ad-hoc queue, which is the whole of the search-then-play flow; no separate
  concept was needed.
- Playlists saved to and loaded from the database, order preserved.
- TUI with library, queue and playlist views, a search bar, and a now-playing
  bar showing source rate, channels, volume, speed and progress.
- Varispeed on `[` and `]`, in semitone steps from 0.5x to 2.0x, pitch moving
  with tempo. Steps are geometric, so each press is the same musical interval
  and twelve of them are exactly an octave. Powers of two were considered and
  rejected: 2x is a whole octave, which leaves three usable settings.
- Seeking on the arrow keys, five seconds a press.
- CLI: `scan`, `search`, `playlist`, `playlists`, `formats`, plus playing paths
  directly and `--db` to point at another library.

### Audio path

- The output stream opens at the file's own sample rate whenever the device
  accepts it, so the common case is not resampled at all. This is not
  bit-perfect: on PipeWire the ALSA `default` device accepts every rate and may
  convert internally.
- Sinc resampling when a rate is refused. Linear interpolation folds audible
  aliasing into the passband, which would defeat decoding losslessly.
- f32 from decoder to device, quantised once. Volume is a float gain applied
  before quantisation.
- Decoding runs on its own thread and feeds a lock-free ring; the realtime
  callback touches only atomics. An underrun emits silence rather than repeating
  stale samples, which would click.
- Pausing is done in the callback, not with `Stream::pause`. ALSA rejects
  `snd_pcm_pause` with errno 77 on a PipeWire setup, and stopping the device
  risks a click on resume.

### Known limits

- Gapless playback holds within a run of tracks at one sample rate. A rate
  change tears down and rebuilds the output stream, which leaves a gap.
- AAC decodes about 1900 frames long. Symphonia has no gapless support for it.
- Opus surround (multistream) is rejected rather than decoded.
- Opus in WebM keeps 648 frames of end padding, because Matroska does not report
  it and Symphonia does not expose the block duration that would.

[0.1.0]: https://github.com/shakfu/playr/releases/tag/v0.1.0
