//! Cutting tracks into slices and writing them for a sampler.

use std::path::{Path, PathBuf};

use playr_core::samples::{
    equal_spans, export, nearest_onset, onsets, plan, plan_with, region, spans_at, Cut, Edges,
    Exported, Fades, Job, OnsetAudio, MAX_SLICES,
};
use playr_core::wave::{snap_reach, Peaks};

/// A distinct, non-zero value for every frame and channel, so any frame read
/// from the wrong place shows.
fn value(frame: u64, channel: u16, bits: u16) -> i32 {
    let full = 1i64 << (bits - 1);
    let v = (frame as i64 * 7919 + channel as i64 * 104_729) % (2 * full - 2) - (full - 1);
    if v == 0 {
        1
    } else {
        v as i32
    }
}

/// Writes `frames` of that pattern as `bits`-bit integer WAV.
fn source(path: &Path, rate: u32, channels: u16, bits: u16, frames: u64) {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: bits,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for f in 0..frames {
        for c in 0..channels {
            w.write_sample(value(f, c, bits)).unwrap();
        }
    }
    w.finalize().unwrap();
}

fn job(path: &Path, rate: u32, marks: &[u64], at: u64, cut: Cut, samples: &Path) -> Job {
    Job {
        path: path.to_path_buf(),
        rate,
        marks: marks.to_vec(),
        at,
        cut,
        range: None,
        samples: samples.to_path_buf(),
        edges: Edges::Exact,
        fades: Fades::default(),
        loops: false,
    }
}

/// The WAV files in `dir`, sorted by name.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "wav"))
        .collect();
    files.sort();
    files
}

/// Checks each file in `out` holds exactly the source frames of its slice,
/// scaled from `bits` to 24 bits, and returns the slices' lengths.
fn assert_exact(out: &Exported, channels: u16, bits: u16) -> Vec<u64> {
    let files = files(&out.dir);
    assert_eq!(files.len(), out.slices.len());
    let mut lengths = Vec::new();
    for (file, &(start, end)) in files.iter().zip(&out.slices) {
        let mut reader = hound::WavReader::open(file).unwrap();
        let spec = reader.spec();
        assert_eq!((spec.bits_per_sample, spec.channels), (24, channels));
        let got: Vec<i32> = reader.samples::<i32>().map(Result::unwrap).collect();
        let want: Vec<i32> = (start..end)
            .flat_map(|f| (0..channels).map(move |c| value(f, c, bits) << (24 - bits)))
            .collect();
        assert_eq!(
            got.len(),
            want.len(),
            "{} has the wrong length",
            file.display()
        );
        if let Some(i) = got.iter().zip(&want).position(|(g, w)| g != w) {
            panic!(
                "{}: frame {} differs: {} != {}",
                file.display(),
                start + i as u64 / channels as u64,
                got[i],
                want[i]
            );
        }
        lengths.push(end - start);
    }
    lengths
}

#[test]
fn the_region_is_between_the_marks_either_side_of_the_playhead() {
    let marks = [100, 500, 900];
    assert_eq!(region(&marks, 0), (0, Some(100)));
    assert_eq!(region(&marks, 100), (100, Some(500)));
    assert_eq!(region(&marks, 700), (500, Some(900)));
    assert_eq!(region(&marks, 950), (900, None));
    assert_eq!(region(&[], 50), (0, None));
}

#[test]
fn spans_divide_equally_or_at_points() {
    assert_eq!(equal_spans(10, 3), [(0, 3), (3, 6), (6, 10)]);
    assert!(equal_spans(2, 3).is_empty());
    assert!(equal_spans(10, 0).is_empty());
    assert_eq!(spans_at(&[0, 4, 4, 9], 12), [(0, 4), (4, 9), (9, 12)]);
    assert!(spans_at(&[], 12).is_empty());
}

