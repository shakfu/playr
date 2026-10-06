//! `:` commands: parsing a command line into an [`Action`], and the prompt
//! that edits one, with Tab completion and a history recalled by the arrows.

use playr_core::analysis::TempoFix;
use std::path::PathBuf;
use std::time::Duration;

use crate::action::{Action, Key, Keymap, Nudge, Slicing, Zoom};
use crate::{Display, Tab, Theme, View};
use playr_core::audio::eq::{Band, RANGE_DB};
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
    /// An extension's command: left out of the help and of completion until
    /// the extension is enabled in the settings.
    pub extension: bool,
}

const fn any(name: &'static str, args: &'static str, help: &'static str) -> Command {
    Command {
        name,
        args,
        help,
        view: None,
        extension: false,
    }
}

/// A command of an extension, which works in every view.
const fn extension(name: &'static str, args: &'static str, help: &'static str) -> Command {
    Command {
        name,
        args,
        help,
        view: None,
        extension: true,
    }
}

const fn only(view: View, name: &'static str, args: &'static str, help: &'static str) -> Command {
    Command {
        name,
        args,
        help,
        view: Some(view),
        extension: false,
    }
}

use View::{Library, Playlists, Sampler, Selection};

/// Every `:` command, grouped by view. A command may be typed as any prefix
/// that names only it among those that work in the current view.
pub const COMMANDS: &[Command] = &[
    any("help", "", "list these commands"),
    any("keys", "", "list the keys for this view"),
    any("quit", "", "quit"),
    any(
        "view",
        "VIEW | next | prev",
        "a view or window tab, or next or prev",
    ),
    any("down", "[N]", "move the cursor down N rows, default 1"),
    any("up", "[N]", "move the cursor up N rows, default 1"),
    any("first", "", "move the cursor to the first row"),
    any("last", "", "move the cursor to the last row"),
    any("play", "", "play the list in view from the cursor"),
    any(
        "enqueue",
        "[next | all]",
        "queue the row, first, or every row",
    ),
    any("search", "[QUERY]", "search the library; no query opens /"),
    any("playlist", "NAME", "play a saved playlist"),
    any(
        "save",
        "[NAME]",
        "save the selection or queue as a playlist",
    ),
    any(
        "save-search",
        "[NAME]",
        "keep the search shown, to run again",
    ),
    any("sql", "SELECT path ...", "list the tracks a query names"),
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
    any(
        "stop",
        "[after | in TIME | in off]",
        "now, after this track, or in TIME",
    ),
    any("restart", "", "play from the range's start, or the track's"),
    any(
        "seek",
        "TIME | +TIME | -TIME",
        "seek to a time, or by one: 1:23, +10",
    ),
    any(
        "volume",
        "PERCENT | +N | -N",
        "set the master, or change it: 60, +10",
    ),
    any(
        "mix",
        "[STRIP N|mute] | law | rec",
        "faders and mutes; :mix alone shows them",
    ),
    any(
        "speed",
        "N | =-N | +N | -N",
        "set varispeed in semitones, or change it",
    ),
    any(
        "eq",
        "BAND =N | +N | -N | flat",
        "bass, mid or treble, -12 to 12 dB",
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
    any(
        "slice-edges",
        "exact|zero|fade",
        "slice edges: exact, at zeros, or faded",
    ),
    extension(
        "convert",
        "FORMAT [EXPORT]",
        "last export, or EXPORT, via ConvertWithMoss",
    ),
    any("mark", "[TIME]", "mark the playing position, or a time"),
    any("mark-undo", "", "undo the last mark"),
    any("mark-clear", "", "clear all marks in this track; asks y/n"),
    any("mark-next", "", "next mark: seek, or select in the sampler"),
    any("mark-prev", "", "prev mark: seek, or select in the sampler"),
    any("undo", "", "undo the last edit to marks, range or plan"),
    any("redo", "", "put back the last edit undone"),
    any(
        "slice",
        "region|marks|N|...",
        "write samples; :slice alone lists all",
    ),
    any("loop", "off", "stop looping"),
    any(
        "bpm",
        "x2|/2|reset",
        "correct the playing track's tempo an octave",
    ),
    any(
        "tape",
        "load|play|stop|save|...",
        "the tape looper; :tape alone lists all",
    ),
    any(
        "dj",
        "a|b load|play|cue|sync|...",
        "the DJ decks; :dj alone lists all",
    ),
    any(
        "map",
        "[VIEW] KEY COMMAND",
        "bind a key, in one view or in all",
    ),
    any("unmap", "[VIEW] KEY", "remove a key binding"),
    any("theme", "THEME", "system, light or dark colours"),
    only(Library, "toggle", "", "select or unselect the track"),
    only(Library, "search-clear", "", "show the whole library again"),
    only(
        Selection,
        "remove",
        "",
        "remove the track from the selection",
    ),
    only(Selection, "move", "+N | -N", "move the track N places"),
    only(Selection, "clear", "", "empty the selection; asks y/n"),
    only(View::Queue, "remove", "", "take the track out of the queue"),
    only(View::Queue, "move", "+N | -N", "move the track N places"),
    only(
        View::Queue,
        "clear",
        "",
        "empty the queue, played too; asks y/n",
    ),
    only(View::Queue, "add", "", "add the track to the selection"),
    only(
        Playlists,
        "add",
        "",
        "add the playlist's tracks to the selection",
    ),
    only(Playlists, "delete", "", "delete the playlist; asks y/n"),
    only(Playlists, "edit", "", "edit the playlist in the selection"),
    only(Playlists, "rename", "[NAME]", "rename the playlist"),
    only(
        Sampler,
        "zoom",
        "+ | - | all | N",
        "zoom in, out, whole track, or to step N",
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
    only(
        Sampler,
        "in",
        "[TIME]",
        "start the range at the playhead or TIME",
    ),
    only(
        Sampler,
        "out",
        "[TIME]",
        "end the range at the playhead or TIME",
    ),
    only(
        Sampler,
        "range",
        "[START END]",
        "set the range to slice, or clear it",
    ),
    only(
        Sampler,
        "loop",
        "[on|off] | N [save|clear]",
        "loop the range or region, or recall or save",
    ),
    only(
        Sampler,
        "loops",
        "clear",
        "clear this track's loops; asks y/n",
    ),
    only(Sampler, "select", "TIME", "select the mark at a time"),
    only(
        Sampler,
        "select-slice",
        "TIME",
        "select the planned slice starting at a time",
    ),
    only(Sampler, "deselect", "", "select nothing"),
    only(
        Sampler,
        "move",
        "+N|-N|N%",
        "move the selected mark or range end",
    ),
    only(Sampler, "move-to", "TIME", "move the selection to a time"),
    only(
        Sampler,
        "onset",
        "",
        "move the selection to the nearest rise",
    ),
    only(
        Sampler,
        "remove",
        "",
        "remove the selected mark, or the range",
    ),
    only(
        Sampler,
        "audition",
        "[next|prev]",
        "play a slice, the range or region once",
    ),
    only(Sampler, "scrub", "TIME", "play a moment from a time"),
    only(Sampler, "edge", "start|end", "select a range end"),
    only(Sampler, "write", "", "write the slices :slice planned"),
    only(Sampler, "discard", "", "discard the planned slices"),
    only(
        Sampler,
        "mark-slices",
        "",
        "mark each planned slice's start",
    ),
];

const MODES: &[(&str, Mode)] = &Mode::NAMES;

const THEMES: &[(&str, Theme)] = &Theme::NAMES;

const REPLAYGAINS: &[(&str, ReplayGain)] = &ReplayGain::NAMES;
const EDGES: &[(&str, playr_core::samples::Edges)] = &playr_core::samples::Edges::NAMES;

const VIEWS: &[(&str, View)] = &[
    ("library", Library),
    ("queue", View::Queue),
    ("selection", Selection),
    ("playlists", Playlists),
    ("sampler", Sampler),
];

/// The window's tabs; `:view` reaches them as it does the views.
const TABS: &[(&str, Tab)] = &[("tape", Tab::Tape), ("dj", Tab::Dj), ("mix", Tab::Mix)];

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

/// Names that were renamed, and what each is now, with its argument where
/// the old name implied one. Bindings written for an old name keep working;
/// help and completion list only the new.
const ALIASES: &[(&str, &str)] = &[
    ("sync", "rescan"),
    ("next-view", "view next"),
    ("prev-view", "view prev"),
    ("unmark", "mark-undo"),
    ("delmarks", "mark-clear"),
    ("next-mark", "mark-next"),
    ("prev-mark", "mark-prev"),
    ("move-mark", "move-to"),
    ("mark-move", "move-to"),
    ("clear-search", "search-clear"),
    ("dequeue", "remove"),
    ("reorder", "move"),
    ("queue-clear", "clear"),
];

/// The commands that work in `view`, or in every view when `view` is `None`.
fn usable(view: Option<View>) -> impl Iterator<Item = &'static Command> + Clone {
    COMMANDS
        .iter()
        .filter(move |c| c.view.is_none_or(|v| Some(v) == view))
}

/// The names of the commands that work in `view`, each once.
fn usable_names(view: Option<View>) -> Vec<&'static str> {
    let mut seen = std::collections::HashSet::new();
    usable(view)
        .map(|c| c.name)
        .filter(|n| seen.insert(*n))
        .collect()
}

