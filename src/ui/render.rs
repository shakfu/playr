//! Drawing. Reads app state, writes widgets, changes nothing.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Tabs};
use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

use super::{fmt_time, state_glyph, Input, Screen, Snapshot, View, KEYS};
use crate::audio::State;
use crate::db::Track;

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
/// Background of the selected row.
const SELECTED_BG: Color = Color::DarkGray;
/// Secondary text on the selected row.
///
/// [`DIM`] is the same colour as [`SELECTED_BG`], so a dimmed column on the
/// selected row would render as blank space. Secondary text is lightened there
/// instead of being allowed to disappear.
const DIM_SELECTED: Color = Color::Gray;

pub fn draw(app: &mut Screen<'_>, f: &mut Frame) {
    let [tabs_area, body, bar] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(4),
    ])
    .areas(f.area());

    draw_tabs(app, f, tabs_area);
    match app.view {
        View::Library => draw_library(app, f, body),
        View::Queue => draw_queue(app, f, body),
        View::Playlists => draw_playlists(app, f, body),
    }
    draw_bar(app, f, bar);
    if matches!(app.input, Input::Help) {
        draw_help(f, f.area());
    }
}

/// The key list, centred over everything else.
fn draw_help(f: &mut Frame, area: Rect) {
    let key_width = KEYS.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    let action_width = KEYS.iter().map(|(_, a)| a.len()).max().unwrap_or(0);
    let lines: Vec<Line> = KEYS
        .iter()
        .map(|(keys, action)| {
            Line::from(vec![
                Span::styled(
                    format!(" {keys:<key_width$}  "),
                    Style::default().fg(ACCENT),
                ),
                Span::raw(*action),
            ])
        })
        .collect();
    // Keys, two spaces, the action, a space either side, and the borders.
    let width = ((key_width + action_width + 6) as u16).min(area.width);
    let height = (KEYS.len() as u16 + 2).min(area.height);
    let popup = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    f.render_widget(Clear, popup);
    f.render_widget(
        Paragraph::new(lines).block(list_block("Keys: any key closes")),
        popup,
    );
}

fn draw_tabs(app: &Screen<'_>, f: &mut Frame, area: Rect) {
    let titles = [View::Library, View::Queue, View::Playlists].map(|v| {
        let n = match v {
            View::Library => app.visible().len(),
            View::Queue => app.queue.len(),
            View::Playlists => app.playlists.len(),
        };
        format!(" {} {} ", v.title(), n)
    });
    let selected = match app.view {
        View::Library => 0,
        View::Queue => 1,
        View::Playlists => 2,
    };
    let tabs = Tabs::new(titles.to_vec())
        .select(selected)
        .style(Style::default().fg(DIM))
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
        .divider("");
    f.render_widget(tabs, area);
}

/// Fixed cost of a track row: 2 for the play marker, 1 space after each of the
/// three text columns, 6 for the duration, 2 for the block borders.
const ROW_OVERHEAD: usize = 13;

/// Column widths, sized from the available space rather than fixed, so the
/// title column absorbs whatever is left.
///
/// The three widths plus [`ROW_OVERHEAD`] must not exceed the terminal width,
/// or the duration is truncated off the right edge.
fn columns(width: u16) -> (usize, usize, usize) {
    let usable = (width as usize).saturating_sub(ROW_OVERHEAD);
    let artist = (usable / 4).clamp(6, 28).min(usable);
    let album = (usable.saturating_sub(artist) / 3)
        .clamp(6, 28)
        .min(usable - artist);
    let title = usable.saturating_sub(artist + album);
    (artist, album, title)
}

/// `s` cut and padded to exactly `width` terminal cells.
///
/// `format!` pads by character count, so a row of CJK text, two cells a
/// character, came out twice its column width and pushed the duration off.
fn fit(s: &str, width: usize) -> String {
    let mut out = String::with_capacity(width);
    let mut used = 0;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.extend(std::iter::repeat_n(' ', width - used));
    out
}

