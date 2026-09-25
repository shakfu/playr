//! `:` commands: parsing a command line into an [`Action`], and the prompt
//! that edits one, with Tab completion and a history recalled by the arrows.

use std::path::PathBuf;
use std::time::Duration;

use crate::action::{Action, Key, Keymap, Nudge, Slicing, Zoom};
use crate::{Display, Theme, View};
use playr_core::audio::Mode;
use playr_core::columns::{Column, SortKey};
use playr_core::gain::ReplayGain;
use playr_core::samples::MAX_SLICES;

/// A `:` command: its name, what may follow it, what it does, and the one
/// view it works in, if it is not every view.
pub struct Command {
    pub name: &'static str,
    pub args: &'static str,
    pub help: &'static str,
    pub view: Option<View>,
}

const fn any(name: &'static str, args: &'static str, help: &'static str) -> Command {
    Command {
        name,
        args,
        help,
        view: None,
    }
}

const fn only(view: View, name: &'static str, args: &'static str, help: &'static str) -> Command {
    Command {
        name,
        args,
        help,
        view: Some(view),
    }
}

use View::{Library, Playlists, Sampler, Selection};

/// Every `:` command, grouped by view. A command may be typed as any prefix
/// that names only it among those that work in the current view.
pub const COMMANDS: &[Command] = &[
    any("help", "", "list these commands"),
    any("keys", "", "list the keys for this view"),
    any("quit", "", "quit"),
    any("view", "VIEW", "library, selection, playlists or sampler"),
    any("next-view", "", "switch to the next view"),
    any("down", "[N]", "move the cursor down N rows, default 1"),
    any("up", "[N]", "move the cursor up N rows, default 1"),
    any("first", "", "move the cursor to the first row"),
    any("last", "", "move the cursor to the last row"),
    any("play", "", "play the list in view from the cursor"),
    any("search", "[QUERY]", "search the library; no query opens /"),
    any("playlist", "NAME", "play a saved playlist"),
    any("save", "[NAME]", "save the selection as a playlist"),
    any("scan", "DIR", "add a directory to the library"),
    any("rescan", "", "re-scan directories previously added"),
    any(
        "analyze",
        "[DIR]",
        "measure loudness, tempo and file health",
    ),
    any(
        "roots",
        "[add|rm DIR]",
        "list the directories the library covers",
    ),
    any("prune", "[DIR]", "remove tracks and marks of missing files"),
    any("info", "", "what analysis measured about this track"),
    any(
        "columns",
        "NAME...",
        "which columns a track list shows, in order",
    ),
    any(
        "sort",
        "KEY[ desc]... | off",
        "sort every track list by these columns",
    ),
    any("open", "PATH", "play a file or directory, and select it"),
    any("pause", "", "play or pause"),
    any("next", "", "next track"),
    any("prev", "", "previous track"),
    any("stop", "", "stop"),
    any("restart", "", "play from the range's start, or the track's"),
    any(
        "seek",
        "TIME | +TIME | -TIME",
        "seek to a time, or by one: 1:23, +10",
    ),
    any(
        "volume",
        "PERCENT | +N | -N",
        "set the volume, or change it: 60, +10",
    ),
    any(
        "speed",
        "N | =-N | +N | -N",
        "set varispeed in semitones, or change it",
    ),
    any(
        "mode",
        "MODE | + | -",
        "normal, shuffle, repeat, repeat-one, + or -",
    ),
    any(
        "replaygain",
        "SETTING",
        "level by loudness: off, track, album, auto",
    ),
    any("mark", "[TIME]", "mark the playing position, or a time"),
    any("unmark", "", "undo the last mark"),
    any("delmarks", "", "clear all marks in this track; asks y/n"),
    any("next-mark", "", "seek to the next mark"),
    any("prev-mark", "", "seek to the previous mark"),
    any(
        "slice",
        "region|marks|N|onsets [S]",
        "write samples from the region or the track",
    ),
    any(
        "map",
        "[VIEW] KEY COMMAND",
        "bind a key, in one view or in all",
    ),
    any("unmap", "[VIEW] KEY", "remove a key binding"),
    any("theme", "THEME", "system, light or dark colours"),
    only(Library, "toggle", "", "select or unselect the track"),
    only(Library, "clear-search", "", "show the whole library again"),
    only(
        Selection,
        "remove",
        "",
        "remove the track from the selection",
    ),
    only(Selection, "move", "+N | -N", "move the track N places"),
    only(Selection, "clear", "", "empty the selection; asks y/n"),
    only(
        Playlists,
        "add",
        "",
        "add the playlist's tracks to the selection",
    ),
    only(Playlists, "delete", "", "delete the playlist; asks y/n"),
    only(Playlists, "rename", "[NAME]", "rename the playlist"),
    only(
        Sampler,
        "zoom",
        "+ | - | all",
        "zoom in, out, or to the whole track",
    ),
    only(
        Sampler,
        "display",
        "[DISPLAY]",
        "envelope, db, braille or spectrogram",
    ),
    only(
        Sampler,
        "nudge",
        "+N | -N | +N% | -N%",
        "move N columns, or N% of the view",
    ),
    only(
        Sampler,
        "snap",
        "[on|off]",
        "snap moves and marks to zero crossings",
    ),
    only(
        Sampler,
        "fit",
        "[on|off]",
        "zoom to the range and keep it centred",
    ),
    only(Sampler, "in", "", "start the range at the playhead"),
    only(Sampler, "out", "", "end the range at the playhead"),
    only(
        Sampler,
        "range",
        "[START END]",
        "set the range to slice, or clear it",
    ),
    only(Sampler, "loop", "[on|off]", "play the range over and over"),
    only(Sampler, "cursor", "TIME|+N|-N|N%|off", "move the cursor"),
    only(Sampler, "pick", "next|prev", "move the cursor to a mark"),
    only(
        Sampler,
        "nudge-mark",
        "+N|-N|N%",
        "move the mark under the cursor",
    ),
    only(Sampler, "move-mark", "TIME", "move it to a time"),
    only(Sampler, "snap-mark", "", "move it to the nearest rise"),
    only(Sampler, "del-mark", "", "remove it"),
    only(
        Sampler,
        "audition",
        "",
        "play the range, slice or region once",
    ),
    only(
        Sampler,
        "edge",
        "start|end | +N | -N | +N%",
        "pick a range end, or move it N columns",
    ),
    only(Sampler, "write", "", "write the slices :slice planned"),
    only(
        Sampler,
        "discard",
        "",
        "discard planned slices, else the range",
    ),
];