#[test]
fn a_region_is_written_bit_exact_from_24_and_16_bit_sources() {
    for (bits, rate, channels) in [(24, 96_000, 2), (16, 44_100, 1), (24, 48_000, 6)] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("src.wav");
        source(&file, rate, channels, bits, rate as u64 * 3);
        // Odd frame numbers, well into the file, so a seek that lands a frame
        // early or late shows.
        let (a, b) = (rate as u64 + 12_345, 2 * rate as u64 + 6_789);
        let out = export(&job(&file, rate, &[a, b], a + 10, Cut::Region, dir.path())).unwrap();
        assert_eq!(out.slices, [(a, b)]);
        assert_exact(&out, channels, bits);
    }
}

#[test]
fn marks_cut_the_whole_track_and_the_last_slice_runs_to_the_end() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("Loop 01.wav");
    source(&file, 44_100, 2, 16, 50_000);
    let out = export(&job(
        &file,
        44_100,
        &[30_000, 10_000, 10_000],
        0,
        Cut::Marks,
        dir.path(),
    ))
    .unwrap();
    assert_eq!(
        out.slices,
        [(0, 10_000), (10_000, 30_000), (30_000, 50_000)]
    );
    assert_exact(&out, 2, 16);
    let names: Vec<String> = files(&out.dir)
        .iter()
        .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        [
            "000-Loop 01_S00.wav",
            "001-Loop 01_S01.wav",
            "002-Loop 01_S02.wav"
        ]
    );
    assert_eq!(out.dir, dir.path().join("Loop 01"));

    let none = export(&job(&file, 44_100, &[], 0, Cut::Marks, dir.path()));
    assert_eq!(none, Err("no marks in this track".into()));
}

#[test]
fn equal_slices_cover_the_region_or_the_rest_of_the_track() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("src.wav");
    source(&file, 48_000, 2, 24, 100_003);

    let out = export(&job(
        &file,
        48_000,
        &[1_000, 61_000],
        5_000,
        Cut::Equal(4),
        dir.path(),
    ))
    .unwrap();
    assert_eq!(assert_exact(&out, 2, 24), [15_000; 4]);
    assert_eq!(out.slices.first().unwrap().0, 1_000);

    // No mark after the playhead: to the end, the remainder in the last slice.
    let out = export(&job(
        &file,
        48_000,
        &[1_000, 61_000],
        70_000,
        Cut::Equal(3),
        dir.path(),
    ))
    .unwrap();
    assert_eq!(assert_exact(&out, 2, 24), [13_001, 13_001, 13_001]);
    assert_eq!(out.slices.last().unwrap().1, 100_003);

    assert_eq!(
        export(&job(
            &file,
            48_000,
            &[10, 12],
            11,
            Cut::Equal(3),
            dir.path()
        )),
        Err("the region is too short for 3 slices".into())
    );
}

#[test]
fn a_range_replaces_the_region_for_every_cut() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("src.wav");
    source(&file, 48_000, 2, 24, 100_003);
    let marks = [1_000, 30_000, 45_000, 61_000];
    let ranged = |cut| Job {
        range: Some((20_000, 60_000)),
        ..job(&file, 48_000, &marks, 5_000, cut, dir.path())
    };

    let out = export(&ranged(Cut::Region)).unwrap();
    assert_eq!(out.slices, [(20_000, 60_000)]);
    assert_exact(&out, 2, 24);
    let out = export(&ranged(Cut::Equal(4))).unwrap();
    assert_eq!(assert_exact(&out, 2, 24), [10_000; 4]);
    assert_eq!(out.slices[0].0, 20_000);
    // Only the marks inside the range cut it, from its start to its end.
    let out = export(&ranged(Cut::Marks)).unwrap();
    assert_eq!(
        out.slices,
        [(20_000, 30_000), (30_000, 45_000), (45_000, 60_000)]
    );
    assert_exact(&out, 2, 24);

    let empty = Job {
        range: Some((2_000, 29_000)),
        ..job(&file, 48_000, &marks, 5_000, Cut::Marks, dir.path())
    };
    assert_eq!(export(&empty), Err("no marks in the range".into()));
}

/// A mono buffer of `len` frames with a decaying burst at each onset.
fn bursts(len: usize, hits: &[(usize, f32)]) -> Vec<f32> {
    let mut data = vec![0.0f32; len];
    for &(at, amp) in hits {
        for k in 0..4000.min(len - at) {
            data[at + k] = amp * (1.0 - k as f32 / 4000.0) * (k as f32 * 0.3).sin();
        }
    }
    data
}

