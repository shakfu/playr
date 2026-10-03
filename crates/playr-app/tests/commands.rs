//! `:` command parsing, completion and history, and the key map.

use std::time::Duration;

use playr_app::action::{Action, Key, Keymap};
use playr_app::command::{completions, line, parse, CommandLine, History, COMMANDS, HISTORY_LEN};
use playr_app::View::{self, Library, Playlists, Sampler, Selection};
use playr_core::audio::Mode;
use playr_core::gain::ReplayGain;

fn secs(s: f64) -> Duration {
    Duration::from_secs_f64(s)
}

/// Parses `line` as typed in the library view.
fn lib(line: &str) -> Result<Action, String> {
    parse(line, Library)
}

#[test]
fn commands_without_arguments() {
    for (line, action) in [
        ("quit", Action::Quit),
        ("q", Action::Quit),
        ("help", Action::CommandHelp),
        ("keys", Action::Help),
        ("next-view", Action::NextView),
        ("first", Action::CursorFirst),
        ("last", Action::CursorLast),
        ("play", Action::Activate),
        ("search", Action::StartSearch),
        ("pause", Action::TogglePause),
        ("next", Action::Next),
        ("prev", Action::Prev),
        ("stop", Action::Stop),
        ("unmark", Action::UndoMark),
        ("delmarks", Action::ClearMarks),
        ("next-mark", Action::NextMark),
        ("prev-mark", Action::PrevMark),
        ("  mark  ", Action::Mark),
        ("save", Action::StartSave),
    ] {
        assert_eq!(lib(line), Ok(action), "{line:?}");
    }
    assert_eq!(lib("pause now"), Err(":pause takes no arguments".into()));
}

#[test]
fn view_commands_work_only_in_their_view() {
    for (view, line, action) in [
        (Library, "toggle", Action::Add),
        (Library, "search-clear", Action::ClearSearch),
        (Playlists, "delete", Action::DeletePlaylist),
        (Playlists, "rename", Action::StartRename),
        (Playlists, "rename  dawn ", Action::RenameTo("dawn".into())),
    ] {
        assert_eq!(parse(line, view), Ok(action), "{line:?}");
        for other in View::ALL.into_iter().filter(|v| *v != view) {
            let name = line.split_whitespace().next().unwrap();
            let there = format!("{view:?}").to_lowercase();
            assert_eq!(
                parse(line, other),
                Err(format!(":{name} works in the {there} view")),
                "{line:?} in {other:?}"
            );
        }
    }
    // The selection and the queue share their editing commands.
    for (line, selection, queue) in [
        ("remove", Action::Remove, Action::Remove),
        ("move +2", Action::MoveTrack(2), Action::MoveTrack(2)),
        ("clear", Action::ClearSelection, Action::ClearQueue),
    ] {
        assert_eq!(parse(line, Selection), Ok(selection), "{line:?}");
        assert_eq!(parse(line, View::Queue), Ok(queue), "{line:?}");
        let name = line.split_whitespace().next().unwrap();
        // The sampler's `move` and `remove` act on its selection.
        let views = match name {
            "clear" => "selection and queue",
            _ => "selection, queue and sampler",
        };
        assert_eq!(
            parse(line, Library),
            Err(format!(":{name} works in the {views} views"))
        );
    }
    assert_eq!(parse("add", Playlists), Ok(Action::Add));
    assert_eq!(parse("add", View::Queue), Ok(Action::Add));
    // A prefix that matches nothing here names the views it would work in,
    // though several views list it.
    assert_eq!(
        parse("rem", Library),
        Err(":remove works in the selection, queue and sampler views".into())
    );
    // Typed in full, a command from another view is not taken as a prefix of one here.
    assert_eq!(
        parse("clear", Library),
        Err(":clear works in the selection and queue views".into())
    );
    assert_eq!(parse("search-c", Library), Ok(Action::ClearSearch));
    assert_eq!(parse("cl", Selection), Ok(Action::ClearSelection));
    assert_eq!(
        parse("move 2", Selection),
        Err("usage: :move +N | -N".into())
    );
}

#[test]
fn the_queue_takes_the_selection_s_command_names_and_its_old_ones() {
    for (action, name, old) in [
        (Action::Remove, "remove", "dequeue"),
        (Action::MoveTrack(-1), "move -1", "reorder -1"),
        (Action::ClearQueue, "clear", "queue-clear"),
    ] {
        assert_eq!(line(&action, Some(View::Queue)), name);
        assert_eq!(parse(old, View::Queue), Ok(action), "{old:?}");
    }
    assert_eq!(
        lib("enqueue"),
        Ok(Action::Enqueue(false)),
        "enqueue works in every view"
    );
    assert_eq!(lib("enqueue next"), Ok(Action::Enqueue(true)));
    assert!(lib("enqueue later").is_err());
    // `:q` still quits: the queue's command is not called `queue`.
    assert_eq!(lib("q"), Ok(Action::Quit));
}

#[test]
fn a_unique_prefix_names_a_command_and_an_ambiguous_one_lists_the_choices() {
    assert_eq!(lib("vol 40"), Ok(Action::SetVolume(0.4)));
    assert_eq!(lib("playl late"), Ok(Action::PlayPlaylist("late".into())));
    assert_eq!(
        lib("pl"),
        Err("ambiguous command pl: play, playlist".into())
    );
    assert_eq!(
        lib("s"),
        Err(
            "ambiguous command s: search, save, save-search, sql, scan, sort, stop, seek, speed, slice-edges, slice, search-clear"
                .into()
        )
    );
    // Only the commands usable here count: here `move-t` names a command
    // usable elsewhere.
    assert_eq!(
        lib("move-t"),
        Err(":move-to works in the sampler view".into())
    );
    assert_eq!(
        parse("move-t 0:02", Sampler),
        Ok(Action::MoveSelectedTo(std::time::Duration::from_secs(2)))
    );
    assert_eq!(lib("frobnicate"), Err("unknown command: frobnicate".into()));
    assert!(lib("").is_err());
}

#[test]
fn every_command_parses_in_its_views_and_names_only_itself() {
    for c in COMMANDS {
        let views = match c.view {
            Some(v) => vec![v],
            None => View::ALL.to_vec(),
        };
        for view in views {
            let result = parse(c.name, view);
            assert!(
                !matches!(&result, Err(e) if e.starts_with("unknown") || e.starts_with("ambiguous") || e.contains("works in")),
                ":{} in {view:?} did not resolve: {result:?}",
                c.name
            );
        }
    }
}

