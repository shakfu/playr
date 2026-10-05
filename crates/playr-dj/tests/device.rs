//! The stream's channel mapping and chunking, with no device: the test calls
//! `device::render` as the stream's callback does.

use playr_dj::{device, CueOut, Engine, Handle, Setting, Side, Track};

const SR: u32 = 48_000;
const FRAMES: usize = 1000;

/// An engine with deck A on the main mix and deck B on the cue, playing
/// distinct ramps.
fn engine() -> (Engine, Handle) {
    let (e, mut h) = playr_dj::new(SR);
    let ramp = |k: f32| {
        (0..SR as usize * 2)
            .map(|i| k * (i % 97) as f32 / 97.0)
            .collect()
    };
    h.load(Side::A, Track::new(ramp(0.5), 2, None).unwrap())
        .unwrap();
    h.load(Side::B, Track::new(ramp(-0.3), 2, None).unwrap())
        .unwrap();
    for s in [
        Setting::Xfade(0.0),
        Setting::CueBus(Some(Side::B)),
        Setting::CueOut(CueOut::Channels),
        Setting::Play(Side::A),
        Setting::Play(Side::B),
    ] {
        h.set(s).unwrap();
    }
    (e, h)
}

/// What the engine renders at `stride` channels in one call.
fn whole(stride: usize) -> Vec<f32> {
    let (mut e, _h) = engine();
    let mut out = vec![0.0; FRAMES * stride];
    e.process_channels(&mut out, stride);
    out
}

/// What `render` writes for a device of `ch` channels, in chunks of 64 frames.
fn rendered(ch: usize) -> Vec<f32> {
    let (mut e, _h) = engine();
    let mut out = vec![9.0; FRAMES * ch];
    let mut four = vec![0.0; 64 * 4];
    device::render(&mut e, &mut out, ch, &mut four, |v| v);
    out
}

#[test]
fn render_maps_the_engine_s_channels_to_the_device_s_across_chunks() {
    let two = whole(2);
    let four = whole(4);
    assert!(
        four.chunks(4).any(|f| f[2] != 0.0),
        "the cue reaches 3 and 4"
    );

    // Mono: the mean of the first two.
    let mono = rendered(1);
    for (o, s) in mono.iter().zip(two.chunks(2)) {
        assert_eq!(*o, (s[0] + s[1]) / 2.0);
    }
    assert_eq!(rendered(2), two);
    assert_eq!(rendered(4), four);
    // Past the fourth, silence.
    let six = rendered(6);
    for (o, s) in six.chunks(6).zip(four.chunks(4)) {
        assert_eq!(o[..4], *s);
        assert_eq!(o[4..], [0.0, 0.0]);
    }
}
