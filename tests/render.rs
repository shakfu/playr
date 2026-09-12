//! Rendering tests against a headless backend. No audio device involved.

use playr::audio::{Spec, State, Status};
use playr::db::query::Playlist;
use playr::db::Track;
use playr::ui::{render, Input, Screen, Snapshot, View};
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
    queue: &'a [Track],
    playlists: &'a [Playlist],
    input: &'a Input,
    message: Option<&'a str>,
    width: u16,
    height: u16,
}

impl<'a> Case<'a> {
    fn new(view: View, snapshot: &'a Snapshot) -> Self {
        Case {
            view,
            snapshot,
            all: &[],
            queue: &[],
            playlists: &[],
            input: &Input::None,
            message: None,
            width: 100,
            height: 20,
        }
    }

    fn all(mut self, v: &'a [Track]) -> Self {
        self.all = v;
        self
    }

    fn queue(mut self, v: &'a [Track]) -> Self {
        self.queue = v;
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

    fn size(mut self, width: u16, height: u16) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Draws one frame and returns it as plain text lines.
    fn render(self) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(self.width, self.height)).unwrap();
        let mut ls = ListState::default();
        ls.select(if self.all.is_empty() { None } else { Some(0) });
        let mut qs = ListState::default();
        qs.select(if self.queue.is_empty() { None } else { Some(0) });
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
                    results: None,
                    queue: self.queue,
                    playlists: self.playlists,
                    input: self.input,
                    message: self.message,
                    library_state: &mut ls,
                    queue_state: &mut qs,
                    playlist_state: &mut ps,
                };
                render::draw(&mut screen, f);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
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
    }
}

/// Returns cells on `row` whose foreground equals their background.
///
/// Such a cell renders as a blank: the text is there but cannot be read.
fn invisible_cells(
    view: View,
    all: &[Track],
    queue: &[Track],
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
    let mut qs = sel(queue);
    let mut ps = ListState::default();
    ps.select((!playlists.is_empty()).then_some(0));

    terminal
        .draw(|f| {
            let mut screen = Screen {
                view,
                snapshot: &snapshot,
                all,
                results: None,
                queue,
                playlists,
                input: &Input::None,
                message: None,
                library_state: &mut ls,
                queue_state: &mut qs,
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

    for (view, all, queue, pls) in [
        (View::Library, &tracks[..], &[][..], &[][..]),
        (View::Queue, &[][..], &tracks[..], &[][..]),
        (View::Playlists, &[][..], &[][..], &playlists[..]),
    ] {
        let hidden = invisible_cells(view, all, queue, pls, 2);
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
    let queue = vec![track("So What", "Miles Davis", "Kind of Blue", 545)];
    let snapshot = Snapshot {
        status: Status {
            state: State::Playing,
            queue: vec![std::path::PathBuf::from("/m/So What.flac")],
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
    };
    let joined = Case::new(View::Queue, &snapshot).queue(&queue).text();
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
    let joined = Case::new(View::Queue, &snapshot).text();
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

// --- queue cursor following playback ---

use playr::ui::follow_target;

#[test]
fn the_cursor_moves_to_the_track_that_starts_playing() {
    // Track 2 starts while the cursor sits on track 1.
    assert_eq!(follow_target(Some(0), Some(1), Some(0)), Some(1));
}

#[test]
fn the_cursor_is_placed_when_a_queue_first_loads() {
    assert_eq!(follow_target(None, Some(3), None), Some(3));
}

#[test]
fn the_cursor_is_left_alone_while_a_track_keeps_playing() {
    // Scrolled to track 12 during track 4; nothing has changed since, so the
    // cursor must not be dragged back on every frame.
    assert_eq!(follow_target(Some(4), Some(4), Some(12)), None);
}

#[test]
fn the_cursor_is_left_alone_when_nothing_is_playing() {
    assert_eq!(follow_target(Some(2), None, Some(7)), None);
}

#[test]
fn browsing_is_interrupted_only_by_a_real_track_change() {
    // Scroll away during track 4, then let track 5 begin.
    let mut followed = Some(4usize);

    // Scrolled away to track 12 while track 4 keeps playing.
    let mut selected = Some(12usize);
    for _ in 0..5 {
        if let Some(t) = follow_target(followed, Some(4), selected) {
            followed = Some(t);
            selected = Some(t);
        }
    }
    assert_eq!(
        selected,
        Some(12),
        "scrolling was fought while the track played"
    );

    // Track 5 starts.
    if let Some(t) = follow_target(followed, Some(5), selected) {
        followed = Some(t);
        selected = Some(t);
    }
    assert_eq!(selected, Some(5), "cursor did not follow the new track");
    assert_eq!(followed, Some(5));
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

    let joined = Case::new(View::Queue, &snapshot).text();
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
    let joined = Case::new(View::Queue, &snapshot).text();
    assert!(
        !joined.contains(" st)"),
        "speed shown when normal:\n{joined}"
    );
}
