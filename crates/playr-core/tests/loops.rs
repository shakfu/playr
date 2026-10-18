//! The loop library: schema, slot migration, scanning loops, search.

mod common;

use std::path::{Path, PathBuf};

use playr_core::db::loops::{self as lib, Origin, TempoFrom};
use playr_core::db::{self, query};
use playr_core::loops;
use playr_core::sliced::append_chunk;

/// Writes `secs` of stereo silence at `rate` under `path`, making its directory.
fn wav(path: &Path, rate: u32, secs: f32) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    common::silence(path, rate, secs);
    path.to_path_buf()
}

/// An `acid` chunk: `flags`, root note, beats and tempo.
fn acid(flags: u32, root: u16, beats: u32, tempo: f32) -> Vec<u8> {
    let mut c = b"acid".to_vec();
    c.extend(24u32.to_le_bytes());
    c.extend(flags.to_le_bytes());
    c.extend(root.to_le_bytes());
    c.extend(0u16.to_le_bytes());
    c.extend(0f32.to_le_bytes());
    c.extend(beats.to_le_bytes());
    c.extend(4u16.to_le_bytes());
    c.extend(4u16.to_le_bytes());
    c.extend(tempo.to_le_bytes());
    c
}

#[test]
fn tempo_follows_from_beats_and_length() {
    assert_eq!(loops::bpm(4, 88_200, 44_100), 120.0);
    assert_eq!(loops::beats(120.0, 88_200, 44_100), Some(4));
    assert_eq!(loops::beats(120.0, 100, 44_100), None, "under one beat");
    assert_eq!(loops::guess(88_200, 44_100), Some((4, 120.0)));
    // 3 s: 4 beats is 80 BPM, 8 is 160; 160 is nearer 120 on a log scale.
    assert_eq!(loops::guess(3 * 44_100, 44_100).unwrap().0, 8);
    assert_eq!(loops::guess(0, 44_100), None);
}

#[test]
fn file_names_give_tempo_and_key() {
    let cases: [(&str, Option<f64>, Option<&str>); 9] = [
        ("Drums_120bpm_Am", Some(120.0), Some("Am")),
        ("bass 98 BPM F#m", Some(98.0), Some("F#m")),
        ("Pad_Cmaj_90", Some(90.0), Some("C")),
        ("Keys Bb loop", None, Some("Bb")),
        ("Ebmin stab 128", Some(128.0), Some("Ebm")),
        ("Kick_01", None, None),
        ("Loop_A_808", None, None),
        ("Amen break", None, None),
        ("Drums 140", Some(140.0), None),
    ];
    for (name, bpm, key) in cases {
        let (b, k) = loops::from_name(name);
        assert_eq!((b, k.as_deref()), (bpm, key), "{name}");
    }
}

#[test]
fn a_data_size_past_the_file_or_left_unset_reads_to_the_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = wav(&dir.path().join("a.wav"), 8_000, 2.0);
    let bytes = std::fs::read(&path).unwrap();
    let at = bytes.windows(4).position(|w| w == b"data").unwrap() + 4;
    for size in [0u32, 0xFFFF_FFFF, 0x7FFF_FFFF] {
        let mut b = bytes.clone();
        b[at..at + 4].copy_from_slice(&size.to_le_bytes());
        std::fs::write(&path, &b).unwrap();
        let w = loops::read_wav(&path).unwrap();
        assert_eq!(w.frames, 16_000, "data size {size:#x}");
    }
}

#[test]
fn wav_chunks_give_frames_acid_and_smpl() {
    let dir = tempfile::tempdir().unwrap();
    let path = wav(&dir.path().join("a.wav"), 8_000, 2.0);
    append_chunk(&path, &acid(2, 57, 8, 240.0)).unwrap();
    append_chunk(&path, &playr_core::sliced::smpl_chunk(8_000, 100, 7_999)).unwrap();
    let w = loops::read_wav(&path).unwrap();
    assert_eq!((w.channels, w.rate, w.frames), (2, 8_000, 16_000));
    let a = w.acid.unwrap();
    assert_eq!(
        (a.one_shot, a.root, a.beats, a.tempo),
        (false, Some(57), 8, 240.0)
    );
    assert_eq!(
        w.smpl,
        Some((100, 8_000)),
        "the last frame is played, so end is past it"
    );
}