const MODES: &[(&str, Mode)] = &Mode::NAMES;

const THEMES: &[(&str, Theme)] = &Theme::NAMES;

const REPLAYGAINS: &[(&str, ReplayGain)] = &ReplayGain::NAMES;

const VIEWS: &[(&str, View)] = &[
    ("library", Library),
    ("selection", Selection),
    ("playlists", Playlists),
    ("sampler", Sampler),
];

/// The name of `view` as commands spell it.
pub fn view_name(view: View) -> &'static str {
    VIEWS.iter().find(|v| v.1 == view).expect("every view").0
}

/// The one name among `names` that is `word` or starts with it.
///
/// An exact match wins over longer names it prefixes, so `repeat` is not
/// ambiguous with `repeat-one`.
fn resolve<'a>(
    word: &str,
    names: impl Iterator<Item = &'a str> + Clone,
    what: &str,
) -> Result<&'a str, String> {
    if let Some(exact) = names.clone().find(|n| *n == word) {
        return Ok(exact);
    }
    let matches: Vec<&str> = names.filter(|n| n.starts_with(word)).collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("unknown {what}: {word}")),
        many => Err(format!("ambiguous {what} {word}: {}", many.join(", "))),
    }
}

/// The commands that work in `view`, or in every view when `view` is `None`.
fn usable(view: Option<View>) -> impl Iterator<Item = &'static Command> + Clone {
    COMMANDS
        .iter()
        .filter(move |c| c.view.is_none_or(|v| Some(v) == view))
}

