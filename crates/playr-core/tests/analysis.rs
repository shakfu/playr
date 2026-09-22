//! `playr analyze`'s measurements, on signals whose answer is known.

mod common;

use std::path::Path;

use common::have_ffmpeg;
use playr_core::analysis::cutoff::Cutoff;
use playr_core::analysis::loudness::{Histogram, Loudness};
use playr_core::analysis::tempo::{Tempo, MIN_CONFIDENCE};
use playr_core::analysis::{self, analyse, findings, Analysis, Finding, Md5};
use playr_core::db::Track;
use realfft::RealFftPlanner;

/// Deterministic white noise in -1..1.
fn noise(n: usize, seed: u64) -> Vec<f32> {
    let mut x = seed.max(1);
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 40) as f32 / (1u64 << 23) as f32 * 2.0 - 1.0
        })
        .collect()
}

/// Stereo 1 kHz sine at `dbfs` peak, interleaved.
fn sine(rate: u32, secs: f32, dbfs: f32) -> Vec<f32> {
    let a = 10f32.powf(dbfs / 20.0);
    (0..(rate as f32 * secs) as usize)
        .flat_map(|i| {
            let v = a * (std::f32::consts::TAU * 1000.0 * i as f32 / rate as f32).sin();
            [v, v]
        })
        .collect()
}

fn measure(rate: u32, samples: &[f32]) -> playr_core::analysis::loudness::Measured {
    let mut l = Loudness::new(rate, 2);
    l.feed(samples);
    l.finish()
}

#[test]
fn a_stereo_sine_measures_its_level_in_lufs() {
    // BS.1770: a 1 kHz sine in both channels reads its peak level in dBFS.
    let m = measure(48000, &sine(48000, 5.0, -20.0));
    let lufs = m.lufs.expect("a tone is not silence");
    assert!((lufs + 20.0).abs() < 0.2, "{lufs}");
    assert!((m.peak - 0.1).abs() < 1e-3, "{}", m.peak);
}

#[test]
fn quiet_passages_below_the_relative_gate_do_not_count() {
    let mut s = sine(48000, 5.0, -20.0);
    s.extend(sine(48000, 5.0, -60.0));
    let lufs = measure(48000, &s).lufs.unwrap();
    assert!((lufs + 20.0).abs() < 0.2, "{lufs}");
}

#[test]
fn silence_and_short_files_have_no_loudness() {
    assert_eq!(measure(48000, &vec![0.0; 96000 * 2]).lufs, None);
    assert_eq!(measure(48000, &sine(48000, 0.3, -20.0)).lufs, None);
}

#[test]
fn a_pooled_histogram_matches_measuring_the_album_whole() {
    let a = sine(44100, 6.0, -14.0);
    let b = sine(44100, 4.0, -26.0);
    let mut whole = a.clone();
    whole.extend(&b);
    let exact = measure(44100, &whole).lufs.unwrap();

    let mut pooled = measure(44100, &a).histogram;
    pooled.pool(&measure(44100, &b).histogram);
    let approx = pooled.integrated().unwrap();
    assert!((approx - exact).abs() < 0.1, "{approx} against {exact}");
}

#[test]
fn a_histogram_survives_its_bytes() {
    let h = measure(44100, &sine(44100, 2.0, -10.0)).histogram;
    assert_eq!(Histogram::from_bytes(&h.to_bytes()), Some(h));
    assert_eq!(Histogram::from_bytes(&[0; 7]), None);
}

/// Mono clicks at `bpm`: 5 ms noise bursts, `secs` long.
fn clicks(rate: u32, bpm: f32, secs: f32) -> Vec<f32> {
    let period = rate as f32 * 60.0 / bpm;
    let burst = noise((rate as f32 * 0.005) as usize, 7);
    let mut out = vec![0.0; (rate as f32 * secs) as usize];
    let mut at = 0.0;
    while (at as usize) < out.len() {
        for (i, v) in burst.iter().enumerate() {
            if let Some(o) = out.get_mut(at as usize + i) {
                *o = v * 0.5;
            }
        }
        at += period;
    }
    out
}

#[test]
fn a_click_track_measures_its_tempo() {
    for bpm in [45.0, 72.0, 90.0, 120.0, 128.0, 150.0, 165.0] {
        let mut t = Tempo::new(44100);
        t.feed(&clicks(44100, bpm, 30.0), 1);
        let e = t.finish().expect("30 s is long enough");
        assert!((e.bpm - bpm).abs() < 0.5, "{bpm} BPM measured {}", e.bpm);
        assert!(e.confidence >= MIN_CONFIDENCE, "{bpm}: {}", e.confidence);
    }
}