#[test]
fn scan_reads_loops_with_tags_tempo_and_key_and_leaves_out_one_shots() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("loops");
    let named = wav(&root.join("Pack/Drums/beat_120bpm_Am.wav"), 8_000, 2.0);
    let acid_file = wav(&root.join("Pack/Keys/chords.wav"), 8_000, 4.0);
    append_chunk(&acid_file, &acid(2, 60, 8, 0.0)).unwrap();
    let shot = wav(&root.join("Pack/Hits/hit.wav"), 8_000, 0.5);
    append_chunk(&shot, &acid(1, 0, 0, 0.0)).unwrap();
    let plain = wav(&root.join("plain.wav"), 8_000, 2.0);

    let mut conn = db::open_memory().unwrap();
    let stats = loops::scan_dir(&mut conn, &root, false, |_| {}).unwrap();
    assert_eq!(
        (stats.scan.seen, stats.scan.added, stats.one_shots),
        (4, 3, 1)
    );
    assert_eq!(lib::roots(&conn).unwrap(), [root.canonicalize().unwrap()]);

    let all = lib::all(&conn).unwrap();
    let by = |p: &Path| {
        let p = p.canonicalize().unwrap();
        all.iter()
            .find(|l| Path::new(&l.path) == p)
            .unwrap()
            .clone()
    };
    let n = by(&named);
    assert_eq!(
        (n.tags.as_str(), n.origin, n.key.as_deref()),
        ("pack drums", Origin::Scan, Some("Am"))
    );
    assert_eq!(
        (n.bpm, n.beats, n.bpm_from),
        (Some(120.0), Some(4), Some(TempoFrom::Name))
    );
    assert_eq!((n.start, n.end, n.rate), (0, 16_000, 8_000));
    let a = by(&acid_file);
    assert_eq!(
        (a.bpm, a.beats, a.bpm_from),
        (Some(120.0), Some(8), Some(TempoFrom::Acid))
    );
    assert_eq!(a.key.as_deref(), Some("C"), "the acid root note");
    let p = by(&plain);
    assert_eq!(
        (p.beats, p.bpm_from, p.tags.as_str()),
        (Some(4), Some(TempoFrom::Length), "")
    );

    let again = loops::scan_dir(&mut conn, &root, false, |_| {}).unwrap();
    assert_eq!(
        (again.scan.skipped, again.scan.added),
        (3, 0),
        "unchanged files are not read"
    );
}

#[test]
fn a_loop_root_takes_its_files_out_of_the_audio_library() {
    let dir = tempfile::tempdir().unwrap();
    let music = dir.path().join("music");
    wav(&music.join("song.wav"), 8_000, 1.0);
    let inner = wav(&music.join("loops/l.wav"), 8_000, 1.0);
    let mut conn = db::open_memory().unwrap();
    playr_core::scan::scan_dir(&mut conn, &music, |_, _| {}).unwrap();
    db::add_root(&conn, &music).unwrap();
    assert_eq!(query::count(&conn).unwrap(), 2);

    let refused = loops::scan_dir(&mut conn, dir.path(), true, |_| {});
    assert!(
        matches!(refused, Err(loops::Refused::HoldsTrackRoot(_))),
        "a loop root holding a track root would empty the audio library"
    );
    let refused = loops::scan_dir(&mut conn, &music.join("loops"), false, |_| {});
    assert!(
        matches!(refused, Err(loops::Refused::HoldsTracks(1))),
        "tracks leave the audio library only when asked"
    );
    assert_eq!(query::count(&conn).unwrap(), 2);
    let stats = loops::scan_dir(&mut conn, &music.join("loops"), true, |_| {}).unwrap();
    assert_eq!((stats.scan.added, stats.moved), (1, 1));
    assert_eq!(query::count(&conn).unwrap(), 1);

    // A rescan of the music leaves the loop root alone.
    std::fs::remove_file(&inner).unwrap();
    wav(&music.join("loops/m.wav"), 8_000, 1.0);
    playr_core::scan::scan_dir(&mut conn, &music, |_, _| {}).unwrap();
    assert_eq!(query::count(&conn).unwrap(), 1);
    assert_eq!(
        loops::prune_missing(&conn, &music.join("loops")).unwrap(),
        1
    );
}

#[test]
fn a_loop_root_that_reads_empty_is_not_pruned() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("loops");
    let l = wav(&root.join("l.wav"), 8_000, 1.0);
    let mut conn = db::open_memory().unwrap();
    loops::scan_dir(&mut conn, &root, false, |_| {}).unwrap();
    lib::set_beats(&conn, lib::all(&conn).unwrap()[0].id, 8).unwrap();
    // As an unmounted drive's mount point reads.
    std::fs::remove_file(&l).unwrap();
    assert_eq!(loops::prune_missing(&conn, &root).unwrap(), 0);
    assert_eq!(lib::count(&conn).unwrap(), 1, "a hand-set tempo went");
}

