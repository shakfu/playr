# Changelog

Notable changes to playr. Format follows [Keep a Changelog][kac], versioning follows [Semantic Versioning][semver].

[kac]: https://keepachangelog.com/en/1.1.0/ [semver]: https://semver.org/spec/v2.0.0.html

## [0.2.0] - 2026-09-13

### Changed

- Only `playr scan` creates the library file. Every other command used to create an empty one, including playing a file. Without a library, `s` refuses to save a playlist rather than keep it in memory and lose it on exit.

- Modification times are stored in nanoseconds, and the library records its schema version in `PRAGMA user_version`. The first rescan after upgrading re-reads every file once.

- Library API: `Status::queue` is an `Arc<[PathBuf]>`, `Output::open` takes a `Sender<String>` for stream errors, and `output::choose` and `App::on_key` are public.

### Removed

- `AudioStream::time_base`, `Shared::starved` and `Resample::output_frames_max`, which nothing read.

### Fixed

- `playr scan` with a relative path stored relative rows. A later scan from another directory found none of those files and deleted the rows, with their playlist entries. Scan roots and paths passed to `playr` are now canonicalized. Relative rows written by 0.1.0 are not migrated.

- A scan pruned every row whose file was missing, not only rows under the scanned directory. Scanning one directory while a drive was unmounted deleted that drive's tracks and emptied its playlists. Pruning is now limited to each scanned root, and a root that does not resolve prunes nothing. Marking rows missing instead of deleting them would also survive moved files, but needs a schema change.

- A long run of undecodable files, such as an Opus library in a build without `opus`, aborted playr with a stack overflow. The engine skipped a bad file by recursing, and about 2,000 in a row overflowed a debug build. Skipping is now a loop.

- Varispeed reset to normal speed on every track change, while the status bar kept the shifted speed and the position ran fast by the same ratio. Three of the four places that build a resampler checked only the sample rate.

- The engine copied every queued path into the status up to 200 times a second, on the thread that refills the audio buffer. The status now shares the engine's queue.

- The library and queue views formatted every track on every frame. Only the rows on screen are drawn now.

- An interrupted scan kept nothing, because the whole scan was one transaction. Scans now commit every 500 files.

- `d` deleted a playlist and `s` overwrote one without asking. Both now wait for `y`, and any other key cancels.

- A path that is not valid UTF-8 was stored lossily, never played, and was re-read on every rescan. The scanner now counts it as unreadable, and `playr <path>` skips it with a warning. Storing paths as bytes would keep such files playable, but changes the schema.

- A seek resumed up to one packet before its target, 546 frames in a FLAC test, while the position showed the target. Symphonia's accurate seek lands on an earlier packet and leaves the caller to discard up to the target.

- A seek whose output failed to reopen left the engine playing with no output and no message. It now reports the error and stops.

- Moving to the next track skipped some failures silently: a first packet that would not decode, an unknown stream format, a refused output format, and a decode error mid-track. They are now reported as they are at playback start.

- Devices offering only 32-bit or 24-bit integer, or 64-bit float, output could not play. Negotiation accepted only f32, i16 and u16, and used the default config without checking its format.

- Enqueueing started playback only into an empty queue. Tracks added after the queue had played out, or while its last track played from the buffer, did not play.

- `playr playlist NAME` matched case-insensitively first, so with `Late` and `late` saved it always chose one. An exact name now wins, and a case-insensitive match is used only when it is unique.

- CAF and Matroska files were never indexed, because the tag reader cannot parse them. They are now indexed untagged, with stream properties from Symphonia. Matroska rows have no duration: Symphonia reports none, and the alternative was decoding each file in full at scan time.

- A same-size rewrite within one second was skipped by rescan, because modification times were whole seconds.

- Columns were sized by character count, so a row of CJK text overflowed and lost its duration. They are now sized in terminal cells.

- Audio device errors were printed to stderr, over the interface. They now appear in the status line.

- `make install` printed an empty path, and created a file named `bin` when `~/.local/bin` did not exist.

## [0.1.0] - 2026-09-12

First release.

### Added

- Playback of FLAC, ALAC, MP3, MP1, MP2, AAC-LC, Vorbis, PCM and ADPCM, in WAV, AIFF, CAF, MP4/M4A, MKV/WebM, OGG and raw FLAC containers.

- Opus playback in OGG and WebM, behind the `opus` cargo feature. Symphonia 0.6 demuxes Opus but registers no decoder, so `src/audio/opus.rs` supplies one over libopus and installs it in a custom codec registry. The feature is off by default because libopus is built with cmake, which nothing else here needs.

- SQLite library with recursive scanning. Tags and stream properties come from lofty. A rescan skips files whose size and modification time are unchanged, so re-indexing an unchanged library opens no files. Rows for deleted files are pruned.

- Full-text search over title, artist, album and album artist, using an FTS5 external-content index kept in sync by triggers. Results play directly as an ad-hoc queue, which is the whole of the search-then-play flow; no separate concept was needed.

- Playlists saved to and loaded from the database, order preserved.

- TUI with library, queue and playlist views, a search bar, and a now-playing bar showing source rate, channels, volume, speed and progress.

- Varispeed on `[` and `]`, in semitone steps from 0.5x to 2.0x, pitch moving with tempo. Steps are geometric, so each press is the same musical interval and twelve of them are exactly an octave. Powers of two were considered and rejected: 2x is a whole octave, which leaves three usable settings.

- Seeking on the arrow keys, five seconds a press.

- CLI: `scan`, `search`, `playlist`, `playlists`, `formats`, plus playing paths directly and `--db` to point at another library.

### Audio path

- The output stream opens at the file's own sample rate whenever the device accepts it, so the common case is not resampled at all. This is not bit-perfect: on PipeWire the ALSA `default` device accepts every rate and may convert internally.

- Sinc resampling when a rate is refused. Linear interpolation folds audible aliasing into the passband, which would defeat decoding losslessly.

- f32 from decoder to device, quantised once. Volume is a float gain applied before quantisation.

- Decoding runs on its own thread and feeds a lock-free ring; the realtime callback touches only atomics. An underrun emits silence rather than repeating stale samples, which would click.

- Pausing is done in the callback, not with `Stream::pause`. ALSA rejects `snd_pcm_pause` with errno 77 on a PipeWire setup, and stopping the device risks a click on resume.

### Known limits

- Gapless playback holds within a run of tracks at one sample rate. A rate change tears down and rebuilds the output stream, which leaves a gap.

- AAC decodes about 1900 frames long. Symphonia has no gapless support for it.

- Opus surround (multistream) is rejected rather than decoded.

- Opus in WebM keeps 648 frames of end padding, because Matroska does not report it and Symphonia does not expose the block duration that would.

[0.2.0]: https://github.com/shakfu/playr/releases/tag/v0.2.0 [0.1.0]: https://github.com/shakfu/playr/releases/tag/v0.1.0