#[test]
fn onsets_find_each_hit_including_quiet_ones_and_start_before_it() {
    // Ported from rtrack: the second hit is 30 dB quieter than the first.
    let data = bursts(44_100, &[(5_000, 1.0), (22_050, 0.03)]);
    let points = onsets(&data, 44_100, 0.5);
    assert_eq!(points[0], 0);
    assert_eq!(points.len(), 3, "{points:?}");
    for (point, hit) in points[1..].iter().zip([5_000, 22_050]) {
        // At most two 5 ms windows early: the window the rise starts in, and
        // the move back to the quiet before it.
        assert!(
            (hit - 440..=hit).contains(point),
            "onset {point} for a hit at {hit}"
        );
    }
    // Within 50 ms of the start, a hit belongs to the first slice.
    assert_eq!(onsets(&bursts(44_100, &[(2_000, 1.0)]), 44_100, 0.5), [0]);
    assert_eq!(
        onsets(&vec![0.0; 44_100], 44_100, 0.5),
        [0],
        "silence has no onsets"
    );
    assert_eq!(onsets(&[0.5; 10], 44_100, 0.5), [0], "too short to measure");

    let many = bursts(88_200, &[(5_000, 0.5), (30_000, 0.4), (60_000, 0.2)]);
    assert!(onsets(&many, 44_100, 0.9).len() >= onsets(&many, 44_100, 0.1).len());
}

#[test]
fn an_onset_moves_back_to_the_quietest_frame_before_the_attack() {
    // Low noise that is never exactly zero, with one zero sample inside the
    // window before a hit. At 44.1 kHz the window is 220 frames and the hop
    // 110, so a hit on a hop boundary is detected 110 frames early and the
    // search for a quiet frame covers the 220 frames before that.
    let mut seed = 12_345u32;
    let mut data: Vec<f32> = (0..44_100)
        .map(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let amp = 0.004 + 0.002 * (seed >> 8) as f32 / (1u32 << 24) as f32;
            if i % 2 == 0 {
                amp
            } else {
                -amp
            }
        })
        .collect();
    let hit = 11_000;
    for k in 0..4000 {
        data[hit + k] = 0.9 * (1.0 - k as f32 / 4000.0) * if k % 2 == 0 { 1.0 } else { -1.0 };
    }
    data[hit - 200] = 0.0;
    assert_eq!(onsets(&data, 44_100, 0.5), [0, hit - 200]);
}

/// Steady noise at -40 dB with a hit 10 dB louder from frame 20,000: a rise
/// that high sensitivity finds and low sensitivity does not.
fn soft_hit() -> Vec<f32> {
    (0..44_100)
        .map(|i| {
            let amp = if (20_000..24_000).contains(&i) {
                0.0316
            } else {
                0.01
            };
            if i % 2 == 0 {
                amp
            } else {
                -amp
            }
        })
        .collect()
}

#[test]
fn sensitivity_decides_whether_a_soft_hit_counts() {
    assert_eq!(onsets(&soft_hit(), 44_100, 1.0).len(), 2);
    assert_eq!(onsets(&soft_hit(), 44_100, 0.5).len(), 2);
    assert_eq!(onsets(&soft_hit(), 44_100, 0.0).len(), 1);
}

#[test]
fn onset_slices_are_written_from_the_region() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hits.wav");
    let rate: u32 = 44_100;
    let data = bursts(rate as usize, &[(8_000, 0.9), (25_000, 0.9)]);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&file, spec).unwrap();
    for s in &data {
        w.write_sample((s * 32_767.0) as i16).unwrap();
    }
    w.finalize().unwrap();

    // The region starts at a mark 4,000 frames in, so offsets must be added back.
    let out = export(&job(
        &file,
        rate,
        &[4_000],
        5_000,
        Cut::Onsets(0.5),
        dir.path(),
    ))
    .unwrap();
    assert_eq!(out.slices.len(), 3, "{:?}", out.slices);
    assert_eq!(out.slices[0].0, 4_000);
    assert!(
        (7_560..=8_000).contains(&out.slices[1].0),
        "{:?}",
        out.slices
    );
    assert!(
        (24_560..=25_000).contains(&out.slices[2].0),
        "{:?}",
        out.slices
    );
    assert_eq!(out.slices[2].1, rate as u64);
    assert_eq!(files(&out.dir).len(), 3);
}

