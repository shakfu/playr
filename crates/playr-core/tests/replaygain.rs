//! ReplayGain: the gain arithmetic, where gains come from, and what plays.

mod common;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{fake_player, have_ffmpeg, levels};
use playr_core::analysis::{self, loudness::Histogram, Analysis};
use playr_core::audio::{Cmd, Mode, State};
use playr_core::db::{self, Track};
use playr_core::gain::{parse_db, Gain, Gains, ReplayGain};

#[test]
fn a_gain_is_capped_at_its_peak() {
    let g = |db, peak| Gain { db, peak }.linear();
    assert!((g(-6.0206, Some(1.0)) - 0.5).abs() < 1e-4);
    // +12 dB would be 4x; a 0.5 peak allows only 2x.
    assert!((g(12.0, Some(0.5)) - 2.0).abs() < 1e-6);
    // With no peak known, it never boosts.
    assert_eq!(g(6.0, None), 1.0);
    assert!((g(-6.0206, None) - 0.5).abs() < 1e-4);
}

#[test]
fn a_gain_brings_loudness_to_minus_18() {
    let g = Gain::for_loudness(-9.0, 0.9);
    assert_eq!(g.db, -9.0);
}

#[test]
fn album_or_track_gain_follows_the_setting_and_mode() {
    use ReplayGain::*;
    assert_eq!(Off.album(Mode::Normal), None);
    assert_eq!(Track.album(Mode::Normal), Some(false));
    assert_eq!(Album.album(Mode::Shuffle), Some(true));
    assert_eq!(Auto.album(Mode::Normal), Some(true));
    assert_eq!(Auto.album(Mode::Repeat), Some(true));
    assert_eq!(Auto.album(Mode::Shuffle), Some(false));
    assert_eq!(Auto.album(Mode::RepeatOne), Some(false));

    let track = Gain {
        db: -3.0,
        peak: None,
    };
    let album = Gain {
        db: -5.0,
        peak: None,
    };
    let both = Gains {
        track: Some(track),
        album: Some(album),
    };
    assert_eq!(both.pick(true), Some(album));
    assert_eq!(both.pick(false), Some(track));
    let only_track = Gains {
        track: Some(track),
        album: None,
    };
    assert_eq!(only_track.pick(true), Some(track));
}

#[test]
fn gains_parse_as_taggers_write_them() {
    assert_eq!(parse_db("-7.43 dB"), Some(-7.43));
    assert_eq!(parse_db("+2.1 dB"), Some(2.1));
    assert_eq!(parse_db(" -1.5db "), Some(-1.5));
    assert_eq!(parse_db("3"), Some(3.0));
    assert_eq!(parse_db("loud"), None);
    assert_eq!(parse_db("NaN dB"), None);
}

/// A 3 s FLAC of a steady level, tagged with `tags` as `KEY=value`.
fn tagged_flac(dir: &Path, name: &str, tags: &[&str]) -> Option<PathBuf> {
    if !have_ffmpeg() {
        return None;
    }
    let src = dir.join(format!("{name}.wav"));
    levels(&src, 44100, &[(3.0, 0.5)]);
    let out = dir.join(format!("{name}.flac"));
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args(["-loglevel", "error", "-y", "-i"]).arg(&src);
    for tag in tags {
        cmd.args(["-metadata", tag]);
    }
    assert!(cmd.arg(&out).status().unwrap().success(), "ffmpeg failed");
    Some(out)
}

#[test]
fn gains_are_read_from_tags() {
    let dir = tempfile::tempdir().unwrap();
    let Some(path) = tagged_flac(
        dir.path(),
        "rg",
        &[
            "REPLAYGAIN_TRACK_GAIN=-6.50 dB",
            "REPLAYGAIN_TRACK_PEAK=0.988",
            "REPLAYGAIN_ALBUM_GAIN=-7.25 dB",
        ],
    ) else {
        return;
    };
    let g = Gains::from_tags(&path);
    assert_eq!(
        g.track,
        Some(Gain {
            db: -6.5,
            peak: Some(0.988)
        })
    );
    assert_eq!(
        g.album,
        Some(Gain {
            db: -7.25,
            peak: None
        })
    );
    assert_eq!(
        Gains::from_tags(&dir.path().join("rg.wav")),
        Gains::default()
    );
}