/// The command `word` names in `view`.
///
/// A command from another view is an error naming that view, whether typed in
/// full or as a prefix that matches nothing here.
fn resolve_command(word: &str, view: Option<View>) -> Result<&'static Command, String> {
    let elsewhere = |c: &Command| {
        let there = c.view.expect("usable in every view");
        format!(":{} works in the {} view", c.name, view_name(there))
    };
    if let Some(c) = COMMANDS.iter().find(|c| c.name == word) {
        return if usable(view).any(|u| u.name == c.name) {
            Ok(c)
        } else {
            Err(elsewhere(c))
        };
    }
    let find = |name: &str| COMMANDS.iter().find(|c| c.name == name).expect("resolved");
    match resolve(word, usable(view).map(|c| c.name), "command") {
        Ok(name) => Ok(find(name)),
        Err(e) if e.starts_with("unknown") => {
            match resolve(word, COMMANDS.iter().map(|c| c.name), "command") {
                Ok(name) => Err(elsewhere(find(name))),
                Err(_) => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

/// A number without a fractional part when it has none: `5`, `2.5`.
fn number(n: f64) -> String {
    let rounded = (n * 1000.0).round() / 1000.0;
    format!("{rounded}")
}

/// The command line that performs `action` in `view`: the inverse of [`parse`].
pub fn line(action: &Action, view: Option<View>) -> String {
    use Action::*;
    let time = |d: &Duration| number(d.as_secs_f64());
    match action {
        Quit => "quit".into(),
        Help => "keys".into(),
        CommandHelp => "help".into(),
        ShowView(v) => format!("view {}", view_name(*v)),
        NextView => "next-view".into(),
        Cursor(1) => "down".into(),
        Cursor(-1) => "up".into(),
        Cursor(n) if *n > 0 => format!("down {n}"),
        Cursor(n) => format!("up {}", -n),
        CursorFirst => "first".into(),
        CursorLast => "last".into(),
        StartSearch => "search".into(),
        Search(q) => format!("search {q}"),
        ClearSearch => "clear-search".into(),
        StartCommand => "command".into(),
        Activate => "play".into(),
        Add if view == Some(Library) => "toggle".into(),
        Add => "add".into(),
        Remove => "remove".into(),
        MoveTrack(n) => format!("move {n:+}"),
        ClearSelection => "clear".into(),
        StartSave => "save".into(),
        SaveAs(name) => format!("save {name}"),
        DeletePlaylist => "delete".into(),
        StartRename => "rename".into(),
        RenameTo(name) => format!("rename {name}"),
        PlayPlaylist(name) => format!("playlist {name}"),
        Scan(dir) => format!("scan {}", dir.display()),
        Rescan => "rescan".into(),
        Analyze(None) => "analyze".into(),
        Analyze(Some(dir)) => format!("analyze {}", dir.display()),
        ShowRoots => "roots".into(),
        ShowInfo => "info".into(),
        ForgetRoot(dir) => format!("roots rm {}", dir.display()),
        Prune(Some(dir)) => format!("prune {}", dir.display()),
        Prune(None) => "prune".into(),
        Open(paths) => {
            let paths: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
            format!("open {}", paths.join(" "))
        }
        TogglePause => "pause".into(),
        Next => "next".into(),
        Prev => "prev".into(),
        Stop => "stop".into(),
        Restart => "restart".into(),
        SeekBy(n) => format!("seek {n:+}"),
        SeekTo(d) => format!("seek {}", time(d)),
        VolumeBy(v) => format!(
            "volume {}{}",
            if *v < 0.0 { "-" } else { "+" },
            number(f64::from(v.abs()) * 100.0)
        ),
        SetVolume(v) => format!("volume {}", number(f64::from(*v) * 100.0)),
        SpeedBy(n) => format!("speed {n:+}"),
        SetSpeed(n) if *n < 0 => format!("speed ={n}"),
        SetSpeed(n) => format!("speed {n}"),
        CycleMode(true) => "mode +".into(),
        CycleMode(false) => "mode -".into(),
        SetMode(m) => format!(
            "mode {}",
            MODES.iter().find(|x| x.1 == *m).expect("every mode").0
        ),
        SetReplayGain(r) => format!("replaygain {}", r.name()),
        Mark => "mark".into(),
        MarkAt(d) => format!("mark {}", time(d)),
        UndoMark => "unmark".into(),
        ClearMarks => "delmarks".into(),
        NextMark => "next-mark".into(),
        PrevMark => "prev-mark".into(),
        Zoom(crate::action::Zoom::In) => "zoom +".into(),
        Zoom(crate::action::Zoom::Out) => "zoom -".into(),
        Zoom(crate::action::Zoom::All) => "zoom all".into(),
        Display(None) => "display".into(),
        Display(Some(d)) => format!("display {}", d.name()),
        Nudge(crate::action::Nudge::Columns(n)) => format!("nudge {n:+}"),
        Nudge(crate::action::Nudge::Percent(n)) => format!("nudge {n:+}%"),
        Snap(None) => "snap".into(),
        Snap(Some(true)) => "snap on".into(),
        Snap(Some(false)) => "snap off".into(),
        Fit(None) => "fit".into(),
        Fit(Some(true)) => "fit on".into(),
        Fit(Some(false)) => "fit off".into(),
        RangeIn => "in".into(),
        RangeOut => "out".into(),
        SetRange(None) => "range".into(),
        PickEdge(edge) => format!("edge {}", edge.name()),
        MoveEdge(crate::action::Nudge::Columns(n)) => format!("edge {n:+}"),
        MoveEdge(crate::action::Nudge::Percent(n)) => format!("edge {n:+}%"),
        Audition => "audition".into(),
        MoveCursor(crate::action::Nudge::Columns(n)) => format!("cursor {n:+}"),
        MoveCursor(crate::action::Nudge::Percent(n)) => format!("cursor {n:+}%"),
        SetCursor(None) => "cursor off".into(),
        SetCursor(Some(d)) => format!("cursor {}", time(d)),
        PickMark(true) => "pick next".into(),
        PickMark(false) => "pick prev".into(),
        MoveMark(crate::action::Nudge::Columns(n)) => format!("nudge-mark {n:+}"),
        MoveMark(crate::action::Nudge::Percent(n)) => format!("nudge-mark {n:+}%"),
        MoveMarkTo(d) => format!("move-mark {}", time(d)),
        SnapMark => "snap-mark".into(),
        DeleteMark => "del-mark".into(),
        Loop(None) => "loop".into(),
        Loop(Some(true)) => "loop on".into(),
        Loop(Some(false)) => "loop off".into(),
        SetRange(Some((a, b))) => format!("range {} {}", time(a), time(b)),
        WriteSlices => "write".into(),
        DiscardSlices => "discard".into(),
        Theme(t) => format!("theme {}", t.name()),
        SetColumns(columns) => {
            let names: Vec<&str> = columns.iter().map(|c| c.name()).collect();
            format!("columns {}", names.join(" "))
        }
        SetSort(keys) if keys.is_empty() => "sort off".into(),
        SetSort(keys) => {
            let names: Vec<String> = keys.iter().map(|k| k.text()).collect();
            format!("sort {}", names.join(", "))
        }
        Slice(Slicing::Region) => "slice region".into(),
        Slice(Slicing::Marks) => "slice marks".into(),
        Slice(Slicing::Equal(n)) => format!("slice {n}"),
        Slice(Slicing::Onsets(None)) => "slice onsets".into(),
        Slice(Slicing::Onsets(Some(s))) => format!("slice onsets {}", number(f64::from(*s))),
        Map { view, key, action } => {
            let target = match action {
                Some(a) => line(a, *view),
                None => "nop".into(),
            };
            match view {
                Some(v) => format!("map {} {key} {target}", view_name(*v)),
                None => format!("map {key} {target}"),
            }
        }
        Unmap { view: Some(v), key } => format!("unmap {} {key}", view_name(*v)),
        Unmap { view: None, key } => format!("unmap {key}"),
    }
}

/// The value named by `word`, or its prefix, in `table`, ignoring case. An
/// empty or unknown word is an error listing the choices.
fn choose<T: Copy>(word: &str, table: &[(&str, T)], what: &str) -> Result<T, String> {
    let names = || table.iter().map(|t| t.0);
    // Short enough to fit beside the indicators on an 80-column bottom line.
    let listed = || format!("{what}s: {}", names().collect::<Vec<_>>().join(", "));
    if word.is_empty() {
        return Err(listed());
    }
    match resolve(&word.to_ascii_lowercase(), names(), what) {
        Ok(name) => Ok(table.iter().find(|t| t.0 == name).expect("resolved").1),
        Err(e) if e.starts_with("unknown") => Err(listed()),
        Err(e) => Err(e),
    }
}

/// Parses `+N`, `-N`, `+N%` or `-N%`, a sign optional, as a nudge.
fn nudge(text: &str) -> Option<Nudge> {
    let (text, percent) = match text.strip_suffix('%') {
        Some(text) => (text, true),
        None => (text, false),
    };
    let (sign, digits) = signed(text).unwrap_or((1.0, text));
    let n = digits.parse::<i64>().ok().filter(|&n| n > 0)? * sign as i64;
    Some(if percent {
        Nudge::Percent(n)
    } else {
        Nudge::Columns(n)
    })
}

/// Parses `90`, `1:23`, `1:02:03` or `83.5` as a time into a track.
fn parse_time(text: &str) -> Result<Duration, String> {
    let bad = || format!("not a time: {text} (try 1:23 or 90)");
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() > 3 || parts.iter().any(|p| p.is_empty()) {
        return Err(bad());
    }
    let (whole, last) = parts.split_at(parts.len() - 1);
    let mut seconds: f64 = last[0].parse().map_err(|_| bad())?;
    for (i, part) in whole.iter().rev().enumerate() {
        let n: u64 = part.parse().map_err(|_| bad())?;
        seconds += n as f64 * 60f64.powi(i as i32 + 1);
    }
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(bad());
    }
    Ok(Duration::from_secs_f64(seconds))
}

/// `+rest` or `-rest` as a sign and the rest, or `None` for an unsigned value.
fn signed(text: &str) -> Option<(f64, &str)> {
    match text.as_bytes().first() {
        Some(b'+') => Some((1.0, &text[1..])),
        Some(b'-') => Some((-1.0, &text[1..])),
        _ => None,
    }
}

/// The path `text` names, unquoted, with a leading `~` as the home directory.
/// A relative path stays relative, to the directory playr started in.
fn path(text: &str) -> PathBuf {
    let text = unquote(text);
    let home = || std::env::home_dir().unwrap_or_default();
    match text.strip_prefix('~') {
        Some("") => home(),
        Some(rest) if rest.starts_with('/') => home().join(&rest[1..]),
        _ => PathBuf::from(text),
    }
}

/// `text` without one pair of surrounding double quotes.
fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text)
}