#[test]
fn samples_json_names_each_slot_as_its_file_does() {
    let dir = tempfile::tempdir().unwrap();
    // Windows forbids `"` in a file name, so there the backslashes in the
    // path are what JSON must escape.
    let name = if cfg!(windows) {
        "a 'quoted' name.wav"
    } else {
        "a \"quoted\" name.wav"
    };
    let file = dir.path().join(name);
    source(&file, 44_100, 2, 16, 20_000);
    let out = export(&job(&file, 44_100, &[5_000], 0, Cut::Marks, dir.path())).unwrap();
    let json = std::fs::read_to_string(out.dir.join("samples.json")).unwrap();
    let path = file
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    assert_eq!(
        json,
        format!(
            "{{\n  \"source\": \"{path}\",\n  \"sample_rate\": 44100,\n  \"samples\": {{\n    \"000\": {{ \"start_frame\": 0, \"end_frame\": 5000 }},\n    \"001\": {{ \"start_frame\": 5000, \"end_frame\": 20000 }}\n  }}\n}}\n"
        )
    );
    // The quotes in the file name are not safe in a directory name.
    assert_eq!(out.dir.file_name().unwrap(), "a _quoted_ name");
}

#[test]
fn a_second_export_of_a_track_gets_its_own_directory() {
    let dir = tempfile::tempdir().unwrap();
    let samples = dir.path().join("does/not/exist/yet");
    let file = dir.path().join("amen.wav");
    source(&file, 44_100, 2, 16, 10_000);
    let j = job(&file, 44_100, &[], 0, Cut::Region, &samples);
    let dirs: Vec<PathBuf> = (0..3).map(|_| export(&j).unwrap().dir).collect();
    assert_eq!(
        dirs,
        [
            samples.join("amen"),
            samples.join("amen-2"),
            samples.join("amen-3")
        ]
    );
}

#[test]
fn exports_that_cannot_be_done_leave_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let samples = dir.path().join("samples");
    let file = dir.path().join("src.wav");
    source(&file, 44_100, 2, 16, 10_000);

    // Marks recorded at another rate would cut in the wrong places.
    let wrong = export(&job(&file, 48_000, &[], 0, Cut::Region, &samples));
    assert_eq!(
        wrong,
        Err("the file plays at 44100 Hz, but its marks count 48000 Hz".into())
    );
    // A region past the end of the track.
    let empty = export(&job(
        &file,
        44_100,
        &[20_000],
        30_000,
        Cut::Region,
        &samples,
    ));
    assert_eq!(empty, Err("nothing to export: the region is empty".into()));

    let marks: Vec<u64> = (1..=MAX_SLICES as u64).map(|i| i * 30).collect();
    let many = export(&job(&file, 44_100, &marks, 0, Cut::Marks, &samples));
    assert_eq!(
        many,
        Err(format!(
            "257 slices is more than the {MAX_SLICES} a sample bank holds"
        ))
    );

    let missing = export(&job(
        &dir.path().join("gone.wav"),
        44_100,
        &[],
        0,
        Cut::Region,
        &samples,
    ));
    assert!(missing.is_err());
    let left: Vec<_> = std::fs::read_dir(&samples)
        .map(|d| d.collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "left behind: {left:?}");
}

/// Writes `data` as a mono 16-bit WAV.
fn mono_wav(path: &Path, rate: u32, data: &[f32]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for &s in data {
        w.write_sample((s.clamp(-1.0, 1.0) * 32_767.0) as i16)
            .unwrap();
    }
    w.finalize().unwrap();
}