/// A strict pulse at period P correlates as strongly at 2P, so the prior
/// decides, and above 120 * sqrt(2) BPM it favours the half tempo.
#[test]
fn a_pulse_above_170_bpm_reads_at_half_tempo() {
    let mut t = Tempo::new(44100);
    t.feed(&clicks(44100, 180.0, 30.0), 1);
    let e = t.finish().unwrap();
    assert!((e.bpm - 90.0).abs() < 0.5, "{}", e.bpm);
}

/// The halved reading keeps the heard tempo as its alternate, so a search
/// for either finds the track.
#[test]
fn a_halved_reading_records_the_faster_level_too() {
    let mut t = Tempo::new(44100);
    t.feed(&clicks(44100, 180.0, 30.0), 1);
    let e = t.finish().unwrap();
    assert!((e.bpm - 90.0).abs() < 0.5, "{}", e.bpm);
    let alt = e.alt.expect("a strict pulse correlates at both levels");
    assert!((alt - 180.0).abs() < 1.0, "{alt}");
}

#[test]
fn a_tempo_with_nothing_at_double_records_no_alternate() {
    // Clicks at 100 BPM and nothing at 200: the level above is empty, so
    // recording it would match searches for a tempo the track never plays.
    let mut t = Tempo::new(44100);
    t.feed(&clicks(44100, 100.0, 30.0), 1);
    let e = t.finish().unwrap();
    assert!((e.bpm - 100.0).abs() < 0.5, "{}", e.bpm);
    assert_eq!(e.alt, None);
}

/// Above the prior's centre nothing was halved, so a subdivision pulse is
/// not an alternate tempo: 128 BPM with hats on the half beat stays 128.
#[test]
fn a_fast_tempo_records_no_alternate_for_its_subdivision() {
    let mut beat = clicks(44100, 128.0, 30.0);
    for (x, h) in beat.iter_mut().zip(clicks(44100, 256.0, 30.0)) {
        *x += h * 0.3;
    }
    let mut t = Tempo::new(44100);
    t.feed(&beat, 1);
    let e = t.finish().unwrap();
    assert!((e.bpm - 128.0).abs() < 0.5, "{}", e.bpm);
    assert_eq!(e.alt, None);
}

#[test]
fn a_click_track_at_a_high_rate_measures_the_same() {
    let mut t = Tempo::new(96000);
    t.feed(&clicks(96000, 120.0, 20.0), 1);
    let e = t.finish().unwrap();
    assert!((e.bpm - 120.0).abs() < 0.5, "{}", e.bpm);
}

#[test]
fn noise_has_no_confident_tempo() {
    let mut t = Tempo::new(44100);
    t.feed(&noise(44100 * 30, 3), 1);
    let e = t.finish().unwrap();
    assert!(e.confidence < MIN_CONFIDENCE, "{e:?}");
}

#[test]
fn a_short_track_has_no_tempo() {
    let mut t = Tempo::new(44100);
    t.feed(&clicks(44100, 120.0, 4.0), 1);
    assert_eq!(t.finish(), None);
}

/// White noise with everything above `hz` removed.
fn lowpassed(rate: u32, secs: f32, hz: f32) -> Vec<f32> {
    let n = (rate as f32 * secs) as usize;
    let mut planner = RealFftPlanner::<f32>::new();
    let (fwd, inv) = (planner.plan_fft_forward(n), planner.plan_fft_inverse(n));
    let mut signal = noise(n, 11);
    let mut spectrum = fwd.make_output_vec();
    fwd.process(&mut signal, &mut spectrum).unwrap();
    let cut = (hz / rate as f32 * n as f32) as usize;
    for bin in spectrum.iter_mut().skip(cut) {
        *bin = realfft::num_complex::Complex::new(0.0, 0.0);
    }
    spectrum[0].im = 0.0;
    inv.process(&mut spectrum, &mut signal).unwrap();
    signal.iter().map(|x| x / n as f32 * 0.5).collect()
}

fn cutoff_of(rate: u32, samples: &[f32]) -> playr_core::analysis::cutoff::Measured {
    let mut c = Cutoff::new(rate, Some(samples.len() as u64));
    c.feed(samples, 1);
    c.finish().expect("noise is not silence")
}

#[test]
fn a_lowpass_shows_as_a_steep_cutoff() {
    let c = cutoff_of(44100, &lowpassed(44100, 20.0, 16_000.0));
    assert!((c.hz as f32 - 16_000.0).abs() < 300.0, "{c:?}");
    assert!(c.fall_db >= analysis::STEEP_DB, "{c:?}");
}