/// Parses a `:` command line, without the colon, typed in `view`.
///
/// A leading `+` or `-` makes a number relative, for every command that takes one.
pub fn parse(line: &str, view: View) -> Result<Action, String> {
    parse_in(line, Some(view))
}

/// What a key bound to `target` in `view` does: `nop` is nothing, `command`
/// opens the prompt, and anything else is a command usable there.
pub fn key_target(target: &str, view: Option<View>) -> Result<Option<Action>, String> {
    match target.trim() {
        "" => Err("no command".into()),
        "nop" => Ok(None),
        // Opening the prompt only makes sense as a key.
        "command" => Ok(Some(Action::StartCommand)),
        target => match parse_in(target, view)? {
            Action::Map { .. } | Action::Unmap { .. } => {
                Err("a key cannot run :map or :unmap".into())
            }
            action => Ok(Some(action)),
        },
    }
}

/// The one view `target` works in, when it does not work in every view.
pub fn only_view(target: &str) -> Option<View> {
    if parse_in(target, None).is_ok() {
        return None;
    }
    VIEWS
        .iter()
        .map(|v| v.1)
        .find(|v| parse_in(target, Some(*v)).is_ok())
}

/// The playback mode named by `name` or a unique prefix of it.
pub fn mode_named(name: &str) -> Result<Mode, String> {
    choose(name, MODES, "mode")
}

