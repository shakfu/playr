//! Drawing. Reads app state, writes widgets, changes nothing.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Tabs};
use ratatui::Frame;

use super::{fmt_time, state_glyph, Input, Screen, View};
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
            format!("{:<w$.w$} ", t.display_artist(), w = aw),
            Style::default().fg(Color::Green),
        ),
        Span::styled(
            format!("{:<w$.w$} ", t.display_album(), w = alw),
            Style::default().fg(dim),
        ),
        Span::raw(format!("{:<w$.w$} ", t.display_title(), w = tw)),
        Span::styled(format!("{dur:>6}"), Style::default().fg(dim)),
    ]);
    ListItem::new(line)
}

fn list_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT),
        ))
}

fn draw_library(app: &mut Screen<'_>, f: &mut Frame, area: Rect) {
    let current = app.snapshot.status.current().cloned();
    let tracks = app.visible().to_vec();
    let selected = app.library_state.selected();
    let items: Vec<ListItem> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let playing = current
                .as_ref()
                .map(|p| p.as_os_str() == t.path.as_str())
                .unwrap_or(false);
            track_line(t, area.width, playing, selected == Some(i))
        })
        .collect();

    let title = match app.input {
        Input::Search(q) => format!("Search: {q}_"),
        _ => match app.results {
            Some(r) => format!("Results ({})", r.len()),
            None => format!("Library ({})", app.all.len()),
        },
    };

    let empty = items.is_empty();
    let list = List::new(items).block(list_block(&title)).highlight_style(
        Style::default()
            .bg(SELECTED_BG)
            .add_modifier(Modifier::BOLD),
    );
    f.render_stateful_widget(list, area, app.library_state);

    if empty {
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
    let status = app.snapshot.status.clone();
    let selected = app.queue_state.selected();
    let items: Vec<ListItem> = app
        .queue
        .iter()
        .enumerate()
        .map(|(i, t)| {
            track_line(
                t,
                area.width,
                i == status.index && status.state != State::Stopped,
                selected == Some(i),
            )
        })
        .collect();
    let empty = items.is_empty();
    let title = format!("Queue ({})", app.queue.len());
    let list = List::new(items).block(list_block(&title)).highlight_style(
        Style::default()
            .bg(SELECTED_BG)
            .add_modifier(Modifier::BOLD),
    );
    f.render_stateful_widget(list, area, app.queue_state);

    if empty {
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
                Span::styled(
                    format!("{:<40.40}", p.name),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(format!("{:>4} tracks", p.len), Style::default().fg(dim)),
            ]))
        })
        .collect();
    let empty = items.is_empty();
    let title = format!("Playlists ({})", app.playlists.len());
    let list = List::new(items).block(list_block(&title)).highlight_style(
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
    spans.push(Span::styled(
        format!("  vol {:.0}%", app.snapshot.volume * 100.0),
        Style::default().fg(DIM),
    ));
    // Only shown when it is not normal, so the usual case stays uncluttered.
    if status.semitones != 0 {
        let speed = crate::audio::speed_for(status.semitones);
        spans.push(Span::styled(
            format!("  {speed:.2}x ({:+} st)", status.semitones),
            Style::default().fg(Color::Yellow),
        ));
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

    let text = match (app.input, app.message) {
        (Input::SavePlaylist(name), _) => {
            Line::from(vec![Span::styled("save playlist as: ", Style::default().fg(ACCENT)), Span::raw(format!("{name}_"))])
        }
        (_, Some(msg)) => Line::from(Span::styled(msg.to_string(), Style::default().fg(Color::Yellow))),
        _ => Line::from(Span::styled(
            "tab views  / search  enter play  a queue  s save  space pause  n/p track  arrows seek  [ ] speed  +/- vol  q quit",
            Style::default().fg(DIM),
        )),
    };
    f.render_widget(Paragraph::new(text), hints);
}
