//! Rendering tests against a headless backend. No audio device involved.

use playr::ui::render::{self, scroll_offset};
use playr::ui::{Drawn, Input, Lists, Screen, Scroll, Snapshot, Theme, View};
use playr_app::action::Keymap;
use playr_app::sampler::Sampler;
use playr_core::audio::{Spec, State, Status};
use playr_core::db::query::Playlist;
use playr_core::db::Track;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::time::Duration;

fn track(title: &str, artist: &str, album: &str, secs: i64) -> Track {
    Track {
        id: 1,
        path: format!("/m/{title}.flac"),
        title: Some(title.into()),
        artist: Some(artist.into()),
        album: Some(album.into()),
        duration_ms: Some(secs * 1000),
        ..Default::default()
    }
}

/// One frame to draw. Defaults to an idle, empty player.
struct Case<'a> {
    view: View,
    snapshot: &'a Snapshot,
    all: &'a [Track],
    results: Option<&'a [Track]>,
    playing: &'a [Track],
    selection: &'a [Track],
    playlists: &'a [Playlist],
    input: &'a Input,
    message: Option<&'a str>,
    width: u16,
    height: u16,
    /// Cursor row in each list; the first row when unset.
    selected: Option<usize>,
    help_scroll: usize,
    /// The key map; the defaults when unset.
    keys: Option<&'a Keymap>,
    sampler: Sampler,
    colour: bool,
    theme: Theme,
}

impl<'a> Case<'a> {
    fn new(view: View, snapshot: &'a Snapshot) -> Self {
        Case {
            view,
            snapshot,
            all: &[],
            results: None,
            playing: &[],
            selection: &[],
            playlists: &[],
            input: &Input::None,
            message: None,
            width: 100,
            height: 20,
            selected: None,
            help_scroll: 0,
            keys: None,
            sampler: Sampler::default(),
            colour: true,
            theme: Theme::Dark,
        }
    }

    fn all(mut self, v: &'a [Track]) -> Self {
        self.all = v;
        self
    }

    fn results(mut self, v: &'a [Track]) -> Self {
        self.results = Some(v);
        self
    }

    fn playing(mut self, v: &'a [Track]) -> Self {
        self.playing = v;
        self
    }

    fn selection(mut self, v: &'a [Track]) -> Self {
        self.selection = v;
        self
    }

    fn playlists(mut self, v: &'a [Playlist]) -> Self {
        self.playlists = v;
        self
    }

    fn input(mut self, v: &'a Input) -> Self {
        self.input = v;
        self
    }

    fn message(mut self, v: &'a str) -> Self {
        self.message = Some(v);
        self
    }

    fn selected(mut self, i: usize) -> Self {
        self.selected = Some(i);
        self
    }

    fn sampler(mut self, sampler: Sampler) -> Self {
        self.sampler = sampler;
        self
    }

    fn keys(mut self, keys: &'a Keymap) -> Self {
        self.keys = Some(keys);
        self
    }

    fn help_scroll(mut self, rows: usize) -> Self {
        self.help_scroll = rows;
        self
    }

    fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    fn no_colour(mut self) -> Self {
        self.colour = false;
        self
    }