/// The view named by `name`, in full.
pub fn view_named(name: &str) -> Option<View> {
    VIEWS.iter().find(|v| v.0 == name).map(|v| v.1)
}

/// The first word of `text` and the rest, trimmed.
fn first_word(text: &str) -> (&str, &str) {
    match text.split_once(char::is_whitespace) {
        Some((word, rest)) => (word, rest.trim()),
        None => (text, ""),
    }
}

/// Parses `[VIEW] KEY [COMMAND]` after `map` or `unmap`.
fn binding(rest: &str) -> Result<(Option<View>, Key, &str), String> {
    let (first, after) = first_word(rest);
    let (view, key, after) = match VIEWS.iter().find(|v| v.0 == first) {
        Some(&(_, view)) if !after.is_empty() => {
            let (key, after) = first_word(after);
            (Some(view), key, after)
        }
        _ => (None, first, after),
    };
    Ok((view, Key::parse(key)?, after))
}

/// Where a binding applies, for messages: `in the library view` or `for all views`.
pub fn scope(view: Option<View>) -> String {
    match view {
        Some(v) => format!("in the {} view", view_name(v)),
        None => "for all views".into(),
    }
}

/// `line` parsed in `view`, or with only every-view commands when `None`.
fn parse_in(line: &str, view: Option<View>) -> Result<Action, String> {
    let line = line.trim();
    let (word, rest) = match line.split_once(char::is_whitespace) {
        Some((word, rest)) => (word, rest.trim()),
        None => (line, ""),
    };
    if line.is_empty() {
        return Err("no command".into());
    }
    // `:sync` is an alias, so help and completion list only `:rescan`.
    let word = if word == "sync" { "rescan" } else { word };
    let command = resolve_command(word, view)?;
    let name = command.name;
    let usage = || format!("usage: :{} {}", name, command.args);
    let nothing = |action: Action| {
        if rest.is_empty() {
            Ok(action)
        } else {
            Err(format!(":{name} takes no arguments"))
        }
    };
    let rows = |sign: i64| match rest {
        "" => Ok(Action::Cursor(sign)),
        n => match n.parse::<i64>() {
            Ok(n) if n > 0 => Ok(Action::Cursor(sign * n)),
            _ => Err(format!("not a number of rows: {n}")),
        },
    };

    match name {
        "help" => nothing(Action::CommandHelp),
        "keys" => nothing(Action::Help),
        "quit" => nothing(Action::Quit),
        "view" => choose(rest, VIEWS, "view").map(Action::ShowView),
        "next-view" => nothing(Action::NextView),
        "down" => rows(1),
        "up" => rows(-1),
        "first" => nothing(Action::CursorFirst),
        "last" => nothing(Action::CursorLast),
        "play" => nothing(Action::Activate),
        "search" if rest.is_empty() => Ok(Action::StartSearch),
        "search" => Ok(Action::Search(rest.to_string())),
        "playlist" if rest.is_empty() => Err(usage()),
        "playlist" => Ok(Action::PlayPlaylist(unquote(rest).to_string())),
        "scan" | "open" if rest.is_empty() => Err(usage()),
        "scan" => Ok(Action::Scan(path(rest))),
        "rescan" => nothing(Action::Rescan),
        "analyze" if rest.is_empty() => Ok(Action::Analyze(None)),
        "analyze" => Ok(Action::Analyze(Some(path(rest)))),
        // `:roots add DIR` is `:scan DIR`: recording a root without scanning
        // it would leave a root the library holds nothing for.
        "roots" if rest.is_empty() => Ok(Action::ShowRoots),
        "roots" => match rest.split_once(char::is_whitespace) {
            Some(("add", dir)) if !dir.trim().is_empty() => Ok(Action::Scan(path(dir.trim()))),
            Some(("rm", dir)) if !dir.trim().is_empty() => Ok(Action::ForgetRoot(path(dir.trim()))),
            _ => Err(usage()),
        },
        "info" => nothing(Action::ShowInfo),
        "columns" if rest.is_empty() => Err(usage()),
        "columns" => {
            let names = rest.split([',', ' ']).filter(|n| !n.trim().is_empty());
            names
                .map(|n| Column::named(n).ok_or_else(|| unknown_column(n)))
                .collect::<Result<Vec<_>, _>>()
                .map(Action::SetColumns)
        }
        "sort" if rest.is_empty() => Err(usage()),
        "sort" if rest == "off" => Ok(Action::SetSort(Vec::new())),
        "sort" => rest
            .split(',')
            .filter(|k| !k.trim().is_empty())
            .map(|k| SortKey::named(k).ok_or_else(|| unknown_column(k)))
            .collect::<Result<Vec<_>, _>>()
            .map(Action::SetSort),
        "prune" if rest.is_empty() => Ok(Action::Prune(None)),
        "prune" => Ok(Action::Prune(Some(path(rest)))),
        "open" => Ok(Action::Open(vec![path(rest)])),
        "save" if rest.is_empty() => Ok(Action::StartSave),
        "save" => Ok(Action::SaveAs(unquote(rest).to_string())),
        "pause" => nothing(Action::TogglePause),
        "next" => nothing(Action::Next),
        "prev" => nothing(Action::Prev),
        "stop" => nothing(Action::Stop),
        "restart" => nothing(Action::Restart),
        "seek" => match signed(rest) {
            _ if rest.is_empty() => Err(usage()),
            Some((sign, time)) => Ok(Action::SeekBy(
                (sign * parse_time(time)?.as_secs_f64()).round() as i64,
            )),
            None => Ok(Action::SeekTo(parse_time(rest)?)),
        },
        "volume" => {
            let number = |t: &str| {
                t.parse::<f32>()
                    .map_err(|_| format!("not a volume: {rest}"))
            };
            match signed(rest) {
                _ if rest.is_empty() => Err(usage()),
                Some((sign, n)) => Ok(Action::VolumeBy(sign as f32 * number(n)? / 100.0)),
                None => match number(rest)? {
                    v if (0.0..=100.0).contains(&v) => Ok(Action::SetVolume(v / 100.0)),
                    _ => Err("volume is 0 to 100".into()),
                },
            }
        }
        "speed" => {
            let semitones = |t: &str| {
                t.parse::<u32>()
                    .ok()
                    .filter(|n| *n <= 24)
                    .map(|n| n as i32)
                    .ok_or_else(|| format!("not a number of semitones: {rest}"))
            };
            match signed(rest) {
                _ if rest.is_empty() => Err(usage()),
                Some((sign, n)) => Ok(Action::SpeedBy(sign as i32 * semitones(n)?)),
                None => {
                    // A sign alone means relative, so `=` sets a negative speed: `=-3`.
                    let (sign, n) = match rest.strip_prefix('=') {
                        Some(t) => signed(t).unwrap_or((1.0, t)),
                        None => (1.0, rest),
                    };
                    match sign as i32 * semitones(n)? {
                        n if n.abs() <= 12 => Ok(Action::SetSpeed(n)),
                        _ => Err("speed is -12 to 12 semitones".into()),
                    }
                }
            }
        }
        "mode" => match rest {
            "+" => Ok(Action::CycleMode(true)),
            "-" => Ok(Action::CycleMode(false)),
            _ => {
                let typed = rest.split_whitespace().collect::<Vec<_>>().join("-");
                choose(&typed, MODES, "mode").map(Action::SetMode)
            }
        },
        "replaygain" => choose(rest, REPLAYGAINS, "replaygain").map(Action::SetReplayGain),
        "mark" if rest.is_empty() => Ok(Action::Mark),
        "mark" => Ok(Action::MarkAt(parse_time(rest)?)),
        "unmark" => nothing(Action::UndoMark),
        "delmarks" => nothing(Action::ClearMarks),
        "next-mark" => nothing(Action::NextMark),
        "prev-mark" => nothing(Action::PrevMark),
        "zoom" => match rest {
            "+" => Ok(Action::Zoom(Zoom::In)),
            "-" => Ok(Action::Zoom(Zoom::Out)),
            "all" => Ok(Action::Zoom(Zoom::All)),
            _ => Err(usage()),
        },
        "display" => match rest {
            "" => Ok(Action::Display(None)),
            "envelope" => Ok(Action::Display(Some(Display::Envelope))),
            "db" => Ok(Action::Display(Some(Display::Decibels))),
            "braille" => Ok(Action::Display(Some(Display::Braille))),
            "spectrogram" => Ok(Action::Display(Some(Display::Spectrogram))),
            _ => Err(usage()),
        },
        "theme" => choose(rest, THEMES, "theme").map(Action::Theme),
        "nudge" => nudge(rest).map(Action::Nudge).ok_or_else(usage),
        "edge" => match rest {
            "start" => Ok(Action::PickEdge(crate::sampler::Edge::Start)),
            "end" => Ok(Action::PickEdge(crate::sampler::Edge::End)),
            _ => nudge(rest).map(Action::MoveEdge).ok_or_else(usage),
        },
        "snap" => match rest {
            "" => Ok(Action::Snap(None)),
            "on" => Ok(Action::Snap(Some(true))),
            "off" => Ok(Action::Snap(Some(false))),
            _ => Err(usage()),
        },
        "fit" => match rest {
            "" => Ok(Action::Fit(None)),
            "on" => Ok(Action::Fit(Some(true))),
            "off" => Ok(Action::Fit(Some(false))),
            _ => Err(usage()),
        },
        "loop" => match rest {
            "" => Ok(Action::Loop(None)),
            "on" => Ok(Action::Loop(Some(true))),
            "off" => Ok(Action::Loop(Some(false))),
            _ => Err(usage()),
        },
        "audition" => nothing(Action::Audition),
        "cursor" if rest == "off" => Ok(Action::SetCursor(None)),
        "cursor" if rest.starts_with(['+', '-']) => {
            nudge(rest).map(Action::MoveCursor).ok_or_else(usage)
        }
        "cursor" if rest.is_empty() => Err(usage()),
        "cursor" => Ok(Action::SetCursor(Some(parse_time(rest)?))),
        "pick" => match rest {
            "next" => Ok(Action::PickMark(true)),
            "prev" => Ok(Action::PickMark(false)),
            _ => Err(usage()),
        },
        "nudge-mark" => nudge(rest).map(Action::MoveMark).ok_or_else(usage),
        "move-mark" if rest.is_empty() => Err(usage()),
        "move-mark" => Ok(Action::MoveMarkTo(parse_time(rest)?)),
        "snap-mark" => nothing(Action::SnapMark),
        "del-mark" => nothing(Action::DeleteMark),
        "in" => nothing(Action::RangeIn),
        "out" => nothing(Action::RangeOut),
        "range" if rest.is_empty() => Ok(Action::SetRange(None)),
        "range" => match rest.split_whitespace().collect::<Vec<_>>()[..] {
            [a, b] => {
                let (a, b) = (parse_time(a)?, parse_time(b)?);
                if a == b {
                    return Err("the range is empty".into());
                }
                Ok(Action::SetRange(Some((a.min(b), a.max(b)))))
            }
            _ => Err(usage()),
        },
        "write" => nothing(Action::WriteSlices),
        "discard" => nothing(Action::DiscardSlices),
        "slice" => match first_word(rest) {
            ("region", "") => Ok(Action::Slice(Slicing::Region)),
            ("marks", "") => Ok(Action::Slice(Slicing::Marks)),
            ("onsets", "") => Ok(Action::Slice(Slicing::Onsets(None))),
            ("onsets", s) => match s.parse::<f32>() {
                Ok(s) if (0.0..=1.0).contains(&s) => Ok(Action::Slice(Slicing::Onsets(Some(s)))),
                _ => Err("onset sensitivity is 0 to 1".into()),
            },
            (n, "") if n.parse::<usize>().is_ok() => match n.parse::<usize>() {
                Ok(n) if (2..=MAX_SLICES).contains(&n) => Ok(Action::Slice(Slicing::Equal(n))),
                _ => Err(format!("slices are 2 to {MAX_SLICES}")),
            },
            _ => Err(usage()),
        },
        "map" => {
            let (view, key, target) = binding(rest)?;
            if target.is_empty() {
                return Err(usage());
            }
            let action = key_target(target, view).map_err(|e| {
                match only_view(target).filter(|v| view != Some(*v)) {
                    Some(v) => format!("{e}; use map {} {key} {target}", view_name(v)),
                    None => e,
                }
            })?;
            Ok(Action::Map {
                view,
                key,
                action: action.map(Box::new),
            })
        }
        "unmap" => match binding(rest)? {
            (view, key, "") => Ok(Action::Unmap { view, key }),
            _ => Err(usage()),
        },
        "toggle" | "add" => nothing(Action::Add),
        "clear-search" => nothing(Action::ClearSearch),
        "remove" => nothing(Action::Remove),
        "move" => match signed(rest).map(|(sign, n)| (sign as i64, n.parse::<i64>())) {
            Some((sign, Ok(n))) if n > 0 => Ok(Action::MoveTrack(sign * n)),
            _ => Err(usage()),
        },
        "clear" => nothing(Action::ClearSelection),
        "delete" => nothing(Action::DeletePlaylist),
        "rename" if rest.is_empty() => Ok(Action::StartRename),
        "rename" => Ok(Action::RenameTo(unquote(rest).to_string())),
        _ => unreachable!("command {name} has no parser"),
    }
}

