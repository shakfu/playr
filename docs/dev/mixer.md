# Mixer

Design, written 2026-10-06 against `dev` at `f6034bb`, before any of it was built. It answers "Mixer" in `TODO.md`. Phases 1 and 2 were built the same day; "Built" at the end records where they differ.

## Goal

One master volume over everything playr plays, a level and a mute per source, and a headphone level for the DJ cue that the master does not scale.

Taken in discussion, 2026-10-06:

- **Three sources:** the player, the tape and the decks. The sampler is not a source: it plays through the player, and auditions share the player's level.
- **Gains only (design A).** The three streams stay separate. One output stream is phase 3, optional.
- **Cut only.** Every fader tops out at unity (0 dB) and defaults to it.
- **A dB-linear gain law over 60 dB** in place of linear amplitude, with cubic as a setting and a command to compare them. No starting level changes on upgrade.
- **The player still pauses** while the tape or a deck plays.
- **Mute per source**, keeping its level.
- **Record and the meters do not follow the faders.**
- **A panel of its own**, `:mix` commands, and the server page and OSC in the first version.

## What exists

| source | stream | gain | ramped | meter |
|-|-|-|-|-|
| player | `playr-core` `audio/output.rs` | `Cmd::SetVolume`, linear, 0 to 1 | no: read once per callback (`output.rs:584`) | LUFS and peak, before the gain (`output.rs:594`) |
| decks | `playr-dj` | `Setting::Volume`, after the soft clip, on main and cue (`mixer.rs:606-614`) | yes | peak, after the gain, over main and cue (`engine.rs:236`) |
| tape | `playr-looper` | only the fade round a load (`looper.rs:148`) | yes | peak |

- The player's volume is the master today. `dj::poll` copies `player().volume()` to the decks each poll (`dj.rs:548`). The tape follows nothing.
- `volume` in `settings.toml` is a percent; `Settings` divides it by 100 (`settings.rs:326`). The `persist` value is the same 0 to 1 number as text (`persist.rs:45`).
- Controls: `:volume` and `+`/`-` (`keys.toml:46-49`), the window's transport slider (`transport.rs:114`), the page's slider (`page.html:257`), and OSC `/playr/volume` (`osc.rs:161`).
- Nothing stops the tape and the decks from playing at once (inference: no code links them).

## Design

### Gain law

Two laws map a fader position `p`, 0 to 1, to a gain:

- **`db`, the default:** `gain = 10^(60 (p - 1) / 20)`, and 0 at `p = 0`. Each 1% of travel is 0.6 dB, so every step sounds alike, and a fader can show its value in dB.
- **`cubic`:** `gain = p^3`. PulseAudio maps its software volume this way ([`volume.h`](https://code.delx.au/pulseaudio/blob/a8f0d7ec1e5cb3f52c4b1abfce1016ed6d9b00f8:/src/pulse/volume.h)), so 50% in playr matches 50% in the desktop's mixer on Linux (inference for PipeWire).

| position | linear today | `db` | `cubic` |
|-|-|-|-|
| 100% | 0 dB | 0 dB | 0 dB |
| 95% (one `-` press) | -0.4 dB | -3.0 dB | -1.3 dB |
| 50% | -6.0 dB | -30.0 dB | -18.1 dB |
| 10% | -20.0 dB | -54.0 dB | -60.0 dB |
| 1% | -40.0 dB | -59.4 dB | -120 dB |

Linear amplitude puts the first 20 dB of cut in the top 90% of travel. 60 dB over `db` makes 0% a fade to silence rather than a jump; 40 dB left -39.6 dB at 1%, clearly heard before the cut. The range is a constant, not a setting.

The law lives in `playr-app`, as one function. The engines keep taking linear gain, so `Cmd::SetVolume` and `Setting::Volume` keep their meaning.

A `fader` setting chooses the law at start, `"db"` or `"cubic"`, a top-level key `playr-app` reads as it reads `theme`. `:mix law cubic`, `:mix law db`, or `:mix law` to toggle, switch it for the session. The positions stay, so the level changes where the faders stand: that is the comparison. Switching is not remembered; the setting is.

### `Mix`, in playr-app

```rust
pub enum Strip { Master, Player, Tape, Decks }

pub struct Mix {
    level: [f32; 4],     // positions, by Strip
    muted: [bool; 4],
    headphones: f32,     // position
}
```

Each source gets `law(master) * law(level)`, or 0 when the source or the master is muted. `Mix` sends it when it changes, as `follow_volume` does now:

- **Player:** `Cmd::SetVolume(gain)`.
- **Decks:** `Setting::Volume(gain)` on the main mix only. A new `Setting::Headphones(gain)` scales the cue bus, from `law(headphones)` alone. Muting the decks leaves the cue, as `Setting::Mute(Side)` already does for one deck.
- **Tape:** a new `Setting::Level(gain)`, ramped.

`Mix` holds the master, not the player. `Snapshot.volume` reads `Mix`, and `Session::volume_by`, which reads the player's gain to add to it, moves to `Mix`. The architecture's open issue "`Session` does not apply `Settings`" grows: a frontend that skips `playr-app` gets no mixer. Accepted; no such frontend exists.

### Engines

- **Player ramp.** `render` ramps from the last callback's gain to the new one across the block, so a fader move does not step. One `f32` of state in the callback.
- **Tape level.** Applied in `Looper::process` after the recording push and after `publish`. Record keeps the tape's own mix, and the peak meter reads before the fader.
- **DJ headphones.** The cue bus multiplies by its own ramp in place of `vol`. With `CueOut::Split` on a stereo device, that is the cue side.
- **DJ meters.** The decks' peak is taken over the whole output after the mixer (`engine.rs:236`), so it follows `vol` and includes the cue channels. It moves to the main mix before `vol`, to match the others.

### Commands

`:mix` is new, so decision 4 does not apply. `:volume` stays as the master's command, and its keys stay.

```
:mix                      the strips, their levels and mutes
:mix tape 80              set a level, in percent of travel
:mix decks -10            change it
:mix player mute          mute, unmute, or toggle with no word
:mix headphones 70
:mix master 60            the same as :volume 60
:mix law cubic            the gain law for this session: db, cubic, or toggle with no word
```

`Action::Mix(Strip, Change)`, with `Strip` gaining `Headphones` for the parser only, since it has no mute.

### Frontends

- **Window:** a Mixer tab beside Tape and DJ. A vertical fader, a mute and a pre-fader peak per strip, and a headphone knob. The transport's Volume slider stays as the master. The DJ tab gets a headphone knob too, showing the same value.
- **Terminal:** commands only. The status line's volume bar shows the master, as now.
- **Server:** the page gets the strips under its volume slider. OSC adds `/playr/mix/<strip>` (0 to 1) and `/playr/mix/<strip>/mute` (0 or 1), and reports them as `/playr/volume` is. `/playr/volume` stays the master.

### Settings and state

No starting level changes on upgrade. An old value is a linear gain; it converts to a position with the inverse of the law in use.

- **A new setting, `master`,** is the master's position in percent. The defaults file leaves it unset, so a user's file decides.
- **`volume` still reads,** as the linear gain it always was, and converts. playr never writes `settings.toml` (decision 1), so it cannot rename the key for the user. A file setting both is an error naming `master`; the CHANGELOG says to replace `volume`.
- **The remembered volume moves to a new state key, `master`,** holding a position. When only the old `volume` key exists, it is read once as a gain and converted. Reusing the key cannot tell an old gain from a new position.
- **A gain below the law's floor** converts to position 0 under `db`: -60 dB and less is silent at the bottom of the fader.
- **`persist` gains `mix`:** the source levels, the mutes and the headphones, as one line of text, as `eq` is stored. Without it each run starts at unity and unmuted.

## Tests

- Laws: both give 0 at 0 and 1 at 1, and are monotonic; `db` gives -30 dB at 0.5, `cubic` 0.125; each inverse round-trips.
- Law switch: `:mix law` keeps the positions and changes the gains sent.
- `Mix`: the product of master and level; a mute gives 0 and unmute restores the level; master mute silences all three; headphones ignore master.
- Player: a gain change across a callback steps by no more than one ramp step.
- Tape: a level change does not step; a recording is identical at level 1 and level 0.2; the peak is unchanged by the level.
- Decks: `Volume(0)` leaves the cue at full; `Headphones` leaves the main mix; the full-scale test at unity still passes.
- Persist: the old `volume` gain converts to a position once, under either law, and plays at the same gain; `mix` round-trips; bad text is ignored, as for the other keys.
- Settings: `master = 50` is position 0.5; `volume = 50` is gain 0.5 under either law; both set is an error; `fader` takes `db` and `cubic` and rejects other words.
- Commands and dispatch: each `:mix` form, and `:mix master` equal to `:volume`.
- Server: the OSC addresses set and report; the page's JSON carries the strips.
- Window: the Mixer tab fits at 800 by 592, as the DJ tab's test does.

## Alternative considered

**One output stream (design B).** A new crate owns the only callback. It reads the player's ring and calls the decks' and the tape's `process` directly, then sums them through one clip and one meter. It adds a master limiter, a real master meter, and works on an exclusive `hw:` device. It does not add the player's 2 s ring to the decks, which is what open question 6 in `docs/dev/dj-engine.md` rejected. Against it: the player opens its stream at each track's rate. One stream means resampling the player to a fixed rate, which ends bit-perfect output, or reopening on every rate change, which interrupts the decks. Phase 3 takes it up once master recording became a requirement, resampling the player only while another source is attached; see "Phase 3: one output stream".

Rejected for phase 1:

- **Boost above unity.** The decks' and the tape's fader comes after their soft clip, so gain above 1 clips hard at conversion. Trim and ReplayGain already add gain before the clip.
- **Solo.** Three sources; mute does it.
- **Dropping the player's pause.** Mute replaces it if that changes.

## Built

Phase 1, 2026-10-06. `make test` passes: 932 tests. Differences from the design above:

- **Five strips, each with a mute.** The headphones take a mute too, so `Mix` treats every strip alike.
- **`:mix STRIP mute` toggles; `mute on` and `mute off` set it.** This follows `:tape write on|off`. There is no `unmute`.
- **The tape's volume is `Cmd::Volume` on the looper**, with `Handle::set_volume`. `Setting::Level` already names a voice's level. A new looper gets the mixer's volume before its stream opens, so a muted tape stays silent through a load.
- **The player's ramp keeps its state in `Shared::applied`.** That leaves `render`'s signature and its five callers alone.
- **The server page allows `Action::Mix`.** It has no faders yet; `:mix` works from its command bar. The window's parity test lists `Mix` as reached through the command bar until the Mixer tab exists.
- **`persist`'s `mix` text** is the player, tape, decks and headphones positions, then the five mutes as 0 or 1, master first.
- **`:volume`'s help** now reads "set the master". `:help` sizes its argument column to the longest, so `:mix`'s usage is shortened to fit 80 columns.

Phase 2, 2026-10-06. `make test` passes: 933 tests. `make page-test` was not run: Playwright's browser is not installed on the machine that built it.

- **A dialog, not a tab.** Mixer, beside EQ in the transport, opens a window as EQ does. A tab hid the faders while the Tape or DJ tab showed, which is when the tape's and the decks' faders are used.
- **No meters in the dialog.** The reason given was wrong: it said a second reader would take the tape's and the decks' peaks from the tabs. Nothing reads those peaks; only the player's is taken, by `Model::refresh`. Meters were left out, not blocked.
- **The server has the master and a mute only.** It refuses `Tape` and `Dj` actions, so the tape's, the decks' and the headphones' faders change nothing there. The page has a mute checkbox beside its volume slider; OSC has `/playr/mute`, received and sent, and the TouchOSC layout a toggle beside Volume. The page's JSON carries `muted`. `:mix` is allowed from the page's command line.

## Phase 3: one output stream

Design, written 2026-10-06 against the phase 2 tree, before any of it was built.

### Why

Two requirements, taken 2026-10-06:

- **Record the master.** What the device plays, as one file.
- **A Mix tab** for calibrating levels while two or three sources play, with meters and the master's clipping in view. It replaces the Mixer dialog.

Phases 1 and 2 cannot meet the first. The player, the tape and the decks each open a cpal stream, and the OS mixer (PipeWire, CoreAudio, WASAPI) sums them after playr has let go. No part of playr holds the master, so it can be neither recorded nor metered. A sum of the strips' peaks bounds the master's peak from above, but it is not the master.

Alternatives rejected:

| | records | against |
|-|-|-|
| A software sum for recording only | the three post-fader outputs, summed on a writer thread | the callbacks are not aligned: sources land up to a device buffer apart, about 10 to 20 ms. It records a mix the device never played |
| The OS's loopback | the device's output | it also records other programs. Per platform: a PipeWire monitor works; WASAPI loopback through cpal 0.18 is unchecked; macOS needs a third-party driver |

### The bus

`playr-core` owns the only output stream, the bus. Its callback sums the sources:

```rust
/// A source the bus calls from its callback. It must not lock, allocate or free.
pub trait Source: Send {
    /// Fills `main`, and `cue` when it has one, interleaved stereo at the bus's rate.
    fn process(&mut self, main: &mut [f32], cue: Option<&mut [f32]>);
    /// What it last played, before its fader, for its meter.
    fn take_peak(&mut self) -> f32;
}
```

- **The player is one source.** It keeps its ring, its EQ and its ramp; `render` becomes its `process`. Its 2 s of read-ahead stays its own, so cue and nudge latency on the decks do not change. This is the objection open question 6 in `docs/dev/dj-engine.md` raised against mixing into the player's callback; the bus is a different callback.
- **The tape and the decks are sources.** Each has a type in its own crate that implements `Source`, so `playr-core` depends on neither. Their own streams (`playr-looper/src/device.rs`, `playr-dj/src/device.rs`) go.
- **Attaching.** A source is moved to the callback through a command ring, boxed on the sending thread. A detached source comes back through a return ring, to be dropped off the audio thread, as a loaded loop does now.
- **The bus outlives the player's tracks.** Today `Engine::stop` drops the stream (`audio/mod.rs:1008`). The bus stays open while any source is attached, the player included while it plays.

The signal path, per callback:

1. Each source fills `main`, and the decks `cue`. Gains stay where phase 1 put them, in each engine.
2. The bus sums every `main` into the master, and meters each source and the master.
3. The master goes to the recorder, if recording.
4. Routing: master on channels 1 and 2. The cue goes on 3 and 4, or on stereo devices the split: master in mono left, cue in mono right. This moves out of `Mixer::process_channels`, which keeps computing main and cue but stops placing them.
5. Conversion to the device's format.

No clip on the master. With one source at unity the master equals that source sample for sample, so a lone player stays bit-perfect where it is now. A sum over full scale clips hard at conversion; the master meter shows it, and the recording keeps it unclipped, so a take with overs is mended by lowering its gain. A soft clip, as the DJ master's at `dj_knee`, would bend a lone player's loud samples too; a limiter adds delay and changes the dynamics.

### Rate

The bus runs at one rate.

- **The player alone** sets the rate per track, as now: the bus reopens when a track's rate changes, and nothing else plays.
- **With the tape or the decks attached**, the rate is pinned. A player track at another rate is resampled to it, with the resampler the player already uses when a device refuses a rate (`Plan::needs_resample`). Bit-perfect output is lost only then.
- **A tape or deck load** resamples to the bus's rate, or, with the bus closed, to the rate `device::rate` picks now, and the bus opens at that.
- **Recording pins the rate** too, so one file has one rate.

### Channels

The bus opens with the channels the player's negotiation picks, at least 2. `:dj cue-out 3-4`, the cue on outputs 3 and 4 of an interface whose 1 and 2 feed the room, needs 4: choosing it reopens the bus with 4 where the device has them, which interrupts whatever plays once. It is a setup step, done before a set. The decks' own stream opened 4 whenever the device had them; the bus does not, since a 4-channel stream gains a listener nothing, and a PipeWire sink advertising 4 may fold 3 and 4 into the speakers (inference, from the review).

### Recording the master

- `:mix rec` starts and stops. The Mix tab has a Record button.
- A 32-bit float stereo WAV of the master at the bus's rate, before routing, so the cue is never in it. Overs are kept.
- Written as the tape's mix recording is now: a ring the callback fills, a writer thread, a count of blocks dropped if the disk falls behind. That code moves from `playr-looper` to `playr-core`, which the tape then uses too.
- Saved under `samples/mix-<time>.wav` and added to the library, as tape saves are.

### Meters

The bus takes each source's peak and the master's each callback, through atomics, as the player's meter is now. `Model` reads them once a frame and holds each peak for 1.5 s.

- A strip's meter is after its fader: the source's peak times its gain. Exact while the fader is still.
- The master's meter is the master, measured. Over 0 dBFS lights it red and leaves the hold.

### The player's pause, and Take

Three things pause the player today: the tape's Play (`playr-app/src/tape.rs:598`), a deck starting by Play, Cue, a held cue or a hot cue (`dj.rs:1006-1013`), and the DJ Take (`hand_over`, `dj.rs:841`).

- **A deck starting no longer pauses the player.** Calibrating with the decks and the player playing needs it gone; a mute does what the pause did.
- **The tape's Play still pauses it.** The pause is once, at the start: Play on the player afterwards runs both.
- **The DJ Take still pauses it**, as part of the handover: the track would otherwise play twice. On the bus the player stops on the frame the deck starts; on separate streams the two were up to a device buffer apart (inference, not measured).
- **The tape gains a Take.** `:tape take` loads the sampler's range of the playing track, as `:tape load` does, and hands the player over to the tape once the load is in. The position is read then, as the DJ Take reads it:

  | the player | the tape |
  |-|-|
  | inside the range | starts voice 1 at the player's position in the loop; the player pauses on that frame |
  | before the range | waits, and takes over at the loop's start when the player reaches the range's start |
  | past the range | refuses: the player is past the range |

  The other voices keep their window starts. The wait needs the shared clock, so the tape's Take comes with the bus.

### The Mix tab

A tab beside Tape and DJ, a strip per source and one for the master and one for the headphones:

- a vertical fader, its level in dB, a meter, a mute;
- the player's strip has the three EQ bands; the EQ dialog becomes a popup under the transport's EQ button, closed by a click elsewhere;
- the master's strip has Record, its time, and the blocks dropped;
- the law switch.

The Mixer dialog goes. The tab's label shows a mark while any strip is muted or the master has clipped since the tab was last shown.

### Risks

- **The callback.** The most timing-sensitive code in playr gains the tape's and the decks' DSP. The allocation tests (`playr-looper/tests/alloc.rs`, `playr-dj/tests/alloc.rs`) extend to the bus with every source attached.
- **Device loss** now ends all three sources at once. Each engine already handles its own loss; the bus reports one loss to all.
- **The engine loop** in `audio/mod.rs` decides when the stream opens, closes and reopens. The bus takes that over, so every reopen path (rate change, seek stall, device loss) changes. `tests/engine.rs` and `tests/render.rs` cover them now and must keep passing unchanged.

### Tests

- The bus with a fake backend: two sources sum exactly; one source at unity passes unchanged; attach and detach mid-stream do not allocate in the callback; a detached source comes back.
- Routing: split and channels 3 and 4 place main and cue as `Mixer::process_channels` does now; the recording holds no cue.
- Rate: the player alone reopens per track; with a source attached, a new track at another rate is resampled and the bus does not reopen; recording pins the rate.
- Recording: the file equals the master the callback produced, less dropped blocks, as the tape's recording test does now.
- Meters: a strip's peak follows its fader; the master's is the measured sum.
- The pause: starting a deck leaves the player playing; the DJ Take pauses it on the deck's first frame; the tape's Play pauses it.
- The tape's Take: inside the range, the loop starts at the player's position; before it, the handover lands on the range's start; past it, a refusal.
- Window: the Mix tab fits at 800 by 592; its controls send the actions the dialog sends now.

### Decided

2026-10-06: a deck starting stops pausing the player; the tape's Play and both Takes pause it; the tape gains a Take; cue-out 3-4 reopens the bus; no master clip.

### Built

Step 1, 2026-10-06. `make test` passes: 941 tests, 8 of them the bus's (`crates/playr-core/tests/bus.rs`). No assertion of an existing test changed; `Backend::start` and `render` take the bus, so the fake device in `tests/common` and `render.rs`'s helper pass one. Differences from the design:

- **`Source` has `process` only.** The cue and the peak arrive with the decks and the meters, in steps 2 and 3.
- **The sources run in chunks of 1,024 frames**, summed into a buffer the bus holds, and are read sample by sample as the player's loop reaches them. With no source attached, `render` takes its old path, so a lone player is unchanged sample for sample.
- **A source plays only while a stream is open.** The player's stream still closes on stop; keeping it open for the sources is step 2, with the first real ones.
- **Between streams, the engine holds the sources.** A closing bus sends home its sources and those still in its command ring; the engine hands them to the next stream, and drops one detached meanwhile. At most 8 play at once; more come back unplayed.

Step 2, 2026-10-06. `make test` passes: 949 tests; `make page-test`: 9. Differences from the design:

- **`Sources`**, a cloneable handle from `Player::sources`, attaches, asks the rate (`rate_for`), widens the stream (`widen`) and counts device losses. `attach` returns an `Attachment`, which detaches when dropped and says whether the device was lost since. `rate_for` and `widen` wait for the engine's answer, up to 2 s.
- **The stream outlives a stop while a source is attached.** A stop empties the player's ring as a seek does, by the callback's flush, rather than by dropping the stream. The stream opens for a source with no track, at the rate the source asked, and closes when the last source goes with the player stopped.
- **`Source::process` fills a cue** and returns where it goes (`Cue::None`, `Split`, `Channels`). The routing applies to the master, the player included, frame by frame; with no source the player's old path is unchanged.
- **The DJ mixer's split now means the clipped stereo**: each side is the mean of the main mix's clipped left and right, where the decks' stream clipped the mean. They differ only above `dj_knee`.
- **A device loss** drops every source, as the decks' and the tape's own streams did, and the tape and the decks learn of it from their `Attachment`, not a device event.
- **The tape's Take is checked each refresh** (about 16 ms in the window, 33 ms in the server), not on the frame. A frame-exact handover needs a hook in the bus's callback, which nothing else needs yet.
- **`Setting::Head`** places a voice's head inside its window, for the Take.
- **Removed:** `playr_looper::device`, `playr_dj::device`, their error variants and cpal dependencies, and `playr-dj/tests/device.rs`, whose channel mapping and chunking the bus's routing tests now cover.
- **The Tape tab's hint** reads "Set a sampler range, then Load range or Take." The Take button made the old one overrun 800 points.

Step 3, 2026-10-06. `make test` passes: 953 tests. Differences from the design:

- **The recording lives in `Session`** (`start_master`, `stop_master`, `master_done`), which owns the samples directory and adds the file to the library; `Frontend` is unchanged. `:mix rec` toggles it.
- **The file is `master/master.wav`**, then `master-2/master-2.wav`, each in a directory of its own as the tape's saves are, rather than `mix-<time>.wav`: no clock formatting, and one naming scheme.
- **The writer is `playr-core`'s own**, not the looper's moved: the looper keeps its recorder, which records the tape alone before its fader.
- **A recording pins the rate and holds the stream** as a source does, and opens one at 48 kHz, or the nearest the device takes, when nothing plays. A stop of the player leaves it recording silence.
- **A closing stream ends the recording** with its file complete: a device loss, or a stall's reopen. Found while testing: a device loss kept the stream while a recording held it, so the recording never ended; a loss now drops the stream whatever holds it.
- **The master's peak is measured on both paths**: the player's alone, after its volume, and the bus's sum.
- **The strip meters are the engines' peaks times their own fader and mute**, without the master; exact while the fader is still, as designed.

Step 4, 2026-10-06. `make test` passes: 954 tests. Differences from the design:

- **The strips sit side by side in a horizontal scroll area**, the player's EQ beside its strip and Record and the law beside the master's; at 800 by 592 nothing scrolls or overlaps.
- **The headphones have no meter**: nothing measures the cue alone yet.
- **The tab's mark is `Mix *`**, for a mute, or for a master over full scale while the tab was hidden. The clip half is not under test: the fake device cannot be driven over full scale without a test-only path.
- **The EQ popup closes on a click outside it**; the Mix tab's EQ is the same state.

### Steps

1. `Source` and the bus in `playr-core`, with the player as its only source. The device and engine tests pass unchanged. Built.
2. The tape and the decks as sources; their streams go. A deck's start stops pausing the player; the tape's Take. Built.
3. Meters and master recording. Built.
4. The Mix tab and the EQ popup; the Mixer dialog goes. Built.

## Phases

1. Law, `Mix`, player ramp, tape level, DJ headphones, `:mix`, persistence, tests. Built.

2. The window's Mixer tab, the page and OSC. Built, with a dialog for the tab.

3. One output stream, master recording and the Mix tab, as above.