    fn size(mut self, width: u16, height: u16) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Draws one frame and returns it as plain text lines.
    fn render(self) -> Vec<String> {
        let buf = self.buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// Draws one frame and returns the cells.
    fn buffer(self) -> ratatui::buffer::Buffer {
        self.frame().0
    }

    /// Draws one frame and returns what drawing settled.
    fn drawn(self) -> Drawn {
        self.frame().1
    }

    fn frame(self) -> (ratatui::buffer::Buffer, Drawn) {
        let mut terminal = Terminal::new(TestBackend::new(self.width, self.height)).unwrap();
        let default_keys = Keymap::default();
        let keys = self.keys.unwrap_or(&default_keys);
        let cursor = self.selected.unwrap_or(0);
        let at = |empty: bool, row| Scroll {
            row: (!empty).then_some(row),
            offset: 0,
        };
        let screen = Screen {
            all: self.all,
            results: self.results,
            playing: self.playing,
            selection: self.selection,
            playlists: self.playlists,
            input: self.input,
            help_scroll: self.help_scroll,
            message: self.message,
            colour: self.colour,
            theme: self.theme,
            lists: Lists {
                library: at(self.all.is_empty(), cursor),
                selection: at(self.selection.is_empty(), cursor),
                playlists: at(self.playlists.is_empty(), cursor),
            },
            ..Screen::new(self.view, self.snapshot, keys, &self.sampler)
        };
        let mut drawn = None;
        terminal
            .draw(|f| drawn = Some(render::draw(&screen, f)))
            .unwrap();
        (terminal.backend().buffer().clone(), drawn.unwrap())
    }

    /// Draws one frame and joins it for substring assertions.
    fn text(self) -> String {
        self.render().join("\n")
    }
}

fn stopped() -> Snapshot {
    Snapshot {
        status: Status::default(),
        position: Duration::ZERO,
        volume: 0.8,
        ..Default::default()
    }
}

/// Returns cells on `row` whose foreground equals their background.
///
/// Such a cell renders as a blank: the text is there but cannot be read.
fn invisible_cells(
    view: View,
    all: &[Track],
    selection: &[Track],
    playlists: &[Playlist],
    row: u16,
) -> Vec<String> {
    let snapshot = stopped();
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    let first = |rows: bool| Scroll {
        row: rows.then_some(0),
        offset: 0,
    };
    let keys = Keymap::default();
    let sampler = Sampler::default();
    let screen = Screen {
        all,
        selection,
        playlists,
        lists: Lists {
            library: first(!all.is_empty()),
            selection: first(!selection.is_empty()),
            playlists: first(!playlists.is_empty()),
        },
        ..Screen::new(view, &snapshot, &keys, &sampler)
    };
    terminal
        .draw(|f| {
            render::draw(&screen, f);
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    (0..buf.area.width)
        .filter_map(|x| {
            let cell = &buf[(x, row)];
            let sym = cell.symbol().to_string();
            (cell.fg == cell.bg && !sym.trim().is_empty()).then_some(sym)
        })
        .collect()
}

#[test]
fn the_selected_row_stays_readable_in_every_view() {
    // The selected row gets a background colour. Any column drawn in that same
    // colour vanishes: the text is present but unreadable. This caught the
    // album and duration disappearing from the highlighted track.
    let tracks = [track(
        "Prophecy At 1420 MHz",
        "Boards of Canada",
        "Inferno",
        304,
    )];
    let playlists = [Playlist {
        id: 1,
        name: "late night".into(),
        len: 27,
    }];

    for (view, all, selection, pls) in [
        (View::Library, &tracks[..], &[][..], &[][..]),
        (View::Selection, &[][..], &tracks[..], &[][..]),
        (View::Playlists, &[][..], &[][..], &playlists[..]),
    ] {
        let hidden = invisible_cells(view, all, selection, pls, 2);
        assert!(
            hidden.is_empty(),
            "{view:?}: {} cells on the selected row have fg == bg: {:?}",
            hidden.len(),
            hidden.concat()
        );
    }
}

#[test]
fn library_lists_tracks_with_artist_album_and_duration() {
    let all = vec![track("Waltz for Debby", "Bill Evans", "Sunday", 396)];
    let joined = Case::new(View::Library, &stopped()).all(&all).text();
    assert!(joined.contains("Bill Evans"), "artist missing:\n{joined}");
    assert!(
        joined.contains("Waltz for Debby"),
        "title missing:\n{joined}"
    );
    assert!(joined.contains("Sunday"), "album missing:\n{joined}");
    assert!(joined.contains("6:36"), "duration missing:\n{joined}");
}

#[test]
fn empty_library_explains_how_to_fill_it() {
    let joined = Case::new(View::Library, &stopped()).text();
    assert!(
        joined.contains("playr scan"),
        "no guidance for an empty library:\n{joined}"
    );
}

#[test]
fn now_playing_shows_title_position_and_source_format() {
    let playing = vec![track("So What", "Miles Davis", "Kind of Blue", 545)];
    let snapshot = Snapshot {
        status: Status {
            state: State::Playing,
            queue: vec![std::path::PathBuf::from("/m/So What.flac")].into(),
            index: 0,
            duration: Some(Duration::from_secs(545)),
            source: Some(Spec {
                rate: 44100,
                channels: 2,
            }),
            output_rate: 44100,
            resampling: false,
            error: None,
            error_seq: 0,
            semitones: 0,
            mode: Default::default(),
            looping: None,
        },
        position: Duration::from_secs(151),
        volume: 0.75,
        ..Default::default()
    };
    let joined = Case::new(View::Selection, &snapshot)
        .playing(&playing)
        .text();
    assert!(joined.contains("So What"), "title missing:\n{joined}");
    assert!(joined.contains("Miles Davis"), "artist missing:\n{joined}");
    assert!(joined.contains("2:31"), "position missing:\n{joined}");
    assert!(joined.contains("9:05"), "duration missing:\n{joined}");
    assert!(joined.contains("44.1kHz"), "source rate missing:\n{joined}");
    assert!(joined.contains("75%"), "volume missing:\n{joined}");
}

#[test]
fn resampling_is_shown_when_the_device_forces_it() {
    let mut snapshot = stopped();
    snapshot.status.source = Some(Spec {
        rate: 44100,
        channels: 2,
    });
    snapshot.status.output_rate = 48000;
    snapshot.status.resampling = true;
    let joined = Case::new(View::Selection, &snapshot).text();
    assert!(joined.contains("44.1kHz"), "source rate missing:\n{joined}");
    assert!(joined.contains("48.0kHz"), "device rate missing:\n{joined}");
}

#[test]
fn playback_errors_reach_the_status_line() {
    // The engine reports errors; the app promotes them to a timed message.
    let msg = "bad.opus: unsupported format";
    let joined = Case::new(View::Library, &stopped()).message(msg).text();
    assert!(
        joined.contains("unsupported format"),
        "error not shown:\n{joined}"
    );
}

#[test]
fn search_prompt_replaces_the_library_title() {
    let input = Input::Search("evans".into());
    let joined = Case::new(View::Library, &stopped()).input(&input).text();
    assert!(
        joined.contains("Search: evans"),
        "search prompt missing:\n{joined}"
    );
}

#[test]
fn playlists_show_their_track_counts() {
    let pls = vec![Playlist {
        id: 1,
        name: "late night".into(),
        len: 27,
    }];
    let joined = Case::new(View::Playlists, &stopped())
        .playlists(&pls)
        .text();
    assert!(joined.contains("late night"), "name missing:\n{joined}");
    assert!(joined.contains("27"), "count missing:\n{joined}");
}

#[test]
fn track_rows_never_truncate_the_duration_column() {
    // The duration sits at the right edge; if the column arithmetic is off by
    // even one, it is the first thing to be cut.
    let all = vec![track(
        "A Fairly Long Track Title Here",
        "An Artist Name",
        "An Album Name",
        396,
    )];
    for width in [40u16, 55, 80, 120, 200] {
        let joined = Case::new(View::Library, &stopped())
            .all(&all)
            .size(width, 10)
            .text();
        assert!(
            joined.contains("6:36"),
            "duration truncated at width {width}:\n{joined}"
        );
    }
}

#[test]
fn layout_survives_a_narrow_terminal() {
    let all = vec![track(
        "A Very Long Track Title Indeed",
        "Some Artist",
        "Some Album",
        200,
    )];
    for width in [40u16, 60, 80, 200] {
        let lines = Case::new(View::Library, &stopped())
            .all(&all)
            .size(width, 12)
            .render();
        assert_eq!(lines.len(), 12, "height wrong at width {width}");
        for line in &lines {
            assert!(
                line.chars().count() <= width as usize,
                "line overflows at width {width}: {line:?}"
            );
        }
    }
}

#[test]
fn varispeed_is_shown_but_not_as_a_rate_conversion() {
    // Varispeed engages the resampler at the same rate. Reporting that as
    // "44.1kHz -> 44.1kHz" reads as a fault instead of a speed change.
    let mut snapshot = stopped();
    snapshot.status.source = Some(Spec {
        rate: 44100,
        channels: 2,
    });
    snapshot.status.output_rate = 44100;
    snapshot.status.resampling = true;
    snapshot.status.semitones = 3;

    let joined = Case::new(View::Selection, &snapshot).text();
    assert!(
        joined.contains("1.19x (+3 st)"),
        "speed not shown:\n{joined}"
    );
    assert!(
        !joined.contains("->"),
        "spurious rate conversion shown:\n{joined}"
    );
}

#[test]
fn normal_speed_is_not_announced() {
    let mut snapshot = stopped();
    snapshot.status.source = Some(Spec {
        rate: 44100,
        channels: 2,
    });
    snapshot.status.output_rate = 44100;
    let joined = Case::new(View::Selection, &snapshot).text();
    assert!(
        !joined.contains(" st)"),
        "speed shown when normal:\n{joined}"
    );
}

// --- scrolling long lists ---

#[test]
fn a_cursor_far_down_a_long_library_is_on_screen() {
    let all: Vec<Track> = (0..10_000)
        .map(|i| track(&format!("Song {i:05}"), "A", "B", 60))
        .collect();
    let snapshot = stopped();
    for view in [View::Library, View::Selection] {
        let joined = Case::new(view, &snapshot)
            .all(&all)
            .selection(&all)
            .selected(9_500)
            .text();
        assert!(
            joined.contains("Song 09500"),
            "cursor row not drawn:\n{joined}"
        );
        // Had the list not scrolled, its second row would show.
        assert!(
            !joined.contains("Song 00001"),
            "list did not scroll:\n{joined}"
        );
    }
}

#[test]
fn moving_within_the_view_does_not_scroll() {
    assert_eq!(scroll_offset(100, Some(105), 10, 1_000), 100);
    assert_eq!(scroll_offset(100, Some(100), 10, 1_000), 100);
    assert_eq!(scroll_offset(100, Some(109), 10, 1_000), 100);
}

#[test]
fn leaving_the_view_scrolls_by_the_least_amount() {
    assert_eq!(scroll_offset(100, Some(110), 10, 1_000), 101);
    assert_eq!(scroll_offset(100, Some(99), 10, 1_000), 99);
}

#[test]
fn a_list_that_shrank_is_pulled_back_into_view() {
    // Scrolled to 500 in the library, then a search leaves 3 results.
    assert_eq!(scroll_offset(500, Some(0), 10, 3), 0);
    assert_eq!(scroll_offset(500, None, 10, 30), 20);
}

#[test]
fn an_empty_list_or_area_scrolls_nowhere() {
    assert_eq!(scroll_offset(7, Some(3), 0, 100), 0);
    assert_eq!(scroll_offset(7, None, 10, 0), 0);
}

#[test]
fn a_pending_confirmation_is_shown_in_place_of_the_hints() {
    use playr::ui::Confirm;
    let input = Input::Confirm(Confirm::ReplacePlaylist("late".into()));
    let joined = Case::new(View::Selection, &stopped()).input(&input).text();
    assert!(
        joined.contains("replace playlist \"late\" with the selection? (y/n)"),
        "prompt not shown:\n{joined}"
    );
}

#[test]
fn wide_characters_do_not_push_the_duration_off_the_row() {
    // CJK characters take two cells each. Padding by character count made
    // such a row twice as wide as its columns.
    let all = [
        track("Plain", "Artist", "Album", 599),
        track(
            "\u{6771}\u{4eac}\u{306e}\u{591c}".repeat(12).as_str(),
            "\u{5742}\u{672c}\u{9f8d}\u{4e00}".repeat(4).as_str(),
            "\u{97f3}\u{697d}\u{56f3}\u{9451}".repeat(4).as_str(),
            599,
        ),
    ];
    let width = 100;
    let buf = Case::new(View::Library, &stopped()).all(&all).buffer();
    for y in [2, 3] {
        let tail: String = (width - 5..width - 1)
            .map(|x| buf[(x, y)].symbol())
            .collect();
        assert_eq!(tail, "9:59", "row {y} lost its duration");
    }
}

// --- bottom line and help ---

#[test]
fn the_bottom_line_shows_a_volume_meter_and_the_help_key() {
    let joined = Case::new(View::Library, &stopped()).text();
    assert!(
        joined.contains("vol [########--]  80%"),
        "no volume meter:\n{joined}"
    );
    assert!(joined.contains("? help"), "no help key:\n{joined}");
}

#[test]
fn a_message_shares_the_bottom_line_with_the_indicators() {
    let joined = Case::new(View::Library, &stopped())
        .message("added to selection")
        .text();
    let last = joined
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    assert!(
        last.contains("added to selection"),
        "message missing: {last:?}"
    );
    assert!(last.contains("? help"), "indicators missing: {last:?}");
}

#[test]
fn help_lists_the_keys_that_work_in_the_view() {
    use playr_app::action::Key;
    let input = Input::Help;
    let keys = Keymap::default();
    for view in [
        View::Library,
        View::Selection,
        View::Playlists,
        View::Sampler,
    ] {
        let joined = Case::new(view, &stopped())
            .input(&input)
            .size(100, 80)
            .text();
        let name = format!("{view:?}").to_lowercase();
        assert!(
            joined.contains(&format!("Keys in the {name} view")),
            "{joined}"
        );
        // Rows by their text inside the popup border.
        let rows: Vec<&str> = joined
            .lines()
            .filter_map(|l| l.split('\u{2502}').nth(2))
            .map(str::trim)
            .collect();
        let row_for = |command: &str| {
            rows.iter()
                .find(|r| r.ends_with(&format!(" :{command}")))
                .copied()
        };
        for b in keys.bindings() {
            let Some(action) = keys.lookup(b.key, view) else {
                continue;
            };
            let command = playr_app::command::line(action, Some(view));
            let row = row_for(&command)
                .unwrap_or_else(|| panic!(":{command} missing from {name} help:\n{joined}"));
            if b.view == Some(view) || keys.lookup(b.key, view) == b.action.as_ref() {
                assert!(
                    row.split_whitespace().any(|w| w == b.key.to_string()),
                    "{} not on the row for :{command} in {name}: {row:?}",
                    b.key
                );
            }
        }
        // Another view's keys are not listed, nor a key this view rebinds.
        match view {
            View::Library => assert!(row_for("remove").is_none() && row_for("write").is_none()),
            View::Sampler => {
                assert!(
                    row_for("play").is_none(),
                    "enter shown as :play in the sampler"
                );
                assert!(row_for("write").unwrap().starts_with("enter"));
                assert!(joined.contains("in the sampler view"));
            }
            _ => {}
        }
    }
    // Keys that run the same command share its row.
    let joined = Case::new(View::Library, &stopped())
        .input(&input)
        .size(100, 80)
        .text();
    assert!(joined.contains(" j down shift-down "), "{joined}");

    // A key bound to nothing is not listed.
    let mut silenced = Keymap::default();
    silenced.bind(None, Key::parse("q").unwrap(), None);
    let joined = Case::new(View::Library, &stopped())
        .input(&input)
        .keys(&silenced)
        .size(100, 80)
        .text();
    assert!(!joined.contains(":quit"), "{joined}");
    let listed_q = joined
        .lines()
        .filter_map(|l| l.split('\u{2502}').nth(2))
        .any(|row| row.split_whitespace().any(|w| w == "q"));
    assert!(!listed_q, "q is listed though bound to nothing:\n{joined}");
}

#[test]
fn the_help_hint_names_the_key_bound_to_the_key_list() {
    use playr_app::action::{Action, Key};
    let hint = |keys: &Keymap| {
        Case::new(View::Library, &stopped())
            .keys(keys)
            .render()
            .into_iter()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap()
    };
    assert!(hint(&Keymap::default()).ends_with("? help"));
    let mut keys = Keymap::default();
    keys.unbind(None, Key::parse("?").unwrap());
    keys.bind(None, Key::parse("f1").unwrap(), Some(Action::Help));
    assert!(hint(&keys).ends_with("f1 help"), "{}", hint(&keys));
    keys.unbind(None, Key::parse("f1").unwrap());
    assert!(!hint(&keys).contains("help"), "{}", hint(&keys));

    // A view that rebinds the key has no hint there.
    let mut keys = Keymap::default();
    keys.bind(Some(View::Library), Key::parse("?").unwrap(), None);
    assert!(!hint(&keys).contains("help"), "{}", hint(&keys));
}

#[test]
fn help_command_lists_every_command_with_its_arguments_by_view() {
    let input = Input::CommandHelp;
    // Tall enough for the whole list; 80 columns, the smallest common width.
    let joined = Case::new(View::Library, &stopped())
        .input(&input)
        .size(80, 64)
        .text();
    for c in playr_app::command::COMMANDS {
        let usage = format!(":{} {}", c.name, c.args);
        assert!(
            joined.contains(usage.trim_end()) && joined.contains(c.help),
            ":{} missing from command help:\n{joined}",
            c.name
        );
    }
    // Row numbers, by a row's text inside the border.
    let row = |text: &str| {
        joined
            .lines()
            .position(|l| {
                let inner = l.trim().trim_matches('\u{2502}').trim();
                inner == text || inner.starts_with(&format!("{text} "))
            })
            .unwrap_or_else(|| panic!("no row {text:?}:\n{joined}"))
    };
    let order = [
        "in every view",
        ":quit",
        "library",
        ":toggle",
        "selection",
        ":remove",
        "playlists",
        ":delete",
    ];
    let rows: Vec<usize> = order.iter().map(|t| row(t)).collect();
    assert!(rows.is_sorted(), "out of order: {order:?} at rows {rows:?}");
}

#[test]
fn a_long_help_list_scrolls_and_stops_at_its_end() {
    let input = Input::CommandHelp;
    let snapshot = stopped();
    let case = || {
        Case::new(View::Library, &snapshot)
            .input(&input)
            .size(80, 24)
    };
    let top = case().text();
    assert!(top.contains(":help") && !top.contains(":rename"), "{top}");
    assert!(top.contains("j k scroll"), "no scroll hint:\n{top}");
    let bottom = case().help_scroll(1000).text();
    assert!(
        bottom.contains(":rename") && !bottom.contains(":help"),
        "{bottom}"
    );
    // Drawing returns the last scroll that changes the list, and no further.
    let end = case().help_scroll(1000).drawn().help_scroll;
    assert!(end > 0 && end < 1000, "{end}");
    assert_eq!(case().help_scroll(end).text(), bottom);
    assert_ne!(case().help_scroll(end - 1).text(), bottom);
    assert_eq!(case().help_scroll(end + 1).drawn().help_scroll, end);
}

#[test]
fn drawing_returns_each_cursor_inside_its_list_and_on_screen() {
    let tracks: Vec<Track> = (0..30)
        .map(|i| track(&format!("t{i}"), "a", "b", 60))
        .collect();
    let playlists: Vec<Playlist> = (0..30)
        .map(|i| Playlist {
            id: i,
            name: format!("p{i}"),
            len: 1,
        })
        .collect();
    let snapshot = stopped();
    let case = |view| {
        Case::new(view, &snapshot)
            .all(&tracks)
            .selection(&tracks)
            .playlists(&playlists)
    };
    // 20 rows less the tabs, the bar and two borders leaves 13 list rows.
    let last = Scroll {
        row: Some(29),
        offset: 17,
    };
    let first = Scroll {
        row: Some(0),
        offset: 0,
    };
    for view in [View::Library, View::Selection, View::Playlists] {
        let lists = case(view).selected(99).drawn().lists;
        let (shown, others) = match view {
            View::Library => (lists.library, [lists.selection, lists.playlists]),
            View::Selection => (lists.selection, [lists.library, lists.playlists]),
            _ => (lists.playlists, [lists.library, lists.selection]),
        };
        assert_eq!(shown, last, "{view:?}");
        // Lists not on screen come back as they went in.
        assert_eq!(others[0].row, Some(99), "{view:?}");
        assert_eq!(others[1].row, Some(99), "{view:?}");
        assert_eq!(
            case(view).drawn().lists,
            Lists {
                library: first,
                selection: first,
                playlists: first,
            }
        );
    }
    let empty = Case::new(View::Library, &snapshot).selected(5).drawn();
    assert_eq!(empty.lists.library, Scroll::default());
}

#[test]
fn a_command_being_typed_is_shown_in_place_of_the_hints() {
    let mut line = playr_app::command::CommandLine::default();
    "seek 1:23".chars().for_each(|c| line.push(c));
    let input = Input::Command(line);
    let joined = Case::new(View::Library, &stopped()).input(&input).text();
    let last = joined.lines().rev().find(|l| !l.trim().is_empty()).unwrap();
    assert_eq!(last.trim(), ":seek 1:23_");
}

// --- level meter ---

fn playing(loudness: Option<f32>, peak: Option<f32>) -> Snapshot {
    let mut snapshot = stopped();
    snapshot.status.state = State::Playing;
    snapshot.loudness = loudness;
    snapshot.peak = peak;
    snapshot
}

/// The last non-blank line of a frame `width` wide.
fn bottom_line(snapshot: &Snapshot, width: u16, message: Option<&str>) -> String {
    let mut case = Case::new(View::Library, snapshot).size(width, 12);
    if let Some(m) = message {
        case = case.message(m);
    }
    case.render()
        .into_iter()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
}

/// The loudness bar's cells, between the first `[` and `]` on `line`.
fn bar_of(line: &str) -> &str {
    let open = line.find('[').expect("no bar");
    let close = line[open..].find(']').expect("no bar end") + open;
    &line[open + 1..close]
}

#[test]
fn the_loudness_bar_fills_the_free_space_on_the_bottom_line() {
    let snapshot = playing(Some(-18.2), Some(-3.1));
    let narrow = bottom_line(&snapshot, 100, None);
    let wide = bottom_line(&snapshot, 160, None);
    let (narrow_cells, wide_cells) = (bar_of(&narrow).len(), bar_of(&wide).len());
    assert!(
        narrow_cells > 40,
        "{narrow_cells} cells at 100 columns: {narrow:?}"
    );
    assert_eq!(
        wide_cells,
        narrow_cells + 60,
        "the bar did not grow with the line"
    );
    assert!(
        narrow.contains("] -18.2 LUFS  pk  -3.1"),
        "readout not beside the bar: {narrow:?}"
    );
    assert!(
        narrow.contains("vol [########--]"),
        "volume missing: {narrow:?}"
    );
}

#[test]
fn the_bar_fill_and_peak_marker_follow_the_level() {
    let line = bottom_line(&playing(Some(-18.2), Some(-3.1)), 120, None);
    let bar = bar_of(&line);
    let cells = bar.len() as f32;
    // -40 dB to 0 across the bar: -18.2 LUFS fills 54.5%, the -3.1 peak sits at 92.25%.
    let filled = bar.chars().filter(|c| *c == '#').count();
    assert_eq!(filled, (0.545 * cells).round() as usize, "{bar}");
    assert_eq!(
        bar.find('|'),
        Some((0.9225 * cells).round() as usize - 1),
        "{bar}"
    );
}

#[test]
fn the_meter_is_hidden_unless_playing() {
    let joined = Case::new(View::Library, &stopped()).text();
    assert!(
        !joined.contains("LUFS"),
        "meter shown while stopped:\n{joined}"
    );
}

#[test]
fn silence_shows_an_empty_meter() {
    let line = bottom_line(&playing(None, None), 100, None);
    assert!(bar_of(&line).chars().all(|c| c == '-'), "{line:?}");
    assert!(line.contains("]    -- LUFS  pk    --"), "{line:?}");
}

#[test]
fn a_message_takes_the_bars_place_and_the_readout_stays() {
    let line = bottom_line(
        &playing(Some(-18.2), Some(-3.1)),
        100,
        Some("added to selection"),
    );
    assert!(line.contains("added to selection"), "{line:?}");
    assert!(line.contains("-18.2 LUFS"), "readout hidden: {line:?}");
    // The volume bar is the only bracketed bar left.
    assert_eq!(line.matches('[').count(), 1, "bar still drawn: {line:?}");
}

#[test]
fn the_meter_never_overflows_a_narrow_terminal() {
    let snapshot = playing(Some(-18.2), Some(-3.1));
    for width in [30u16, 50, 70] {
        let lines = Case::new(View::Library, &snapshot).size(width, 12).render();
        for line in lines {
            assert!(
                line.chars().count() <= width as usize,
                "overflow at {width}: {line:?}"
            );
        }
    }
}

// --- pane titles ---

#[test]
fn panes_do_not_repeat_the_tabs() {
    let tracks = vec![track("So What", "Miles Davis", "Kind of Blue", 545)];
    let playlists = vec![Playlist {
        id: 1,
        name: "late".into(),
        len: 1,
    }];
    for view in [
        View::Library,
        View::Selection,
        View::Playlists,
        View::Sampler,
    ] {
        let lines = Case::new(view, &stopped())
            .all(&tracks)
            .selection(&tracks)
            .playlists(&playlists)
            .render();
        // Row 0 is the tabs; row 1 is the pane's top border.
        assert!(
            lines[0].contains("Library 1"),
            "tabs lost their counts: {:?}",
            lines[0]
        );
        assert!(
            lines[1].chars().all(|c| !c.is_alphanumeric()),
            "{view:?} pane is titled: {:?}",
            lines[1]
        );
    }
}

#[test]
fn search_results_say_how_to_leave_them() {
    let tracks = vec![track("So What", "Miles Davis", "Kind of Blue", 545)];
    let lines = Case::new(View::Library, &stopped())
        .all(&tracks)
        .results(&tracks)
        .render();
    assert!(
        lines[1].contains("Search results (esc clears)"),
        "{:?}",
        lines[1]
    );
}

/// Symbol and foreground colour of each loudness bar cell, drawn `width` wide.
fn bar_cells(snapshot: &Snapshot, width: u16) -> Vec<(String, ratatui::style::Color)> {
    let buf = Case::new(View::Library, snapshot).size(width, 12).buffer();
    let row = (0..buf.area.height)
        .find(|&y| {
            let line: String = (0..width).map(|x| buf[(x, y)].symbol()).collect();
            line.contains("LUFS")
        })
        .expect("no meter row");
    let symbols: Vec<(String, ratatui::style::Color)> = (0..width)
        .map(|x| (buf[(x, row)].symbol().to_string(), buf[(x, row)].fg))
        .collect();
    let open = symbols.iter().position(|(s, _)| s == "[").unwrap();
    let close = symbols[open..].iter().position(|(s, _)| s == "]").unwrap() + open;
    symbols[open + 1..close].to_vec()
}

/// The zone a cell's centre falls in: green below -18 dB, yellow to -6, red above.
fn expected_zone(i: usize, cells: usize) -> ratatui::style::Color {
    use ratatui::style::Color;
    let db = -40.0 * (1.0 - (i as f32 + 0.5) / cells as f32);
    match db {
        _ if db >= -6.0 => Color::Red,
        _ if db >= -18.0 => Color::Yellow,
        _ => Color::Green,
    }
}

#[test]
fn the_bar_is_green_then_yellow_then_red_by_position() {
    use ratatui::style::Color;
    let cells = bar_cells(&playing(Some(-2.0), Some(-1.0)), 120);
    let n = cells.len();
    let mut zones = Vec::new();
    for (i, (symbol, fg)) in cells.iter().enumerate() {
        match symbol.as_str() {
            "#" | "|" => {
                assert_eq!(*fg, expected_zone(i, n), "cell {i} of {n} ({symbol})");
                if zones.last() != Some(fg) {
                    zones.push(*fg);
                }
            }
            "-" => assert_eq!(*fg, Color::DarkGray, "empty cell {i} is coloured"),
            other => panic!("unexpected {other:?} in the bar"),
        }
    }
    assert_eq!(zones, [Color::Green, Color::Yellow, Color::Red]);
    let marker = cells
        .iter()
        .find(|(s, _)| s == "|")
        .expect("no peak marker");
    assert_eq!(marker.1, Color::Red, "a -1 dBFS peak is in the red zone");
}

#[test]
fn a_quiet_signal_stays_green() {
    use ratatui::style::Color;
    let cells = bar_cells(&playing(Some(-30.0), Some(-24.0)), 120);
    for (symbol, fg) in &cells {
        if symbol == "#" || symbol == "|" {
            assert_eq!(*fg, Color::Green, "{symbol} is {fg:?} at -30 LUFS");
        }
    }
}

// --- selection marker ---

#[test]
fn selected_tracks_are_marked_in_the_library_but_not_in_the_selection() {
    let all = vec![
        track("Alpha", "A", "X", 60),
        track("Bravo", "A", "X", 60),
        track("Charlie", "A", "X", 60),
    ];
    let selection = vec![all[0].clone(), all[2].clone()];
    let mut snapshot = stopped();
    snapshot.status.state = State::Playing;
    snapshot.status.queue = vec![std::path::PathBuf::from(&all[0].path)].into();

    let lines = Case::new(View::Library, &snapshot)
        .all(&all)
        .selection(&selection)
        .render();
    // Row 2 onwards are the tracks; the gutter is the two cells after the border.
    let gutter = |row: usize| lines[row].chars().skip(1).take(2).collect::<String>();
    assert_eq!(gutter(2), ">+", "playing and selected: {:?}", lines[2]);
    assert_eq!(gutter(3), "  ", "neither: {:?}", lines[3]);
    assert_eq!(gutter(4), " +", "selected: {:?}", lines[4]);

    let lines = Case::new(View::Selection, &snapshot)
        .selection(&selection)
        .render();
    for row in [2, 3] {
        assert!(
            !lines[row].contains('+'),
            "marked in the selection: {:?}",
            lines[row]
        );
    }
}

#[test]
fn the_mode_shows_on_the_bottom_line_unless_normal() {
    use playr_core::audio::Mode;
    let normal = Case::new(View::Library, &stopped()).text();
    assert!(
        !normal.contains("normal"),
        "normal mode announced:\n{normal}"
    );
    for mode in [Mode::Shuffle, Mode::Repeat, Mode::RepeatOne] {
        let mut snapshot = stopped();
        snapshot.status.mode = mode;
        let joined = Case::new(View::Library, &snapshot).text();
        assert!(
            joined.contains(mode.name()),
            "{mode:?} not shown:\n{joined}"
        );
    }
}

#[test]
fn the_rename_prompt_names_the_playlist() {
    let input = Input::RenamePlaylist {
        from: Playlist {
            id: 1,
            name: "late".into(),
            len: 3,
        },
        name: "night".into(),
    };
    let joined = Case::new(View::Playlists, &stopped()).input(&input).text();
    assert!(joined.contains("rename \"late\" to: night_"), "{joined}");
}

#[test]
fn the_progress_label_is_dark_over_the_filled_part_and_default_past_it() {
    use ratatui::style::Color;
    let mut snapshot = stopped();
    snapshot.status.state = State::Playing;
    snapshot.status.duration = Some(Duration::from_secs(100));
    snapshot.position = Duration::from_secs(50);
    for theme in [Theme::Dark, Theme::Light] {
        let buf = Case::new(View::Library, &snapshot)
            .size(100, 12)
            .theme(theme)
            .buffer();
        let row = (0..buf.area.height)
            .find(|&y| {
                let line: String = (0..100).map(|x| buf[(x, y)].symbol()).collect();
                line.contains("0:50 / 1:40")
            })
            .expect("no progress bar");
        // By cell: the filled part's blocks are several bytes each.
        let cells: Vec<&str> = (0..100).map(|x| buf[(x, row)].symbol()).collect();
        let start = cells.windows(4).position(|w| w.concat() == "0:50").unwrap() as u16;
        let label: Vec<_> = (start..start + 11).map(|x| &buf[(x, row)]).collect();
        // Half played: the label starts over the filled part and ends past it.
        let p = playr::ui::palette::of(theme);
        let (filled, past): (Vec<&ratatui::buffer::Cell>, Vec<_>) =
            label.iter().partition(|c| c.bg == p.progress);
        assert!(
            !filled.is_empty() && !past.is_empty(),
            "{theme:?}: {label:?}"
        );
        assert!(
            past.iter().all(|c| c.bg == Color::Reset),
            "{theme:?}: {past:?}"
        );
        assert!(
            filled.iter().all(|c| c.fg == p.progress_text),
            "{theme:?}: {filled:?}"
        );
        assert!(
            past.iter().all(|c| c.fg == Color::Reset),
            "{theme:?}: {past:?}"
        );
    }
}

// --- marks ---

#[test]
fn marks_show_under_the_progress_bar_where_they_fall() {
    let mut snapshot = stopped();
    snapshot.status.state = State::Playing;
    snapshot.status.duration = Some(Duration::from_secs(100));
    snapshot.marks = vec![
        Duration::ZERO,
        Duration::from_secs(50),
        Duration::from_secs(100),
    ];
    let lines = Case::new(View::Library, &snapshot).size(100, 12).render();
    let bar = lines
        .iter()
        .position(|l| l.contains("0:00 / 1:40"))
        .expect("no progress bar");
    let ticks = &lines[bar + 1];
    let columns: Vec<usize> = ticks.match_indices('^').map(|(i, _)| i).collect();
    // The bar is inset one cell either side: 98 cells from column 1.
    let at = |fraction: f64| 1 + (fraction * 97.0).round() as usize;
    assert_eq!(columns, [at(0.0), at(0.5), at(1.0)], "ticks row: {ticks:?}");

    snapshot.status.duration = None;
    let lines = Case::new(View::Library, &snapshot).size(100, 12).render();
    assert!(
        !lines.iter().any(|l| l.contains('^')),
        "marks drawn without a duration"
    );
}

// --- sampler ---

/// Paused at `position` in `path`, with marks at `marks`.
fn sampling(path: &str, position: Duration, marks: &[Duration]) -> Snapshot {
    let mut snapshot = stopped();
    snapshot.status.state = State::Paused;
    snapshot.status.queue = std::sync::Arc::from(vec![std::path::PathBuf::from(path)]);
    snapshot.position = position;
    snapshot.marks = marks.to_vec();
    snapshot
}

#[test]
fn without_colour_no_cell_is_coloured_and_the_cursor_row_is_reversed() {
    use playr_app::sampler::Display;
    use ratatui::style::{Color, Modifier};
    let tracks = vec![track("Alpha", "A", "X", 60), track("Beta", "B", "Y", 90)];
    let snapshot = playing(Some(-2.0), Some(-1.0));
    let row_of = |buf: &ratatui::buffer::Buffer, text: &str| {
        (0..buf.area.height)
            .find(|&y| {
                let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
                line.contains(text)
            })
            .expect("no such row")
    };
    let reversed =
        |buf: &ratatui::buffer::Buffer, y: u16| buf[(5, y)].modifier.contains(Modifier::REVERSED);

    let case = || Case::new(View::Library, &snapshot).all(&tracks).selected(1);
    let buf = case().buffer();
    let beta = row_of(&buf, "Beta");
    assert_eq!(
        buf[(5, beta)].bg,
        Color::DarkGray,
        "in colour, a background"
    );
    assert!(!reversed(&buf, beta));

    let buf = case().no_colour().buffer();
    let uncoloured = |buf: &ratatui::buffer::Buffer| {
        buf.content()
            .iter()
            .all(|c| (c.fg, c.bg) == (Color::Reset, Color::Reset))
    };
    assert!(uncoloured(&buf), "library with the meter");
    assert!(reversed(&buf, row_of(&buf, "Beta")), "cursor row");
    assert!(!reversed(&buf, row_of(&buf, "Alpha")), "other rows");

    let paused = sampling(
        "/m/t.wav",
        Duration::from_secs(1),
        &[Duration::from_secs(2)],
    );
    let buf = Case::new(View::Sampler, &paused)
        .sampler(sampler_with("/m/t.wav", Display::Envelope))
        .no_colour()
        .buffer();
    assert!(uncoloured(&buf), "sampler envelope");
}

#[test]
fn the_light_theme_draws_only_from_the_256_colour_table() {
    use playr_app::sampler::Display;
    use ratatui::style::Color;
    let fixed = |buf: &ratatui::buffer::Buffer| {
        buf.content()
            .iter()
            .flat_map(|c| [c.fg, c.bg])
            .all(|c| matches!(c, Color::Reset | Color::Indexed(16..)))
    };
    let tracks = vec![track("Alpha", "A", "X", 60), track("Beta", "B", "Y", 90)];
    let snapshot = playing(Some(-2.0), Some(-1.0));
    let buf = Case::new(View::Library, &snapshot)
        .all(&tracks)
        .selection(&tracks[..1])
        .theme(Theme::Light)
        .buffer();
    assert!(fixed(&buf), "library with the meter");
    assert_eq!(buf[(5, 2)].bg, playr::ui::palette::LIGHT.selected_bg);

    let paused = sampling(
        "/m/t.wav",
        Duration::from_secs(1),
        &[Duration::from_secs(2)],
    );
    for display in [Display::Envelope, Display::Braille] {
        let buf = Case::new(View::Sampler, &paused)
            .sampler(sampler_with("/m/t.wav", display))
            .theme(Theme::Light)
            .buffer();
        assert!(fixed(&buf), "sampler, {display:?}");
    }
    // `dark` is the ANSI set, as `system` is.
    let dark = Case::new(View::Library, &snapshot)
        .all(&tracks)
        .theme(Theme::Dark)
        .buffer();
    let system = Case::new(View::Library, &snapshot)
        .all(&tracks)
        .theme(Theme::System)
        .buffer();
    assert_eq!(dark, system);
}

/// Four seconds at 1,600 frames a second, so 100 ms columns fall on whole
/// peak buckets: quiet to 2 s, full scale to 3 s, then silence.
fn sampler_with(path: &str, display: playr_app::sampler::Display) -> Sampler {
    let data: Vec<f32> = (0..6400)
        .map(|i| match i {
            0..3200 => 0.1,
            3200..4800 => -1.0,
            _ => 0.0,
        })
        .collect();
    Sampler {
        wave: playr_app::sampler::Wave::Ready {
            path: path.into(),
            peaks: std::sync::Arc::new(playr_core::wave::Peaks::from_interleaved(&data, 1, 1600)),
        },
        display,
        ..Sampler::default()
    }
}

/// Characters 1..=40 of `line`: the 40 columns inside the pane's borders.
fn columns(line: &str) -> Vec<char> {
    line.chars().skip(1).take(40).collect()
}

#[test]
fn the_sampler_draws_the_envelope_with_marks_region_and_playhead() {
    use playr_app::sampler::Display;
    let ms = Duration::from_millis;
    let snapshot = sampling("/m/t.wav", ms(1500), &[ms(1000), ms(2500)]);
    // 42 columns: 40 inside the borders, 100 frames or 100 ms each.
    let case = || {
        Case::new(View::Sampler, &snapshot)
            .sampler(sampler_with("/m/t.wav", Display::Envelope))
            .size(42, 20)
    };
    let lines = case().render();
    // The title is cut at this width; the full one is checked wider.
    assert!(
        lines[1].contains("envelope  0:00.000-0:04.000  1 col = 10"),
        "{:?}",
        lines[1]
    );
    for (display, name) in [
        (Display::Envelope, "envelope"),
        (Display::Braille, "braille"),
    ] {
        let wide = Case::new(View::Sampler, &snapshot)
            .sampler(sampler_with("/m/t.wav", display))
            .size(100, 20)
            .render();
        assert!(
            wide.iter()
                .any(|l| l.contains("region 0:01.000-0:02.500 (1.500 s)  marks 2")),
            "{wide:?}"
        );
        assert!(
            wide[1].contains(&format!("{name}  0:00.000-0:04.000  1 col = 60 ms  t.wav")),
            "{:?}",
            wide[1]
        );
    }
    // Rows 2 to 12 are the waveform, 13 the axis, 14 the detail line.
    let full = '\u{2588}';
    let top = columns(&lines[2]);
    assert!(top[20..30].iter().all(|&c| c == full), "{top:?}");
    assert!(
        top[..20].iter().chain(&top[30..]).all(|&c| c == ' '),
        "{top:?}"
    );
    let bottom = columns(&lines[12]);
    assert!(bottom[..30].iter().all(|&c| c == full), "{bottom:?}");
    assert!(bottom[30..].iter().all(|&c| c == ' '), "{bottom:?}");

    let axis = columns(&lines[13]);
    let marked: Vec<(usize, char)> = axis
        .iter()
        .enumerate()
        .filter(|(_, c)| **c != ' ')
        .map(|(i, c)| (i, *c))
        .collect();
    assert_eq!(marked, [(10, '|'), (15, '^'), (25, '|')]);
    assert!(
        lines[14]
            .trim_start_matches('\u{2502}')
            .starts_with("region 0:01.000-0:02.500 (1.500 s)"),
        "{:?}",
        lines[14]
    );

    // The playhead column is yellow; inside the region the waveform is the
    // accent colour, and outside it dim.
    use ratatui::style::Color;
    let buf = case().buffer();
    let fg = |column: u16| buf[(1 + column, 12)].fg;
    assert_eq!(fg(15), Color::Yellow);
    assert_eq!(fg(12), Color::Cyan);
    assert_eq!(fg(28), Color::Gray);
}

#[test]
fn planned_slices_are_drawn_before_they_are_written() {
    use playr_app::sampler::Display;
    use playr_core::samples::Plan;
    let ms = Duration::from_millis;
    let snapshot = sampling("/m/t.wav", ms(1500), &[ms(1000), ms(2500)]);
    let mut sampler = sampler_with("/m/t.wav", Display::Envelope);
    sampler.pending = Some(Plan {
        job: playr_core::samples::Job {
            path: "/m/t.wav".into(),
            rate: 1600,
            marks: vec![1600, 4000],
            at: 2400,
            cut: playr_core::samples::Cut::Equal(2),
            range: None,
            samples: "/tmp".into(),
        },
        spans: vec![(1600, Some(2880)), (2880, Some(4000))],
    });
    let lines = Case::new(View::Sampler, &snapshot)
        .sampler(sampler)
        .size(42, 20)
        .render();
    let axis = columns(&lines[13]);
    // The edge at 1.8 s shows; edges on a mark show the mark.
    assert_eq!(axis[18], '+');
    assert_eq!((axis[10], axis[25]), ('|', '|'));
    assert!(
        lines[14]
            .trim_start_matches('\u{2502}')
            .starts_with("2 slices planned: enter writes, esc d"),
        "{:?}",
        lines[14]
    );
}

#[test]
fn the_braille_display_draws_the_same_waveform_around_a_centre_line() {
    use playr_app::sampler::Display;
    let snapshot = sampling("/m/t.wav", Duration::ZERO, &[]);
    let lines = Case::new(View::Sampler, &snapshot)
        .sampler(sampler_with("/m/t.wav", Display::Braille))
        .size(42, 20)
        .render();
    for line in &lines[2..13] {
        assert!(
            columns(line)
                .iter()
                .all(|c| ('\u{2800}'..='\u{28ff}').contains(c)),
            "not Braille: {line:?}"
        );
    }
    // Full scale reaches the bottom dots of the lowest row; 0.1 does not.
    let bottom = columns(&lines[12]);
    let bits = |c: char| c as u32 - 0x2800;
    assert_eq!(
        bits(bottom[25]) & 0xc0,
        0xc0,
        "loud column misses the bottom"
    );
    assert_eq!(bits(bottom[5]), 0, "quiet column reaches the bottom");
    let top = columns(&lines[2]);
    assert_eq!(bits(top[25]), 0, "a negative-only column reaches the top");
}

#[test]
fn the_sampler_says_why_there_is_no_waveform() {
    use playr_app::sampler::Wave;
    let idle = Case::new(View::Sampler, &stopped()).text();
    assert!(idle.contains("Nothing is playing"), "{idle}");

    let snapshot = sampling("/m/t.wav", Duration::ZERO, &[]);
    let reading = Sampler {
        wave: Wave::Reading {
            path: "/m/t.wav".into(),
            job: 1,
        },
        ..Sampler::default()
    };
    let text = Case::new(View::Sampler, &snapshot).sampler(reading).text();
    assert!(text.contains("Reading the waveform"), "{text}");

    // Peaks of another track are not drawn for this one.
    let stale = sampler_with("/m/other.wav", playr_app::sampler::Display::Envelope);
    let text = Case::new(View::Sampler, &snapshot).sampler(stale).text();
    assert!(text.contains("Reading the waveform"), "{text}");

    let failed = Sampler {
        wave: Wave::Failed {
            path: "/m/t.wav".into(),
            error: "no decodable audio track".into(),
        },
        ..Sampler::default()
    };
    let text = Case::new(View::Sampler, &snapshot).sampler(failed).text();
    assert!(
        text.contains("Cannot read the waveform: no decodable audio track"),
        "{text}"
    );
}

#[test]
fn a_short_sampler_view_keeps_its_axis_and_detail_lines() {
    let ms = Duration::from_millis;
    let snapshot = sampling("/m/t.wav", ms(1500), &[ms(1000)]);
    for height in [7, 8, 9] {
        let text = Case::new(View::Sampler, &snapshot)
            .sampler(sampler_with(
                "/m/t.wav",
                playr_app::sampler::Display::Envelope,
            ))
            .size(42, height)
            .text();
        assert!(text.contains("region 0:01.000"), "height {height}:\n{text}");
    }
}

#[test]
fn the_envelope_shades_rms_inside_the_peak() {
    use playr_app::sampler::{Display, Wave};
    use ratatui::style::Color;
    // A full-scale square wave has RMS equal to its peak; a sine's RMS is
    // 0.707 of its peak. Four seconds at 1,600 frames a second.
    let data: Vec<f32> = (0..6400)
        .map(|i| {
            let phase = (i % 32) as f32 / 32.0;
            if i < 3200 {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            } else {
                (std::f32::consts::TAU * phase).sin()
            }
        })
        .collect();
    let sampler = Sampler {
        wave: Wave::Ready {
            path: "/m/t.wav".into(),
            peaks: std::sync::Arc::new(playr_core::wave::Peaks::from_interleaved(&data, 1, 1600)),
        },
        display: Display::Envelope,
        ..Sampler::default()
    };
    // No marks: the whole track is the region. The playhead is at column 0.
    let snapshot = sampling("/m/t.wav", Duration::ZERO, &[]);
    let buf = Case::new(View::Sampler, &snapshot)
        .sampler(sampler)
        .size(42, 20)
        .buffer();
    // Rows 2 to 12 are the waveform: 11 rows, 88 eighths.
    let cell = |column: u16, row: u16| &buf[(1 + column, row)];
    for row in 2..=12 {
        assert_eq!(cell(10, row).symbol(), "\u{2588}", "square wave, row {row}");
        assert_eq!(cell(10, row).fg, Color::Cyan, "square wave, row {row}");
    }
    // The sine: RMS fills 62 of 88 eighths, 7 full rows and 6 eighths; the
    // peak fills the rest in the darker shade.
    assert_eq!(cell(30, 2).fg, Color::Indexed(30), "peak above the RMS");
    assert_eq!(cell(30, 12).fg, Color::Cyan, "RMS at the bottom");
    assert_eq!(cell(30, 5).symbol(), "\u{2586}");
    assert_eq!(
        (cell(30, 5).fg, cell(30, 5).bg),
        (Color::Cyan, Color::Indexed(30))
    );
    assert_eq!(cell(30, 4).symbol(), "\u{2588}");
    assert_eq!(cell(30, 4).fg, Color::Indexed(30));
}

#[test]
fn the_sampler_draws_the_range_and_snap_and_returns_its_columns() {
    use playr_app::sampler::{Display, Range, Scale};
    let ms = Duration::from_millis;
    let snapshot = sampling("/m/t.wav", ms(1500), &[ms(1000), ms(2500)]);
    let ranged = |start, end| Sampler {
        snap: true,
        range: Some(Range {
            path: "/m/t.wav".into(),
            start,
            end,
        }),
        ..sampler_with("/m/t.wav", Display::Envelope)
    };
    let axis_of = |lines: &[String]| -> Vec<(usize, char)> {
        columns(&lines[13])
            .into_iter()
            .enumerate()
            .filter(|(_, c)| *c != ' ')
            .collect()
    };
    // 40 columns of 160 frames, 100 ms each: the range runs 0.5 s to 3.5 s.
    let case = |sampler| {
        Case::new(View::Sampler, &snapshot)
            .sampler(sampler)
            .size(42, 20)
    };
    let lines = case(ranged(Some(800), Some(5_600))).render();
    assert_eq!(
        axis_of(&lines),
        [(5, '['), (10, '|'), (15, '^'), (25, '|'), (35, ']')]
    );
    assert!(
        lines[14]
            .trim_start_matches('\u{2502}')
            .starts_with("range 0:00.500-0:03.500 (3.000 s)"),
        "{:?}",
        lines[14]
    );
    let drawn = case(ranged(Some(800), Some(5_600))).drawn();
    assert_eq!(
        drawn.scale,
        Some(Scale {
            start: 0,
            per_column: 160,
            per_frame: 1,
            columns: 40
        })
    );
    let wide = Case::new(View::Sampler, &snapshot)
        .sampler(ranged(None, None))
        .size(100, 20)
        .render();
    assert!(
        wide[1].contains("1 col = 60 ms  snap  t.wav"),
        "{:?}",
        wide[1]
    );
    let mut looping = snapshot.clone();
    looping.status.looping = Some((800, 5_600));
    let wide = Case::new(View::Sampler, &looping)
        .sampler(ranged(None, None))
        .size(100, 20)
        .render();
    assert!(wide[1].contains("snap  loop  t.wav"), "{:?}", wide[1]);

    // The end edge keys move is reversed; the start is chosen at first.
    use ratatui::style::Modifier;
    let reversed = |sampler: Sampler, column: u16| {
        let buf = case(sampler).buffer();
        buf[(1 + column, 13)].modifier.contains(Modifier::REVERSED)
    };
    assert!(reversed(ranged(Some(800), Some(5_600)), 5));
    assert!(!reversed(ranged(Some(800), Some(5_600)), 35));
    let end = Sampler {
        edge: playr_app::sampler::Edge::End,
        ..ranged(Some(800), Some(5_600))
    };
    assert!(reversed(end.clone(), 35) && !reversed(end, 5));

    // One end alone is drawn, and the region stays between the marks.
    let lines = case(ranged(Some(800), None)).render();
    assert_eq!(axis_of(&lines)[0], (5, '['));
    assert!(
        lines[14].contains("region 0:01.000-0:02.500"),
        "{:?}",
        lines[14]
    );
}

#[test]
fn at_a_frame_a_cell_braille_fills_both_dot_columns() {
    use playr_app::sampler::Display;
    let snapshot = sampling("/m/t.wav", Duration::from_millis(2_500), &[]);
    let sampler = Sampler {
        zoom: 50,
        ..sampler_with("/m/t.wav", Display::Braille)
    };
    let case = || {
        Case::new(View::Sampler, &snapshot)
            .sampler(sampler.clone())
            .size(42, 20)
    };
    assert_eq!(case().drawn().scale.map(|s| s.per_column), Some(1));
    let wide = Case::new(View::Sampler, &snapshot)
        .sampler(sampler.clone())
        .size(100, 20)
        .render();
    assert!(wide[1].contains("1 col = 1 frame"), "{:?}", wide[1]);
    let lines = case().render();
    // Left dot column bits, then right: each cell holds one frame in both.
    let (left, right) = (0x01 | 0x02 | 0x04 | 0x40, 0x08 | 0x10 | 0x20 | 0x80);
    let cells: Vec<u32> = lines[2..13]
        .iter()
        .flat_map(|l| columns(l))
        .filter_map(|c| {
            (c as u32)
                .checked_sub(0x2800)
                .filter(|&b| b > 0 && b < 0x100)
        })
        .collect();
    assert!(!cells.is_empty(), "no waveform drawn");
    for bits in cells {
        assert_eq!(
            (bits & left).count_ones(),
            (bits & right).count_ones(),
            "{bits:#x}"
        );
    }
}

#[test]
fn drawing_returns_the_zoom_clamped_to_the_track() {
    use playr_app::sampler::Display;
    let zoomed = |zoom| Sampler {
        zoom,
        ..sampler_with("/m/t.wav", Display::Envelope)
    };
    let snapshot = sampling("/m/t.wav", Duration::ZERO, &[]);
    let draw = |sampler| {
        Case::new(View::Sampler, &snapshot)
            .sampler(sampler)
            .size(42, 20)
            .drawn()
            .zoom
    };
    // 6,400 frames over 40 columns is 160 a column; seven halvings reach one
    // frame a cell, the deepest a terminal goes.
    assert_eq!(draw(zoomed(1)), 1);
    assert_eq!(draw(zoomed(50)), 7);
    // With no waveform to show, the zoom is kept for when there is one.
    let reading = Sampler {
        zoom: 50,
        ..Sampler::default()
    };
    assert_eq!(draw(reading), 50);
}

#[test]
fn the_db_display_draws_levels_from_minus_48_db() {
    use playr_app::sampler::Display;
    let snapshot = sampling("/m/t.wav", Duration::ZERO, &[]);
    let lines = Case::new(View::Sampler, &snapshot)
        .sampler(sampler_with("/m/t.wav", Display::Decibels))
        .size(100, 20)
        .render();
    assert!(lines[1].contains("db  0:00.000-0:04.000"), "{:?}", lines[1]);
    // The quiet part is -20 dB: 28 of 48 dB, so 51 of the 88 eighths in 11
    // rows. On the linear scale it would fill 9.
    let column = |row: usize| lines[row].chars().nth(2).unwrap();
    for row in 7..=12 {
        assert_eq!(column(row), '\u{2588}', "row {row}");
    }
    assert_eq!(column(6), '\u{2583}');
    assert_eq!(column(5), ' ');
}