#[test]
fn nudge_snap_and_range_parse_in_the_sampler_and_round_trip() {
    use playr_app::action::Nudge;
    let s = |line: &str| parse(line, View::Sampler);
    assert_eq!(s("nudge +1"), Ok(Action::Nudge(Nudge::Columns(1))));
    assert_eq!(s("nudge 3"), Ok(Action::Nudge(Nudge::Columns(3))));
    assert_eq!(s("nudge -10%"), Ok(Action::Nudge(Nudge::Percent(-10))));
    assert_eq!(s("snap"), Ok(Action::Snap(None)));
    assert_eq!(s("snap off"), Ok(Action::Snap(Some(false))));
    assert_eq!(s("fit"), Ok(Action::Fit(None)));
    assert_eq!(s("fit on"), Ok(Action::Fit(Some(true))));
    assert_eq!(s("in"), Ok(Action::RangeIn));
    assert_eq!(s("out"), Ok(Action::RangeOut));
    assert_eq!(s("range"), Ok(Action::SetRange(None)));
    assert_eq!(s("loop"), Ok(Action::Loop(None)));
    assert_eq!(s("loop off"), Ok(Action::Loop(Some(false))));
    // Either order; the range is kept start first.
    assert_eq!(
        s("range 2.25 1:01"),
        Ok(Action::SetRange(Some((secs(2.25), secs(61.0)))))
    );
    for bad in [
        "nudge 0",
        "nudge +",
        "nudge x%",
        "snap maybe",
        "fit range",
        "range 1",
        "in 2",
        "loop 9",
    ] {
        assert!(s(bad).is_err(), "{bad:?} parsed");
    }
    assert_eq!(s("range 1 1"), Err("the range is empty".into()));
    assert!(lib("nudge +1").unwrap_err().contains("sampler view"));
    for text in [
        "nudge +4",
        "nudge -10%",
        "snap on",
        "fit off",
        "range 1.5 2.25",
        "in",
        "out",
        "range",
        "loop on",
        "edge end",
        "move -3",
        "move +10%",
        "onset",
        "remove",
        "deselect",
        "select 90",
        "select-slice 90",
        "move-to 90",
    ] {
        let action = s(text).unwrap();
        assert_eq!(line(&action, Some(View::Sampler)), text);
    }
    assert_eq!(
        completions("snap o", View::Sampler, &[], &Vec::new, true),
        ["snap on", "snap off"]
    );
    assert_eq!(
        completions("fit o", View::Sampler, &[], &Vec::new, true),
        ["fit on", "fit off"]
    );
}

#[test]
fn convert_completes_an_export_name_after_the_format() {
    let exports = || vec!["Amen Break".to_string(), "amen".into(), "think".into()];
    assert_eq!(
        completions("convert sf2 ", View::Library, &[], &exports, true),
        [
            "convert sf2 Amen Break",
            "convert sf2 amen",
            "convert sf2 think"
        ]
    );
    // Any case, and a name with a space completes whole.
    assert_eq!(
        completions("convert sf2 AM", View::Sampler, &[], &exports, true),
        ["convert sf2 Amen Break", "convert sf2 amen"]
    );
    assert!(completions("convert sf2 x", View::Library, &[], &exports, true).is_empty());
    // The format first, with no directory read for it.
    let unread = || -> Vec<String> { panic!("exports read before a format") };
    assert_eq!(
        completions("convert s", View::Library, &[], &unread, true),
        [
            "convert s2400",
            "convert sf2",
            "convert sp404mk2",
            "convert sxt"
        ]
    );
    assert!(completions("convert sf2 ", View::Library, &[], &exports, false).is_empty());
}

#[test]
fn convert_takes_a_format_name_in_any_view() {
    assert_eq!(lib("convert sf2"), Ok(Action::Convert("sf2".into(), None)));
    // Any name ConvertWithMoss may know, not only the ones offered.
    assert_eq!(
        parse("convert opxy", View::Sampler),
        Ok(Action::Convert("opxy".into(), None))
    );
    // It names a directory, so nothing that could leave the export's.
    for text in ["convert", "convert ../up", "convert a/b", "convert SF2"] {
        let error = lib(text).unwrap_err();
        assert!(error.contains("1010music, ableton"), "{text}: {error}");
    }
    assert_eq!(
        line(&Action::Convert("bento".into(), None), None),
        "convert bento"
    );
    // Then an earlier export: a name under the samples directory, or a path,
    // which may hold spaces.
    for (text, export) in [
        ("convert sf2 amen", "amen"),
        (
            "convert sf2 /music/my samples/amen",
            "/music/my samples/amen",
        ),
    ] {
        let action = Action::Convert("sf2".into(), Some(export.into()));
        assert_eq!(lib(text), Ok(action.clone()), "{text}");
        assert_eq!(line(&action, None), text);
    }
    // Typed while the extension is off, it is found by its full name only,
    // which the session answers with how to enable it; no prefix finds it
    // and no error names it.
    let typed = |line, extensions| playr_app::command::parse_typed(line, View::Library, extensions);
    assert_eq!(
        typed("convert sf2", false),
        Ok(Action::Convert("sf2".into(), None))
    );
    assert_eq!(
        typed("conv sf2", false),
        Err("unknown command: conv".into())
    );
    assert_eq!(
        typed("conv sf2", true),
        Ok(Action::Convert("sf2".into(), None))
    );
    // Off, `co` still means columns; on, it could be either.
    assert_eq!(typed("co", false), Err("usage: :columns NAME...".into()));
    assert_eq!(
        typed("co", true),
        Err("ambiguous command co: columns, convert".into())
    );

    // An extension's command is completed only once it is enabled.
    assert!(completions("convert ", View::Library, &[], &Vec::new, false).is_empty());
    assert!(completions("conv", View::Library, &[], &Vec::new, false).is_empty());
    assert_eq!(
        completions("conv", View::Library, &[], &Vec::new, true),
        ["convert"]
    );
    let listed = |extensions| {
        playr_app::command::command_rows(extensions)
            .iter()
            .any(|(usage, _)| usage == ":convert FORMAT [EXPORT]")
    };
    assert!(listed(true) && !listed(false));
    assert_eq!(
        completions("convert ", View::Library, &[], &Vec::new, true),
        [
            "convert 1010music",
            "convert ableton",
            "convert bento",
            "convert deluge",
            "convert distingex",
            "convert emulti",
            "convert exs24",
            "convert mc707",
            "convert mpc",
            "convert nki",
            "convert opxy",
            "convert renoise",
            "convert s2400",
            "convert sf2",
            "convert sp404mk2",
            "convert sxt"
        ]
    );
}

#[test]
fn slice_edges_takes_a_choice_or_its_prefix_in_any_view() {
    use playr_core::samples::Edges;
    assert_eq!(
        lib("slice-edges zero"),
        Ok(Action::SetSliceEdges(Edges::Zero))
    );
    assert_eq!(
        parse("slice-edges f", View::Sampler),
        Ok(Action::SetSliceEdges(Edges::Fade))
    );
    assert!(lib("slice-edges").is_err());
    assert!(lib("slice-edges soft").is_err());
    assert_eq!(
        line(&Action::SetSliceEdges(Edges::Exact), None),
        "slice-edges exact"
    );
    assert_eq!(
        completions("slice-edges ", View::Library, &[], &Vec::new, true),
        ["slice-edges exact", "slice-edges zero", "slice-edges fade"]
    );
}

