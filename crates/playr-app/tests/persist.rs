//! Session values remembered in the library between runs, as `persist` says.

#[path = "../../playr-core/tests/common/mod.rs"]
mod common;

use std::path::Path;

use playr_app::action::Action;
use playr_app::config::{Config, Program};
use playr_app::dispatch::Frontend;
use playr_app::mix::{Law, Mix, MixAction, Strip};
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
    let mut mix = Mix::new(Law::Db, 0.6);
    mix.set_level(Strip::Tape, 0.5);
    mix.set_level(Strip::Headphones, 0.8);
    mix.set_muted(Strip::Decks, true);
    Values {
        eq: [3.0, 0.0, -2.5],
        mix,
        mode: Mode::RepeatOne,
        replaygain: ReplayGain::Album,
        theme: Theme::Light,
        history: vec!["view queue".into(), "view library".into()],
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
        mix: Mix::new(Law::Db, 1.0),
        mode: Mode::Normal,
        replaygain: ReplayGain::Off,
        theme: Theme::Dark,
        columns: vec![Column::Artist],
        sort: Vec::new(),
        history: Vec::new(),
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
        (Persist::Mix, "1 1 1"),
        (Persist::Mix, "1.5 1 1 1 0 0 0 0 0"),
        (Persist::Mix, "1 1 1 1 0 0 0 0 2"),
        (Persist::Mix, "1 1 1 1 0 0 0 0 0 0"),
        (Persist::Mode, "sideways"),
        (Persist::Columns, "title,bogus"),
        (Persist::Columns, ""),
        (Persist::Sort, "bogus desc"),
    ] {
        persist::decode(p, text, &mut read);
        assert_eq!(read, before, "{p:?} {text:?}");
    }
    assert_eq!(persist::key(Persist::Eq, Program::Gui), "eq");
    // A position; the old key held a gain.
    assert_eq!(persist::key(Persist::Volume, Program::Gui), "master");
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

const REMEMBER: &str = "persist = ['eq', 'volume', 'mode', 'replaygain', 'theme', 'columns', \
     'sort', 'history', 'mix']";

#[test]
fn remembered_values_win_over_the_settings_at_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library.db");
    let v = values();
    let mut first = model(&library, Program::Gui, REMEMBER);
    for action in [
        Action::SetEq(Band::Bass, v.eq[0]),
        Action::SetEq(Band::Treble, v.eq[2]),
        Action::SetVolume(v.mix.level(Strip::Master)),
        Action::Mix(MixAction::Set(Strip::Tape, 0.5)),
        Action::Mix(MixAction::Set(Strip::Headphones, 0.8)),
        Action::Mix(MixAction::Mute(Strip::Decks, Some(true))),
        Action::SetMode(v.mode),
        Action::SetReplayGain(v.replaygain),
        Action::Theme(v.theme),
        Action::SetColumns(v.columns.clone()),
        Action::SetSort(v.sort.clone()),
    ] {
        first.perform(action);
    }
    for line in &v.history {
        first.run_command(line);
    }
    first.quit();
    drop(first);

    let settings = format!("{REMEMBER}\nmaster = 20\ntheme = 'dark'\n");
    let mut again = model(&library, Program::Gui, &settings);
    assert_eq!(again.mix(), &v.mix);
    let player = again.session().player();
    assert_eq!(player.eq(), v.eq);
    assert_eq!(player.volume(), Law::Db.gain(0.6));
    assert_eq!(player.mode(), v.mode);
    assert_eq!(again.replaygain(), v.replaygain);
    assert_eq!(again.theme(), v.theme);
    assert_eq!(again.columns(), &v.columns[..]);
    assert_eq!(again.session().sort(), &v.sort[..]);
    assert_eq!(again.history().lines(), &v.history[..]);
    drop(again);

    // Another program shares the EQ, but keeps its own columns and sort.
    let terminal = model(&library, Program::Terminal, REMEMBER);
    assert_eq!(terminal.session().player().eq(), v.eq);
    assert_ne!(terminal.columns(), &v.columns[..]);
    assert_ne!(terminal.session().sort(), &v.sort[..]);
    drop(terminal);

    // Unlisted, a remembered value is not used: the setting applies.
    let unlisted = model(&library, Program::Gui, "persist = ['eq']\nmaster = 20\n");
    assert_eq!(unlisted.session().player().volume(), Law::Db.gain(0.2));
    assert_eq!(unlisted.session().player().eq(), v.eq);
}

/// A volume set or remembered before the mixer was a gain. It still plays at
/// that gain, under either law, rather than being read as a fader position.
#[test]
fn a_volume_from_before_the_mixer_plays_at_the_same_level() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library.db");
    for law in ["db", "cubic"] {
        let setting = model(
            &library,
            Program::Gui,
            &format!("fader = '{law}'\nvolume = 50"),
        );
        let gain = setting.session().player().volume();
        assert!((gain - 0.5).abs() < 1e-5, "{law}: {gain}");
        drop(setting);

        // Nothing listed in `persist`, so nothing is written over the old key.
        model(&library, Program::Gui, "")
            .session()
            .set_state("volume", "0.25");
        let remembered = model(
            &library,
            Program::Gui,
            &format!("fader = '{law}'\npersist = ['volume']"),
        );
        let gain = remembered.session().player().volume();
        assert!((gain - 0.25).abs() < 1e-5, "{law}: {gain}");
    }
}

#[test]
fn a_change_is_stored_once_it_holds_still() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library.db");
    let mut m = model(&library, Program::Terminal, "persist = ['volume']");
    let stored = || db::state(&db::open(&library).unwrap(), "master").unwrap();
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
    let conn = db::open(&library).unwrap();
    for key in ["master", "volume"] {
        assert_eq!(db::state(&conn, key).unwrap(), None, "{key}");
    }
}
