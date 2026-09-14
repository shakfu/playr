# Exposing playback to sampling tools

Design note. Written 2026-09-13 against playr 0.2.0. Since 0.4.0, marked regions can be exported as files (README, Samples); the mark event file and OSC below are not implemented.

The goal: hearing a passage in playr, a user can hand it to a tool that samples well. playr itself does not become a sampler. It exposes enough for another tool to take the sample.

Settle the data model before the transport. A sampler needs little, and playr already has most of it.

## What a sampler needs

A sample is a region of a file. It need not be audio captured from playr's output. If playr names the region precisely, the tool reads it from the source file losslessly. Captured output would include volume, varispeed and any resampling.

| Field | In playr | Note |
|---|---|---|
| Absolute file path | Yes: canonical path | `file://` URL form for tools that want one |
| Source sample rate, channels, duration | Yes: `Status::source`, `duration` | |
| Position in the track | Yes: `track_position`, track time at any speed | |
| Position as a source frame index | Derivable: seconds x source rate | Frame indices are sample-exact; seconds are not |
| Speed or semitones | Yes | A sampler may reproduce the pitch shift |
| Tags | Yes: library, or read from disk | |
| A mark event | No | The one new concept: "this moment" |

### Precision problems

1. **Output latency.** `frames_out` counts frames handed to the device, not frames heard. It runs early by the device buffer plus output latency, roughly 10-50 ms (inferred, not measured). cpal's `OutputCallbackInfo::timestamp()` reports callback and playback instants, which could correct this.

2. **Decoder offsets.** Frame 0 in playr is after gapless trimming. For lossless formats this matches any decoder. For MP3 and AAC, another decoder may count encoder delay differently. playr's AAC already decodes about 1,900 frames long, so a sampler reading the same MP3 or AAC file with ffmpeg could disagree by that much.

3. **Reaction time.** The decision to sample comes after the sound has passed. A mark should record the current position and let the sampler look back. Alternatively a mark names a region that ends now, such as the last 8 s.

## Candidate protocols

| Protocol | Fit for sampling tools | Platforms | Cost to playr |
|---|---|---|---|
| OSC over UDP, localhost ([OSC 1.0][osc]) | Best. SuperCollider, Max, Pure Data, TouchOSC, Bitwig and REAPER speak it. SuperCollider's [`Buffer.read(server, path, startFrame, numFrames)`][sc-buffer] takes a path and a frame region. | All | Small encoding, hand-written or `rosc`. Opens a UDP socket, so the README's "no network code in it at all" becomes false as written. |
| MPRIS over D-Bus ([spec][mpris]) | Standard for players: `xesam:url`, `Position` in microseconds, `Rate`, `Seeked`. No marks or regions. | Linux only | A D-Bus dependency |
| JSON over a Unix socket, as [mpv's IPC][mpv-ipc] | Flexible; music tools need a bridge script | macOS, Linux | Small. `AF_UNIX` is not network code, so the README claim holds. |
| Append-only JSON Lines file, e.g. `$XDG_STATE_HOME/playr/marks.jsonl` | Any tool can follow it; a short script can forward it to OSC | All | Smallest: no socket, no protocol to version |
| Audio routing (BlackHole, JACK, PipeWire) | Captures output, not source | Varies | None, but lossy as described above |

Links were written from memory and not checked.

## Recommendation

### First: a mark key that appends to a file

One key appends one JSON line:

```json
{"path":"/music/x.flac","frame":3668160,"rate":44100,"channels":2,"semitones":0,"title":"...","artist":"...","at":"2026-09-13T21:04:11.382Z"}
```

- **Why this over a socket:** one key and one write. The "no network code" claim holds. It settles the region format first, and that format is the part that matters.

- **Trade-off:** tools cannot query the position or receive events live. Following the file gives a sampler marks only, which may be all it needs.

### Next, for live integration: send-only OSC to 127.0.0.1

- playr sends `/playr/mark`, `/playr/track` and `/playr/position`, and never listens. With no inbound socket there is no remote-control surface.

- It is the protocol sampling tools already use.

- It needs the README claim amended, for example: "no network access; optional send-only OSC to localhost, off by default".

### Alternative: a copyable region

playr exposes no protocol. The mark key puts the region on the clipboard in a form any tool can parse, such as `x.flac@3668160+352800`. This is the most minimal option. It fails if the sampler must react without a window switch.

## Open questions

1. **Which tools receive this?** SuperCollider or Max point to OSC. ffmpeg or sox scripts point to the file or the clipboard. A DAW depends on which one.

2. **Is "no network code" a hard property or a default?** It decides between OSC and a file or Unix socket.

3. **One mark or two?** A single mark is simplest. Separate in and out marks are more precise but add state to the interface.

4. **Should a region survive varispeed?** For a sample taken at +3 st, the tool either gets the source region plus `semitones`, or reproduces what was heard.

Further work, if pursued: an OSC address layout with type tags, latency correction from cpal timestamps, and MP3/AAC frame offsets pinned against ffmpeg.

[osc]: https://opensoundcontrol.stanford.edu/spec-1_0.html [sc-buffer]: https://doc.sccode.org/Classes/Buffer.html [mpris]: https://specifications.freedesktop.org/mpris-spec/latest/ [mpv-ipc]: https://mpv.io/manual/stable/#json-ipc