/// What Tab can complete `text` to in `view`, as whole command lines.
///
/// The first word completes to the names of commands that work in `view`.
/// After `edge`, `fit`, `loop`, `mode`, `snap`, `theme` or `view` the argument completes to its choices, and after
/// `playlist` or `rename` to the names in `playlists`.
pub fn completions(text: &str, view: View, playlists: &[String]) -> Vec<String> {
    let Some((word, rest)) = text.split_once(' ') else {
        return usable(Some(view))
            .filter(|c| c.name.starts_with(text))
            .map(|c| c.name.to_string())
            .collect();
    };
    let Ok(command) = resolve_command(word, Some(view)) else {
        return Vec::new();
    };
    let rest = rest.trim_start();
    let choices: Vec<String> = match command.name {
        "mode" => MODES.iter().map(|m| m.0.to_string()).collect(),
        "theme" => THEMES.iter().map(|t| t.0.to_string()).collect(),
        "columns" | "sort" => Column::NAMES.iter().map(|c| c.0.to_string()).collect(),
        "replaygain" => REPLAYGAINS.iter().map(|r| r.0.to_string()).collect(),
        "snap" | "fit" | "loop" => vec!["on".into(), "off".into()],
        "edge" => vec!["start".into(), "end".into()],
        "pick" => vec!["next".into(), "prev".into()],
        "view" => VIEWS.iter().map(|v| v.0.to_string()).collect(),
        "playlist" | "rename" => playlists.to_vec(),
        _ => Vec::new(),
    };
    let lower = rest.to_lowercase();
    choices
        .into_iter()
        .filter(|c| c.to_lowercase().starts_with(&lower))
        .map(|c| format!("{} {c}", command.name))
        .collect()
}

