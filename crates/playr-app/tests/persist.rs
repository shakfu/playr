//! Session values remembered in the library between runs, as `persist` says.

#[path = "../../playr-core/tests/common/mod.rs"]
mod common;

use std::path::Path;

use playr_app::action::Action;
use playr_app::config::{Config, Program};
use playr_app::dispatch::Frontend;
use playr_app::model::{Model, SAVE_AFTER};
use playr_app::persist::{self, Values};
use playr_app::Theme;
use playr_core::audio::eq::Band;
use playr_core::audio::Mode;
use playr_core::columns::{Column, SortKey};
use playr_core::db;
use playr_core::gain::ReplayGain;
use playr_core::settings::Persist;

fn values() -> Values {
    Values {
        eq: [3.0, 0.0, -2.5],
        volume: 0.6,
        mode: Mode::RepeatOne,
        replaygain: ReplayGain::Album,
        theme: Theme::Light,
        columns: vec![Column::Title, Column::Tempo],
        sort: vec![
            SortKey {
                column: Column::Tempo,
                descending: true,
            },
            SortKey {
                column: Column::Title,
                descending: false,
            },
        ],
    }
}

#[test]
fn every_value_reads_back_as_stored() {
    let stored = values();
    let mut read = Values {
        eq: [0.0; 3],
        volume: 1.0,
        mode: Mode::Normal,
        replaygain: ReplayGain::Off,
        theme: Theme::Dark,
        columns: vec![Column::Artist],
        sort: Vec::new(),
    };
    for (_, p) in Persist::NAMES {
        persist::decode(p, &persist::encode(p, &stored), &mut read);
    }
    assert_eq!(read, stored);
    // A value that no longer reads leaves the setting's in place.
    let before = read.clone();
    for (p, text) in [
        (Persist::Eq, "3 0"),
        (Persist::Eq, "13 0 0"),
        (Persist::Volume, "1.5"),
        (Persist::Mode, "sideways"),
        (Persist::Columns, "title,bogus"),
        (Persist::Columns, ""),
        (Persist::Sort, "bogus desc"),
    ] {
        persist::decode(p, text, &mut read);
        assert_eq!(read, before, "{p:?} {text:?}");
    }
    assert_eq!(persist::key(Persist::Eq, Program::Gui), "eq");
    assert_eq!(persist::key(Persist::Sort, Program::Gui), "sort.gui");
}

fn model(library: &Path, program: Program, settings: &str) -> Model {
    let config = Config::parse_for(program, settings).unwrap();
    Model::new(
        db::open(library).unwrap(),
        common::fake_player().0,
        Vec::new(),
        config,
    )
}

const REMEMBER: &str =
    "persist = ['eq', 'volume', 'mode', 'replaygain', 'theme', 'columns', 'sort']";

#[test]
fn remembered_values_win_over_the_settings_at_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library.db");
    let v = values();
    let mut first = model(&library, Program::Gui, REMEMBER);
    for action in [
        Action::SetEq(Band::Bass, v.eq[0]),
        Action::SetEq(Band::Treble, v.eq[2]),
        Action::SetVolume(v.volume),
        Action::SetMode(v.mode),
        Action::SetReplayGain(v.replaygain),
        Action::Theme(v.theme),
        Action::SetColumns(v.columns.clone()),
        Action::SetSort(v.sort.clone()),
    ] {
        first.perform(action);
    }
    first.quit();
    drop(first);

    let settings = format!("{REMEMBER}\nvolume = 20\ntheme = 'dark'\n");
    let again = model(&library, Program::Gui, &settings);
    let player = again.session().player();
    assert_eq!(player.eq(), v.eq);
    assert_eq!(player.volume(), v.volume);
    assert_eq!(player.mode(), v.mode);
    assert_eq!(again.replaygain(), v.replaygain);
    assert_eq!(again.theme(), v.theme);
    assert_eq!(again.columns(), &v.columns[..]);
    assert_eq!(again.session().sort(), &v.sort[..]);
    drop(again);

    // Another program shares the EQ, but keeps its own columns and sort.
    let terminal = model(&library, Program::Terminal, REMEMBER);
    assert_eq!(terminal.session().player().eq(), v.eq);
    assert_ne!(terminal.columns(), &v.columns[..]);
    assert_ne!(terminal.session().sort(), &v.sort[..]);
    drop(terminal);

    // Unlisted, a remembered value is not used: the setting applies.
    let unlisted = model(&library, Program::Gui, "persist = ['eq']\nvolume = 20\n");
    assert_eq!(unlisted.session().player().volume(), 0.2);
    assert_eq!(unlisted.session().player().eq(), v.eq);
}

#[test]
fn a_change_is_stored_once_it_holds_still() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library.db");
    let mut m = model(&library, Program::Terminal, "persist = ['volume']");
    let stored = || db::state(&db::open(&library).unwrap(), "volume").unwrap();
    m.perform(Action::SetVolume(0.3));
    m.refresh();
    assert_eq!(stored(), None, "stored before it held still");
    std::thread::sleep(SAVE_AFTER + SAVE_AFTER / 5);
    m.refresh();
    assert_eq!(stored().as_deref(), Some("0.3"));
}

#[test]
fn nothing_is_stored_unless_named() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library.db");
    let mut m = model(&library, Program::Terminal, "");
    m.perform(Action::SetVolume(0.3));
    m.quit();
    drop(m);
    assert_eq!(
        db::state(&db::open(&library).unwrap(), "volume").unwrap(),
        None
    );
}