fn track_line(t: &Track, width: u16, playing: bool, selected: bool) -> ListItem<'static> {
    let (aw, alw, tw) = columns(width);
    let dim = if selected { DIM_SELECTED } else { DIM };
    let dur = t
        .duration_ms
        .map(|ms| fmt_time(std::time::Duration::from_millis(ms.max(0) as u64)))
        .unwrap_or_else(|| "-".into());
    let marker = if playing { ">" } else { " " };
    let line = Line::from(vec![
        Span::styled(format!("{marker} "), Style::default().fg(ACCENT)),
        Span::styled(
            format!("{} ", fit(t.display_artist(), aw)),
            Style::default().fg(Color::Green),
        ),
        Span::styled(
            format!("{} ", fit(t.display_album(), alw)),
            Style::default().fg(dim),
        ),
        Span::raw(format!("{} ", fit(&t.display_title(), tw))),
        Span::styled(format!("{dur:>6}"), Style::default().fg(dim)),
    ]);
    ListItem::new(line)
}

/// A bordered pane, titled unless `title` is empty.
fn list_block(title: &str) -> Block<'_> {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIM));
    if title.is_empty() {
        return block;
    }
    block.title(Span::styled(
        format!(" {title} "),
        Style::default().fg(ACCENT),
    ))
}

/// First row to show so that `selected` is on screen, moving as little as
/// possible from `offset`.
///
/// A list that shrank below the current offset is pulled back, so a short
/// search result is not scrolled out of view.
pub fn scroll_offset(offset: usize, selected: Option<usize>, rows: usize, len: usize) -> usize {
    if rows == 0 || len == 0 {
        return 0;
    }
    let mut offset = offset.min(len.saturating_sub(rows));
    if let Some(s) = selected {
        if s < offset {
            offset = s;
        } else if s >= offset + rows {
            offset = s + 1 - rows;
        }
    }
    offset
}

/// Draws the rows of `tracks` that fit in `area`.
///
/// Only those rows are formatted. Handing `List` every track would format the
/// whole library on every frame, which is 200,000 strings at 50,000 tracks.
fn draw_tracks(
    f: &mut Frame,
    area: Rect,
    title: &str,
    tracks: &[Track],
    state: &mut ListState,
    playing: impl Fn(usize, &Track) -> bool,
) {
    let len = tracks.len();
    let selected = state.selected().map(|s| s.min(len.saturating_sub(1)));
    let rows = area.height.saturating_sub(2) as usize;
    let offset = scroll_offset(state.offset(), selected, rows, len);
    state.select(if len == 0 { None } else { selected });
    *state.offset_mut() = offset;

    let end = (offset + rows).min(len);
    let items: Vec<ListItem> = tracks[offset..end]
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let i = offset + i;
            track_line(t, area.width, playing(i, t), selected == Some(i))
        })
        .collect();
    let list = List::new(items).block(list_block(title)).highlight_style(
        Style::default()
            .bg(SELECTED_BG)
            .add_modifier(Modifier::BOLD),
    );
    let mut window = ListState::default().with_selected(selected.map(|s| s - offset));
    f.render_stateful_widget(list, area, &mut window);
}

fn draw_library(app: &mut Screen<'_>, f: &mut Frame, area: Rect) {
    let current = app.snapshot.status.current();
    let tracks = app.results.unwrap_or(app.all);

    let title = match app.input {
        // The tabs already name the view and count it; a title says only
        // what they do not.
        Input::Search(q) => format!("Search: {q}_"),
        _ => match app.results {
            Some(_) => "Search results (esc clears)".to_string(),
            None => String::new(),
        },
    };

    draw_tracks(f, area, &title, tracks, app.library_state, |_, t| {
        current.is_some_and(|p| p.as_os_str() == t.path.as_str())
    });

    if tracks.is_empty() {
        let hint = if app.results.is_some() {
            "No matches. Esc clears the search."
        } else {
            "Library is empty. Run: playr scan <directory>"
        };
        let inner = area.inner(ratatui::layout::Margin {
            horizontal: 2,
            vertical: 1,
        });
        f.render_widget(Paragraph::new(hint).style(Style::default().fg(DIM)), inner);
    }
}

fn draw_queue(app: &mut Screen<'_>, f: &mut Frame, area: Rect) {
    let status = &app.snapshot.status;
    draw_tracks(f, area, "", app.queue, app.queue_state, |i, _| {
        i == status.index && status.state != State::Stopped
    });

    if app.queue.is_empty() {
        let inner = area.inner(ratatui::layout::Margin {
            horizontal: 2,
            vertical: 1,
        });
        f.render_widget(
            Paragraph::new("Queue is empty. Press Enter in the library to play from there.")
                .style(Style::default().fg(DIM)),
            inner,
        );
    }
}