/// The command `name` in `view`: the view's own over an every-view one, as
/// `:loop` has both.
fn usable_named(name: &str, view: Option<View>) -> Option<&'static Command> {
    usable(view)
        .filter(|c| c.name == name)
        .max_by_key(|c| c.view.is_some())
}

/// The command `word` names in `view`.
///
/// A command from another view is an error naming that view, whether typed in
/// full or as a prefix that matches nothing here. Without `extensions`, an
/// extension's command is named only in full: no prefix finds it and no
/// error lists it, as the help does not.
fn resolve_command(
    word: &str,
    view: Option<View>,
    extensions: bool,
) -> Result<&'static Command, String> {
    let offered =
        |name: &&str| extensions || !COMMANDS.iter().any(|c| c.extension && c.name == *name);
    // A name may work in more than one view, as `:clear` does.
    let elsewhere = |c: &Command| {
        let there: Vec<&str> = (COMMANDS.iter().filter(|o| o.name == c.name))
            .map(|o| view_name(o.view.expect("usable in every view")))
            .collect();
        match there.as_slice() {
            [one] => format!(":{} works in the {one} view", c.name),
            [rest @ .., last] => {
                format!(
                    ":{} works in the {} and {last} views",
                    c.name,
                    rest.join(", ")
                )
            }
            [] => unreachable!("{} is a command", c.name),
        }
    };
    if let Some(c) = COMMANDS.iter().find(|c| c.name == word) {
        return usable_named(word, view).ok_or_else(|| elsewhere(c));
    }
    let find = |name: &str| COMMANDS.iter().find(|c| c.name == name).expect("resolved");
    match resolve(
        word,
        usable_names(view).into_iter().filter(offered),
        "command",
    ) {
        Ok(name) => Ok(usable_named(name, view).expect("resolved")),
        Err(e) if e.starts_with("unknown") => {
            let names = COMMANDS.iter().map(|c| c.name).filter(offered);
            let mut names: Vec<&str> = names.collect();
            names.sort_unstable();
            names.dedup();
            match resolve(word, names.into_iter(), "command") {
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
        ShowTab(t) => format!(
            "view {}",
            TABS.iter().find(|n| n.1 == *t).expect("every tab").0
        ),
        NextView => "view next".into(),
        PrevView => "view prev".into(),
        Cursor(1) => "down".into(),
        Cursor(-1) => "up".into(),
        Cursor(n) if *n > 0 => format!("down {n}"),
        Cursor(n) => format!("up {}", -n),
        CursorFirst => "first".into(),
        CursorLast => "last".into(),
        StartSearch => "search".into(),
        Search(q) => format!("search {q}"),
        ClearSearch => "search-clear".into(),
        StartCommand => "command".into(),
        Activate => "play".into(),
        Add if view == Some(Library) => "toggle".into(),
        Add => "add".into(),
        Enqueue(false) => "enqueue".into(),
        Enqueue(true) => "enqueue next".into(),
        EnqueueAll => "enqueue all".into(),
        ClearQueue => "clear".into(),
        Remove => "remove".into(),
        MoveTrack(n) => format!("move {n:+}"),
        ClearSelection => "clear".into(),
        StartSave => "save".into(),
        SaveAs(name) => format!("save {name}"),
        DeletePlaylist => "delete".into(),
        EditPlaylist => "edit".into(),
        StartSaveSearch => "save-search".into(),
        Sql(statement) => format!("sql {statement}"),
        SaveSearch(name) => format!("save-search {name}"),
        StartRename => "rename".into(),
        RenameTo(name) => format!("rename {name}"),
        PlayPlaylist(name) => format!("playlist {name}"),
        Scan(dir) => format!("scan {}", dir.display()),
        Rescan => "rescan".into(),
        Analyze(None) => "analyze".into(),
        FixTempo(fix) => format!(
            "bpm {}",
            match fix {
                TempoFix::Double => "x2",
                TempoFix::Halve => "/2",
                TempoFix::Reset => "reset",
            }
        ),
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
        StopAfter => "stop after".into(),
        StopIn(Some(d)) => format!("stop in {}", time(d)),
        StopIn(None) => "stop in off".into(),
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
        SetEq(band, db) => format!("eq {} ={}", band.name(), number(f64::from(*db))),
        EqBy(band, db) => format!(
            "eq {} {}{}",
            band.name(),
            if *db < 0.0 { "-" } else { "+" },
            number(f64::from(db.abs()))
        ),
        FlatEq => "eq flat".into(),
        CycleMode(true) => "mode +".into(),
        CycleMode(false) => "mode -".into(),
        SetMode(m) => format!(
            "mode {}",
            MODES.iter().find(|x| x.1 == *m).expect("every mode").0
        ),
        SetReplayGain(r) => format!("replaygain {}", r.name()),
        SetSliceEdges(e) => format!("slice-edges {}", e.name()),
        Mark => "mark".into(),
        MarkAt(d) => format!("mark {}", time(d)),
        UndoMark => "mark-undo".into(),
        Undo => "undo".into(),
        Redo => "redo".into(),
        ClearMarks => "mark-clear".into(),
        NextMark => "mark-next".into(),
        PrevMark => "mark-prev".into(),
        Zoom(crate::action::Zoom::In) => "zoom +".into(),
        Zoom(crate::action::Zoom::Out) => "zoom -".into(),
        Zoom(crate::action::Zoom::All) => "zoom all".into(),
        Zoom(crate::action::Zoom::To(n)) => format!("zoom {n}"),
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
        RangeIn(None) => "in".into(),
        RangeIn(Some(t)) => format!("in {}", time(t)),
        RangeOut(None) => "out".into(),
        RangeOut(Some(t)) => format!("out {}", time(t)),
        SetRange(None) => "range".into(),
        PickEdge(edge) => format!("edge {}", edge.name()),
        Audition => "audition".into(),
        AuditionSlice(true) => "audition next".into(),
        AuditionSlice(false) => "audition prev".into(),
        Scrub(d) => format!("scrub {}", time(d)),
        SelectMarkAt(d) => format!("select {}", time(d)),
        Deselect => "deselect".into(),
        MoveSelected(crate::action::Nudge::Columns(n)) => format!("move {n:+}"),
        MoveSelected(crate::action::Nudge::Percent(n)) => format!("move {n:+}%"),
        MoveSelectedTo(d) => format!("move-to {}", time(d)),
        SelectSliceAt(d) => format!("select-slice {}", time(d)),
        SnapSelected => "onset".into(),
        RemoveSelected => "remove".into(),
        Loop(None) => "loop".into(),
        Loop(Some(true)) => "loop on".into(),
        Loop(Some(false)) => "loop off".into(),
        LoopSlot(n, crate::action::SlotOp::Use) => format!("loop {n}"),
        LoopSlot(n, crate::action::SlotOp::Save) => format!("loop {n} save"),
        LoopSlot(n, crate::action::SlotOp::Clear) => format!("loop {n} clear"),
        ClearLoops => "loops clear".into(),
        SetRange(Some((a, b))) => format!("range {} {}", time(a), time(b)),
        WriteSlices => "write".into(),
        DiscardSlices => "discard".into(),
        MarkSlices => "mark-slices".into(),
        Tape(t) => format!("tape {}", tape_line(t)),
        Dj(d) => format!("dj {}", dj_line(d)),
        Mix(m) => mix_line(m),
        Convert(format, None) => format!("convert {format}"),
        Convert(format, Some(dir)) => format!("convert {format} {}", dir.display()),
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
        Slice(Slicing::Beats(n)) => format!("slice beats {}", number(f64::from(*n))),
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

/// What `:tape` takes, as its usage says it.
/// What `:slice` takes, as its usage says it. The help row is shorter, so
/// every row of `:help` fits 80 columns.
const SLICE_USAGE: &str = "usage: :slice region|marks|N|onsets [S]|beats [N]";

/// Beats a slice of `:slice beats` with no count: a bar of 4/4.
pub const DEFAULT_BEATS: f32 = 4.0;

/// Fewest beats in one slice of `:slice beats`: a 32nd note in 4/4.
pub const MIN_BEATS: f32 = 0.125;

/// Most beats in one slice of `:slice beats`: 16 bars of 4/4.
pub const MAX_BEATS: f32 = 64.0;

const TAPE_USAGE: &str = "usage: :tape load [N] | take | play | stop | reset | save | rec | \
     write on|off | feedback F | wear W | thin T | window A B | \
     V on|off | V rate R|window A B|level L|pan P|send S|wear W|fade MS|\
     ping on|off|slew MS|drive D|filter F|lp|hp|bp|solo on|off";

/// Parses what follows `:tape`. Voices count from 1; windows take a time or
/// a percentage of the loop at each end.
fn tape(rest: &str) -> Result<crate::tape::TapeAction, String> {
    use crate::tape::{Filter, TapeAction as T, VoiceSetting as V};
    let usage = || TAPE_USAGE.to_string();
    let value = |w: &str, what: &str, lo: f32, hi: f32| {
        w.parse::<f32>()
            .ok()
            .filter(|v| (lo..=hi).contains(v))
            .ok_or_else(|| format!("{what} is {} to {}", number(lo.into()), number(hi.into())))
    };
    let on = |w: &str| match w {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(usage()),
    };
    let words: Vec<&str> = rest.split_whitespace().collect();
    match words.as_slice() {
        ["load"] => Ok(T::Load(None)),
        ["take"] => Ok(T::Take),
        ["load", n] => match n.parse::<u8>() {
            Ok(n) if (1..=playr_core::session::LOOP_SLOTS).contains(&n) => Ok(T::Load(Some(n))),
            _ => Err(format!("not a loop slot: {n}")),
        },
        ["play"] => Ok(T::Play),
        ["stop"] => Ok(T::Stop),
        ["reset"] => Ok(T::Reset),
        ["save"] => Ok(T::Save),
        ["rec"] => Ok(T::Record),
        ["write", w] => on(w).map(T::Write),
        ["feedback", v] => value(v, "feedback", 0.0, 1.0).map(T::Feedback),
        ["wear", v] => value(v, "wear", 0.0, 1.0).map(T::Wear),
        ["thin", v] => value(v, "thin", 0.0, 1.0).map(T::Thin),
        ["window", a, b] => Ok(T::WriteWindow(tape_pos(a)?, tape_pos(b)?)),
        [n, setting @ ..] if n.parse::<u8>().is_ok() => {
            let n: u8 = n.parse().expect("checked");
            if !(1..=playr_looper::VOICES as u8).contains(&n) {
                return Err(format!("voices are 1 to {}", playr_looper::VOICES));
            }
            let s = match setting {
                [w] => V::On(on(w)?),
                ["rate", r] => V::Rate(value(r, "rate", -4.0, 4.0)?),
                ["window", a, b] => V::Window(tape_pos(a)?, tape_pos(b)?),
                ["level", v] => V::Level(value(v, "level", 0.0, 1.0)?),
                ["pan", v] => V::Pan(value(v, "pan", -1.0, 1.0)?),
                ["send", v] => V::Send(value(v, "send", 0.0, 1.0)?),
                ["wear", v] => V::Wear(value(v, "wear", 0.0, 1.0)?),
                ["fade", ms] => V::Fade(value(ms, "fade", 0.0, 1000.0)?),
                ["ping", w] => V::Ping(on(w)?),
                ["slew", ms] => V::Slew(value(ms, "slew", 0.0, playr_looper::MAX_SLEW_MS)?),
                ["drive", v] => V::Drive(value(v, "drive", 0.0, 1.0)?),
                ["filter", "lp"] => V::Filter(Filter::Low),
                ["filter", "hp"] => V::Filter(Filter::High),
                ["filter", "bp"] => V::Filter(Filter::Band),
                ["filter", v] => V::Cutoff(value(v, "filter", 0.0, 1.0)?),
                ["solo", w] => V::Solo(on(w)?),
                _ => return Err(usage()),
            };
            Ok(T::Voice(n, s))
        }
        _ => Err(usage()),
    }
}

const DJ_USAGE: &str = "usage: :dj a|b load | take | play | pause | cue [down|up] | \
     sync [on|off] | rate PCT | range 8|16|50 | nudge +|-|off | gain DB | level L | \
     eq low|mid|high DB | kill low|mid|high on|off | filter K | hot N [clear] | \
     jump BEATS | loop BEATS|off | seek TIME|PCT% | mute on|off | unqueue | mark prev|next | \
     grid x2|/2|<|>|reset | grid offset MS | tap; \
     :dj quantize on|off | strict on|off | cue a|b|off | cue-out split|3-4 | curve smooth|sharp | xfade X|a|b|centre";

/// A deck's letter.
pub fn side_name(side: crate::dj::Side) -> &'static str {
    match side {
        crate::dj::Side::A => "a",
        crate::dj::Side::B => "b",
    }
}

/// Parses what follows `:dj`.
fn dj(rest: &str) -> Result<crate::dj::DjAction, String> {
    use crate::dj::{Band, CueOut, Curve, DjAction as D, GridEdit as G, Nudge, Range, Side};
    let usage = || DJ_USAGE.to_string();
    let band = |w: &str| match w {
        "low" => Ok(Band::Low),
        "mid" => Ok(Band::Mid),
        "high" => Ok(Band::High),
        _ => Err(usage()),
    };
    let hot = |w: &str| match w.parse::<u8>() {
        Ok(n) if (1..=crate::dj::HOT_CUES as u8).contains(&n) => Ok(n),
        _ => Err(format!("hot cues are 1 to {}", crate::dj::HOT_CUES)),
    };
    let value = |w: &str, what: &str, lo: f32, hi: f32| {
        w.parse::<f32>()
            .ok()
            .filter(|v| (lo..=hi).contains(v))
            .ok_or_else(|| format!("{what} is {} to {}", number(lo.into()), number(hi.into())))
    };
    let on = |w: &str| match w {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(usage()),
    };
    let side = |w: &str| match w {
        "a" => Some(Side::A),
        "b" => Some(Side::B),
        _ => None,
    };
    let words: Vec<&str> = rest.split_whitespace().collect();
    match words.as_slice() {
        ["quantize", w] => on(w).map(D::Quantize),
        ["cue", "off"] => Ok(D::CueBus(None)),
        ["cue", w] if side(w).is_some() => Ok(D::CueBus(side(w))),
        ["xfade", "a"] => Ok(D::XfadeTo(Some(Side::A))),
        ["xfade", "b"] => Ok(D::XfadeTo(Some(Side::B))),
        ["xfade", "centre"] => Ok(D::XfadeTo(None)),
        ["xfade", x] => value(x, "xfade", 0.0, 1.0).map(D::Xfade),
        ["strict", w] => on(w).map(D::Strict),
        ["cue-out", "split"] => Ok(D::CueOut(CueOut::Split)),
        ["cue-out", "3-4"] => Ok(D::CueOut(CueOut::Channels)),
        ["curve", "smooth"] => Ok(D::Curve(Curve::Smooth)),
        ["curve", "sharp"] => Ok(D::Curve(Curve::Sharp)),
        [s, setting @ ..] if side(s).is_some() => {
            let s = side(s).expect("checked");
            Ok(match setting {
                ["load"] => D::Load(s),
                ["take"] => D::Take(s),
                ["play"] => D::Play(s),
                ["pause"] => D::Pause(s),
                ["cue"] => D::Cue(s),
                ["cue", "down"] => D::CueHold(s, true),
                ["cue", "up"] => D::CueHold(s, false),
                ["sync"] => D::Sync(s, true),
                ["sync", w] => D::Sync(s, on(w)?),
                ["rate", r] => D::Rate(s, value(r, "rate", -50.0, 50.0)?),
                ["range", "8"] => D::Range(s, Range::Narrow),
                ["range", "16"] => D::Range(s, Range::Medium),
                ["range", "50"] => D::Range(s, Range::Wide),
                ["nudge", "+"] => D::Nudge(s, Nudge::Ahead),
                ["nudge", "-"] => D::Nudge(s, Nudge::Behind),
                ["nudge", "off"] => D::Nudge(s, Nudge::Off),
                ["gain", g] => D::Gain(s, value(g, "gain", -12.0, 12.0)?),
                ["level", l] => D::Level(s, value(l, "level", 0.0, 1.0)?),
                ["grid", "x2"] => D::Grid(s, G::Double),
                ["grid", "/2"] => D::Grid(s, G::Halve),
                ["grid", "<"] => D::Grid(s, G::Earlier),
                ["grid", ">"] => D::Grid(s, G::Later),
                ["grid", "reset"] => D::Grid(s, G::Reset),
                ["grid", "offset", ms] => {
                    D::Grid(s, G::Offset(value(ms, "offset", -1000.0, 1000.0)?))
                }
                ["tap"] => D::Grid(s, G::Tap),
                ["eq", b, db] => {
                    let (lo, hi) = crate::dj::EQ_DB;
                    D::Eq(s, band(b)?, value(db, "eq", lo as f32, hi as f32)?)
                }
                ["kill", b, w] => D::Kill(s, band(b)?, on(w)?),
                ["filter", k] => D::Filter(s, value(k, "filter", -1.0, 1.0)?),
                ["hot", n] => D::HotCue(s, hot(n)?),
                ["hot", n, "clear"] => D::HotClear(s, hot(n)?),
                ["jump", b] => D::Jump(s, value(b, "jump", -64.0, 64.0)?),
                ["seek", at] => D::Seek(s, tape_pos(at)?),
                ["mute", w] => D::Mute(s, on(w)?),
                ["unqueue"] => D::Unqueue(s),
                ["mark", "next"] => D::Mark(s, true),
                ["mark", "prev"] => D::Mark(s, false),
                ["loop", "off"] => D::Loop(s, None),
                ["loop", b] => match b.parse::<f32>() {
                    Ok(b) if crate::dj::LOOP_BEATS.contains(&f64::from(b)) => D::Loop(s, Some(b)),
                    _ => return Err("loops are 0.25, 0.5, 1, 2, 4, 8, 16 or 32 beats".into()),
                },
                _ => return Err(usage()),
            })
        }
        _ => Err(usage()),
    }
}

/// What follows `dj ` in the command line for `d`.
fn dj_line(d: &crate::dj::DjAction) -> String {
    use crate::dj::{Band, CueOut, Curve, DjAction as D, GridEdit as G, Nudge, Range};
    let band = |b: &Band| match b {
        Band::Low => "low",
        Band::Mid => "mid",
        Band::High => "high",
    };
    let n = |v: &f32| number(f64::from(*v));
    let on = |b: &bool| if *b { "on" } else { "off" };
    let deck = side_name;
    match d {
        D::Quantize(b) => format!("quantize {}", on(b)),
        D::CueBus(None) => "cue off".into(),
        D::CueBus(Some(s)) => format!("cue {}", deck(*s)),
        D::Xfade(x) => format!("xfade {}", n(x)),
        D::XfadeTo(None) => "xfade centre".into(),
        D::XfadeTo(Some(s)) => format!("xfade {}", deck(*s)),
        D::Load(s) => format!("{} load", deck(*s)),
        D::Take(s) => format!("{} take", deck(*s)),
        D::Play(s) => format!("{} play", deck(*s)),
        D::Pause(s) => format!("{} pause", deck(*s)),
        D::Cue(s) => format!("{} cue", deck(*s)),
        D::CueHold(s, true) => format!("{} cue down", deck(*s)),
        D::CueHold(s, false) => format!("{} cue up", deck(*s)),
        D::Sync(s, true) => format!("{} sync", deck(*s)),
        D::Sync(s, false) => format!("{} sync off", deck(*s)),
        D::Rate(s, r) => format!("{} rate {}", deck(*s), n(r)),
        D::Range(s, r) => format!(
            "{} range {}",
            deck(*s),
            match r {
                Range::Narrow => 8,
                Range::Medium => 16,
                Range::Wide => 50,
            }
        ),
        D::Nudge(s, k) => format!(
            "{} nudge {}",
            deck(*s),
            match k {
                Nudge::Ahead => "+",
                Nudge::Behind => "-",
                Nudge::Off => "off",
            }
        ),
        D::Gain(s, g) => format!("{} gain {}", deck(*s), n(g)),
        D::Level(s, l) => format!("{} level {}", deck(*s), n(l)),
        D::Grid(s, G::Tap) => format!("{} tap", deck(*s)),
        D::Eq(s, b, db) => format!("{} eq {} {}", deck(*s), band(b), n(db)),
        D::Kill(s, b, k) => format!("{} kill {} {}", deck(*s), band(b), on(k)),
        D::Filter(s, k) => format!("{} filter {}", deck(*s), n(k)),
        D::HotCue(s, h) => format!("{} hot {h}", deck(*s)),
        D::HotClear(s, h) => format!("{} hot {h} clear", deck(*s)),
        D::Jump(s, b) => format!("{} jump {}", deck(*s), n(b)),
        D::Loop(s, None) => format!("{} loop off", deck(*s)),
        D::Loop(s, Some(b)) => format!("{} loop {}", deck(*s), n(b)),
        D::Seek(s, crate::tape::Pos::Percent(p)) => format!("{} seek {}%", deck(*s), n(p)),
        D::Seek(s, crate::tape::Pos::Time(t)) => {
            format!("{} seek {}", deck(*s), number(t.as_secs_f64()))
        }
        D::Mute(s, m) => format!("{} mute {}", deck(*s), on(m)),
        D::Unqueue(s) => format!("{} unqueue", deck(*s)),
        D::Mark(s, true) => format!("{} mark next", deck(*s)),
        D::Mark(s, false) => format!("{} mark prev", deck(*s)),
        D::Strict(b) => format!("strict {}", on(b)),
        D::CueOut(CueOut::Split) => "cue-out split".into(),
        D::CueOut(CueOut::Channels) => "cue-out 3-4".into(),
        D::Curve(Curve::Smooth) => "curve smooth".into(),
        D::Curve(Curve::Sharp) => "curve sharp".into(),
        D::Grid(s, g) => format!(
            "{} grid {}",
            deck(*s),
            match g {
                G::Double => "x2".into(),
                G::Halve => "/2".into(),
                G::Earlier => "<".into(),
                G::Later => ">".into(),
                G::Reset => "reset".into(),
                G::Offset(ms) => format!("offset {}", n(ms)),
                G::Tap => unreachable!("matched above"),
            }
        ),
    }
}

/// `25%` of the loop's range, or a time into it: `1.5`, `0:02`. Below 0% or
/// past 100% reaches into the handles either side.
fn tape_pos(word: &str) -> Result<crate::tape::Pos, String> {
    use crate::tape::Pos;
    match word.strip_suffix('%') {
        Some(p) => p
            .parse::<f32>()
            .ok()
            .filter(|p| (-100.0..=200.0).contains(p))
            .map(Pos::Percent)
            .ok_or_else(|| format!("not a percentage: {word}")),
        None => parse_time(word).map(Pos::Time),
    }
}

const MIX_USAGE: &str = "usage: :mix [master|player|tape|decks|headphones \
     PERCENT|+N|-N|mute [on|off]] | law [db|cubic] | rec";

/// Parses what follows `:mix`.
fn mix(rest: &str) -> Result<crate::mix::MixAction, String> {
    use crate::mix::{Law, MixAction as M, Strip};
    let usage = || MIX_USAGE.to_string();
    let words: Vec<&str> = rest.split_whitespace().collect();
    match words.as_slice() {
        [] => Ok(M::Show),
        ["law"] => Ok(M::Law(None)),
        ["rec"] => Ok(M::Record),
        ["law", w] => Law::named(w).map(|l| M::Law(Some(l))).ok_or_else(usage),
        [s, rest @ ..] => {
            let strip = Strip::named(s).ok_or_else(usage)?;
            match rest {
                ["mute"] => Ok(M::Mute(strip, None)),
                ["mute", "on"] => Ok(M::Mute(strip, Some(true))),
                ["mute", "off"] => Ok(M::Mute(strip, Some(false))),
                [n] => {
                    let (by, sign, n) = amount(n);
                    let n = n
                        .parse::<f32>()
                        .ok()
                        .filter(|n| n.is_finite())
                        .ok_or_else(|| format!("not a percentage: {n}"))?;
                    match (by, sign as f32 * n) {
                        (true, d) => Ok(M::By(strip, d / 100.0)),
                        (false, p) if (0.0..=100.0).contains(&p) => Ok(M::Set(strip, p / 100.0)),
                        _ => Err(format!("{s} is 0 to 100")),
                    }
                }
                _ => Err(usage()),
            }
        }
    }
}

/// The command line for `m`.
fn mix_line(m: &crate::mix::MixAction) -> String {
    use crate::mix::MixAction as M;
    let on = |b: &bool| if *b { "on" } else { "off" };
    match m {
        M::Show => "mix".into(),
        M::Set(s, p) => format!("mix {} {}", s.name(), number(f64::from(*p) * 100.0)),
        M::By(s, d) => format!(
            "mix {} {}{}",
            s.name(),
            if *d < 0.0 { "-" } else { "+" },
            number(f64::from(d.abs()) * 100.0)
        ),
        M::Mute(s, None) => format!("mix {} mute", s.name()),
        M::Mute(s, Some(b)) => format!("mix {} mute {}", s.name(), on(b)),
        M::Law(None) => "mix law".into(),
        M::Record => "mix rec".into(),
        M::Law(Some(l)) => format!("mix law {}", l.name()),
    }
}

/// What follows `tape ` in the command line for `t`.
fn tape_line(t: &crate::tape::TapeAction) -> String {
    use crate::tape::{Filter, Pos, TapeAction as T, VoiceSetting as V};
    let n = |v: &f32| number(f64::from(*v));
    let pos = |p: &Pos| match p {
        Pos::Percent(p) => format!("{}%", n(p)),
        Pos::Time(d) => number(d.as_secs_f64()),
    };
    let on = |b: &bool| if *b { "on" } else { "off" };
    match t {
        T::Load(None) => "load".into(),
        T::Take => "take".into(),
        T::Load(Some(slot)) => format!("load {slot}"),
        T::Play => "play".into(),
        T::Stop => "stop".into(),
        T::Reset => "reset".into(),
        T::Save => "save".into(),
        T::Record => "rec".into(),
        T::Write(b) => format!("write {}", on(b)),
        T::WriteWindow(a, b) => format!("window {} {}", pos(a), pos(b)),
        T::Feedback(v) => format!("feedback {}", n(v)),
        T::Wear(v) => format!("wear {}", n(v)),
        T::Thin(v) => format!("thin {}", n(v)),
        T::Voice(i, s) => match s {
            V::On(b) => format!("{i} {}", on(b)),
            V::Rate(v) => format!("{i} rate {}", n(v)),
            V::Window(a, b) => format!("{i} window {} {}", pos(a), pos(b)),
            V::Level(v) => format!("{i} level {}", n(v)),
            V::Pan(v) => format!("{i} pan {}", n(v)),
            V::Send(v) => format!("{i} send {}", n(v)),
            V::Wear(v) => format!("{i} wear {}", n(v)),
            V::Fade(v) => format!("{i} fade {}", n(v)),
            V::Ping(b) => format!("{i} ping {}", on(b)),
            V::Slew(v) => format!("{i} slew {}", n(v)),
            V::Drive(v) => format!("{i} drive {}", n(v)),
            V::Cutoff(v) => format!("{i} filter {}", n(v)),
            V::Filter(f) => format!(
                "{i} filter {}",
                match f {
                    Filter::Low => "lp",
                    Filter::High => "hp",
                    Filter::Band => "bp",
                }
            ),
            V::Solo(b) => format!("{i} solo {}", on(b)),
        },
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

/// A number to set or to change by, as `:volume`, `:speed` and `:eq` take
/// it: a sign makes it a change, and `=` makes it a value, so `=-3` sets a
/// negative one. Returns whether it is a change, its sign, and its digits.
fn amount(text: &str) -> (bool, f64, &str) {
    match (text.strip_prefix('='), signed(text)) {
        (Some(t), _) => {
            let (sign, n) = signed(t).unwrap_or((1.0, t));
            (false, sign, n)
        }
        (None, Some((sign, n))) => (true, sign, n),
        (None, None) => (false, 1.0, text),
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
    parse_in(line, Some(view), true)
}

/// As [`parse`], for a line someone typed: without `extensions`, the
/// extensions enabled in the settings, a prefix does not find an extension's
/// command and no error names it.
pub fn parse_typed(line: &str, view: View, extensions: bool) -> Result<Action, String> {
    parse_in(line, Some(view), extensions)
}

/// What a key bound to `target` in `view` does: `nop` is nothing, `command`
/// opens the prompt, and anything else is a command usable there.
pub fn key_target(target: &str, view: Option<View>) -> Result<Option<Action>, String> {
    match target.trim() {
        "" => Err("no command".into()),
        "nop" => Ok(None),
        // Opening the prompt only makes sense as a key.
        "command" => Ok(Some(Action::StartCommand)),
        target => match parse_in(target, view, true)? {
            Action::Map { .. } | Action::Unmap { .. } => {
                Err("a key cannot run :map or :unmap".into())
            }
            action => Ok(Some(action)),
        },
    }
}

/// The one view `target` works in, when it does not work in every view.
pub fn only_view(target: &str) -> Option<View> {
    if parse_in(target, None, true).is_ok() {
        return None;
    }
    let mut views =
        (VIEWS.iter().map(|v| v.1)).filter(|v| parse_in(target, Some(*v), true).is_ok());
    views.next().filter(|_| views.next().is_none())
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
fn parse_in(line: &str, view: Option<View>, extensions: bool) -> Result<Action, String> {
    let line = line.trim();
    let (word, rest) = match line.split_once(char::is_whitespace) {
        Some((word, rest)) => (word, rest.trim()),
        None => (line, ""),
    };
    if line.is_empty() {
        return Err("no command".into());
    }
    if let Some((_, new)) = ALIASES.iter().find(|a| a.0 == word) {
        return parse_in(&format!("{new} {rest}"), view, extensions);
    }
    let command = resolve_command(word, view, extensions)?;
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
        "view" => match rest {
            "next" => Ok(Action::NextView),
            "prev" => Ok(Action::PrevView),
            _ => match (choose(rest, TABS, "view"), choose(rest, VIEWS, "view")) {
                (Ok(tab), _) => Ok(Action::ShowTab(tab)),
                (_, Ok(view)) => Ok(Action::ShowView(view)),
                (_, Err(e)) if e.starts_with("views: ") => {
                    let tabs: Vec<&str> = TABS.iter().map(|t| t.0).collect();
                    Err(format!("{e}, {}", tabs.join(", ")))
                }
                (_, Err(e)) => Err(e),
            },
        },
        "down" => rows(1),
        "up" => rows(-1),
        "first" => nothing(Action::CursorFirst),
        "last" => nothing(Action::CursorLast),
        "play" => nothing(Action::Activate),
        "enqueue" => match rest {
            "" => Ok(Action::Enqueue(false)),
            "next" => Ok(Action::Enqueue(true)),
            "all" => Ok(Action::EnqueueAll),
            _ => Err(usage()),
        },
        "search" if rest.is_empty() => Ok(Action::StartSearch),
        "search" => Ok(Action::Search(rest.to_string())),
        "playlist" if rest.is_empty() => Err(usage()),
        "playlist" => Ok(Action::PlayPlaylist(unquote(rest).to_string())),
        "scan" | "open" if rest.is_empty() => Err(usage()),
        "scan" => Ok(Action::Scan(path(rest))),
        "rescan" => nothing(Action::Rescan),
        "analyze" if rest.is_empty() => Ok(Action::Analyze(None)),
        "analyze" => Ok(Action::Analyze(Some(path(rest)))),
        "bpm" => match rest {
            "x2" => Ok(Action::FixTempo(TempoFix::Double)),
            "/2" => Ok(Action::FixTempo(TempoFix::Halve)),
            "reset" => Ok(Action::FixTempo(TempoFix::Reset)),
            _ => Err(usage()),
        },
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
        "sort" => {
            // Split as `:columns` is; `desc` or `asc` turns the key before it.
            let mut keys: Vec<String> = Vec::new();
            for word in rest.split([',', ' ']).filter(|w| !w.trim().is_empty()) {
                match (keys.last_mut(), word) {
                    (Some(key), "desc" | "asc") => *key = format!("{key} {word}"),
                    _ => keys.push(word.to_string()),
                }
            }
            keys.iter()
                .map(|k| SortKey::named(k).ok_or_else(|| unknown_column(k)))
                .collect::<Result<Vec<_>, _>>()
                .map(Action::SetSort)
        }
        "prune" if rest.is_empty() => Ok(Action::Prune(None)),
        "prune" => Ok(Action::Prune(Some(path(rest)))),
        "open" => Ok(Action::Open(vec![path(rest)])),
        "save" if rest.is_empty() => Ok(Action::StartSave),
        "save" => Ok(Action::SaveAs(unquote(rest).to_string())),
        "pause" => nothing(Action::TogglePause),
        "next" => nothing(Action::Next),
        "prev" => nothing(Action::Prev),
        "stop" => match rest.split_once(' ').unwrap_or((rest, "")) {
            ("", _) => Ok(Action::Stop),
            ("after", "") => Ok(Action::StopAfter),
            ("in", "off") => Ok(Action::StopIn(None)),
            ("in", t) if !t.is_empty() => parse_time(t).map(|d| Action::StopIn(Some(d))),
            _ => Err(usage()),
        },
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
            match amount(rest) {
                _ if rest.is_empty() => Err(usage()),
                (true, sign, n) => Ok(Action::VolumeBy(sign as f32 * number(n)? / 100.0)),
                (false, sign, n) => match sign as f32 * number(n)? {
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
            match amount(rest) {
                _ if rest.is_empty() => Err(usage()),
                (true, sign, n) => Ok(Action::SpeedBy(sign as i32 * semitones(n)?)),
                (false, sign, n) => match sign as i32 * semitones(n)? {
                    n if n.abs() <= 12 => Ok(Action::SetSpeed(n)),
                    _ => Err("speed is -12 to 12 semitones".into()),
                },
            }
        }
        "eq" => {
            let (band, change) = rest.split_once(' ').unwrap_or((rest, ""));
            if band == "flat" && change.is_empty() {
                return Ok(Action::FlatEq);
            }
            let band = choose(band, &Band::NAMES, "band")?;
            let db = |t: &str| {
                t.parse::<f32>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| format!("not a number of dB: {change}"))
            };
            match amount(change) {
                _ if change.is_empty() => Err(usage()),
                (true, sign, n) => Ok(Action::EqBy(band, sign as f32 * db(n)?)),
                (false, sign, n) => match sign as f32 * db(n)? {
                    n if n.abs() <= RANGE_DB => Ok(Action::SetEq(band, n)),
                    _ => Err("eq is -12 to 12 dB".into()),
                },
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
        "slice-edges" => choose(rest, EDGES, "slice edges").map(Action::SetSliceEdges),
        "mark" if rest.is_empty() => Ok(Action::Mark),
        "mark" => Ok(Action::MarkAt(parse_time(rest)?)),
        "mark-undo" => nothing(Action::UndoMark),
        "mark-clear" => nothing(Action::ClearMarks),
        "mark-next" => nothing(Action::NextMark),
        "mark-prev" => nothing(Action::PrevMark),
        "zoom" => match rest {
            "+" => Ok(Action::Zoom(Zoom::In)),
            "-" => Ok(Action::Zoom(Zoom::Out)),
            "all" => Ok(Action::Zoom(Zoom::All)),
            n => n
                .parse()
                .map(|n| Action::Zoom(Zoom::To(n)))
                .map_err(|_| usage()),
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
            _ => Err(usage()),
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
        "loop" if command.view.is_none() && rest != "off" => {
            Err("outside the sampler view, :loop takes only off".into())
        }
        "loop" => match rest {
            "" => Ok(Action::Loop(None)),
            "on" => Ok(Action::Loop(Some(true))),
            "off" => Ok(Action::Loop(Some(false))),
            _ => {
                use crate::action::SlotOp;
                let (n, op) = rest.split_once(' ').unwrap_or((rest, ""));
                let op = match op.trim() {
                    "" => SlotOp::Use,
                    "save" => SlotOp::Save,
                    "clear" => SlotOp::Clear,
                    _ => return Err(usage()),
                };
                match n.parse::<u8>() {
                    Ok(n) if (1..=playr_core::session::LOOP_SLOTS).contains(&n) => {
                        Ok(Action::LoopSlot(n, op))
                    }
                    _ => Err(usage()),
                }
            }
        },
        "tape" => tape(rest).map(Action::Tape),
        "dj" => dj(rest).map(Action::Dj),
        "mix" => mix(rest).map(Action::Mix),
        "loops" => match rest {
            "clear" => Ok(Action::ClearLoops),
            _ => Err(usage()),
        },
        "audition" => match rest {
            "" => Ok(Action::Audition),
            "next" => Ok(Action::AuditionSlice(true)),
            "prev" => Ok(Action::AuditionSlice(false)),
            _ => Err(usage()),
        },
        "scrub" if rest.is_empty() => Err(usage()),
        "scrub" => Ok(Action::Scrub(parse_time(rest)?)),
        "select" if rest.is_empty() => Err(usage()),
        "select" => Ok(Action::SelectMarkAt(parse_time(rest)?)),
        "deselect" => nothing(Action::Deselect),
        "move" if command.view == Some(Sampler) => {
            nudge(rest).map(Action::MoveSelected).ok_or_else(usage)
        }
        "move-to" if rest.is_empty() => Err(usage()),
        "move-to" => Ok(Action::MoveSelectedTo(parse_time(rest)?)),
        "select-slice" if rest.is_empty() => Err(usage()),
        "select-slice" => Ok(Action::SelectSliceAt(parse_time(rest)?)),
        "onset" => nothing(Action::SnapSelected),
        "remove" if command.view == Some(Sampler) => nothing(Action::RemoveSelected),
        "in" if rest.is_empty() => Ok(Action::RangeIn(None)),
        "in" => Ok(Action::RangeIn(Some(parse_time(rest)?))),
        "out" if rest.is_empty() => Ok(Action::RangeOut(None)),
        "out" => Ok(Action::RangeOut(Some(parse_time(rest)?))),
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
        "convert" if playr_core::convertwithmoss::is_format_name(first_word(rest).0) => {
            let (format, export) = first_word(rest);
            let export = (!export.is_empty()).then(|| path(export));
            Ok(Action::Convert(format.to_string(), export))
        }
        "convert" => Err(format!(
            "convert takes a ConvertWithMoss format: {}",
            playr_core::convertwithmoss::FORMATS.join(", ")
        )),
        "undo" => nothing(Action::Undo),
        "redo" => nothing(Action::Redo),
        "write" => nothing(Action::WriteSlices),
        "discard" => nothing(Action::DiscardSlices),
        "mark-slices" => nothing(Action::MarkSlices),
        "slice" => match first_word(rest) {
            ("region", "") => Ok(Action::Slice(Slicing::Region)),
            ("marks", "") => Ok(Action::Slice(Slicing::Marks)),
            ("onsets", "") => Ok(Action::Slice(Slicing::Onsets(None))),
            ("onsets", s) => match s.parse::<f32>() {
                Ok(s) if (0.0..=1.0).contains(&s) => Ok(Action::Slice(Slicing::Onsets(Some(s)))),
                _ => Err("onset sensitivity is 0 to 1".into()),
            },
            ("beats", "") => Ok(Action::Slice(Slicing::Beats(DEFAULT_BEATS))),
            ("beats", n) => match n.parse::<f32>() {
                Ok(n) if (MIN_BEATS..=MAX_BEATS).contains(&n) => {
                    Ok(Action::Slice(Slicing::Beats(n)))
                }
                _ => Err(format!("beats a slice are {MIN_BEATS} to {MAX_BEATS}")),
            },
            (n, "") if n.parse::<usize>().is_ok() => match n.parse::<usize>() {
                Ok(n) if (2..=MAX_SLICES).contains(&n) => Ok(Action::Slice(Slicing::Equal(n))),
                _ => Err(format!("slices are 2 to {MAX_SLICES}")),
            },
            _ => Err(SLICE_USAGE.into()),
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
        "search-clear" => nothing(Action::ClearSearch),
        "remove" => nothing(Action::Remove),
        "move" => match signed(rest).map(|(sign, n)| (sign as i64, n.parse::<i64>())) {
            Some((sign, Ok(n))) if n > 0 => Ok(Action::MoveTrack(sign * n)),
            _ => Err(usage()),
        },
        "clear" if view == Some(View::Queue) => nothing(Action::ClearQueue),
        "clear" => nothing(Action::ClearSelection),
        "delete" => nothing(Action::DeletePlaylist),
        "edit" => nothing(Action::EditPlaylist),
        "save-search" if rest.is_empty() => Ok(Action::StartSaveSearch),
        "sql" if rest.is_empty() => Err(usage()),
        "sql" => Ok(Action::Sql(rest.to_string())),
        "save-search" => Ok(Action::SaveSearch(unquote(rest).to_string())),
        "rename" if rest.is_empty() => Ok(Action::StartRename),
        "rename" => Ok(Action::RenameTo(unquote(rest).to_string())),
        _ => unreachable!("command {name} has no parser"),
    }
}

/// What Tab can complete `text` to in `view`, as whole command lines.
///
/// The first word completes to the names of commands that work in `view`.
/// After `edge`, `fit`, `loop`, `mode`, `snap`, `theme` or `view` the argument completes to its choices, and after
/// `playlist` or `rename` to the names in `playlists`, and after `convert
/// FORMAT` to the names `exports` lists, called only then since it reads a
/// directory. An extension's command is completed only with `extensions`:
/// when it is enabled in the settings.
pub fn completions(
    text: &str,
    view: View,
    playlists: &[String],
    exports: &dyn Fn() -> Vec<String>,
    extensions: bool,
) -> Vec<String> {
    let offered =
        |name: &str| extensions || !COMMANDS.iter().any(|c| c.extension && c.name == name);
    let Some((word, rest)) = text.split_once(' ') else {
        return usable_names(Some(view))
            .into_iter()
            .filter(|n| n.starts_with(text) && offered(n))
            .map(String::from)
            .collect();
    };
    let Ok(command) = resolve_command(word, Some(view), extensions) else {
        return Vec::new();
    };
    if !offered(command.name) {
        return Vec::new();
    }
    let rest = rest.trim_start();
    if let ("convert", Some((format, export))) = (command.name, rest.split_once(' ')) {
        let lower = export.trim_start().to_lowercase();
        return exports()
            .into_iter()
            .filter(|n| n.to_lowercase().starts_with(&lower))
            .map(|n| format!("convert {format} {n}"))
            .collect();
    }
    let choices: Vec<String> = match command.name {
        "mode" => MODES.iter().map(|m| m.0.to_string()).collect(),
        "theme" => THEMES.iter().map(|t| t.0.to_string()).collect(),
        "columns" | "sort" => Column::NAMES.iter().map(|c| c.0.to_string()).collect(),
        "replaygain" => REPLAYGAINS.iter().map(|r| r.0.to_string()).collect(),
        "slice-edges" => EDGES.iter().map(|e| e.0.to_string()).collect(),
        "convert" => playr_core::convertwithmoss::FORMATS
            .map(String::from)
            .to_vec(),
        "loop" if command.view.is_none() => vec!["off".into()],
        "snap" | "fit" | "loop" => vec!["on".into(), "off".into()],
        "edge" => vec!["start".into(), "end".into()],
        "audition" => vec!["next".into(), "prev".into()],
        "loops" => vec!["clear".into()],
        "view" => VIEWS
            .iter()
            .map(|v| v.0)
            .chain(TABS.iter().map(|t| t.0))
            .chain(["next", "prev"])
            .map(String::from)
            .collect(),
        "enqueue" => vec!["next".into(), "all".into()],
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
    /// A history holding the last [`HISTORY_LEN`] of `lines`, oldest first.
    pub fn new(mut lines: Vec<String>) -> History {
        lines.drain(..lines.len().saturating_sub(HISTORY_LEN));
        History { lines }
    }

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
    pub fn complete(
        &mut self,
        forward: bool,
        view: View,
        playlists: &[String],
        exports: &dyn Fn() -> Vec<String>,
        extensions: bool,
    ) {
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
                let choices = completions(&self.text, view, playlists, exports, extensions);
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
/// for every view first. A heading row has no description. An extension's
/// command is listed only with `extensions`.
pub fn command_rows(extensions: bool) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut group = None;
    for c in COMMANDS.iter().filter(|c| extensions || !c.extension) {
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