#[test]
fn r128_gains_count_from_minus_23_lufs() {
    let dir = tempfile::tempdir().unwrap();
    // -1280 / 256 = -5 dB to reach -23 LUFS, so 0 dB to reach -18.
    let Some(path) = tagged_flac(dir.path(), "r128", &["R128_TRACK_GAIN=-1280"]) else {
        return;
    };
    let g = Gains::from_tags(&path);
    assert_eq!(g.track.map(|g| g.db), Some(0.0));
}

fn library_track(conn: &rusqlite::Connection, path: &str, album: &str) -> Track {
    let mut t = Track {
        path: path.into(),
        album: Some(album.into()),
        album_artist: Some("A".into()),
        mtime: 1,
        size: 1,
        ..Default::default()
    };
    t.id = db::upsert(conn, &t).unwrap();
    t
}

/// A measurement at `lufs`, pooled from a histogram of one block level.
fn measured(lufs: f32, peak: f32) -> Analysis {
    let mut l = analysis::loudness::Loudness::new(48000, 2);
    let a = 10f32.powf(lufs / 20.0);
    let tone: Vec<f32> = (0..48000 * 3)
        .flat_map(|i| {
            let v = a * (std::f32::consts::TAU * 1000.0 * i as f32 / 48000.0).sin();
            [v, v]
        })
        .collect();
    l.feed(&tone);
    let m = l.finish();
    Analysis {
        rate: 48000,
        loudness: m.lufs,
        peak: Some(peak),
        histogram: Some(m.histogram),
        ..Default::default()
    }
}

#[test]
fn the_library_gives_album_gain_only_for_a_whole_album() {
    let mut conn = db::open_memory().unwrap();
    let a = library_track(&conn, "/m/a.flac", "X");
    let b = library_track(&conn, "/m/b.flac", "X");
    let c = library_track(&conn, "/m/c.flac", "Y");
    let _d = library_track(&conn, "/m/d.flac", "Y");
    db::analysis::put(&conn, &a, measured(-10.0, 0.9)).unwrap();
    db::analysis::put(&conn, &b, measured(-20.0, 0.5)).unwrap();
    // Album Y's second track has no analysis.
    db::analysis::put(&conn, &c, measured(-12.0, 0.8)).unwrap();
    let library = db::query::all(&conn).unwrap();
    let keys: HashSet<String> = library.iter().filter_map(analysis::album_key).collect();
    assert_eq!(
        analysis::update_albums(&mut conn, &library, &keys).unwrap(),
        1
    );

    let gains = db::analysis::gains(&conn, &library).unwrap();
    let a_gains = gains[Path::new("/m/a.flac")];
    assert!(
        (a_gains.track.unwrap().db - -8.0).abs() < 0.2,
        "{a_gains:?}"
    );
    // Equal lengths at -10 and -20 LUFS pool to 10 log10((0.1 + 0.01) / 2),
    // -12.6 LUFS, not their -15 LUFS mean in dB; both pass the gate.
    let album = a_gains.album.expect("both of X's tracks are analysed");
    assert!((album.db - -5.4).abs() < 0.1, "{album:?}");
    assert_eq!(album.peak, Some(0.9));
    assert_eq!(gains[Path::new("/m/b.flac")].album, Some(album));
    assert_eq!(gains[Path::new("/m/c.flac")].album, None);
    assert!(!gains.contains_key(Path::new("/m/d.flac")));
}