#[test]
fn loop_takes_a_slot_to_recall_save_or_clear() {
    use playr_app::action::SlotOp;
    let s = |line: &str| parse(line, View::Sampler);
    assert_eq!(s("loop 1"), Ok(Action::LoopSlot(1, SlotOp::Use)));
    assert_eq!(s("loop 8 save"), Ok(Action::LoopSlot(8, SlotOp::Save)));
    assert_eq!(s("loop 3 clear"), Ok(Action::LoopSlot(3, SlotOp::Clear)));
    assert_eq!(s("loop on"), Ok(Action::Loop(Some(true))));
    for bad in ["loop 0", "loop 9", "loop 1 keep", "loop x"] {
        assert!(s(bad).is_err(), "{bad:?} parsed");
    }
    for text in ["loop 2", "loop 2 save", "loop 2 clear"] {
        assert_eq!(line(&s(text).unwrap(), Some(View::Sampler)), text);
    }
    assert!(lib("loop 1").unwrap_err().contains("sampler view"));
    // Only stopping a loop works outside the sampler.
    assert_eq!(lib("loop off"), Ok(Action::Loop(Some(false))));
    for text in ["loop", "loop on"] {
        assert!(lib(text).unwrap_err().contains("sampler view"), "{text}");
    }
    assert_eq!(
        completions("loop ", View::Library, &[], &Vec::new, true),
        ["loop off"]
    );
    assert_eq!(
        completions("loop ", View::Sampler, &[], &Vec::new, true),
        ["loop on", "loop off"]
    );
    assert_eq!(
        completions("loo", View::Sampler, &[], &Vec::new, true),
        ["loop", "loops"]
    );
    assert_eq!(s("loops clear"), Ok(Action::ClearLoops));
    assert!(s("loops").is_err() && s("loops 1").is_err());
    assert_eq!(
        line(&Action::ClearLoops, Some(View::Sampler)),
        "loops clear"
    );
    assert_eq!(
        completions("loops ", View::Sampler, &[], &Vec::new, true),
        ["loops clear"]
    );
}

#[test]
fn restart_parses_in_any_view_and_round_trips() {
    assert_eq!(lib("restart"), Ok(Action::Restart));
    assert_eq!(parse("restart", View::Sampler), Ok(Action::Restart));
    assert!(lib("restart 1").is_err());
    assert_eq!(line(&Action::Restart, None), "restart");
}

#[test]
fn the_cursor_moves_by_a_count_of_rows() {
    assert_eq!(lib("down"), Ok(Action::Cursor(1)));
    assert_eq!(lib("down 10"), Ok(Action::Cursor(10)));
    assert_eq!(lib("up"), Ok(Action::Cursor(-1)));
    assert_eq!(lib("up 3"), Ok(Action::Cursor(-3)));
    for bad in ["down 0", "down -2", "up x"] {
        assert!(lib(bad).is_err(), "{bad:?} parsed");
    }
}

#[test]
fn seek_takes_a_time_or_a_signed_offset() {
    assert_eq!(lib("seek 90"), Ok(Action::SeekTo(secs(90.0))));
    assert_eq!(lib("seek 1:23"), Ok(Action::SeekTo(secs(83.0))));
    assert_eq!(lib("seek 1:02:03"), Ok(Action::SeekTo(secs(3723.0))));
    assert_eq!(lib("seek 0:01.5"), Ok(Action::SeekTo(secs(1.5))));
    assert_eq!(lib("seek +10"), Ok(Action::SeekBy(10)));
    assert_eq!(lib("seek -1:30"), Ok(Action::SeekBy(-90)));
    for bad in [
        "seek",
        "seek abc",
        "seek 1::2",
        "seek 1:2:3:4",
        "seek :30",
        "seek -x",
    ] {
        assert!(lib(bad).is_err(), "{bad:?} parsed");
    }
    assert_eq!(lib("seek"), Err("usage: :seek TIME | +TIME | -TIME".into()));
}

#[test]
fn volume_is_a_percentage_or_a_signed_change() {
    assert_eq!(lib("volume 100"), Ok(Action::SetVolume(1.0)));
    assert_eq!(lib("volume 0"), Ok(Action::SetVolume(0.0)));
    assert_eq!(lib("volume +10"), Ok(Action::VolumeBy(0.1)));
    assert_eq!(lib("volume -25"), Ok(Action::VolumeBy(-0.25)));
    assert_eq!(lib("volume =60"), Ok(Action::SetVolume(0.6)));
    assert_eq!(lib("volume 101"), Err("volume is 0 to 100".into()));
    assert_eq!(lib("volume =-5"), Err("volume is 0 to 100".into()));
    assert!(lib("volume loud").is_err());
    assert!(lib("volume =").is_err());
}

#[test]
fn eq_names_a_band_and_a_sign_makes_it_relative() {
    use playr_core::audio::eq::Band;
    assert_eq!(lib("eq bass =3"), Ok(Action::SetEq(Band::Bass, 3.0)));
    assert_eq!(
        lib("eq treble =-4.5"),
        Ok(Action::SetEq(Band::Treble, -4.5))
    );
    assert_eq!(lib("eq mid +2"), Ok(Action::EqBy(Band::Mid, 2.0)));
    assert_eq!(lib("eq b -1"), Ok(Action::EqBy(Band::Bass, -1.0)));
    assert_eq!(lib("eq flat"), Ok(Action::FlatEq));
    assert_eq!(lib("eq bass =13"), Err("eq is -12 to 12 dB".into()));
    // Unsigned sets, as for :volume and :speed.
    assert_eq!(lib("eq bass 3"), Ok(Action::SetEq(Band::Bass, 3.0)));
    assert_eq!(lib("eq bass 13"), Err("eq is -12 to 12 dB".into()));
    assert!(lib("eq bass").is_err());
    assert!(lib("eq bass =").is_err());
    assert!(lib("eq loud +1").is_err());
    assert!(lib("eq bass =x").is_err());
    assert!(lib("eq").is_err());
    for action in [
        Action::SetEq(Band::Mid, -2.5),
        Action::EqBy(Band::Treble, -1.0),
        Action::EqBy(Band::Bass, 0.5),
        Action::FlatEq,
    ] {
        let text = line(&action, None);
        assert_eq!(lib(&text), Ok(action), "{text}");
    }
}

