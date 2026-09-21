# cpal issue: ALSA `device_by_id` misses IDs it lists

A draft report for [RustAudio/cpal](https://github.com/RustAudio/cpal/issues), not yet filed. playr works around the bug in `output::device` (`crates/playr-core/src/audio/output.rs:290`), which matches the exact ID before calling `device_by_id`. The workaround can go once a fixed cpal is released.

Everything below the line is the issue text.

---

## ALSA: `device_by_id` returns `None` for `sysdefault:CARD=X`, an ID `output_devices` lists

### Summary

On ALSA, `HostTrait::device_by_id` cannot find a device whose ID has a `CARD=` argument and no comma, such as `sysdefault:CARD=Generic_1`. `output_devices()` lists that device with that ID, so an ID saved from enumeration cannot be looked up again.

### Environment

- cpal 0.18.2, the latest release when this was written

- Ubuntu 24.04.5, Linux 7.0.0, alsa-lib 1.2.11, PipeWire 1.0.5

- rustc 1.98.1

### Reproduction

```rust
use cpal::traits::{DeviceTrait, HostTrait};

fn main() {
    let host = cpal::default_host();
    let mut listed = 0;
    for device in host.output_devices().unwrap() {
        let id = device.id().unwrap();
        listed += 1;
        if host.device_by_id(&id).is_none() {
            println!("listed, but device_by_id finds nothing: {id}");
        }
    }
    println!("{listed} output devices listed");
}
```

Output on the machine above:

```
listed, but device_by_id finds nothing: alsa:sysdefault:CARD=Generic_1
40 output devices listed
```

The other 39 IDs round-trip. `aplay -L` lists `sysdefault:CARD=Generic_1` as the only PCM of that shape here.

### Expected

`device_by_id(&device.id()?)` returns the device for every device `output_devices()` yields.

### Cause

`AlsaHost::device_by_id` canonicalises the ID asked for, but not the IDs it compares against (`src/host/alsa/mod.rs:164`):

```rust
fn device_by_id(&self, id: &DeviceId) -> Option<Self::Device> {
    let canonical_id = DeviceId::new(id.host(), canonical_pcm_id(id.id()));
    self.devices()
        .ok()?
        .find(|d| d.id().ok().as_ref() == Some(&canonical_id))
}
```

`canonical_pcm_id` (`src/host/alsa/mod.rs:1696`) appends `,DEV=0` to an ID with `CARD=` and no comma:

```rust
if card_str.contains('=') {
    if !rest.contains(',') {
        return format!("{prefix}:{rest},DEV=0");
    }
}
```

So `sysdefault:CARD=Generic_1` becomes `sysdefault:CARD=Generic_1,DEV=0`. Enumeration reports the PCM under its hint name, without `DEV`, so no enumerated ID matches.

Other hint names of the same shape probably fail the same way, such as `default:CARD=X`, which `aplay -L` lists on some systems. That is inferred from the code; only `sysdefault` was observed.

### Suggested fix

Compare canonical forms on both sides:

```diff
 fn device_by_id(&self, id: &DeviceId) -> Option<Self::Device> {
     let canonical_id = DeviceId::new(id.host(), canonical_pcm_id(id.id()));
     self.devices()
         .ok()?
-        .find(|d| d.id().ok().as_ref() == Some(&canonical_id))

+        .find(|d| {

+            d.id()

+                .is_ok_and(|d| DeviceId::new(d.host(), canonical_pcm_id(d.id())) == canonical_id)

+        })
 }
```

With this change, all 40 IDs in the reproduction round-trip on the machine above. `alsa:hw:1,0` and `alsa:hw:Generic_1,0` still resolve, to `hw:CARD=1,DEV=0` and `hw:CARD=Generic_1,DEV=0`, so the aliasing that canonicalisation exists for is kept. An alternative is to try an exact match before the canonical one. That fixes the round trip, but leaves an alias such as `sysdefault:CARD=X,DEV=0` unmatched.