#[test]
fn nearest_onset_finds_the_rise_beside_a_mark_and_nothing_in_silence() {
    let dir = tempfile::tempdir().unwrap();
    let rate = 44_100;
    let file = dir.path().join("hits.wav");
    // Hits at 1 s and 5 s, in ten seconds of otherwise silent audio.
    let hit = |at: usize| (at, 1.0f32);
    mono_wav(
        &file,
        rate,
        &bursts(
            rate as usize * 10,
            &[hit(rate as usize), hit(rate as usize * 5)],
        ),
    );

    // A mark 200 ms late snaps back to the hit, within a window or two.
    let at = rate as u64 + rate as u64 / 5;
    let found = nearest_onset(&file, rate, at, 0.5).unwrap().unwrap();
    assert!(
        (rate as u64).abs_diff(found) <= rate as u64 / 100,
        "snapped to {found}, not the hit at {rate}"
    );

    // The window is two seconds either side, so the hit at 5 s is not a
    // candidate for a mark at 8 s; that stretch is silent.
    assert_eq!(
        nearest_onset(&file, rate, rate as u64 * 8, 0.5).unwrap(),
        None
    );
}

/// Writes `frames` of a stereo 16-bit tone at `hz`, the right channel a
/// quarter turn behind and quieter, so the channels' mean crosses zero
/// where neither channel does. Returns the samples as the decoder reads them.
fn tone(path: &Path, rate: u32, hz: f64, frames: u64) -> Vec<f32> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    let mut read = Vec::new();
    for f in 0..frames {
        let t = std::f64::consts::TAU * hz * f as f64 / rate as f64;
        for v in [0.6 * t.sin(), 0.3 * (t - 1.2).sin()] {
            let v = (v * 32_767.0) as i16;
            w.write_sample(v).unwrap();
            read.push(v as f32 / 32_768.0);
        }
    }
    w.finalize().unwrap();
    read
}

#[test]
fn zero_edges_land_where_the_sampler_snap_puts_them() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tone.wav");
    let rate = 8_000;
    let read = tone(&file, rate, 37.0, 40_000);
    let peaks = Peaks::from_interleaved(&read, 2, rate);
    let reach = snap_reach(rate);
    let marks = [1_003, 31_007];
    let cut = |edges| Job {
        edges,
        ..job(&file, rate, &marks, 2_000, Cut::Equal(6), dir.path())
    };

    let exact = plan(&cut(Edges::Exact)).unwrap();
    let zero = plan(&cut(Edges::Zero)).unwrap();
    assert_eq!(zero.len(), exact.len());
    let snap = |e: u64| peaks.crossing(e - reach, e + reach, e).unwrap_or(e);
    let want: Vec<_> = exact.iter().map(|&(s, e)| (snap(s), e.map(snap))).collect();
    assert_eq!(zero, want);
    assert_ne!(zero, exact, "no edge moved");
    // Neighbouring slices still meet.
    assert!(zero.windows(2).all(|w| w[0].1 == Some(w[1].0)));
}

#[test]
fn zero_edges_too_close_to_snap_apart_stay_exact() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tone.wav");
    let rate = 8_000;
    // 5 Hz: one crossing within reach of all three edges.
    let read = tone(&file, rate, 5.0, 16_000);
    let peaks = Peaks::from_interleaved(&read, 2, rate);
    let crossing = peaks.crossing(1, 15_999, 1_600).unwrap();
    let range = Some((crossing - 30, crossing + 30));
    let job = Job {
        edges: Edges::Zero,
        range,
        ..job(&file, rate, &[], 0, Cut::Equal(3), dir.path())
    };
    // Edges at c-30, c-10, c+10 and c+30 all reach the crossing c. The
    // first to reach it without emptying a slice takes it; the rest stay.
    let c = crossing;
    assert_eq!(
        plan(&job).unwrap(),
        [(c - 30, Some(c)), (c, Some(c + 10)), (c + 10, Some(c + 30))]
    );
}

/// The samples of the first channel of each file written, as 24-bit values.
fn first_channel(out: &Exported, channels: usize) -> Vec<Vec<i32>> {
    files(&out.dir)
        .iter()
        .map(|f| {
            let samples: Vec<i32> = hound::WavReader::open(f)
                .unwrap()
                .samples::<i32>()
                .map(Result::unwrap)
                .collect();
            samples.chunks(channels).map(|c| c[0]).collect()
        })
        .collect()
}