#[test]
fn full_band_noise_has_no_steep_cutoff() {
    let c = cutoff_of(44100, &noise(44100 * 20, 5));
    assert!(c.fall_db < analysis::STEEP_DB, "{c:?}");
}

#[test]
fn a_high_rate_file_cut_at_22k_shows_it() {
    let c = cutoff_of(96000, &lowpassed(96000, 10.0, 22_050.0));
    assert!((c.hz as f32 - 22_050.0).abs() < 300.0, "{c:?}");
    assert!(c.fall_db >= analysis::STEEP_DB, "{c:?}");
}

#[test]
fn silence_has_no_cutoff() {
    let mut c = Cutoff::new(44100, Some(44100 * 10));
    c.feed(&vec![0.0; 44100 * 10], 1);
    assert_eq!(c.finish(), None);
}

/// Writes `samples`, mono, as a WAV of `bits` integer bits at 44.1 kHz.
fn wav(path: &Path, bits: u16, samples: &[f32]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: bits,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    let full = (1i32 << (bits - 1)) as f32;
    for s in samples {
        w.write_sample((s * (full - 1.0)).round() as i32).unwrap();
    }
    w.finalize().unwrap();
}

#[test]
fn a_file_decodes_to_its_measurements() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.wav");
    let s: Vec<f32> = sine(44100, 3.0, -20.0).into_iter().step_by(2).collect();
    wav(&path, 16, &s);

    let a = analyse(&path);
    assert_eq!(a.error, None);
    assert_eq!(a.frames, 3 * 44100);
    assert_eq!(a.header_frames, Some(3 * 44100));
    assert!(a.lossless);
    assert_eq!(a.bits, Some(16));
    assert_eq!(a.bits_used, Some(16));
    assert_eq!(a.md5, None, "not FLAC");
    // Mono: one channel of the stereo sine reads 3 dB below it.
    let lufs = a.loudness.unwrap();
    assert!((lufs + 23.0).abs() < 0.3, "{lufs}");
    assert!(findings(&a).is_empty(), "{:?}", findings(&a));
}

#[test]
fn a_24_bit_file_holding_16_bits_is_padded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("padded.wav");
    // 16-bit values, shifted up into 24 bits: the low 8 are always zero.
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    for i in 0..44100 {
        let v = ((i as f32 * 0.05).sin() * 20000.0) as i32;
        w.write_sample(v << 8).unwrap();
    }
    w.finalize().unwrap();

    let a = analyse(&path);
    assert_eq!((a.bits, a.bits_used), (Some(24), Some(16)));
    assert_eq!(findings(&a), vec![Finding::Padded { used: 16, bits: 24 }]);
}

#[test]
fn a_file_that_is_not_audio_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.flac");
    std::fs::write(&path, b"not audio at all").unwrap();
    let a = analyse(&path);
    assert!(a.error.is_some());
    assert!(matches!(findings(&a)[..], [Finding::Unreadable(_)]));
}

/// A FLAC of a second of noise, from ffmpeg.
fn flac(dir: &Path) -> Option<std::path::PathBuf> {
    if !have_ffmpeg() {
        return None;
    }
    let src = dir.join("src.wav");
    wav(
        &src,
        16,
        &noise(44100 * 2, 9)
            .iter()
            .map(|x| x * 0.3)
            .collect::<Vec<_>>(),
    );
    let out = dir.join("t.flac");
    let ok = std::process::Command::new("ffmpeg")
        .args(["-loglevel", "error", "-y", "-i"])
        .arg(&src)
        .arg(&out)
        .status()
        .unwrap()
        .success();
    assert!(ok, "ffmpeg failed");
    Some(out)
}

#[test]
fn a_flac_file_is_checked_against_its_md5() {
    let dir = tempfile::tempdir().unwrap();
    let Some(path) = flac(dir.path()) else { return };
    let a = analyse(&path);
    assert_eq!(a.md5, Some(Md5::Ok));
    assert!(a.md5_hex.is_some());
    assert!(findings(&a).is_empty(), "{:?}", findings(&a));

    // STREAMINFO follows "fLaC" and a 4-byte block header; its MD5 is the
    // last 16 of its 34 bytes.
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[26..42].fill(0);
    let zeroed = dir.path().join("zeroed.flac");
    std::fs::write(&zeroed, &bytes).unwrap();
    let a = analyse(&zeroed);
    assert_eq!(a.md5, Some(Md5::Absent));
    assert_eq!(findings(&a), vec![Finding::NoChecksum]);
}

