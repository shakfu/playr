//! Decoder and channel-mapping tests.
//!
//! The format cases generate real files with ffmpeg, so they check what this
//! build can actually decode rather than what the dependency claims.

use playr::audio::decode::AudioStream;
use playr::audio::output::remap_channels;
use std::path::Path;
use std::process::Command;

fn have_ffmpeg() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Encodes a 2 second 440Hz stereo tone with the given codec arguments.
fn encode(path: &Path, rate: u32, args: &[&str]) -> bool {
    Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency=440:sample_rate={rate}:duration=2"),
            "-ac",
            "2",
        ])
        .args(args)
        .arg(path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Decodes the whole file, returning `(rate, channels, frames, peak)`.
fn decode_all(path: &Path) -> Result<(u32, u16, usize, f32), String> {
    let mut s = AudioStream::open(path).map_err(|e| e.to_string())?;
    let mut frames = 0usize;
    let mut peak = 0f32;
    loop {
        let ch = s.spec().channels.max(1) as usize;
        match s.next_chunk() {
            Ok(Some(c)) => {
                frames += c.len() / ch;
                for v in c {
                    peak = peak.max(v.abs());
                }
            }
            Ok(None) => break,
            Err(e) => return Err(e.to_string()),
        }
    }
    let spec = s.spec();
    Ok((spec.rate, spec.channels, frames, peak))
}

#[test]
fn decodes_the_formats_this_build_claims_to_support() {
    if !have_ffmpeg() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // (file name, source rate, ffmpeg args, tolerance in frames)
    let cases: &[(&str, u32, &[&str], usize)] = &[
        ("t.wav", 44100, &["-c:a", "pcm_s16le"], 0),
        ("t.flac", 44100, &["-c:a", "flac"], 0),
        ("hi.flac", 96000, &["-c:a", "flac"], 0),
        ("t.aiff", 44100, &["-c:a", "pcm_s16be"], 0),
        ("t.ogg", 44100, &["-c:a", "libvorbis", "-b:a", "192k"], 0),
        ("t.mp3", 44100, &["-c:a", "libmp3lame", "-b:a", "192k"], 0),
        // AAC has no gapless support upstream, so encoder delay and padding
        // survive into the decoded output.
        ("t.m4a", 44100, &["-c:a", "aac", "-b:a", "192k"], 2048),
        ("alac.m4a", 44100, &["-c:a", "alac"], 0),
        // Opus always decodes at 48kHz regardless of the encoder input rate.
        ("t.opus", 48000, &["-c:a", "libopus", "-b:a", "128k"], 0),
        (
            "opus.webm",
            48000,
            &["-c:a", "libopus", "-b:a", "128k"],
            648,
        ),
        ("vorbis.webm", 48000, &["-c:a", "libvorbis"], 1024),
    ];

    for (name, rate, args, tolerance) in cases {
        // Opus sits behind a feature flag; its cases only apply when built in.
        if !cfg!(feature = "opus") && name.contains("opus") {
            continue;
        }
        let path = dir.path().join(name);
        if !encode(&path, *rate, args) {
            eprintln!("skipping {name}: this ffmpeg cannot encode it");
            continue;
        }
        let (got_rate, ch, frames, peak) =
            decode_all(&path).unwrap_or_else(|e| panic!("{name} failed to decode: {e}"));

        assert_eq!(got_rate, *rate, "{name}: wrong sample rate");
        assert_eq!(ch, 2, "{name}: wrong channel count");
        let expected = (*rate as usize) * 2;
        let diff = frames.abs_diff(expected);
        assert!(
            diff <= *tolerance,
            "{name}: got {frames} frames, expected {expected} (+/-{tolerance})"
        );
        // ffmpeg's sine generator peaks near 0.088, not full scale.
        assert!(
            peak > 0.05 && peak < 0.15,
            "{name}: peak {peak} suggests a scaling error"
        );
    }
}

#[cfg(feature = "opus")]
#[test]
fn opus_pre_skip_is_removed() {
    if !have_ffmpeg() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.opus");
    if !encode(&path, 48000, &["-c:a", "libopus", "-b:a", "128k"]) {
        eprintln!("skipping: this ffmpeg cannot encode Opus");
        return;
    }
    // No container reports the OpusHead pre-skip as a packet trim, so the
    // decoder has to drop it. Left in, the track runs 312 frames long and every
    // Opus file opens with a few milliseconds of encoder warm-up.
    let (rate, ch, frames, _) = decode_all(&path).expect("Opus failed to decode");
    assert_eq!(rate, 48000);
    assert_eq!(ch, 2);
    assert_eq!(
        frames, 96_000,
        "expected exactly 2s; pre-skip or padding mishandled"
    );
}

#[cfg(feature = "opus")]
#[test]
fn mono_opus_decodes() {
    if !have_ffmpeg() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mono.opus");
    let ok = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=2",
            "-ac",
            "1",
            "-c:a",
            "libopus",
            "-b:a",
            "96k",
        ])
        .arg(&path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("skipping: this ffmpeg cannot encode mono Opus");
        return;
    }
    let (rate, ch, frames, peak) = decode_all(&path).expect("mono Opus failed to decode");
    assert_eq!(rate, 48000);
    assert_eq!(ch, 1);
    assert_eq!(frames, 96_000);
    assert!(
        peak > 0.05 && peak < 0.15,
        "peak {peak} suggests a scaling error"
    );
}