#[test]
fn speed_is_whole_semitones_and_a_sign_makes_it_relative() {
    assert_eq!(lib("speed 3"), Ok(Action::SetSpeed(3)));
    assert_eq!(lib("speed 0"), Ok(Action::SetSpeed(0)));
    assert_eq!(lib("speed +1"), Ok(Action::SpeedBy(1)));
    assert_eq!(lib("speed -12"), Ok(Action::SpeedBy(-12)));
    assert_eq!(lib("speed =-3"), Ok(Action::SetSpeed(-3)));
    assert_eq!(lib("speed =3"), Ok(Action::SetSpeed(3)));
    assert_eq!(lib("speed 13"), Err("speed is -12 to 12 semitones".into()));
    assert_eq!(
        lib("speed =-13"),
        Err("speed is -12 to 12 semitones".into())
    );
    for bad in [
        "speed",
        "speed 1.5",
        "speed +x",
        "speed +25",
        "speed =",
        "speed =--3",
    ] {
        assert!(lib(bad).is_err(), "{bad:?} parsed");
    }
}

#[test]
fn analyze_takes_a_directory_or_none() {
    assert_eq!(lib("analyze"), Ok(Action::Analyze(None)));
    assert_eq!(
        lib("analyze ~/music"),
        Ok(Action::Analyze(Some(
            std::env::home_dir().unwrap().join("music")
        )))
    );
}

#[test]
fn replaygain_takes_a_setting_or_its_prefix() {
    assert_eq!(
        lib("replaygain au"),
        Ok(Action::SetReplayGain(ReplayGain::Auto))
    );
    assert_eq!(
        lib("replaygain off"),
        Ok(Action::SetReplayGain(ReplayGain::Off))
    );
    assert_eq!(
        lib("replaygain a"),
        Err("ambiguous replaygain a: album, auto".into())
    );
    assert!(lib("replaygain").is_err());
    assert_eq!(
        completions("replaygain a", Library, &[], &Vec::new, true),
        vec![
            "replaygain album".to_string(),
            "replaygain auto".to_string()
        ]
    );
}

#[test]
fn mode_and_view_take_a_name_or_its_prefix() {
    assert_eq!(lib("mode shuf"), Ok(Action::SetMode(Mode::Shuffle)));
    // `repeat` is a whole name, so it is not ambiguous with `repeat-one`.
    assert_eq!(lib("mode repeat"), Ok(Action::SetMode(Mode::Repeat)));
    assert_eq!(lib("mode repeat one"), Ok(Action::SetMode(Mode::RepeatOne)));
    assert_eq!(lib("mode Repeat-One"), Ok(Action::SetMode(Mode::RepeatOne)));
    assert_eq!(lib("mode +"), Ok(Action::CycleMode(true)));
    assert_eq!(lib("mode -"), Ok(Action::CycleMode(false)));
    assert_eq!(
        lib("mode rep"),
        Err("ambiguous mode rep: repeat, repeat-one".into())
    );
    assert_eq!(lib("view sel"), Ok(Action::ShowView(Selection)));
    assert_eq!(lib("view p"), Ok(Action::ShowView(Playlists)));
    assert_eq!(lib("view q"), Ok(Action::ShowView(View::Queue)));
    assert_eq!(
        lib("view nope"),
        Err("views: library, queue, selection, playlists, sampler".into())
    );
    assert_eq!(
        lib("mode"),
        Err("modes: normal, shuffle, repeat, repeat-one".into())
    );
}

#[test]
fn names_and_queries_keep_their_spaces() {
    assert_eq!(
        lib("save late night"),
        Ok(Action::SaveAs("late night".into()))
    );
    assert_eq!(
        lib("save \"late night\""),
        Ok(Action::SaveAs("late night".into()))
    );
    assert_eq!(
        lib("playlist late night"),
        Ok(Action::PlayPlaylist("late night".into()))
    );
    // Quotes in a search are phrase syntax, so they stay.
    assert_eq!(
        lib("search artist:\"bill evans\""),
        Ok(Action::Search("artist:\"bill evans\"".into()))
    );
    assert!(lib("playlist").is_err());
    assert_eq!(lib("mark 1:00"), Ok(Action::MarkAt(secs(60.0))));
}

#[test]
fn completion_offers_commands_usable_here_then_their_arguments() {
    let playlists = ["Late Night".to_string(), "dawn".to_string()];
    let usable = |view: View| {
        COMMANDS
            .iter()
            .filter(|c| c.view.is_none_or(|v| v == view))
            .count()
    };
    assert_eq!(
        completions("p", Library, &playlists, &Vec::new, true),
        ["play", "playlist", "prune", "pause", "prev",]
    );
    assert_eq!(
        completions("", Library, &playlists, &Vec::new, true).len(),
        usable(Library)
    );
    assert_eq!(
        completions("re", Selection, &playlists, &Vec::new, true),
        ["rescan", "restart", "replaygain", "redo", "remove"]
    );
    assert_eq!(
        completions("re", Playlists, &playlists, &Vec::new, true),
        ["rescan", "restart", "replaygain", "redo", "rename"]
    );
    assert_eq!(
        completions("re", Library, &playlists, &Vec::new, true),
        ["rescan", "restart", "replaygain", "redo"]
    );
    assert_eq!(
        completions("mode r", Library, &playlists, &Vec::new, true),
        ["mode repeat", "mode repeat-one"]
    );
    assert_eq!(
        completions("vi ", Library, &playlists, &Vec::new, true),
        [
            "view library",
            "view queue",
            "view selection",
            "view playlists",
            "view sampler",
            "view next",
            "view prev"
        ]
    );
    // Playlist names match without regard to case.
    assert_eq!(
        completions("playlist la", Library, &playlists, &Vec::new, true),
        ["playlist Late Night"]
    );
    assert_eq!(
        completions("rename ", Playlists, &playlists, &Vec::new, true),
        ["rename Late Night", "rename dawn"]
    );
    assert!(completions("rename ", Library, &playlists, &Vec::new, true).is_empty());
    assert_eq!(
        completions("theme ", Playlists, &playlists, &Vec::new, true),
        ["theme system", "theme light", "theme dark"]
    );
    assert!(completions("seek ", Library, &playlists, &Vec::new, true).is_empty());
    assert!(completions("zz ", Library, &playlists, &Vec::new, true).is_empty());
}

