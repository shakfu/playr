//! Rendering tests against a headless backend. No audio device involved.

use playr::audio::{Spec, State, Status};
use playr::db::query::Playlist;
use playr::db::Track;
use playr::ui::render::{self, scroll_offset};
use playr::ui::{Input, Screen, Snapshot, View};
use ratatui::backend::TestBackend;
use ratatui::widgets::ListState;
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
    /// Cursor row in the library and selection views; the first row when unset.
    selected: Option<usize>,
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
        let mut terminal = Terminal::new(TestBackend::new(self.width, self.height)).unwrap();
        let mut ls = ListState::default();
        let cursor = self.selected.unwrap_or(0);
        ls.select(if self.all.is_empty() {
            None
        } else {
            Some(cursor)
        });
        let mut qs = ListState::default();
        qs.select(if self.selection.is_empty() {
            None
        } else {
            Some(cursor)
        });
        let mut ps = ListState::default();
        ps.select(if self.playlists.is_empty() {
            None
        } else {
            Some(0)
        });

        terminal
            .draw(|f| {
                let mut screen = Screen {
                    view: self.view,
                    snapshot: self.snapshot,
                    all: self.all,
                    results: self.results,
                    playing: self.playing,
                    selection: self.selection,
                    playlists: self.playlists,
                    input: self.input,
                    message: self.message,
                    library_state: &mut ls,
                    selection_state: &mut qs,
                    playlist_state: &mut ps,
                };
                render::draw(&mut screen, f);
            })
            .unwrap();

        terminal.backend().buffer().clone()
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
    let sel = |v: &[Track]| {
        let mut st = ListState::default();
        st.select((!v.is_empty()).then_some(0));
        st
    };
    let mut ls = sel(all);
    let mut qs = sel(selection);
    let mut ps = ListState::default();
    ps.select((!playlists.is_empty()).then_some(0));

    terminal
        .draw(|f| {
            let mut screen = Screen {
                view,
                snapshot: &snapshot,
                all,
                results: None,
                playing: &[],
                selection,
                playlists,
                input: &Input::None,
                message: None,
                library_state: &mut ls,
                selection_state: &mut qs,
                playlist_state: &mut ps,
            };
            render::draw(&mut screen, f);
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
fn help_lists_every_key() {
    let input = Input::Help;
    let joined = Case::new(View::Library, &stopped())
        .input(&input)
        .size(100, 30)
        .text();
    for (keys, action) in playr::ui::KEYS {
        assert!(
            joined.contains(action),
            "{keys:?} missing from help:\n{joined}"
        );
    }
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

#[test]
fn a_peak_is_held_then_released() {
    use playr::ui::{hold_peak, PEAK_HOLD};
    use std::time::Instant;
    let start = Instant::now();
    let held = hold_peak(None, 0.9, start);
    assert_eq!(held, Some((0.9, start)));
    // A lower reading inside the hold keeps the peak.
    let soon = start + PEAK_HOLD / 2;
    assert_eq!(hold_peak(held, 0.2, soon), held);
    // A higher one replaces it at once.
    assert_eq!(hold_peak(held, 0.95, soon), Some((0.95, soon)));
    // Once the hold has passed, the current reading shows.
    let later = start + PEAK_HOLD;
    assert_eq!(hold_peak(held, 0.2, later), Some((0.2, later)));
    assert_eq!(hold_peak(held, 0.0, later), None);
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
    for view in [View::Library, View::Selection, View::Playlists] {
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