/// Most command lines kept in the history.
pub const HISTORY_LEN: usize = 100;

/// Command lines entered this session, oldest first.
#[derive(Debug, Default)]
pub struct History {
    lines: Vec<String>,
}

impl History {
    /// Records `line`, unless it is blank or repeats the line before it.
    pub fn push(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() || self.lines.last().is_some_and(|l| l == line) {
            return;
        }
        if self.lines.len() == HISTORY_LEN {
            self.lines.remove(0);
        }
        self.lines.push(line.to_string());
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}

/// A command line being typed after `:`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CommandLine {
    pub text: String,
    /// The completions Tab is cycling through, and which one is shown.
    tab: Option<(Vec<String>, usize)>,
    /// While recalling history: the text typed before the first Up, which
    /// recalled lines must start with, and the history index shown.
    recall: Option<(String, usize)>,
}

impl CommandLine {
    /// Replaces the text, as a frontend's text field edits it, ending any
    /// completion or recall in progress.
    pub fn replace(&mut self, text: &str) {
        self.text = text.to_string();
        self.settle();
    }

    pub fn push(&mut self, c: char) {
        self.text.push(c);
        self.settle();
    }

    /// Deletes the last character; false if there was none to delete.
    pub fn pop(&mut self) -> bool {
        self.settle();
        self.text.pop().is_some()
    }

