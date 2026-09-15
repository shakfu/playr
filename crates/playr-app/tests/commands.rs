//! `:` command parsing, completion and history, and the key map.

use std::time::Duration;

use playr_app::action::{Action, Key, Keymap};
use playr_app::command::{completions, line, parse, CommandLine, History, COMMANDS, HISTORY_LEN};
use playr_app::View::{self, Library, Playlists, Selection};
use playr_core::audio::Mode;

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
    assert_eq!(lib("stop now"), Err(":stop takes no arguments".into()));
}

#[test]
fn view_commands_work_only_in_their_view() {
    for (view, line, action) in [
        (Library, "toggle", Action::Add),
        (Library, "clear-search", Action::ClearSearch),
        (Selection, "remove", Action::Remove),
        (Selection, "move +2", Action::MoveTrack(2)),
        (Selection, "move -1", Action::MoveTrack(-1)),
        (Selection, "clear", Action::ClearSelection),
        (Playlists, "add", Action::Add),
        (Playlists, "delete", Action::DeletePlaylist),
        (Playlists, "rename", Action::StartRename),
        (Playlists, "rename  dawn ", Action::RenameTo("dawn".into())),
    ] {
        assert_eq!(parse(line, view), Ok(action), "{line:?}");
        for other in [Library, Selection, Playlists, View::Sampler]
            .into_iter()
            .filter(|v| *v != view)
        {
            let name = line.split_whitespace().next().unwrap();
            let there = format!("{view:?}").to_lowercase();
            assert_eq!(
                parse(line, other),
                Err(format!(":{name} works in the {there} view")),
                "{line:?} in {other:?}"
            );
        }
    }
    // A prefix that matches nothing here names the view it would work in.
    assert_eq!(
        parse("rem", Library),
        Err(":remove works in the selection view".into())
    );
    // Typed in full, a command from another view is not taken as a prefix of one here.
    assert_eq!(
        parse("clear", Library),
        Err(":clear works in the selection view".into())
    );
    assert_eq!(parse("cl", Library), Ok(Action::ClearSearch));
    assert_eq!(parse("cl", Selection), Ok(Action::ClearSelection));
    assert_eq!(
        parse("move 2", Selection),
        Err("usage: :move +N | -N".into())
    );
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
        Err("ambiguous command s: search, save, scan, stop, seek, speed, slice".into())
    );
    // Only the commands usable here count: `de` is `delmarks` unless `delete` works too.
    assert_eq!(lib("de"), Ok(Action::ClearMarks));
    assert_eq!(
        parse("de", Playlists),
        Err("ambiguous command de: delmarks, delete".into())
    );
    assert_eq!(lib("frobnicate"), Err("unknown command: frobnicate".into()));
    assert!(lib("").is_err());
}

#[test]
fn every_command_parses_in_its_views_and_names_only_itself() {
    for c in COMMANDS {
        let views = match c.view {
            Some(v) => vec![v],
            None => vec![Library, Selection, Playlists, View::Sampler],
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
    assert_eq!(lib("volume 101"), Err("volume is 0 to 100".into()));
    assert!(lib("volume loud").is_err());
}

#[test]
fn speed_is_whole_semitones_and_a_sign_makes_it_relative() {
    assert_eq!(lib("speed 3"), Ok(Action::SetSpeed(3)));
    assert_eq!(lib("speed 0"), Ok(Action::SetSpeed(0)));
    assert_eq!(lib("speed +1"), Ok(Action::SpeedBy(1)));
    assert_eq!(lib("speed -12"), Ok(Action::SpeedBy(-12)));
    assert_eq!(lib("speed 13"), Err("speed is -12 to 12 semitones".into()));
    for bad in ["speed", "speed 1.5", "speed +x", "speed +25"] {
        assert!(lib(bad).is_err(), "{bad:?} parsed");
    }
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
    assert_eq!(
        lib("view queue"),
        Err("views: library, selection, playlists, sampler".into())
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
        completions("p", Library, &playlists),
        ["play", "playlist", "prune", "pause", "prev", "prev-mark"]
    );
    assert_eq!(completions("", Library, &playlists).len(), usable(Library));
    assert_eq!(completions("re", Selection, &playlists), ["remove"]);
    assert_eq!(completions("re", Playlists, &playlists), ["rename"]);
    assert!(completions("re", Library, &playlists).is_empty());
    assert_eq!(
        completions("mode r", Library, &playlists),
        ["mode repeat", "mode repeat-one"]
    );
    assert_eq!(
        completions("vi ", Library, &playlists),
        [
            "view library",
            "view selection",
            "view playlists",
            "view sampler"
        ]
    );
    // Playlist names match without regard to case.
    assert_eq!(
        completions("playlist la", Library, &playlists),
        ["playlist Late Night"]
    );
    assert_eq!(
        completions("rename ", Playlists, &playlists),
        ["rename Late Night", "rename dawn"]
    );
    assert!(completions("rename ", Library, &playlists).is_empty());
    assert!(completions("seek ", Library, &playlists).is_empty());
    assert!(completions("zz ", Library, &playlists).is_empty());
}

#[test]
fn tab_cycles_forward_and_back_and_typing_starts_afresh() {
    let mut line = CommandLine::default();
    line.push('p');
    line.complete(true, Library, &[]);
    assert_eq!(line.text, "play");
    line.complete(true, Library, &[]);
    assert_eq!(line.text, "playlist");
    for _ in 0..5 {
        line.complete(true, Library, &[]);
    }
    assert_eq!(line.text, "play", "the cycle does not wrap");
    line.complete(false, Library, &[]);
    assert_eq!(line.text, "prev-mark");

    // Typing ends the cycle, so Tab now completes the new text.
    line.text.clear();
    "playlist ".chars().for_each(|c| line.push(c));
    line.complete(true, Library, &["late".into()]);
    assert_eq!(line.text, "playlist late");

    let mut back = CommandLine::default();
    back.push('p');
    back.complete(false, Library, &[]);
    assert_eq!(
        back.text, "prev-mark",
        "shift-tab does not start from the last"
    );

    let mut none = CommandLine::default();
    none.push('z');
    none.complete(true, Library, &[]);
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
}

/// Every default binding is a command line that parses back to its action,
/// in every view the binding applies to.
#[test]
fn every_default_key_runs_a_command_usable_in_its_views() {
    for b in Keymap::default().bindings() {
        let action = b.action.clone().expect("no default binds a key to nothing");
        let views = match b.view {
            Some(v) => vec![v],
            None => vec![Library, Selection, Playlists, View::Sampler],
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
        lib("map d remove"),
        Err(":remove works in the selection view; use map selection d remove".into())
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
        "mode repeat-one",
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
            None => "Every view",
            Some(Library) => "Library",
            Some(Selection) => "Selection",
            Some(Playlists) => "Playlists",
            Some(View::Sampler) => "Sampler",
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
}