#[test]
fn tab_cycles_forward_and_back_and_typing_starts_afresh() {
    let mut line = CommandLine::default();
    line.push('p');
    line.complete(true, Library, &[], &Vec::new, true);
    assert_eq!(line.text, "play");
    line.complete(true, Library, &[], &Vec::new, true);
    assert_eq!(line.text, "playlist");
    for _ in 0..4 {
        line.complete(true, Library, &[], &Vec::new, true);
    }
    assert_eq!(line.text, "play", "the cycle does not wrap");
    line.complete(false, Library, &[], &Vec::new, true);
    assert_eq!(line.text, "prev");

    // Typing ends the cycle, so Tab now completes the new text.
    line.text.clear();
    "playlist ".chars().for_each(|c| line.push(c));
    line.complete(true, Library, &["late".into()], &Vec::new, true);
    assert_eq!(line.text, "playlist late");

    let mut back = CommandLine::default();
    back.push('p');
    back.complete(false, Library, &[], &Vec::new, true);
    assert_eq!(back.text, "prev", "shift-tab does not start from the last");

    let mut none = CommandLine::default();
    none.push('z');
    none.complete(true, Library, &[], &Vec::new, true);
    assert_eq!(none.text, "z");
}

fn history(lines: &[&str]) -> History {
    let mut h = History::default();
    for l in lines {
        h.push(l);
    }
    h
}

#[test]
fn history_skips_blanks_and_repeats_and_keeps_the_newest() {
    let h = history(&["seek 10", "seek 10", "  ", "mode shuffle", "seek 10"]);
    assert_eq!(h.lines(), ["seek 10", "mode shuffle", "seek 10"]);

    let mut full = History::default();
    for i in 0..HISTORY_LEN + 5 {
        full.push(&format!("seek {i}"));
    }
    assert_eq!(full.lines().len(), HISTORY_LEN);
    assert_eq!(full.lines()[0], "seek 5");
}

#[test]
fn up_recalls_lines_starting_with_what_was_typed_and_down_returns_to_it() {
    let h = history(&["seek 10", "mode shuffle", "seek 1:00", "volume 50"]);
    let mut line = CommandLine::default();
    line.push('s');
    line.push('e');

    line.recall(true, &h);
    assert_eq!(line.text, "seek 1:00");
    line.recall(true, &h);
    assert_eq!(line.text, "seek 10");
    line.recall(true, &h);
    assert_eq!(line.text, "seek 10", "up past the oldest match moved");
    line.recall(false, &h);
    assert_eq!(line.text, "seek 1:00");
    line.recall(false, &h);
    assert_eq!(
        line.text, "se",
        "down past the newest did not restore the draft"
    );

    // Unfiltered from an empty line; editing a recalled line starts a new draft.
    let mut empty = CommandLine::default();
    empty.recall(true, &h);
    assert_eq!(empty.text, "volume 50");
    empty.pop();
    empty.recall(true, &h);
    assert_eq!(empty.text, "volume 50", "the edited line is the new draft");
    empty.push('!');
    empty.recall(true, &h);
    assert_eq!(empty.text, "volume 50!", "no match should leave the text");
    empty.recall(false, &h);
    assert_eq!(empty.text, "volume 50!");

    let mut nothing = CommandLine::default();
    nothing.push('x');
    nothing.recall(true, &h);
    nothing.recall(false, &h);
    assert_eq!(nothing.text, "x");
}

/// The default action for `key` in `view`.
fn default_key(key: &str, view: View) -> Option<Action> {
    Keymap::default()
        .lookup(Key::parse(key).unwrap(), view)
        .cloned()
}

#[test]
fn default_keys_map_to_actions_by_view() {
    assert_eq!(default_key(":", Library), Some(Action::StartCommand));
    assert_eq!(default_key("d", Selection), Some(Action::Remove));
    assert_eq!(default_key("d", Playlists), Some(Action::DeletePlaylist));
    assert_eq!(default_key("d", Library), None);
    assert_eq!(default_key("r", Library), None);
    assert_eq!(default_key("a", Selection), None);
    assert_eq!(default_key("J", Selection), Some(Action::MoveTrack(1)));
    assert_eq!(default_key("|", Sampler), Some(Action::Fit(Some(true))));
    assert_eq!(default_key("|", Library), None);
    assert_eq!(default_key("shift-down", Library), Some(Action::Cursor(1)));
    assert_eq!(
        default_key("shift-down", Selection),
        Some(Action::MoveTrack(1))
    );
    assert_eq!(
        default_key("shift-left", Library),
        Some(Action::SeekBy(-30))
    );
    assert_eq!(default_key("pagedown", Library), Some(Action::Cursor(10)));
    assert_eq!(default_key("\\", Library), Some(Action::SetSpeed(0)));
    // Chords are bound only where named, so Ctrl-M is not `M`.
    assert_eq!(default_key("ctrl-m", Library), None);
    // The sampler's arrows, range keys and edit keys; the mark keys are global.
    use playr_app::action::Nudge;
    assert_eq!(
        default_key("right", View::Sampler),
        Some(Action::Nudge(Nudge::Columns(1)))
    );
    assert_eq!(default_key("right", Library), Some(Action::SeekBy(5)));
    assert_eq!(default_key("i", View::Sampler), Some(Action::RangeIn));
    assert_eq!(default_key("o", View::Sampler), Some(Action::RangeOut));
    assert_eq!(default_key("{", View::Sampler), Some(Action::PrevMark));
    assert_eq!(default_key("}", Library), Some(Action::NextMark));
    assert_eq!(default_key(",", Library), Some(Action::PrevMark));
    assert_eq!(
        default_key(",", View::Sampler),
        Some(Action::AuditionSlice(false))
    );
    assert_eq!(
        default_key("<", View::Sampler),
        Some(Action::MoveSelected(Nudge::Columns(-1)))
    );
    assert_eq!(
        default_key(">", View::Sampler),
        Some(Action::MoveSelected(Nudge::Columns(1)))
    );
    assert_eq!(default_key("#", View::Sampler), Some(Action::SnapSelected));
    assert_eq!(
        default_key("backspace", View::Sampler),
        Some(Action::RemoveSelected)
    );
    assert_eq!(default_key("D", View::Sampler), Some(Action::Deselect));
    assert_eq!(default_key("u", View::Sampler), Some(Action::Undo));
    assert_eq!(default_key("r", View::Sampler), Some(Action::Redo));
    assert_eq!(default_key("r", Playlists), Some(Action::StartRename));
    // n and p skip tracks in the sampler as everywhere else.
    assert_eq!(default_key("n", View::Sampler), Some(Action::Next));
    assert_eq!(default_key("<", Library), None);
    assert_eq!(default_key("l", View::Sampler), Some(Action::Loop(None)));
    assert_eq!(
        default_key("esc", View::Sampler),
        Some(Action::DiscardSlices)
    );
    // Brackets select a range end in the sampler; parentheses change speed.
    use playr_app::sampler::Edge;
    assert_eq!(
        default_key("[", View::Sampler),
        Some(Action::PickEdge(Edge::Start))
    );
    assert_eq!(default_key("[", Library), None);
    assert_eq!(default_key("(", Library), Some(Action::SpeedBy(-1)));
    assert_eq!(default_key(")", View::Sampler), Some(Action::SpeedBy(1)));
}

