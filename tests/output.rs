//! Output format choice, against device capabilities built by hand.

use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfig, SupportedStreamConfigRange};
use playr::audio::output::{choose, Plan};
use playr::audio::Spec;

const CD: Spec = Spec {
    rate: 44100,
    channels: 2,
};

fn range(format: SampleFormat) -> SupportedStreamConfigRange {
    SupportedStreamConfigRange::new(2, 8000, 192_000, SupportedBufferSize::Unknown, format)
}

fn default(rate: u32, format: SampleFormat) -> SupportedStreamConfig {
    SupportedStreamConfig::new(2, rate, SupportedBufferSize::Unknown, format)
}

fn plan(rate: u32, format: SampleFormat) -> Option<Plan> {
    Some(Plan {
        rate,
        channels: 2,
        format,
    })
}

#[test]
fn an_integer_only_device_still_plays_at_the_source_rate() {
    // Common for `hw:` ALSA devices and USB DACs.
    for f in [SampleFormat::I32, SampleFormat::I24] {
        let got = choose(&[range(f)], Some(default(48000, f)), CD);
        assert_eq!(got, plan(44100, f), "{f:?}");
    }
}

#[test]
fn float_is_preferred_then_the_widest_integer() {
    let all = [
        range(SampleFormat::U16),
        range(SampleFormat::I16),
        range(SampleFormat::I24),
        range(SampleFormat::I32),
        range(SampleFormat::F64),
        range(SampleFormat::F32),
    ];
    let mut offered = all.to_vec();
    for want in [
        SampleFormat::F32,
        SampleFormat::F64,
        SampleFormat::I32,
        SampleFormat::I24,
        SampleFormat::I16,
        SampleFormat::U16,
    ] {
        assert_eq!(choose(&offered, None, CD), plan(44100, want));
        offered.retain(|r| r.sample_format() != want);
    }
}

#[test]
fn a_default_in_an_unusable_format_is_not_chosen() {
    // The source rate is refused, and the default config is 8-bit.
    let ranges = [
        SupportedStreamConfigRange::new(
            2,
            48000,
            48000,
            SupportedBufferSize::Unknown,
            SampleFormat::U8,
        ),
        SupportedStreamConfigRange::new(
            2,
            48000,
            48000,
            SupportedBufferSize::Unknown,
            SampleFormat::I16,
        ),
    ];
    let got = choose(&ranges, Some(default(48000, SampleFormat::U8)), CD);
    assert_eq!(got, plan(48000, SampleFormat::I16));
}

#[test]
fn nothing_usable_is_reported_as_no_plan() {
    let got = choose(
        &[range(SampleFormat::U8)],
        Some(default(48000, SampleFormat::U8)),
        CD,
    );
    assert_eq!(got, None);
}