fn draw_playlists(app: &mut Screen<'_>, f: &mut Frame, area: Rect) {
    let selected = app.playlist_state.selected();
    let items: Vec<ListItem> = app
        .playlists
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let dim = if selected == Some(i) {
                DIM_SELECTED
            } else {
                DIM
            };
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(fit(&p.name, 40), Style::default().fg(Color::Green)),
                Span::styled(format!("{:>4} tracks", p.len), Style::default().fg(dim)),
            ]))
        })
        .collect();
    let empty = items.is_empty();
    let list = List::new(items).block(list_block("")).highlight_style(
        Style::default()
            .bg(SELECTED_BG)
            .add_modifier(Modifier::BOLD),
    );
    f.render_stateful_widget(list, area, app.playlist_state);

    if empty {
        let inner = area.inner(ratatui::layout::Margin {
            horizontal: 2,
            vertical: 1,
        });
        f.render_widget(
            Paragraph::new("No playlists. Build a queue, then press s to save it.")
                .style(Style::default().fg(DIM)),
            inner,
        );
    }
}

/// Now-playing line, progress bar, and the key hints.
fn draw_bar(app: &Screen<'_>, f: &mut Frame, area: Rect) {
    let status = &app.snapshot.status;
    let pos = app.snapshot.position;

    let [now, progress, hints] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 0,
    }));

    // Prefer the tagged title from the queue over the bare file name.
    let label = app
        .queue
        .get(status.index)
        .map(|t| format!("{} - {}", t.display_title(), t.display_artist()))
        .or_else(|| {
            status.current().map(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
        })
        .unwrap_or_else(|| "nothing playing".into());

    let mut spans = vec![
        Span::styled(
            format!("{} ", state_glyph(status.state)),
            Style::default().fg(ACCENT),
        ),
        Span::raw(label).bold(),
    ];
    if let Some(src) = status.source {
        let khz = src.rate as f64 / 1000.0;
        let mut fmt = format!("  {khz:.1}kHz {}ch", src.channels);
        // Only a genuine rate conversion is worth showing. Varispeed also runs
        // the resampler, but reporting "44.1kHz -> 44.1kHz" for it reads as a
        // fault rather than as the speed change it is.
        if status.output_rate != 0 && status.output_rate != src.rate {
            fmt.push_str(&format!(" -> {:.1}kHz", status.output_rate as f64 / 1000.0));
        }
        spans.push(Span::styled(fmt, Style::default().fg(DIM)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), now);

    let total = status.duration.unwrap_or_default();
    let ratio = if total.is_zero() {
        0.0
    } else {
        (pos.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
    };
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(ACCENT))
        .ratio(ratio)
        .label(format!("{} / {}", fmt_time(pos), fmt_time(total)));
    f.render_widget(gauge, progress);

    // A prompt needs the whole line; anything else shares it with the indicators.
    let prompt = match app.input {
        Input::SavePlaylist(name) => Some(Line::from(vec![
            Span::styled("save playlist as: ", Style::default().fg(ACCENT)),
            Span::raw(format!("{name}_")),
        ])),
        Input::Confirm(c) => Some(Line::from(Span::styled(
            c.prompt(),
            Style::default().fg(Color::Yellow),
        ))),
        _ => None,
    };
    if let Some(prompt) = prompt {
        f.render_widget(Paragraph::new(prompt), hints);
        return;
    }

    let playing = status.state == State::Playing;
    let indicators = indicators(status.semitones, app.snapshot, playing);
    let [left, right] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(indicators.width() as u16),
    ])
    .areas(hints);
    // The bar takes whatever the indicators leave, less a space before them;
    // a message borrows that space while it shows.
    if let Some(msg) = app.message {
        f.render_widget(
            Paragraph::new(Span::styled(msg, Style::default().fg(Color::Yellow))),
            left,
        );
    } else if playing {
        let cells = (left.width as usize).saturating_sub(3);
        if cells > 0 {
            let bar = level_bar(app.snapshot.loudness, app.snapshot.peak, cells);
            f.render_widget(Paragraph::new(bar), left);
        }
    }
    f.render_widget(Paragraph::new(indicators), right);
}