/// Every default binding is a command line that parses back to its action,
/// in every view the binding applies to.
#[test]
fn every_default_key_runs_a_command_usable_in_its_views() {
    for b in Keymap::default().bindings() {
        let action = b.action.clone().expect("no default binds a key to nothing");
        let views = match b.view {
            Some(v) => vec![v],
            None => View::ALL.to_vec(),
        };
        let text = line(&action, b.view);
        if action == Action::StartCommand {
            assert_eq!(text, "command");
            continue;
        }
        for view in views {
            assert_eq!(
                parse(&text, view),
                Ok(action.clone()),
                "{} in {view:?}: {action:?} as {text:?}",
                b.key
            );
        }
    }
}

#[test]
fn map_and_unmap_parse_a_view_a_key_and_a_command() {
    let key = |k| Key::parse(k).unwrap();
    assert_eq!(
        lib("map L seek +60"),
        Ok(Action::Map {
            view: None,
            key: key("L"),
            action: Some(Box::new(Action::SeekBy(60)))
        })
    );
    assert_eq!(
        lib("map selection x remove"),
        Ok(Action::Map {
            view: Some(Selection),
            key: key("x"),
            action: Some(Box::new(Action::Remove))
        })
    );
    assert_eq!(
        lib("map playlists d nop"),
        Ok(Action::Map {
            view: Some(Playlists),
            key: key("d"),
            action: None
        })
    );
    assert_eq!(
        lib("map ; command"),
        Ok(Action::Map {
            view: None,
            key: key(";"),
            action: Some(Box::new(Action::StartCommand))
        })
    );
    assert_eq!(
        lib("unmap selection d"),
        Ok(Action::Unmap {
            view: Some(Selection),
            key: key("d")
        })
    );
    assert_eq!(
        lib("map d delete"),
        Err(":delete works in the playlists view; use map playlists d delete".into())
    );
    // Several views take it, so no one view is suggested.
    assert_eq!(
        lib("map d remove"),
        Err(":remove works in the selection, queue and sampler views".into())
    );
    assert_eq!(
        lib("map x map y quit"),
        Err("a key cannot run :map or :unmap".into())
    );
    assert_eq!(lib("map x"), Err("usage: :map [VIEW] KEY COMMAND".into()));
    assert_eq!(lib("map selection"), Err("not a key: selection".into()));
    assert_eq!(lib("unmap d q"), Err("usage: :unmap [VIEW] KEY".into()));
    assert_eq!(lib("map zz quit"), Err("not a key: zz".into()));
}

#[test]
fn command_lines_round_trip_through_parse() {
    for text in [
        "search artist:evans",
        "seek 83.5",
        "seek -30",
        "volume 60",
        "volume +2.5",
        "speed -3",
        "speed =-3",
        "analyze",
        "analyze /music/new arrivals",
        "mode repeat-one",
        "replaygain album",
        "theme light",
        "mark 1.25",
        "save late night",
        "playlist late night",
        "map shift-right seek +60",
        "map selection ctrl-x nop",
        "unmap playlists f5",
        "scan /music/new arrivals",
        "open /music/a.flac",
    ] {
        let action = parse(text, Library).unwrap();
        assert_eq!(line(&action, Some(Library)), text);
    }
}

#[test]
fn export_and_slice_choose_how_the_track_is_cut() {
    use playr_app::action::Slicing;
    for (text, cut) in [
        ("slice region", Slicing::Region),
        ("slice marks", Slicing::Marks),
        ("slice 16", Slicing::Equal(16)),
        ("slice onsets", Slicing::Onsets(None)),
        ("slice onsets 0.8", Slicing::Onsets(Some(0.8))),
    ] {
        assert_eq!(lib(text), Ok(Action::Slice(cut)), "{text}");
        for view in [Selection, Playlists] {
            assert_eq!(
                parse(text, view),
                Ok(Action::Slice(cut)),
                "{text} in {view:?}"
            );
        }
    }
    assert_eq!(
        line(&Action::Slice(Slicing::Onsets(Some(0.5))), None),
        "slice onsets 0.5"
    );
    assert_eq!(line(&Action::Slice(Slicing::Equal(8)), None), "slice 8");
    assert_eq!(
        line(&Action::Slice(Slicing::Onsets(None)), None),
        "slice onsets"
    );
    assert_eq!(lib("slice 1"), Err("slices are 2 to 256".into()));
    assert_eq!(lib("slice 257"), Err("slices are 2 to 256".into()));
    assert_eq!(
        lib("slice onsets 2"),
        Err("onset sensitivity is 0 to 1".into())
    );
    assert_eq!(
        lib("slice"),
        Err("usage: :slice region|marks|N|onsets [S]".into())
    );
    assert_eq!(
        lib("slice bars"),
        Err("usage: :slice region|marks|N|onsets [S]".into())
    );
    assert_eq!(
        lib("slice region 2"),
        Err("usage: :slice region|marks|N|onsets [S]".into())
    );
    assert_eq!(lib("export"), Err("unknown command: export".into()));
}

#[test]
fn the_cheatsheet_lists_every_command_under_its_view() {
    // A Windows checkout may end lines with CRLF.
    let sheet = include_str!("../../../docs/cheatsheet.md").replace("\r\n", "\n");
    let section = |heading: &str| {
        let start = sheet
            .find(&format!("## {heading}\n"))
            .unwrap_or_else(|| panic!("no {heading} section"));
        let rest = &sheet[start + 1..];
        &rest[..rest.find("\n## ").unwrap_or(rest.len())]
    };
    for c in COMMANDS {
        let heading = match c.view {
            None if c.extension => "Extensions",
            None => "Every view",
            Some(Library) => "Library",
            Some(Selection) => "Selection",
            Some(Playlists) => "Playlists",
            Some(View::Sampler) => "Sampler",
            Some(View::Queue) => "Queue",
        };
        let usage = format!("`:{} {}", c.name, c.args.replace('|', "\\|"));
        let usage = format!("{}`", usage.trim_end());
        assert!(
            section(heading).contains(&usage),
            "{usage} missing from the {heading} section of docs/cheatsheet.md"
        );
    }
}