#[cfg(feature = "opus")]
#[test]
fn seeking_an_opus_stream_does_not_reapply_pre_skip() {
    if !have_ffmpeg() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.opus");
    if !encode(&path, 48000, &["-c:a", "libopus", "-b:a", "128k"]) {
        eprintln!("skipping: this ffmpeg cannot encode Opus");
        return;
    }
    let mut s = AudioStream::open(&path).unwrap();
    s.seek(std::time::Duration::from_secs(1)).unwrap();
    let mut frames = 0usize;
    while let Ok(Some(c)) = s.next_chunk() {
        frames += c.len() / 2;
    }
    assert!(
        (frames as i64 - 48_000).abs() < 4_800,
        "after seeking to 1s, {frames} frames remained (expected ~48000)"
    );
}

#[test]
fn a_codec_with_no_decoder_reports_rather_than_panics() {
    // WMA has no decoder in this build. The failure must stay a clean error so
    // the player skips the track rather than stopping.
    if !have_ffmpeg() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.wma");
    if !encode(&path, 44100, &["-c:a", "wmav2", "-b:a", "128k"]) {
        eprintln!("skipping: this ffmpeg cannot encode WMA");
        return;
    }
    let err = decode_all(&path).expect_err("WMA unexpectedly decoded; update the docs");
    assert!(
        err.contains("no decoder") || err.contains("unsupported"),
        "unhelpful error: {err}"
    );
}

#[test]
fn a_file_that_is_not_audio_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fake.flac");
    std::fs::write(&path, b"definitely not a flac stream").unwrap();
    assert!(AudioStream::open(&path).is_err());
}

#[test]
fn a_missing_file_is_rejected() {
    assert!(AudioStream::open(Path::new("/nonexistent/nope.flac")).is_err());
}

#[test]
fn seeking_moves_the_read_position() {
    if !have_ffmpeg() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.flac");
    assert!(encode(&path, 44100, &["-c:a", "flac"]));

    let mut s = AudioStream::open(&path).unwrap();
    s.seek(std::time::Duration::from_secs(1)).unwrap();
    let mut frames = 0usize;
    while let Ok(Some(c)) = s.next_chunk() {
        frames += c.len() / 2;
    }
    // About one second should remain of a two second file.
    assert!(
        (frames as i64 - 44100).abs() < 4410,
        "after seeking to 1s, {frames} frames remained (expected ~44100)"
    );
}

#[test]
fn duration_is_reported_from_the_container() {
    if !have_ffmpeg() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.flac");
    assert!(encode(&path, 44100, &["-c:a", "flac"]));
    let s = AudioStream::open(&path).unwrap();
    let d = s.duration().expect("no duration reported");
    assert!(
        (d.as_secs_f64() - 2.0).abs() < 0.05,
        "duration {d:?} is not ~2s"
    );
}

// --- channel mapping ---

#[test]
fn matching_channel_counts_pass_through_untouched() {
    let input = vec![0.1, 0.2, 0.3, 0.4];
    let mut out = Vec::new();
    remap_channels(&input, 2, 2, &mut out);
    assert_eq!(out, input);
}

#[test]
fn mono_fans_out_to_every_output_channel() {
    let input = vec![0.5, -0.5];
    let mut out = Vec::new();
    remap_channels(&input, 1, 2, &mut out);
    assert_eq!(out, vec![0.5, 0.5, -0.5, -0.5]);

    let mut out4 = Vec::new();
    remap_channels(&input, 1, 4, &mut out4);
    assert_eq!(out4, vec![0.5, 0.5, 0.5, 0.5, -0.5, -0.5, -0.5, -0.5]);
}

#[test]
fn extra_source_channels_are_dropped() {
    // One 6 channel frame down to stereo keeps the first two.
    let input = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let mut out = Vec::new();
    remap_channels(&input, 6, 2, &mut out);
    assert_eq!(out, vec![1.0, 2.0]);
}

#[test]
fn widening_pads_missing_channels_with_silence() {
    let input = vec![1.0, 2.0];
    let mut out = Vec::new();
    remap_channels(&input, 2, 4, &mut out);
    assert_eq!(out, vec![1.0, 2.0, 0.0, 0.0]);
}

#[test]
fn remapping_output_is_always_frame_aligned() {
    for (src, dst) in [(1usize, 2usize), (2, 2), (2, 1), (6, 2), (2, 6), (3, 5)] {
        let input = vec![0.25f32; src * 7];
        let mut out = Vec::new();
        remap_channels(&input, src, dst, &mut out);
        assert_eq!(
            out.len(),
            dst * 7,
            "{src}ch -> {dst}ch produced a ragged buffer"
        );
    }
}

#[test]
fn a_zero_channel_count_produces_nothing_instead_of_dividing_by_zero() {
    let mut out = Vec::new();
    remap_channels(&[1.0, 2.0], 0, 2, &mut out);
    assert!(out.is_empty());
    remap_channels(&[1.0, 2.0], 2, 0, &mut out);
    assert!(out.is_empty());
}

#[cfg(not(feature = "opus"))]
#[test]
fn opus_reports_cleanly_when_the_feature_is_off() {
    if !have_ffmpeg() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.opus");
    if !encode(&path, 48000, &["-c:a", "libopus", "-b:a", "128k"]) {
        eprintln!("skipping: this ffmpeg cannot encode Opus");
        return;
    }
    // Built without libopus, an Opus file must be skipped with a clear message
    // rather than crashing or playing silence.
    let err = decode_all(&path).expect_err("Opus decoded without the opus feature");
    assert!(err.contains("no decoder"), "unhelpful error: {err}");
}
