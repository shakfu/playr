//! Every action the terminal can do, the window can do: through a control, a
//! key binding, or, for a named few, a way that is not an `Action`.

use std::collections::BTreeSet;
use std::time::Duration;

use playr_app::action::{Action, Key, Keymap, Slicing, Zoom};
use playr_app::{Theme, View};
use playr_core::audio::Mode;
use playr_gui::controls;

/// One of each action.
fn every_action() -> Vec<Action> {
    let key = Key::parse("f5").unwrap();
    let actions = vec![
        Action::Quit,
        Action::Help,
        Action::CommandHelp,
        Action::ShowView(View::Library),
        Action::NextView,
        Action::Cursor(1),
        Action::CursorFirst,
        Action::CursorLast,
        Action::StartSearch,
        Action::Search("q".into()),
        Action::ClearSearch,
        Action::StartCommand,
        Action::Activate,
        Action::Add,
        Action::Remove,
        Action::MoveTrack(1),
        Action::ClearSelection,
        Action::StartSave,
        Action::SaveAs("n".into()),
        Action::DeletePlaylist,
        Action::StartRename,
        Action::RenameTo("n".into()),
        Action::PlayPlaylist("n".into()),
        Action::Scan("/m".into()),
        Action::Prune("/m".into()),
        Action::Open(vec!["/m".into()]),
        Action::TogglePause,
        Action::Next,
        Action::Prev,
        Action::Stop,
        Action::SeekBy(5),
        Action::SeekTo(Duration::ZERO),
        Action::VolumeBy(0.05),
        Action::SetVolume(1.0),
        Action::SpeedBy(1),
        Action::SetSpeed(0),
        Action::CycleMode(true),
        Action::SetMode(Mode::Normal),
        Action::Mark,
        Action::MarkAt(Duration::ZERO),
        Action::UndoMark,
        Action::ClearMarks,
        Action::NextMark,
        Action::PrevMark,
        Action::Slice(Slicing::Region),
        Action::Zoom(Zoom::In),
        Action::Display(None),
        Action::Nudge(playr_app::action::Nudge::Columns(1)),
        Action::Snap(None),
        Action::RangeIn,
        Action::RangeOut,
        Action::SetRange(None),
        Action::Loop(None),
        Action::PickEdge(playr_app::sampler::Edge::Start),
        Action::MoveEdge(playr_app::action::Nudge::Columns(1)),
        Action::WriteSlices,
        Action::DiscardSlices,
        Action::Theme(Theme::System),
        Action::Map {
            view: None,
            key,
            action: None,
        },
        Action::Unmap { view: None, key },
    ];
    // No wildcard: an action added to `Action` stops this compiling until it
    // is added to the list above and weighed below.
    for action in &actions {
        match action {
            Action::Quit
            | Action::Help
            | Action::CommandHelp
            | Action::ShowView(_)
            | Action::NextView
            | Action::Cursor(_)
            | Action::CursorFirst
            | Action::CursorLast
            | Action::StartSearch
            | Action::Search(_)
            | Action::ClearSearch
            | Action::StartCommand
            | Action::Activate
            | Action::Add
            | Action::Remove
            | Action::MoveTrack(_)
            | Action::ClearSelection
            | Action::StartSave
            | Action::SaveAs(_)
            | Action::DeletePlaylist
            | Action::StartRename
            | Action::RenameTo(_)
            | Action::PlayPlaylist(_)
            | Action::Scan(_)
            | Action::Prune(_)
            | Action::Open(_)
            | Action::TogglePause
            | Action::Next
            | Action::Prev
            | Action::Stop
            | Action::SeekBy(_)
            | Action::SeekTo(_)
            | Action::VolumeBy(_)
            | Action::SetVolume(_)
            | Action::SpeedBy(_)
            | Action::SetSpeed(_)
            | Action::CycleMode(_)
            | Action::SetMode(_)
            | Action::Mark
            | Action::MarkAt(_)
            | Action::UndoMark
            | Action::ClearMarks
            | Action::NextMark
            | Action::PrevMark
            | Action::Slice(_)
            | Action::Zoom(_)
            | Action::Display(_)
            | Action::Nudge(_)
            | Action::Snap(_)
            | Action::RangeIn
            | Action::RangeOut
            | Action::SetRange(_)
            | Action::Loop(_)
            | Action::PickEdge(_)
            | Action::MoveEdge(_)
            | Action::WriteSlices
            | Action::DiscardSlices
            | Action::Theme(_)
            | Action::Map { .. }
            | Action::Unmap { .. } => {}
        }
    }
    actions
}

/// The variant's name: `MoveTrack` for `MoveTrack(1)`.
fn name(action: &Action) -> String {
    let debug = format!("{action:?}");
    debug
        .split(['(', ' ', '{'])
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Actions the window does another way, and how.
const ELSEWHERE: &[(&str, &str)] = &[
    (
        "Search",
        "the search field shows results through Model::search_as_typed",
    ),
    ("SaveAs", "the name dialog saves through Model::save_as"),
    (
        "RenameTo",
        "the name dialog renames through Model::rename_to",
    ),
    ("PlayPlaylist", "a playlist plays from its row, as Activate"),
    (
        "Map",
        "binds a key for the session; typed in the command bar",
    ),
    (
        "Unmap",
        "removes a binding for the session; typed in the command bar",
    ),
];

#[test]
fn every_action_has_a_control_or_a_key() {
    let all: BTreeSet<String> = every_action().iter().map(name).collect();
    let keys: BTreeSet<String> = Keymap::default()
        .bindings()
        .iter()
        .filter_map(|b| b.action.as_ref())
        .map(name)
        .collect();
    let controls: BTreeSet<String> = controls::TABLES
        .iter()
        .flat_map(|table| table.iter().map(|c| name(&c.action)))
        .chain(controls::WITH_VALUES.iter().map(|(n, _)| n.to_string()))
        .collect();
    let elsewhere: BTreeSet<String> = ELSEWHERE.iter().map(|(n, _)| n.to_string()).collect();

    let missing: Vec<&String> = all
        .iter()
        .filter(|a| !keys.contains(*a) && !controls.contains(*a) && !elsewhere.contains(*a))
        .collect();
    assert!(missing.is_empty(), "no control or key performs {missing:?}");

    // The list of exceptions stays true: none has gained a control or a key.
    let reachable: Vec<&String> = elsewhere
        .iter()
        .filter(|a| keys.contains(*a) || controls.contains(*a))
        .collect();
    assert!(reachable.is_empty(), "{reachable:?} need no exception");
    // Every name counted is a real action.
    for n in controls.iter().chain(&elsewhere) {
        assert!(all.contains(n), "{n} is not an action");
    }
}
