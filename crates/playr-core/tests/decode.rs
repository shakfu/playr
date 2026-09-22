//! Decoder and channel-mapping tests.
//!
//! The format cases generate real files with ffmpeg, so they check what this
//! build can actually decode rather than what the dependency claims.

mod common;

use common::{have_ffmpeg, skip};
use playr_core::audio::decode::AudioStream;
use playr_core::audio::output::remap_channels;
use std::path::Path;
use std::process::Command;

/// Encodes a 2 second 440Hz stereo tone with the given codec arguments.
///
/// On failure, returns ffmpeg's own message, such as a missing encoder.
fn encode(path: &Path, rate: u32, args: &[&str]) -> Result<(), String> {
    let out = Command::new("ffmpeg")
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
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    Err(stderr.lines().next().unwrap_or("ffmpeg failed").to_string())
}

/// Whether this ffmpeg has the encoder named `name`.
fn has_encoder(name: &str) -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .is_ok_and(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .any(|word| word == name)
        })
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
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // Vorbis from libvorbis, which real Ogg files come from, when this ffmpeg
    // has it: its end trim makes an Ogg file decode to exactly its length.
    // ffmpeg's own experimental encoder stamps the end inconsistently across
    // versions, 56 frames long with 9.0 and 968 short with 4.4, and playr
    // follows the stamp as ffmpeg 9.0's decoder does; 4.4's decoder ignores it.
    let (vorbis, ogg_tolerance): (&[&str], usize) = if has_encoder("libvorbis") {
        (&["-c:a", "libvorbis"], 0)
    } else {
        (&["-c:a", "vorbis", "-strict", "experimental"], 1024)
    };
    // (file name, source rate, ffmpeg args, tolerance in frames)
    let cases: &[(&str, u32, &[&str], usize)] = &[
        ("t.wav", 44100, &["-c:a", "pcm_s16le"], 0),
        ("t.flac", 44100, &["-c:a", "flac"], 0),
        ("hi.flac", 96000, &["-c:a", "flac"], 0),
        ("t.aiff", 44100, &["-c:a", "pcm_s16be"], 0),
        ("t.ogg", 44100, vorbis, ogg_tolerance),
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
        // Matroska carries no Vorbis end trim, so the encoder's padding, or
        // its short stamp, stays: 832 frames over with libvorbis in ffmpeg
        // 4.4, 256 over and 768 under with the experimental encoder in 9.0
        // and 4.4.
        ("vorbis.webm", 48000, vorbis, 1024),
    ];

    for (name, rate, args, tolerance) in cases {
        // Opus sits behind a feature flag; its cases only apply when built in.
        if !cfg!(feature = "opus") && name.contains("opus") {
            continue;
        }
        let path = dir.path().join(name);
        if let Err(why) = encode(&path, *rate, args) {
            skip(
                "PLAYR_REQUIRE_FFMPEG",
                &format!("cannot encode {name}: {why}"),
            );
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
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.opus");
    if let Err(why) = encode(&path, 48000, &["-c:a", "libopus", "-b:a", "128k"]) {
        skip(
            "PLAYR_REQUIRE_FFMPEG",
            &format!("cannot encode Opus: {why}"),
        );
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
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        skip(
            "PLAYR_REQUIRE_FFMPEG",
            "this ffmpeg cannot encode mono Opus",
        );
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
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.opus");
    if let Err(why) = encode(&path, 48000, &["-c:a", "libopus", "-b:a", "128k"]) {
        skip(
            "PLAYR_REQUIRE_FFMPEG",
            &format!("cannot encode Opus: {why}"),
        );
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
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.wma");
    if let Err(why) = encode(&path, 44100, &["-c:a", "wmav2", "-b:a", "128k"]) {
        skip("PLAYR_REQUIRE_FFMPEG", &format!("cannot encode WMA: {why}"));
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
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.flac");
    encode(&path, 44100, &["-c:a", "flac"]).unwrap();

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

/// Decodes every sample of `s` from its current position.
fn samples(s: &mut AudioStream) -> Vec<f32> {
    let mut out = Vec::new();
    while let Ok(Some(c)) = s.next_chunk() {
        out.extend_from_slice(c);
    }
    out
}

#[test]
fn a_seek_resumes_on_the_exact_sample() {
    if !have_ffmpeg() {
        return;
    }
    // Noise, so a misaligned comparison cannot match by accident.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("n.flac");
    let status = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
        .arg("anoisesrc=d=3:c=white:r=44100:seed=7")
        .args(["-ac", "2", "-c:a", "flac"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());

    let whole = samples(&mut AudioStream::open(&path).unwrap());
    for secs in [0.5, 1.0, 1.7] {
        let mut s = AudioStream::open(&path).unwrap();
        s.seek(std::time::Duration::from_secs_f64(secs)).unwrap();
        let after = samples(&mut s);
        let at = (secs * 44100.0).round() as usize * 2;
        assert_eq!(
            after.len(),
            whole.len() - at,
            "seek to {secs}s resumed {} frames early",
            (after.len() as i64 - (whole.len() - at) as i64) / 2
        );
        assert!(
            after[..4096] == whole[at..at + 4096],
            "samples differ after seek to {secs}s"
        );
    }
}

#[test]
fn duration_is_reported_from_the_container() {
    if !have_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.flac");
    encode(&path, 44100, &["-c:a", "flac"]).unwrap();
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
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.opus");
    if let Err(why) = encode(&path, 48000, &["-c:a", "libopus", "-b:a", "128k"]) {
        skip(
            "PLAYR_REQUIRE_FFMPEG",
            &format!("cannot encode Opus: {why}"),
        );
        return;
    }
    // Built without libopus, an Opus file must be skipped with a clear message
    // rather than crashing or playing silence.
    let err = decode_all(&path).expect_err("Opus decoded without the opus feature");
    assert!(err.contains("no decoder"), "unhelpful error: {err}");
}

/// Largest difference, relative to the signal, between `after` and the
/// uninterrupted decode `whole` from frame `at`, over `frames` stereo frames.
fn error_db(after: &[f32], whole: &[f32], at: usize, frames: usize) -> f64 {
    let (mut err, mut sig) = (0f64, 0f64);
    for i in 0..frames * 2 {
        let x = whole[at * 2 + i] as f64;
        err += (after[i] as f64 - x).powi(2);
        sig += x * x;
    }
    10.0 * (err / sig).log10()
}

/// Seeks into white noise encoded with `codec`, and checks that the first
/// `frames` after each seek match decoding from the start.
fn assert_seeks_match_a_full_decode(ext: &str, rate: u32, codec: &[&str], frames: usize) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("n.{ext}"));
    let ok = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
        .arg(format!("anoisesrc=d=3:c=white:r={rate}:seed=7"))
        .args(["-ac", "2"])
        .args(codec)
        .arg(&path)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        return skip(
            "PLAYR_REQUIRE_FFMPEG",
            &format!("this ffmpeg cannot encode {ext}"),
        );
    }
    let whole = samples(&mut AudioStream::open(&path).unwrap());
    for secs in [0.5, 1.7] {
        let mut s = AudioStream::open(&path).unwrap();
        s.seek(std::time::Duration::from_secs_f64(secs)).unwrap();
        let after = samples(&mut s);
        let at = (secs * rate as f64).round() as usize;
        let db = error_db(&after, &whole, at, frames);
        assert!(db < -60.0, "{ext}: seek to {secs}s is off by {db:.1} dB");
    }
}

#[cfg(feature = "opus")]
#[test]
fn an_opus_seek_matches_a_full_decode() {
    // OGG timestamps include the pre-skip, which put every seek 6.5 ms early,
    // and a decoder reset at the target took about 200 ms to settle.
    if !have_ffmpeg() {
        return;
    }
    assert_seeks_match_a_full_decode("opus", 48000, &["-c:a", "libopus", "-b:a", "256k"], 4800);
}

#[test]
fn an_aac_seek_matches_a_full_decode() {
    // A reset at the target left the first AAC frame, which overlaps the one
    // before it, 20 dB off.
    if !have_ffmpeg() {
        return;
    }
    assert_seeks_match_a_full_decode("m4a", 44100, &["-c:a", "aac", "-b:a", "256k"], 4410);
}

#[cfg(feature = "opus")]
#[test]
fn the_opus_header_output_gain_is_applied() {
    if !have_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain.opus");
    if let Err(why) = encode(&plain, 48000, &["-c:a", "libopus", "-b:a", "128k"]) {
        skip(
            "PLAYR_REQUIRE_FFMPEG",
            &format!("cannot encode Opus: {why}"),
        );
        return;
    }
    // RFC 7845 section 5.1: a signed Q7.8 dB value at offset 16 of OpusHead,
    // which ffmpeg writes as 0. -6.02 dB is half the amplitude.
    let mut bytes = std::fs::read(&plain).unwrap();
    let head = bytes
        .windows(8)
        .position(|w| w == b"OpusHead")
        .expect("no OpusHead in the file");
    let q78 = (-6.0206f32 * 256.0).round() as i16;
    bytes[head + 16..head + 18].copy_from_slice(&q78.to_le_bytes());
    // The page carrying the header now fails its checksum, so redo it.
    let page = bytes[..head]
        .windows(4)
        .rposition(|w| w == b"OggS")
        .expect("no Ogg page before the header");
    reseat_ogg_crc(&mut bytes, page);
    let quieter = dir.path().join("quieter.opus");
    std::fs::write(&quieter, &bytes).unwrap();

    let (_, _, frames, loud) = decode_all(&plain).expect("Opus failed to decode");
    let (_, _, frames_q, quiet) = decode_all(&quieter).expect("patched Opus failed to decode");
    assert_eq!(frames, frames_q, "the patch changed the length");
    assert!(
        (quiet / loud - 0.5).abs() < 0.02,
        "gain not applied: {loud} then {quiet}"
    );
}

/// Recomputes the checksum of the Ogg page starting at `at`, after its
/// payload was edited.
///
/// Ogg uses CRC-32 with polynomial 0x04c11db7, no reflection and no final
/// xor, over the whole page with the checksum field zeroed (RFC 3533
/// section 6).
#[cfg(feature = "opus")]
fn reseat_ogg_crc(bytes: &mut [u8], at: usize) {
    let segments = bytes[at + 26] as usize;
    let table = &bytes[at + 27..at + 27 + segments];
    let payload: usize = table.iter().map(|n| *n as usize).sum();
    let end = at + 27 + segments + payload;
    bytes[at + 22..at + 26].fill(0);
    let mut crc: u32 = 0;
    for byte in &bytes[at..end] {
        crc ^= (*byte as u32) << 24;
        for _ in 0..8 {
            crc = match crc & 0x8000_0000 {
                0 => crc << 1,
                _ => (crc << 1) ^ 0x04c1_1db7,
            };
        }
    }
    bytes[at + 22..at + 26].copy_from_slice(&crc.to_le_bytes());
}