#[test]
fn scan_and_open_take_a_path_with_home_as_tilde() {
    let home = std::env::home_dir().unwrap();
    assert_eq!(
        lib("scan ~/music/new arrivals"),
        Ok(Action::Scan(home.join("music/new arrivals")))
    );
    assert_eq!(
        lib("open \"/music/a b.flac\""),
        Ok(Action::Open(vec!["/music/a b.flac".into()]))
    );
    assert_eq!(lib("open ~"), Ok(Action::Open(vec![home.clone()])));
    // Only `~` and `~/` name the home directory; `~bob` is a relative path.
    assert_eq!(lib("scan ~bob"), Ok(Action::Scan("~bob".into())));
    assert_eq!(lib("scan"), Err("usage: :scan DIR".into()));
    assert_eq!(lib("open"), Err("usage: :open PATH".into()));
    assert_eq!(lib("rescan"), Ok(Action::Rescan));
    assert_eq!(lib("sync"), Ok(Action::Rescan));
    assert_eq!(
        lib("rescan ~/music"),
        Err(":rescan takes no arguments".into())
    );
    assert_eq!(lib("prune"), Ok(Action::Prune(None)));
    assert_eq!(lib("roots"), Ok(Action::ShowRoots));
    // `:roots add DIR` is another spelling of `:scan DIR`.
    assert_eq!(
        lib("roots add ~/music"),
        Ok(Action::Scan(home.join("music")))
    );
    assert_eq!(
        lib("roots rm ~/music"),
        Ok(Action::ForgetRoot(home.join("music")))
    );
    assert_eq!(lib("roots add"), Err("usage: :roots [add|rm DIR]".into()));
    assert_eq!(lib("roots rm"), Err("usage: :roots [add|rm DIR]".into()));
    assert_eq!(
        lib("roots wat ~/music"),
        Err("usage: :roots [add|rm DIR]".into())
    );
    assert_eq!(
        lib("prune ~/music"),
        Ok(Action::Prune(Some(home.join("music"))))
    );
}

#[test]
fn the_selection_commands_parse_in_the_sampler_only() {
    use playr_app::action::Nudge;
    use std::time::Duration;
    let sampler = |line: &str| parse(line, Sampler);
    let library = |line: &str| parse(line, Library);

    assert_eq!(
        sampler("select 1:30"),
        Ok(Action::SelectMarkAt(Duration::from_secs(90)))
    );
    assert_eq!(sampler("select"), Err("usage: :select TIME".into()));
    assert_eq!(sampler("deselect"), Ok(Action::Deselect));
    assert_eq!(
        sampler("move -1"),
        Ok(Action::MoveSelected(Nudge::Columns(-1)))
    );
    assert_eq!(
        sampler("move +10%"),
        Ok(Action::MoveSelected(Nudge::Percent(10)))
    );
    assert_eq!(
        sampler("move-to 0:02"),
        Ok(Action::MoveSelectedTo(Duration::from_secs(2)))
    );
    assert_eq!(sampler("move-to"), Err("usage: :move-to TIME".into()));
    assert_eq!(
        sampler("select-slice 1"),
        Ok(Action::SelectSliceAt(Duration::from_secs(1)))
    );
    assert_eq!(
        sampler("scrub 0:02"),
        Ok(Action::Scrub(Duration::from_secs(2)))
    );
    assert_eq!(sampler("scrub"), Err("usage: :scrub TIME".into()));
    assert_eq!(sampler("onset"), Ok(Action::SnapSelected));
    assert_eq!(sampler("remove"), Ok(Action::RemoveSelected));
    assert_eq!(sampler("edge +1"), Err("usage: :edge start|end".into()));
    // Undo works in every view.
    assert_eq!(library("undo"), Ok(Action::Undo));
    assert_eq!(library("redo"), Ok(Action::Redo));
    // The cursor is gone.
    assert!(sampler("cursor off").is_err());
    assert!(sampler("mark-pick next").is_err());

    // They belong to the sampler, so elsewhere they say where to go.
    assert_eq!(
        library("deselect"),
        Err(":deselect works in the sampler view".into())
    );
    assert_eq!(
        library("onset"),
        Err(":onset works in the sampler view".into())
    );
    // `move` and `remove` are the selection's there, and the sampler's here.
    assert_eq!(parse("remove", Selection), Ok(Action::Remove));
    assert_eq!(parse("move +1", Selection), Ok(Action::MoveTrack(1)));
}

#[test]
fn renamed_commands_keep_their_old_names_as_aliases() {
    for (old, new) in [
        ("sync", "rescan"),
        ("next-view", "view next"),
        ("prev-view", "view prev"),
        ("unmark", "mark-undo"),
        ("delmarks", "mark-clear"),
        ("next-mark", "mark-next"),
        ("prev-mark", "mark-prev"),
        ("clear-search", "search-clear"),
    ] {
        assert_eq!(lib(old), lib(new), "{old}");
        assert!(lib(new).is_ok(), "{new}");
    }
    for old in ["move-mark 0:02", "mark-move 0:02"] {
        assert_eq!(parse(old, Sampler), parse("move-to 0:02", Sampler), "{old}");
    }
    assert!(parse("move-to 0:02", Sampler).is_ok());
    // An old name still says where it works, by its new name.
    assert_eq!(
        parse("mark-move 0:02", Library),
        Err(":move-to works in the sampler view".into())
    );
    // Help and completion list only the new names.
    let names: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
    for old in [
        "sync",
        "unmark",
        "delmarks",
        "move-mark",
        "mark-move",
        "clear-search",
        "next-view",
    ] {
        assert!(!names.contains(&old), "{old} is listed");
    }
    // The shipped bindings use the new names.
    let keys = include_str!("../src/keys.toml");
    for old in [
        "\"unmark\"",
        "\"delmarks\"",
        "\"pick ",
        "\"del-mark\"",
        "\"next-view\"",
    ] {
        assert!(!keys.contains(old), "keys.toml binds {old}");
    }
}

#[test]
fn a_panel_request_becomes_the_action_playr_has_for_it() {
    use playr_app::media::action;
    use souvlaki::{MediaControlEvent as E, MediaPosition, SeekDirection};
    use std::time::Duration;

    // playr has one pause key, so play and pause are the same toggle; each is
    // dropped when it asks for the state playr is already in.
    assert_eq!(action(E::Play, false), Some(Action::TogglePause));
    assert_eq!(action(E::Play, true), None);
    assert_eq!(action(E::Pause, true), Some(Action::TogglePause));
    assert_eq!(action(E::Pause, false), None);
    assert_eq!(action(E::Toggle, true), Some(Action::TogglePause));
    assert_eq!(action(E::Toggle, false), Some(Action::TogglePause));

    assert_eq!(action(E::Next, true), Some(Action::Next));
    assert_eq!(action(E::Previous, true), Some(Action::Prev));
    assert_eq!(action(E::Stop, true), Some(Action::Stop));
    assert_eq!(
        action(E::Seek(SeekDirection::Forward), true),
        Some(Action::SeekBy(10))
    );
    assert_eq!(
        action(
            E::SeekBy(SeekDirection::Backward, Duration::from_secs(30)),
            true
        ),
        Some(Action::SeekBy(-30))
    );
    assert_eq!(
        action(E::SetPosition(MediaPosition(Duration::from_secs(90))), true),
        Some(Action::SeekTo(Duration::from_secs(90)))
    );
    assert_eq!(
        action(E::SetVolume(0.5), true),
        Some(Action::SetVolume(0.5))
    );

    // Opening a URI and raising a window are not playr's to do.
    assert_eq!(action(E::OpenUri("http://example.com".into()), true), None);
    assert_eq!(action(E::Raise, true), None);
}

