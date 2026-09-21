# Output device selection

Design, written 2026-09-21 against playr 0.8.1, before any of it was built. It answers "Device selection" under Output in `TODO.md`. Phase 1 was built the same day, with each open decision taken as recommended; "Built" at the end records where it differs.

## Goal

Play to an output device other than the host default, chosen by flag or setting. It is also the prerequisite for "Bit-perfect output", which needs a `hw:` ALSA device.

Open decisions, with the recommendation first:

- **Missing device:** fail at startup, or fall back to the default with a warning.

- **Window control in phase 1:** leave it out, or ship it without persistence.

- **Name matching:** IDs only on ALSA, where names are shared by up to 15 PCMs (see "Checked on ALSA").

## What exists

- **One call site.** `Player::new` calls `output::default_device()` (`crates/playr-core/src/audio/mod.rs`). `playr`, `playr-gui` and `playr-server` each call `Player::new` after `Config` loads, so a setting is available where the device opens.

- **Stable IDs in cpal.** cpal 0.18 `DeviceId` implements `Display` and `FromStr` as `<host>:<id>`, e.g. `alsa:hw:CARD=DAC,DEV=0`. `HostTrait::device_by_id` looks one up; on ALSA it canonicalises the ID first (`canonical_pcm_id`, `cpal-0.18.2/src/host/alsa/mod.rs`). playr needs no naming scheme of its own.

- **A seam for tests.** The `Backend` trait already lets tests supply a fake device.

- **Device loss.** `DeviceEvent::Lost` stops playback and reports it.

## Design

### Resolution, in the core

`output::device(want: Option<&str>) -> Result<Device, OutputError>` replaces `default_device()`:

- `None`: the host default.

- A string: parsed as a `DeviceId` and looked up with `device_by_id`, so `hw:2,0` and `hw:CARD=Generic_1,DEV=0` both work.

Matching by `description().name()` is left out: on ALSA the name is the card's, shared by its `hw`, `plughw`, `dmix`, `front` and `surround*` PCMs. Whether it is usable on macOS and Windows is untested.

`OutputError::NotFound { want, available }` lists the IDs to use instead. `Player::new` takes `Option<&str>`; `with_backend` is unchanged.

### Setting

`device` is a top-level core key in `Settings`, `Option<String>`, shipped as `device = ""` (the default device). As a core key, every frontend reads it, so it does not depend on "Settings tables for more than one frontend".

Parsing checks the type only. Existence is checked at open, since a settings file may be read on a machine without that device.

### Command line

- `--device ID` on all three binaries, overriding the setting.

- `playr devices` lists output devices: ID, name, and which is the default. It drops the `CARD=<index>` duplicates cpal lists, since card numbers can change at boot, and marks devices that offer no config. Optionally it shows each device's rates and formats, to check a `hw:` device before relying on it for bit-perfect output.

`playr-gui` and `playr-server` have no subcommands; their `--help` for `--device` points to `playr devices`.

### Missing or busy device

Startup fails with the list of devices. No fallback.

- It matches `playr-app/src/config.rs`: any settings error stops playr starting.

- A fallback plays through the wrong speakers, and hides why output is not bit-perfect.

- Under systemd, `playr-server` restarts until a USB DAC appears.

A `hw:` device held by PipeWire fails with `EBUSY`, which cpal maps to `ErrorKind::DeviceBusy`. `Cpal::start` reports this as a generic `OutputError::Build` today; it gets its own message naming the likely holder.

A device lost during playback stops, as now. No automatic reroute to the default, for the same reasons.

### Window control (phase 2)

A `Cmd::SetDevice` swaps the engine's `Box<dyn Backend>`, tears down the output, and restarts at the current position, reusing the sample-rate-change rebuild.

It waits on persistence. Nothing writes `settings.toml` today (inference, from reading the settings code). A choice lost at restart has little use. Storing it in the library, as `resume` is, gives the device two sources of truth.

### Null device (phase 3, optional)

On Linux, ALSA already lists `alsa:null`, which needs no sound card. `--device alsa:null` may be enough for `make page-test` in CI; see "Browser tests in CI" in `TODO.md`. Untested: whether it paces the callback at real-time rate, and whether CI runners' alsa-lib lists it. If either fails, a `Backend` that drains the ring on its own thread replaces it.

## Tests

- Resolution: an unknown ID is an error listing the available IDs; the `CARD=<index>` filter in `playr devices` is a pure function over IDs.

- `Settings`: `device` parses, and a non-string is an error.

- CLI: `--device` overrides the setting; an unknown device exits non-zero with the list.

- Opening a real device needs hardware, as `the_default_output_device_plays` in `crates/playr-core/tests/engine.rs` already does.

## Checked on ALSA

Checked 2026-09-21 on one Linux machine (3 cards, PipeWire, cpal 0.18.2), with a throwaway program listing `output_devices()` and calling `device_by_id`.

- **ID forms.** Enumeration lists both forms: 46 IDs with `CARD=<name>` and 20 duplicates with `CARD=<index>` (73 devices in all). `hw:2,0` resolves to the index form, `hw:Generic_1,0` to the name form.

- **User PCMs.** PCMs from `~/.asoundrc` are listed and found by `device_by_id`, with or without a `hint` block. The description is the hint's text, or else the PCM's name.

- **Names.** "HD-Audio Generic, ALC269VC Analog" names 15 PCMs. Names identify a card, not a device.

- **Unusable entries.** An HDMI output with its monitor off is listed with 0 configs.

- **Null.** `alsa:null` is listed, offering 832 configs.

Not checked: macOS and Windows, a USB DAC, and a Raspberry Pi.

## Alternative considered

Leave routing to the system. PipeWire moves a stream between devices (`wpctl`, `pavucontrol`, the `target.object` property), and `default.clock.allowed-rates` reduces its resampling. playr would then only need `--device` for bit-perfect `hw:` output, with no window control. This fits Linux; macOS and Windows have no equivalent, which decides it by how many users run there.

## Built

Phase 1, 2026-09-21. Differences from the design above:

- **Exact match first.** cpal 0.18.2's `canonical_pcm_id` appends `,DEV=0` to an ID with `CARD=` and no comma, so `device_by_id` misses `alsa:sysdefault:CARD=X`, which cpal itself lists. `output::device` looks for the exact ID among the output devices, then falls back to `device_by_id` for forms such as `hw:2,0`. The upstream report is drafted in `docs/dev/cpal-issue.md`.

- **Bare IDs.** A string without a known host in front is an ID on the default host, so `hw:2,0` works without `alsa:`.

- **No config probing in `playr devices`.** Marking devices with no configs, and listing rates and formats, would open every PCM. Left out.

- **Busy, checked 2026-09-21.** `OutputError::Busy` comes from `ErrorKind::DeviceBusy` at config query or stream build. With PipeWire playing to HDMI (`card0/pcm7p` open, owner `pipewire`), `alsa:hw:CARD=Generic,DEV=7` failed at the config query and playback stopped with the busy message. The analog `hw:CARD=Generic_1,DEV=0` on the other card opened and played alongside it: PipeWire holds only the device it plays to. The message shows at play, not at startup, since the device opens when a track starts.

- **`alsa:null` does not pace.** It played 2 s of audio in under 0.8 s (one run). Phase 3 therefore needs its own `Backend` for CI, not `--device alsa:null`.

## Phases

1. Core resolution, setting, `--device`, `playr devices`, busy-device message, tests.

2. Window control, once persistence is decided.

3. Null device.