#[test]
fn a_damaged_flac_file_is_found() {
    let dir = tempfile::tempdir().unwrap();
    let Some(path) = flac(dir.path()) else { return };
    let mut bytes = std::fs::read(&path).unwrap();
    let middle = bytes.len() / 2;
    for b in &mut bytes[middle..middle + 64] {
        *b ^= 0x5a;
    }
    let broken = dir.path().join("broken.flac");
    std::fs::write(&broken, &bytes).unwrap();
    let a = analyse(&broken);
    assert!(
        findings(&a)
            .iter()
            .any(|f| matches!(f, Finding::Damaged { .. } | Finding::Unreadable(_))),
        "{a:?}"
    );
    assert_eq!(a.loudness.is_some(), a.error.is_none());
}

#[test]
fn a_cutoff_is_a_finding_only_where_it_means_something() {
    let steep = |hz| Some(playr_core::analysis::cutoff::Measured { hz, fall_db: 40.0 });
    let lossless = Analysis {
        rate: 44100,
        lossless: true,
        cutoff: steep(16_000),
        ..Default::default()
    };
    assert_eq!(
        findings(&lossless),
        vec![Finding::PossibleLossySource { hz: 16_000 }]
    );
    // An MP3 is expected to stop there.
    let mp3 = Analysis {
        lossless: false,
        ..lossless.clone()
    };
    assert!(findings(&mp3).is_empty());
    let hires = Analysis {
        rate: 96000,
        cutoff: steep(22_000),
        ..lossless.clone()
    };
    assert_eq!(
        findings(&hires),
        vec![Finding::PossibleUpsampling { hz: 22_000 }]
    );
    let gentle = Analysis {
        cutoff: Some(playr_core::analysis::cutoff::Measured {
            hz: 16_000,
            fall_db: 6.0,
        }),
        ..lossless
    };
    assert!(findings(&gentle).is_empty());
}

#[test]
fn a_bpm_tag_is_preferred_and_an_unsure_estimate_ignored() {
    let est = |confidence| {
        Some(playr_core::analysis::tempo::Estimate {
            bpm: 121.0,
            confidence,
            alt: None,
        })
    };
    let a = Analysis {
        tempo: est(0.9),
        ..Default::default()
    };
    assert_eq!(a.bpm(), Some(121.0));
    assert_eq!(
        Analysis {
            bpm_tag: Some(120.0),
            ..a.clone()
        }
        .bpm(),
        Some(120.0)
    );
    assert_eq!(
        Analysis {
            tempo: est(0.05),
            ..a
        }
        .bpm(),
        None
    );
}

fn track(path: &str, title: &str, ms: i64) -> Track {
    Track {
        path: path.into(),
        title: Some(title.into()),
        artist: Some("Artist".into()),
        duration_ms: Some(ms),
        ..Default::default()
    }
}

#[test]
fn duplicates_share_a_title_artist_and_length_or_a_checksum() {
    let tracks = vec![
        track("/a/one.flac", "One", 200_000),
        track("/b/one.mp3", "one", 201_500),
        track("/c/one-live.flac", "One", 260_000),
        track("/a/two.flac", "Two", 100_000),
        track("/d/copy.flac", "Renamed", 100_000),
    ];
    let md5 = |hex: &str| Analysis {
        md5_hex: Some(hex.into()),
        ..Default::default()
    };
    let rows = [
        ("/a/two.flac".to_string(), md5("ab")),
        ("/d/copy.flac".to_string(), md5("ab")),
    ]
    .into_iter()
    .collect();
    let groups = analysis::duplicates(&tracks, &rows);
    assert!(groups.contains(&vec!["/a/two.flac".into(), "/d/copy.flac".into()]));
    assert!(groups.contains(&vec!["/a/one.flac".into(), "/b/one.mp3".into()]));
    assert_eq!(groups.len(), 2, "{groups:?}");
}

#[test]
fn an_album_is_its_album_and_artist_or_its_directory() {
    let t = |album: Option<&str>, artist: Option<&str>, path: &str| Track {
        path: path.into(),
        album: album.map(Into::into),
        album_artist: artist.map(Into::into),
        ..Default::default()
    };
    let key = analysis::album_key;
    assert_eq!(
        key(&t(Some("A"), Some("X"), "/1/a.flac")),
        key(&t(Some("A"), Some("X"), "/2/b.flac"))
    );
    assert_ne!(
        key(&t(Some("A"), None, "/1/a.flac")),
        key(&t(Some("A"), None, "/2/b.flac"))
    );
    assert_eq!(
        key(&t(Some("A"), Some(""), "/1/a.flac")),
        key(&t(Some("A"), None, "/1/b.flac"))
    );
    assert_eq!(key(&t(None, Some("X"), "/1/a.flac")), None);
}