#[test]
fn columns_and_sort_name_columns() {
    use playr_core::columns::{Column, SortKey};
    assert_eq!(
        lib("columns title tempo"),
        Ok(Action::SetColumns(vec![Column::Title, Column::Tempo]))
    );
    assert_eq!(
        lib("columns title, loudness"),
        Ok(Action::SetColumns(vec![Column::Title, Column::Loudness]))
    );
    assert_eq!(
        lib("sort tempo desc, title"),
        Ok(Action::SetSort(vec![
            SortKey {
                column: Column::Tempo,
                descending: true
            },
            SortKey {
                column: Column::Title,
                descending: false
            },
        ]))
    );
    // Spaces separate keys as commas do; `desc` turns the key before it.
    for text in ["sort tempo desc title", "sort tempo desc,title"] {
        assert_eq!(lib(text), lib("sort tempo desc, title"), "{text}");
    }
    assert_eq!(
        lib("sort tempo title"),
        Ok(Action::SetSort(vec![
            SortKey {
                column: Column::Tempo,
                descending: false
            },
            SortKey {
                column: Column::Title,
                descending: false
            },
        ]))
    );
    assert!(lib("sort desc").unwrap_err().contains("unknown column"));
    assert_eq!(lib("sort off"), Ok(Action::SetSort(Vec::new())));
    assert!(lib("columns").is_err());
    assert!(lib("sort").is_err());
    assert!(lib("columns nope").unwrap_err().contains("unknown column"));
}

#[test]
fn views_step_forward_and_back_in_tab_order() {
    assert_eq!(parse("prev-view", Library), Ok(Action::PrevView));
    assert_eq!(default_key("backtab", Library), Some(Action::PrevView));
    for view in View::ALL {
        assert_eq!(view.next().prev(), view);
    }
    assert_eq!(Library.prev(), Sampler);
    assert_eq!(View::Queue.prev(), Library);
    assert_eq!(Sampler.prev(), Playlists);
}

#[test]
fn stop_takes_after_or_a_sleep_time_and_round_trips() {
    assert_eq!(lib("stop"), Ok(Action::Stop));
    assert_eq!(lib("stop after"), Ok(Action::StopAfter));
    assert_eq!(lib("stop in 30:00"), Ok(Action::StopIn(Some(secs(1800.0)))));
    assert_eq!(lib("stop in off"), Ok(Action::StopIn(None)));
    for bad in ["stop in", "stop after 1", "stop soon", "stop in x"] {
        assert!(lib(bad).is_err(), "{bad:?} parsed");
    }
    for action in [
        Action::StopAfter,
        Action::StopIn(Some(secs(1800.0))),
        Action::StopIn(None),
    ] {
        assert_eq!(lib(&line(&action, None)), Ok(action));
    }
}

#[test]
fn tape_commands_parse_in_any_view_and_round_trip() {
    use playr_app::tape::{Filter, Pos, TapeAction as T, VoiceSetting as V};
    let pct = Pos::Percent;
    for (text, action) in [
        ("tape load", T::Load(None)),
        ("tape load 3", T::Load(Some(3))),
        ("tape play", T::Play),
        ("tape stop", T::Stop),
        ("tape reset", T::Reset),
        ("tape save", T::Save),
        ("tape rec", T::Record),
        ("tape write off", T::Write(false)),
        ("tape feedback 0.85", T::Feedback(0.85)),
        ("tape wear 0.3", T::Wear(0.3)),
        (
            "tape window 0 50%",
            T::WriteWindow(Pos::Time(Duration::ZERO), pct(50.0)),
        ),
        ("tape 2 on", T::Voice(2, V::On(true))),
        ("tape 2 rate -0.5", T::Voice(2, V::Rate(-0.5))),
        (
            "tape 3 window 25% 75%",
            T::Voice(3, V::Window(pct(25.0), pct(75.0))),
        ),
        (
            "tape 1 window 0.5 1.25",
            T::Voice(
                1,
                V::Window(
                    Pos::Time(Duration::from_millis(500)),
                    Pos::Time(Duration::from_millis(1250)),
                ),
            ),
        ),
        ("tape 2 send 0.6", T::Voice(2, V::Send(0.6))),
        ("tape 2 wear 0.4", T::Voice(2, V::Wear(0.4))),
        ("tape 1 level 0.5", T::Voice(1, V::Level(0.5))),
        ("tape 1 pan -1", T::Voice(1, V::Pan(-1.0))),
        ("tape 1 fade 40", T::Voice(1, V::Fade(40.0))),
        ("tape thin 0.2", T::Thin(0.2)),
        ("tape 2 ping on", T::Voice(2, V::Ping(true))),
        ("tape 1 slew 500", T::Voice(1, V::Slew(500.0))),
        ("tape 3 drive 0.25", T::Voice(3, V::Drive(0.25))),
        ("tape 1 filter 0.4", T::Voice(1, V::Cutoff(0.4))),
        ("tape 1 filter hp", T::Voice(1, V::Filter(Filter::High))),
        ("tape 2 filter bp", T::Voice(2, V::Filter(Filter::Band))),
        ("tape 2 filter lp", T::Voice(2, V::Filter(Filter::Low))),
        ("tape 3 solo on", T::Voice(3, V::Solo(true))),
    ] {
        let action = Action::Tape(action);
        for view in [Library, Sampler] {
            assert_eq!(parse(text, view), Ok(action.clone()), "{text}");
        }
        assert_eq!(line(&action, None), text);
    }
    for (text, error) in [
        ("tape", "usage: :tape"),
        ("tape 4 on", "voices are 1 to 3"),
        ("tape 0 on", "voices are 1 to 3"),
        ("tape 1 rate 5", "rate is -4 to 4"),
        ("tape 1 rate nan", "rate is -4 to 4"),
        ("tape feedback 1.1", "feedback is 0 to 1"),
        ("tape 1 pan 2", "pan is -1 to 1"),
        ("tape 1 fade 2000", "fade is 0 to 1000"),
        ("tape 1 slew 20000", "slew is 0 to 10000"),
        ("tape 1 filter notch", "filter is 0 to 1"),
        ("tape thin 2", "thin is 0 to 1"),
        ("tape 1 ping maybe", "usage: :tape"),
        ("tape 3 wear -1", "wear is 0 to 1"),
        ("tape window 0 250%", "not a percentage: 250%"),
        ("tape load 9", "not a loop slot: 9"),
        ("tape 1 rate", "usage: :tape"),
        ("tape write maybe", "usage: :tape"),
    ] {
        let got = parse(text, Library).unwrap_err();
        assert!(got.contains(error), "{text}: {got}");
    }
}