/// Lowest level the loudness bar shows, in dB relative to full scale.
const METER_FLOOR_DB: f32 = -40.0;
/// Cells in the volume bar.
const VOLUME_CELLS: usize = 10;

/// Cells of `cells` that a level of `db` fills, from the floor to full scale.
fn meter_cells(db: f32, cells: usize) -> usize {
    let fraction = (db - METER_FLOOR_DB) / -METER_FLOOR_DB;
    (fraction * cells as f32).round().clamp(0.0, cells as f32) as usize
}

/// Where the bar turns yellow: the EBU R68 digital alignment level.
const YELLOW_FROM_DB: f32 = -18.0;
/// Where the bar turns red: a conventional headroom mark.
const RED_FROM_DB: f32 = -6.0;

/// Colour of cell `i` of `cells`, by the level at its centre, as on an LED meter.
fn zone_colour(i: usize, cells: usize) -> Color {
    let db = METER_FLOOR_DB * (1.0 - (i as f32 + 0.5) / cells as f32);
    if db >= RED_FROM_DB {
        Color::Red
    } else if db >= YELLOW_FROM_DB {
        Color::Yellow
    } else {
        Color::Green
    }
}

/// The loudness bar, `cells` wide, with the held peak marked on the same scale.
///
/// LUFS and dBFS are both relative to full scale, so one bar can show both.
/// Filled cells and the marker take their zone's colour; the numbers beside
/// the bar carry the same reading for anyone who cannot tell the colours apart.
fn level_bar(loudness: Option<f32>, peak: Option<f32>, cells: usize) -> Line<'static> {
    let fill = loudness.map_or(0, |l| meter_cells(l, cells));
    let marker = peak
        .map(|p| meter_cells(p, cells))
        .filter(|c| *c > 0)
        .map(|c| c - 1);
    let dim = Style::default().fg(DIM);
    let mut spans = vec![Span::styled("[", dim)];
    spans.extend((0..cells).map(|i| {
        let zone = Style::default().fg(zone_colour(i, cells));
        match i {
            _ if Some(i) == marker => Span::styled("|", zone),
            _ if i < fill => Span::styled("#", zone),
            _ => Span::styled("-", dim),
        }
    }));
    spans.push(Span::styled("]", dim));
    Line::from(spans)
}

/// Loudness and held peak as numbers.
fn level_readout(loudness: Option<f32>, peak: Option<f32>) -> Vec<Span<'static>> {
    let lufs = loudness.map_or("   --".into(), |l| format!("{l:>5.1}"));
    let pk = peak.map_or("   --".into(), |p| format!("{p:>5.1}"));
    // At full scale the source itself is clipping; playr's gain never exceeds 1.
    let clipping = peak.is_some_and(|p| p >= -0.1);
    vec![
        Span::styled(format!("{lufs} LUFS  "), Style::default().fg(DIM)),
        Span::styled(
            format!("pk {pk}  "),
            Style::default().fg(if clipping { Color::Red } else { DIM }),
        ),
    ]
}

/// The level readout while playing, speed when it is not normal, the volume,
/// and the help key. The readout comes first, next to its bar.
fn indicators(semitones: i32, snapshot: &Snapshot, playing: bool) -> Line<'static> {
    let volume = snapshot.volume;
    let mut spans = Vec::new();
    if playing {
        spans.extend(level_readout(snapshot.loudness, snapshot.peak));
    }
    // Only shown when it is not normal, so the usual case stays uncluttered.
    if semitones != 0 {
        let speed = crate::audio::speed_for(semitones);
        spans.push(Span::styled(
            format!("{speed:.2}x ({semitones:+} st)  "),
            Style::default().fg(Color::Yellow),
        ));
    }
    let filled = (volume.clamp(0.0, 1.0) * VOLUME_CELLS as f32).round() as usize;
    spans.push(Span::styled(
        format!(
            "vol [{}{}] {:>3.0}%",
            "#".repeat(filled),
            "-".repeat(VOLUME_CELLS - filled),
            volume * 100.0
        ),
        Style::default().fg(DIM),
    ));
    spans.push(Span::styled("  ? help", Style::default().fg(ACCENT)));
    Line::from(spans)
}