#[test]
fn slots_move_into_the_loop_library_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let conn = db::open(&path).unwrap();
    // As an older playr saves a slot.
    conn.execute(
        "INSERT INTO loops (path, slot, start, end, rate) VALUES ('/m/a.flac', 2, 10, 20, 44100)",
        [],
    )
    .unwrap();
    drop(conn);
    let conn = db::open(&path).unwrap();
    assert_eq!(query::loops(&conn, "/m/a.flac").unwrap(), [(2, 10, 20)]);
    let moved: i64 = conn
        .query_row("SELECT COUNT(*) FROM loops", [], |r| r.get(0))
        .unwrap();
    assert_eq!(moved, 0);
    let l = &lib::all(&conn).unwrap()[0];
    assert_eq!(
        (l.origin, l.slot, l.name.as_str()),
        (Origin::Slot, Some(2), "a")
    );

    assert!(query::clear_loop(&conn, "/m/a.flac", 2).unwrap());
    drop(conn);
    let conn = db::open(&path).unwrap();
    assert!(
        query::loops(&conn, "/m/a.flac").unwrap().is_empty(),
        "a cleared slot stays cleared"
    );
}

#[test]
fn a_file_and_its_slots_are_separate_loops() {
    let conn = db::open_memory().unwrap();
    query::save_loop(&conn, "/m/a.wav", 1, (0, 10), 8_000).unwrap();
    query::save_loop(&conn, "/m/a.wav", 1, (5, 10), 8_000).unwrap();
    query::save_loop(&conn, "/m/a.wav", 3, (0, 4), 8_000).unwrap();
    let mut file = lib::all(&conn).unwrap()[0].clone();
    file.slot = None;
    file.origin = Origin::Tape;
    lib::upsert_file(&conn, &file).unwrap();
    lib::upsert_file(&conn, &file).unwrap();
    assert_eq!(lib::count(&conn).unwrap(), 3);
    assert_eq!(
        query::loops(&conn, "/m/a.wav").unwrap(),
        [(1, 5, 10), (3, 0, 4)]
    );
}

#[test]
fn search_matches_words_tempo_and_key() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("loops");
    wav(&root.join("Drums/beat_120bpm_Am.wav"), 8_000, 2.0);
    wav(&root.join("Keys/chords 90bpm Cmaj.wav"), 8_000, 2.0);
    let mut conn = db::open_memory().unwrap();
    loops::scan_dir(&mut conn, &root, false, |_| {}).unwrap();
    let names = |q: &str| -> Vec<String> {
        lib::search(&conn, q)
            .unwrap()
            .into_iter()
            .map(|l| l.name)
            .collect()
    };
    assert_eq!(names("drum"), ["beat_120bpm_Am"], "tags match as prefixes");
    assert_eq!(names("bpm:88..92"), ["chords 90bpm Cmaj"]);
    assert_eq!(names("key:am"), ["beat_120bpm_Am"]);
    assert_eq!(names("bpm:fast"), Vec::<String>::new());
    assert_eq!(names("").len(), 2);
}

#[test]
fn beats_set_by_hand_survive_a_rescan_and_roots_can_be_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("loops");
    let file = wav(&root.join("x.wav"), 8_000, 2.0);
    let mut conn = db::open_memory().unwrap();
    loops::scan_dir(&mut conn, &root, false, |_| {}).unwrap();
    let id = lib::all(&conn).unwrap()[0].id;
    assert!(lib::set_beats(&conn, id, 2).unwrap());
    assert!(lib::set_key(&conn, id, Some("Dm")).unwrap());
    // Rewritten, so the rescan reads it again.
    wav(&file, 8_000, 2.0);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
        .unwrap();
    let again = loops::scan_dir(&mut conn, &root, false, |_| {}).unwrap();
    assert_eq!(again.scan.added, 1);
    let l = lib::get(&conn, id).unwrap().unwrap();
    assert_eq!(
        (l.beats, l.bpm, l.bpm_from),
        (Some(2), Some(60.0), Some(TempoFrom::User))
    );
    assert_eq!(l.key.as_deref(), Some("Dm"));

    assert_eq!(lib::forget_root(&conn, &root).unwrap(), Some(1));
    assert!(lib::roots(&conn).unwrap().is_empty());
    assert_eq!(lib::forget_root(&conn, &root).unwrap(), None);
}