#[test]
fn a_changed_file_or_a_new_album_track_drops_its_gains() {
    let mut conn = db::open_memory().unwrap();
    let a = library_track(&conn, "/m/a.flac", "X");
    db::analysis::put(&conn, &a, measured(-10.0, 0.9)).unwrap();
    let library = db::query::all(&conn).unwrap();
    let keys = library.iter().filter_map(analysis::album_key).collect();
    analysis::update_albums(&mut conn, &library, &keys).unwrap();
    assert!(
        db::analysis::gains(&conn, &library).unwrap()[Path::new("/m/a.flac")]
            .album
            .is_some()
    );

    // A track added to the album since leaves the album row short.
    library_track(&conn, "/m/b.flac", "X");
    let library = db::query::all(&conn).unwrap();
    let gains = db::analysis::gains(&conn, &library).unwrap();
    assert_eq!(gains[Path::new("/m/a.flac")].album, None);

    // A rescan that finds the file changed makes its analysis stale.
    let changed = Track {
        mtime: 2,
        ..a.clone()
    };
    db::upsert(&conn, &changed).unwrap();
    let library = db::query::all(&conn).unwrap();
    assert!(!db::analysis::gains(&conn, &library)
        .unwrap()
        .contains_key(Path::new("/m/a.flac")));
}

#[test]
fn analysis_rows_read_back_as_written() {
    let conn = db::open_memory().unwrap();
    let t = library_track(&conn, "/m/a.flac", "X");
    let mut a = measured(-14.0, 0.7);
    a.md5 = Some(analysis::Md5::Bad);
    a.md5_hex = Some("00ff".into());
    a.bits = Some(24);
    a.bits_used = Some(16);
    a.cutoff = Some(analysis::cutoff::Measured {
        hz: 16000,
        fall_db: 40.0,
    });
    a.tempo = Some(analysis::tempo::Estimate {
        bpm: 120.5,
        confidence: 0.8,
        alt: Some(60.25),
    });
    a.bpm_tag = Some(121.0);
    db::analysis::put(&conn, &t, a.clone()).unwrap();
    let rows = db::analysis::rows(&conn).unwrap();
    let (stat, back) = &rows["/m/a.flac"];
    assert_eq!(back, &a);
    assert!(analysis::is_current(&t, Some(stat)));
    assert_eq!(
        Histogram::from_bytes(&a.histogram.unwrap().to_bytes()),
        back.histogram.clone()
    );
}

#[test]
fn pruning_removes_the_analysis_of_files_gone() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (kept, gone) = (root.join("kept.wav"), root.join("gone.wav"));
    levels(&kept, 44100, &[(0.1, 0.1)]);
    let conn = db::open_memory().unwrap();
    for p in [&kept, &gone] {
        let t = library_track(&conn, p.to_str().unwrap(), "X");
        db::analysis::put(&conn, &t, Analysis::default()).unwrap();
    }
    db::prune_missing(&conn, &root).unwrap();
    let rows = db::analysis::rows(&conn).unwrap();
    assert!(rows.contains_key(kept.to_str().unwrap()));
    assert!(!rows.contains_key(gone.to_str().unwrap()));
}

/// Plays `path` at `replaygain` with `gains`, and returns the median of the
/// absolute sample values played.
fn played_level(path: PathBuf, replaygain: ReplayGain, gains: HashMap<PathBuf, Gains>) -> f32 {
    let (player, control) = fake_player();
    player.send(Cmd::SetReplayGain(replaygain));
    player.send(Cmd::SetGains(Arc::new(gains)));
    player.send(Cmd::Play(vec![path], 0));
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let s = player.status();
        if s.state == State::Stopped && control.played.lock().unwrap().len() > 44100 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut played: Vec<f32> = control
        .played
        .lock()
        .unwrap()
        .iter()
        .map(|v| v.abs())
        .filter(|v| *v > 0.0)
        .collect();
    assert!(!played.is_empty(), "{}", control.report());
    played.sort_by(|a, b| a.partial_cmp(b).unwrap());
    played[played.len() / 2]
}

#[test]
fn the_engine_plays_at_the_library_gain() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.wav");
    levels(&path, 44100, &[(1.0, 0.5)]);
    let half = Gain {
        db: -6.0206,
        peak: Some(0.5),
    };
    let gains = HashMap::from([(
        path.clone(),
        Gains {
            track: Some(half),
            album: None,
        },
    )]);

    let off = played_level(path.clone(), ReplayGain::Off, gains.clone());
    let on = played_level(path, ReplayGain::Track, gains);
    // The WAV holds 0.5 at 16 bits: 16383 / 32767.
    let source = 16383.0 / 32767.0;
    assert!((off - source).abs() < 1e-4, "off played {off}");
    assert!((on - source / 2.0).abs() < 1e-3, "on played {on}");
}

