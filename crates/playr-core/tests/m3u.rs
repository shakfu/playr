//! M3U playlists: what is written, and what is read back from other players.

use std::path::{Path, PathBuf};

use playr_core::db::{self, Track};
use playr_core::m3u;

fn track(path: &str, title: Option<&str>, artist: Option<&str>, ms: Option<i64>) -> Track {
    Track {
        path: path.into(),
        title: title.map(Into::into),
        artist: artist.map(Into::into),
        duration_ms: ms,
        mtime: 1,
        size: 1,
        ..Default::default()
    }
}

#[test]
fn a_playlist_is_written_as_extended_m3u_with_absolute_paths() {
    let tracks = [
        track(
            "/m/so what.flac",
            Some("So What"),
            Some("Miles Davis"),
            Some(545_400),
        ),
        track("/m/untagged.wav", None, None, None),
        track("/m/odd\nname.wav", None, None, None),
    ];
    let (text, left_out) = m3u::write("late\nnight", &tracks);
    assert_eq!(
        text,
        "#EXTM3U\n#PLAYLIST:late night\n\
         #EXTINF:545,Miles Davis - So What\n/m/so what.flac\n\
         #EXTINF:-1,untagged\n/m/untagged.wav\n"
    );
    assert_eq!(left_out, 1, "a path with a line break cannot be a line");
    assert_eq!(m3u::name(&text).as_deref(), Some("late night"));
}

#[test]
fn paths_are_read_as_other_players_write_them() {
    let text = "\u{feff}#EXTM3U\n\
                #EXTINF:10,Some Title\n\
                /abs/a.flac\n\
                \n\
                rel/b.mp3\r\n\
                ../c.ogg\n\
                file:///abs/with%20space%C3%A9.flac\n\
                file://localhost/abs/d.flac\n\
                https://example.com/stream\n\
                # a comment\n";
    assert_eq!(
        m3u::paths(text, Path::new("/lists")),
        [
            PathBuf::from("/abs/a.flac"),
            PathBuf::from("/lists/rel/b.mp3"),
            PathBuf::from("/lists/../c.ogg"),
            PathBuf::from("/abs/with space\u{e9}.flac"),
            PathBuf::from("/abs/d.flac"),
        ]
    );
    assert_eq!(m3u::name(text), None);
}

#[test]
fn a_path_is_matched_as_written_or_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::create_dir(real.join("music")).unwrap();
    let file = real.join("music/a.flac");
    std::fs::write(&file, b"x").unwrap();
    let conn = db::open_memory().unwrap();
    db::upsert(&conn, &track(&file.to_string_lossy(), None, None, None)).unwrap();

    let roundabout = real.join("music/../music/a.flac");
    let gone = real.join("music/b.flac");
    let (found, missing) = m3u::resolve(&conn, &[file.clone(), roundabout, gone.clone()]).unwrap();
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|t| Path::new(&t.path) == file));
    assert_eq!(missing, [gone]);
}