#[test]
fn fade_edges_ramp_each_slice_in_and_out() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("src.wav");
    let rate = 8_000;
    source(&file, rate, 2, 24, 20_000);
    // 1 ms and 5 ms at 8 kHz: 8 frames in, 40 out.
    let fade = |cut, marks: &[u64]| Job {
        edges: Edges::Fade,
        fades: Fades::default(),
        ..job(&file, rate, marks, 5_000, cut, dir.path())
    };

    let out = export(&fade(Cut::Region, &[4_000, 6_000])).unwrap();
    let got = &first_channel(&out, 2)[0];
    assert_eq!(got.len(), 2_000);
    let exact = |i: usize| value(4_000 + i as u64, 0, 24);
    assert_eq!((got[0], got[1_999]), (0, 0));
    assert_eq!(got[4], (exact(4) as f32 * 0.5).round() as i32);
    assert!((8..1_960).all(|i| got[i] == exact(i)), "the middle changed");
    assert_eq!(got[1_979], (exact(1_979) as f32 * 0.5).round() as i32);

    // A slice running to the end of the track still fades out.
    let out = export(&fade(Cut::Marks, &[15_000])).unwrap();
    let last = first_channel(&out, 2).pop().unwrap();
    assert_eq!(last.len(), 5_000);
    assert_eq!(*last.last().unwrap(), 0);
}

#[test]
fn a_looped_range_is_written_to_loop_whole_with_its_edges_as_set() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("src.wav");
    source(&file, 48_000, 2, 24, 100_003);
    for edges in [Edges::Zero, Edges::Fade] {
        let job = Job {
            range: Some((20_011, 60_007)),
            loops: true,
            edges,
            ..job(&file, 48_000, &[], 0, Cut::Region, dir.path())
        };
        let out = export(&job).unwrap();
        assert_eq!(out.slices, [(20_011, 60_007)]);
        assert_exact(&out, 2, 24);
        let json = std::fs::read_to_string(out.dir.join("samples.json")).unwrap();
        assert!(
            json.contains(r#""loop_enabled": true, "loop_start": 0, "loop_end": 39996"#),
            "{json}"
        );
    }
    let out = export(&job(&file, 48_000, &[5_000], 0, Cut::Region, dir.path())).unwrap();
    let json = std::fs::read_to_string(out.dir.join("samples.json")).unwrap();
    assert!(!json.contains("loop"), "a slice not looped loops: {json}");
}

#[test]
fn onsets_found_again_at_another_sensitivity_read_the_track_once() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hits.wav");
    let rate: u32 = 44_100;
    // Over a steady tone, so a quiet hit rises a few dB and a loud one more:
    // which count as onsets depends on the sensitivity.
    let hits = bursts(88_200, &[(5_000, 0.6), (30_000, 0.12), (60_000, 0.3)]);
    let data: Vec<f32> = hits
        .iter()
        .enumerate()
        .map(|(i, h)| h + 0.15 * (i as f32 * 0.031).sin())
        .collect();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&file, spec).unwrap();
    for s in &data {
        w.write_sample((s * 32_767.0) as i16).unwrap();
    }
    w.finalize().unwrap();
    let at = |sensitivity| {
        job(
            &file,
            rate,
            &[1_000],
            2_000,
            Cut::Onsets(sensitivity),
            dir.path(),
        )
    };
    let fresh: Vec<_> = [0.2, 0.5, 0.9].map(|s| plan(&at(s)).unwrap()).into();
    assert_ne!(fresh[0], fresh[2], "sensitivity changed nothing");

    let audio = OnsetAudio::default();
    assert_eq!(plan_with(&at(0.2), &audio).unwrap(), fresh[0]);
    // Gone from disk, so what follows is found in the audio already read.
    std::fs::remove_file(&file).unwrap();
    assert_eq!(plan_with(&at(0.9), &audio).unwrap(), fresh[2]);
    assert_eq!(plan_with(&at(0.5), &audio).unwrap(), fresh[1]);
    // Another region is read again, and so fails here.
    let other = job(&file, rate, &[3_000], 4_000, Cut::Onsets(0.5), dir.path());
    assert!(plan_with(&other, &audio).is_err());
}