    /// Ends Tab cycling and history recall, keeping the text shown.
    fn settle(&mut self) {
        self.tab = None;
        self.recall = None;
    }

    /// Shows the next completion, or the previous one when `forward` is false.
    pub fn complete(&mut self, forward: bool, view: View, playlists: &[String]) {
        self.recall = None;
        let (choices, at) = match self.tab.take() {
            Some((choices, at)) => {
                let n = choices.len();
                let at = if forward {
                    (at + 1) % n
                } else {
                    (at + n - 1) % n
                };
                (choices, at)
            }
            None => {
                let choices = completions(&self.text, view, playlists);
                if choices.is_empty() {
                    return;
                }
                let at = if forward { 0 } else { choices.len() - 1 };
                (choices, at)
            }
        };
        self.text = choices[at].clone();
        self.tab = Some((choices, at));
    }

    /// Shows an older history line starting with the typed text, or a newer
    /// one when `older` is false. Past the newest, the typed text returns.
    pub fn recall(&mut self, older: bool, history: &History) {
        self.tab = None;
        let lines = history.lines();
        let (draft, at) = self
            .recall
            .take()
            .unwrap_or_else(|| (self.text.clone(), lines.len()));
        let found = if older {
            lines[..at].iter().rposition(|l| l.starts_with(&draft))
        } else {
            lines
                .iter()
                .enumerate()
                .skip(at + 1)
                .find(|(_, l)| l.starts_with(&draft))
                .map(|(i, _)| i)
        };
        match found {
            Some(i) => {
                self.text = lines[i].clone();
                self.recall = Some((draft, i));
            }
            // Nothing older: stay on the oldest match shown.
            None if older => {
                if at < lines.len() {
                    self.recall = Some((draft, at));
                }
            }
            None => self.text = draft,
        }
    }
}

/// The keys that work in `view`: its own bindings, then those for every view
/// that it does not rebind. Keys that run the same command share a row, and a
/// key bound to nothing is left out. A heading row has no command.
pub fn key_rows(keys: &Keymap, view: View) -> Vec<(String, String)> {
    let rebound = |key| {
        keys.bindings()
            .iter()
            .any(|b| b.view == Some(view) && b.key == key)
    };
    let mut rows = Vec::new();
    for (scope, heading) in [
        (Some(view), format!("in the {} view", view_name(view))),
        (None, "in every view".to_string()),
    ] {
        let mut group: Vec<(String, String)> = Vec::new();
        let bindings = keys
            .bindings()
            .iter()
            .filter(|b| b.view == scope && (scope.is_some() || !rebound(b.key)));
        for b in bindings {
            let Some(action) = &b.action else {
                continue;
            };
            let command = format!(":{}", line(action, Some(view)));
            match group.iter_mut().find(|(_, c)| *c == command) {
                Some((k, _)) => *k = format!("{k} {}", b.key),
                None => group.push((b.key.to_string(), command)),
            }
        }
        if !group.is_empty() {
            rows.push((heading, String::new()));
            rows.extend(group);
        }
    }
    rows
}

/// Every command and what it does, grouped by the view it works in, those
/// for every view first. A heading row has no description.
pub fn command_rows() -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut group = None;
    for c in COMMANDS {
        if rows.is_empty() || c.view != group {
            group = c.view;
            let heading = c.view.map_or("in every view", view_name);
            rows.push((heading.to_string(), String::new()));
        }
        let usage = format!(":{} {}", c.name, c.args);
        rows.push((usage.trim_end().to_string(), c.help.to_string()));
    }
    rows
}

/// The words for a column name playr does not know, with the choices.
fn unknown_column(name: &str) -> String {
    let names: Vec<&str> = Column::NAMES.iter().map(|c| c.0).collect();
    format!(
        "unknown column {}; columns: {}",
        name.trim(),
        names.join(", ")
    )
}