#[test]
fn off_plays_bit_for_bit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.wav");
    levels(&path, 44100, &[(1.0, 0.3)]);
    let zero = Gains {
        track: Some(Gain {
            db: 0.0,
            peak: Some(0.3),
        }),
        album: None,
    };
    let exact = played_level(path.clone(), ReplayGain::Off, HashMap::new());
    let unity = played_level(
        path.clone(),
        ReplayGain::Track,
        HashMap::from([(path, zero)]),
    );
    assert_eq!(exact, unity);
}

#[test]
fn the_engine_falls_back_to_tags_and_reports_the_gain() {
    let dir = tempfile::tempdir().unwrap();
    let Some(path) = tagged_flac(
        dir.path(),
        "tagged",
        &[
            "REPLAYGAIN_TRACK_GAIN=-6.0206 dB",
            "REPLAYGAIN_TRACK_PEAK=0.5",
        ],
    ) else {
        return;
    };
    let level = played_level(path.clone(), ReplayGain::Auto, HashMap::new());
    assert!((level - 0.25).abs() < 2e-3, "played {level}");

    let (player, _control) = fake_player();
    player.send(Cmd::SetReplayGain(ReplayGain::Track));
    player.send(Cmd::Play(vec![path], 0));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut db = None;
    while Instant::now() < deadline && db.is_none_or(|d: f32| d == 0.0) {
        db = player.status().gain_db;
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(db.is_some_and(|d| (d + 6.02).abs() < 0.01), "{db:?}");
}

/// A library track with a stored tempo: `tag` as its BPM tag, `est` as an
/// estimate at `conf`.
fn with_tempo(
    conn: &rusqlite::Connection,
    path: &str,
    tag: Option<f32>,
    est: Option<(f32, f32)>,
) -> Track {
    let t = library_track(conn, path, "X");
    let a = Analysis {
        bpm_tag: tag,
        tempo: est.map(|(bpm, confidence)| analysis::tempo::Estimate {
            bpm,
            confidence,
            alt: None,
        }),
        ..Default::default()
    };
    db::analysis::put(conn, &t, a).unwrap();
    t
}

#[test]
fn bpm_searches_the_tempos_analysis_recorded() {
    let conn = db::open_memory().unwrap();
    with_tempo(&conn, "/m/slow.flac", None, Some((90.0, 0.9)));
    with_tempo(&conn, "/m/house.flac", Some(128.0), None);
    with_tempo(&conn, "/m/unsure.flac", None, Some((128.0, 0.05)));
    let untouched = library_track(&conn, "/m/none.flac", "X");

    let found = |q: &str| {
        db::query::search(&conn, q)
            .unwrap()
            .into_iter()
            .map(|t| t.path)
            .collect::<Vec<_>>()
    };
    assert_eq!(found("bpm:120..130"), ["/m/house.flac"]);
    assert_eq!(found("bpm:128"), ["/m/house.flac"]);
    assert_eq!(found("bpm:80.."), ["/m/house.flac", "/m/slow.flac"]);
    assert_eq!(found("bpm:..100"), ["/m/slow.flac"]);
    // An estimate playr is not sure of is not searched, and neither is a
    // track with no analysis.
    assert!(found("bpm:126..130").iter().all(|p| p != "/m/unsure.flac"));
    assert!(!found("bpm:0..300").contains(&untouched.path));
    // A range that parses as nothing matches nothing.
    assert!(found("bpm:fast").is_empty());
    assert!(found("bpm:").is_empty());

    // With text, both must match.
    let conn2 = db::open_memory().unwrap();
    let mut t = with_tempo(&conn2, "/m/e.flac", Some(128.0), None);
    t.title = Some("Evans".into());
    db::upsert(&conn2, &t).unwrap();
    db::analysis::put(
        &conn2,
        &t,
        Analysis {
            bpm_tag: Some(128.0),
            ..Default::default()
        },
    )
    .unwrap();
    let hits = db::query::search(&conn2, "evans bpm:120..130").unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(db::query::search(&conn2, "davis bpm:120..130")
        .unwrap()
        .is_empty());
}

#[test]
fn a_bpm_search_matches_the_alternate_level_too() {
    let conn = db::open_memory().unwrap();
    let t = library_track(&conn, "/m/dnb.flac", "X");
    // Recorded at 87, heard at 174: the prior halved it.
    db::analysis::put(
        &conn,
        &t,
        Analysis {
            tempo: Some(analysis::tempo::Estimate {
                bpm: 87.0,
                confidence: 0.9,
                alt: Some(174.0),
            }),
            ..Default::default()
        },
    )
    .unwrap();
    let found = |q: &str| db::query::search(&conn, q).unwrap().len();
    assert_eq!(found("bpm:87"), 1, "not found at the tempo recorded");
    assert_eq!(found("bpm:174"), 1, "not found at the tempo heard");
    assert_eq!(found("bpm:120..130"), 0);

    // A BPM tag is the answer on its own; the alternate does not widen it.
    let tagged = with_tempo(&conn, "/m/tagged.flac", Some(100.0), Some((50.0, 0.9)));
    db::analysis::put(
        &conn,
        &tagged,
        Analysis {
            bpm_tag: Some(100.0),
            tempo: Some(analysis::tempo::Estimate {
                bpm: 50.0,
                confidence: 0.9,
                alt: Some(100.0),
            }),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(found("bpm:50"), 0, "a tagged track matched its estimate");
    assert_eq!(found("bpm:100"), 1);
}

#[test]
fn an_older_analysis_table_gains_the_alternate_column() {
    // A library from playr 0.9.1 has an `analysis` table without `bpm_alt`;
    // `CREATE TABLE IF NOT EXISTS` leaves it alone, so opening must add it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch(
        "CREATE TABLE analysis (path TEXT PRIMARY KEY, mtime INTEGER NOT NULL,
           size INTEGER NOT NULL, version INTEGER NOT NULL, error TEXT,
           rate INTEGER NOT NULL, frames INTEGER NOT NULL, header_frames INTEGER,
           skipped INTEGER NOT NULL, lossless INTEGER NOT NULL, bits INTEGER,
           md5 TEXT, md5_hex TEXT, bits_used INTEGER, cutoff_hz INTEGER,
           cutoff_db REAL, loudness REAL, peak REAL, histogram BLOB,
           bpm REAL, bpm_conf REAL, bpm_tag REAL);
         INSERT INTO analysis (path, mtime, size, version, rate, frames, skipped,
           lossless, bpm, bpm_conf)
         VALUES ('/m/a.flac', 1, 1, 1, 44100, 100, 0, 1, 90.0, 0.9);",
    )
    .unwrap();
    drop(old);

    let conn = db::open(&path).unwrap();
    let rows = db::analysis::rows(&conn).unwrap();
    assert_eq!(rows.len(), 1, "the old row was lost");
    // Version 1, so it is analysed again rather than read as current.
    let t = Track {
        path: "/m/a.flac".into(),
        mtime: 1,
        size: 1,
        ..Default::default()
    };
    assert!(!analysis::is_current(&t, Some(&rows["/m/a.flac"].0)));
    db::analysis::put(&conn, &t, Analysis::default()).unwrap();
}

#[test]
fn a_changed_file_drops_out_of_a_bpm_search() {
    let conn = db::open_memory().unwrap();
    let t = with_tempo(&conn, "/m/house.flac", Some(128.0), None);
    assert_eq!(db::query::search(&conn, "bpm:128").unwrap().len(), 1);
    db::upsert(&conn, &Track { mtime: 99, ..t }).unwrap();
    assert!(db::query::search(&conn, "bpm:128").unwrap().is_empty());
}
