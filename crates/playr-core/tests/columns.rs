//! Columns and sorting: what a cell holds, and the order a list comes in.

mod common;

use playr_core::columns::{cell, compare, Cell, Column, Measures, SortKey};
use playr_core::db::{self, Track};

fn track(title: &str, artist: &str, album: &str, year: Option<i32>) -> Track {
    Track {
        path: format!("/m/{artist}-{title}.flac"),
        title: Some(title.into()),
        artist: Some(artist.into()),
        album_artist: Some(artist.into()),
        album: Some(album.into()),
        year,
        duration_ms: Some(200_000),
        mtime: 1,
        size: 1,
        ..Default::default()
    }
}

fn measures(loudness: Option<f32>, bpm: Option<f32>) -> Measures {
    Measures {
        loudness,
        bpm,
        peak: None,
    }
}

fn key(name: &str) -> SortKey {
    SortKey::named(name).expect("a column name")
}

#[test]
fn a_cell_holds_a_tag_a_measurement_or_nothing() {
    let t = track("Peace Piece", "Bill Evans", "Everybody Digs", Some(1958));
    let m = measures(Some(-13.5), Some(87.0));
    assert_eq!(cell(&t, m, Column::Title), Cell::Text("Peace Piece".into()));
    assert_eq!(cell(&t, m, Column::Year), Cell::Number(1958.0));
    assert_eq!(
        cell(&t, m, Column::Loudness).text(Column::Loudness),
        "-13.5"
    );
    assert_eq!(cell(&t, m, Column::Tempo).text(Column::Tempo), "87");
    assert_eq!(cell(&t, m, Column::Time).text(Column::Time), "3:20");
    // Nothing measured reads as nothing, not as zero.
    assert_eq!(cell(&t, Measures::default(), Column::Tempo), Cell::Missing);
    assert_eq!(
        cell(&t, Measures::default(), Column::Tempo).text(Column::Tempo),
        "-"
    );
}

#[test]
fn a_sort_key_is_a_column_and_a_direction() {
    assert_eq!(
        key("tempo"),
        SortKey {
            column: Column::Tempo,
            descending: false
        }
    );
    assert!(key("tempo desc").descending);
    assert!(!key("tempo asc").descending);
    assert_eq!(key("tempo desc").text(), "tempo desc");
    assert_eq!(SortKey::named("loudest"), None);
}

#[test]
fn tracks_sort_by_each_key_in_turn() {
    let a = track("A", "Evans", "One", Some(1958));
    let b = track("B", "Evans", "One", Some(1961));
    let c = track("C", "Davis", "Two", Some(1959));
    let none = Measures::default();
    let order = |keys: &[SortKey]| {
        let mut rows = [&b, &c, &a];
        rows.sort_by(|x, y| compare((x, none), (y, none), keys));
        rows.iter()
            .map(|t| t.title.clone().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(order(&[key("title")]), ["A", "B", "C"]);
    assert_eq!(order(&[key("title desc")]), ["C", "B", "A"]);
    assert_eq!(order(&[key("year")]), ["A", "C", "B"]);
    // Album artist, then title, which is how the library reads by default.
    assert_eq!(order(&[key("album_artist"), key("title")]), ["C", "A", "B"]);
}

#[test]
fn tracks_with_nothing_measured_sort_last_either_way() {
    let (loud, quiet, unknown) = (
        track("loud", "A", "X", None),
        track("quiet", "A", "X", None),
        track("unknown", "A", "X", None),
    );
    let of = |t: &Track| match t.title.as_deref() {
        Some("loud") => measures(Some(-6.0), None),
        Some("quiet") => measures(Some(-20.0), None),
        _ => Measures::default(),
    };
    let order = |keys: &[SortKey]| {
        let mut rows = [&unknown, &loud, &quiet];
        rows.sort_by(|x, y| compare((x, of(x)), (y, of(y)), keys));
        rows.iter()
            .map(|t| t.title.clone().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(order(&[key("loudness")]), ["quiet", "loud", "unknown"]);
    assert_eq!(
        order(&[key("loudness desc")]),
        ["loud", "quiet", "unknown"],
        "an unanalysed track filled the top"
    );
}

#[test]
fn ties_keep_a_stable_order() {
    // Same artist, album and title: only the path separates them, and it
    // must, or a redraw could shuffle rows under the cursor.
    let mut first = track("Take", "A", "X", None);
    let mut second = first.clone();
    first.path = "/m/1.flac".into();
    second.path = "/m/2.flac".into();
    let none = Measures::default();
    let keys = [key("album")];
    let mut rows = [&second, &first];
    rows.sort_by(|x, y| compare((x, none), (y, none), &keys));
    assert_eq!(rows[0].path, "/m/1.flac");
}

#[test]
fn the_library_and_its_searches_come_back_in_the_same_order() {
    let conn = db::open_memory().unwrap();
    for t in [
        track("Blue in Green", "Miles Davis", "Kind of Blue", Some(1959)),
        track("Peace Piece", "Bill Evans", "Everybody Digs", Some(1958)),
        track("So What", "Miles Davis", "Kind of Blue", Some(1959)),
    ] {
        db::upsert(&conn, &t).unwrap();
    }
    let mut session = playr_core::session::Session::new(
        conn,
        common::fake_player().0,
        playr_core::event::ignore(),
    );

    session.set_sort(vec![key("title desc")]);
    let titles = |tracks: &[Track]| {
        tracks
            .iter()
            .map(|t| t.title.clone().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        titles(session.tracks()),
        ["So What", "Peace Piece", "Blue in Green"]
    );
    // A search is the library filtered, so it is ordered the same way.
    assert_eq!(
        titles(&session.search("davis")),
        ["So What", "Blue in Green"]
    );
    session.set_sort(vec![key("title")]);
    assert_eq!(
        titles(&session.search("davis")),
        ["Blue in Green", "So What"]
    );
}
