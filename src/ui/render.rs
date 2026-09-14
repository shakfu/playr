//! Drawing. Reads app state, writes widgets, changes nothing.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Tabs};
use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

use super::action::Keymap;
use super::command::{self, view_name, COMMANDS};
use super::sampler::{self, Display, Fill, Wave};
use super::{fmt_time, state_glyph, Input, Screen, Snapshot, View};
use playr_core::audio::State;
use playr_core::db::Track;

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
        View::Selection => draw_selection(app, f, body),
        View::Playlists => draw_playlists(app, f, body),
        View::Sampler => draw_sampler(app, f, body),
    }
    draw_bar(app, f, bar);
    match app.input {
        Input::Help => {
            let rows = key_rows(app.keys, app.view);
            let rows: Vec<(&str, &str)> =
                rows.iter().map(|(k, c)| (k.as_str(), c.as_str())).collect();
            let name = format!("Keys in the {} view", view_name(app.view));
            draw_help(f, f.area(), &name, &rows, app.help_scroll);
        }
        Input::CommandHelp => {
            let mut rows: Vec<(String, &str)> = Vec::new();
            let mut group = None;
            for c in COMMANDS {
                if rows.is_empty() || c.view != group {
                    group = c.view;
                    let heading = c.view.map_or("in every view", view_name);
                    rows.push((heading.to_string(), ""));
                }
                rows.push((format!(":{} {}", c.name, c.args), c.help));
            }
            let rows: Vec<(&str, &str)> = rows.iter().map(|(k, h)| (k.trim_end(), *h)).collect();
            draw_help(f, f.area(), "Commands", &rows, app.help_scroll);
        }
        _ => {}
    }
}

/// The keys that work in `view`: its own bindings, then those for every view
/// that it does not rebind. Keys that run the same command share a row, and a
/// key bound to nothing is left out. A heading row has no command.
fn key_rows(keys: &Keymap, view: View) -> Vec<(String, String)> {
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
            let command = format!(":{}", command::line(action, Some(view)));
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

/// A list of keys or commands and what they do, centred over everything else.
/// A row with no description is a heading. `scroll` is clamped to the rows
/// that can scroll into view.
fn draw_help(f: &mut Frame, area: Rect, name: &str, rows: &[(&str, &str)], scroll: &mut usize) {
    let key_width = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    let action_width = rows.iter().map(|(_, a)| a.len()).max().unwrap_or(0);
    let lines: Vec<Line> = rows
        .iter()
        .map(|(keys, action)| match *action {
            "" => Line::from(Span::styled(
                format!(" {keys}"),
                Style::default().fg(DIM).add_modifier(Modifier::BOLD),
            )),
            _ => Line::from(vec![
                Span::styled(
                    format!(" {keys:<key_width$}  "),
                    Style::default().fg(ACCENT),
                ),
                Span::raw(*action),
            ]),
        })
        .collect();
    let height = (rows.len() as u16 + 2).min(area.height);
    let visible = height.saturating_sub(2) as usize;
    *scroll = (*scroll).min(rows.len().saturating_sub(visible));
    let title = if rows.len() > visible {
        format!("{name}: j k scroll, any other key closes")
    } else {
        format!("{name}: any key closes")
    };
    // Keys, two spaces, the action, a space either side, and the borders; or
    // the title with its corners, if that is wider.
    let width = (key_width + action_width + 6).max(title.len() + 4);
    let width = (width as u16).min(area.width);
    let popup = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    f.render_widget(Clear, popup);
    f.render_widget(
        Paragraph::new(lines)
            .scroll((*scroll as u16, 0))
            .block(list_block(&title)),
        popup,
    );
}

/// The playing track's waveform, with its region, marks, playhead and any
/// slices planned, above an axis row marking them and a line of detail.
fn draw_sampler(app: &mut Screen<'_>, f: &mut Frame, area: Rect) {
    let current = app.snapshot.status.current();
    let name = current
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    let peaks = match (&app.sampler.wave, current) {
        (Wave::Ready { path, peaks }, Some(playing)) if path == playing => Some(peaks.clone()),
        _ => None,
    };
    let Some(peaks) = peaks else {
        let hint = match (&app.sampler.wave, current) {
            (_, None) => "Nothing is playing. Play a track to see its waveform.".to_string(),
            (Wave::Failed { error, .. }, _) => format!("Cannot read the waveform: {error}"),
            _ => "Reading the waveform...".to_string(),
        };
        let block = list_block(name.as_deref().unwrap_or(""));
        let inner = block.inner(area).inner(ratatui::layout::Margin {
            horizontal: 1,
            vertical: 0,
        });
        f.render_widget(block, area);
        f.render_widget(Paragraph::new(hint).style(Style::default().fg(DIM)), inner);
        return;
    };

    let rate = peaks.rate.max(1);
    let frame_of = |d: std::time::Duration| (d.as_secs_f64() * rate as f64).round() as u64;
    let at = frame_of(app.snapshot.position);
    let inner_width = area.width.saturating_sub(2).max(1) as u64;
    let (start, per_column, zoom) =
        sampler::window(peaks.frames, inner_width, app.sampler.zoom, at);
    app.sampler.zoom = zoom;
    let shown_end = (start + per_column * inner_width).min(peaks.frames);

    let ms = per_column as f64 * 1000.0 / rate as f64;
    let scale = if ms < 10.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{:.0} ms", ms)
    };
    // The file name last: it is the longest part, and the bar below names the
    // track too, so a narrow terminal cuts it rather than the view's scale.
    let title = format!(
        "{}  {}-{}  1 col = {scale}  {}",
        app.sampler.display.name(),
        sampler::fmt_frames(start, rate),
        sampler::fmt_frames(shown_end, rate),
        name.as_deref().unwrap_or(""),
    );
    let block = list_block(&title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let width = inner.width as usize;
    let rows = inner.height.saturating_sub(2) as usize;

    let marks: Vec<u64> = app.snapshot.marks.iter().map(|&d| frame_of(d)).collect();
    let (region_start, region_end) = playr_core::samples::region(&marks, at);
    let region_end = region_end.unwrap_or(peaks.frames);
    let column_of = |frame: u64| {
        (frame >= start && frame < start + per_column * width as u64)
            .then(|| ((frame - start) / per_column) as usize)
    };
    let loudest = peaks.loudest().max(f32::MIN_POSITIVE);
    let span_of = |c: usize| {
        let a = start + c as u64 * per_column;
        (a, a + per_column)
    };

    let playhead = column_of(at);
    // Where a column is: under the playhead, in the region, or outside it.
    let colours = |c: usize| {
        let (a, b) = span_of(c);
        if Some(c) == playhead {
            (Color::Yellow, Color::Indexed(136))
        } else if a < region_end && b > region_start {
            (ACCENT, Color::Indexed(30))
        } else {
            (Color::Gray, DIM)
        }
    };
    let mut lines: Vec<Line> = match app.sampler.display {
        Display::Envelope | Display::Decibels => {
            let height = |magnitude: f32| match app.sampler.display {
                Display::Decibels => sampler::db_height(magnitude),
                _ => magnitude / loudest,
            };
            let columns: Vec<(f32, f32)> = (0..width)
                .map(|c| {
                    let (a, b) = span_of(c);
                    peaks.range(a, b).map_or((0.0, 0.0), |e| {
                        (height(e.rms), height(e.min.abs().max(e.max.abs())))
                    })
                })
                .collect();
            sampler::envelope_rows(&columns, rows)
                .iter()
                .map(|row| {
                    runs(row.iter().enumerate().map(|(c, cell)| {
                        let (rms, peak) = colours(c);
                        let colour = |fill| match fill {
                            Fill::Rms => Some(rms),
                            Fill::Peak => Some(peak),
                            Fill::Empty => None,
                        };
                        let mut style = Style::default();
                        if let Some(fg) = colour(cell.fg) {
                            style = style.fg(fg);
                        }
                        if let Some(bg) = colour(cell.behind) {
                            style = style.bg(bg);
                        }
                        (cell.glyph, style)
                    }))
                })
                .collect()
        }
        Display::Braille => {
            let extents: Vec<(f32, f32)> = (0..width)
                .flat_map(|c| {
                    let (a, b) = span_of(c);
                    let mid = a + (b - a).div_ceil(2);
                    [(a, mid), (mid, b)]
                })
                .map(|(a, b)| {
                    peaks
                        .range(a, b)
                        .map_or((1.0, -1.0), |e| (e.min / loudest, e.max / loudest))
                })
                .collect();
            sampler::braille_rows(&extents, rows)
                .iter()
                .map(|row| {
                    runs(
                        row.chars()
                            .enumerate()
                            .map(|(c, g)| (g, Style::default().fg(colours(c).0))),
                    )
                })
                .collect()
        }
    };

    // Planned slice edges, then marks, then the playhead: the later wins a column.
    let mut axis: Vec<(char, Style)> = vec![(' ', Style::default()); width];
    if let Some(pending) = &app.sampler.pending {
        let edges = pending
            .spans
            .iter()
            .flat_map(|&(a, b)| [Some(a), b])
            .flatten();
        for c in edges.filter_map(column_of) {
            axis[c] = ('+', Style::default().fg(Color::Magenta));
        }
    }
    for c in marks.iter().filter_map(|&m| column_of(m)) {
        axis[c] = ('|', Style::default().fg(Color::Yellow));
    }
    if let Some(c) = playhead {
        axis[c] = (
            '^',
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    }
    lines.push(runs(axis.into_iter()));

    let region = format!(
        "region {}-{} ({:.3} s)  marks {}",
        sampler::fmt_frames(region_start, rate),
        sampler::fmt_frames(region_end, rate),
        (region_end - region_start) as f64 / rate as f64,
        marks.len(),
    );
    // First, so a narrow terminal cuts the region detail rather than the keys.
    let plan = match (&app.sampler.pending, app.sampler.planning) {
        (_, true) => "planning slices  ".to_string(),
        (Some(p), _) => format!(
            "{} slices planned: enter writes, esc discards  ",
            p.spans.len()
        ),
        (None, _) => String::new(),
    };
    lines.push(Line::from(vec![
        Span::styled(plan, Style::default().fg(Color::Magenta)),
        Span::styled(region, Style::default().fg(DIM)),
    ]));
    // A short view keeps the axis and detail lines and loses waveform rows.
    let skip = lines.len().saturating_sub(inner.height as usize);
    f.render_widget(
        Paragraph::new(lines.split_off(skip.min(lines.len()))),
        inner,
    );
}

/// A line of styled characters, with each run of one style in one span.
fn runs(cells: impl Iterator<Item = (char, Style)>) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut text = String::new();
    let mut style = None;
    for (ch, s) in cells {
        if style.is_some_and(|current| current != s) {
            spans.push(Span::styled(
                std::mem::take(&mut text),
                style.expect("checked"),
            ));
        }
        style = Some(s);
        text.push(ch);
    }
    if let Some(s) = style {
        spans.push(Span::styled(text, s));
    }
    Line::from(spans)
}

fn draw_tabs(app: &Screen<'_>, f: &mut Frame, area: Rect) {
    let views = [
        View::Library,
        View::Selection,
        View::Playlists,
        View::Sampler,
    ];
    let titles = views.map(|v| {
        let n = match v {
            View::Library => app.visible().len(),
            View::Selection => app.selection.len(),
            View::Playlists => app.playlists.len(),
            View::Sampler => return format!(" {} ", v.title()),
        };
        format!(" {} {} ", v.title(), n)
    });
    let selected = views.iter().position(|v| *v == app.view).unwrap_or(0);
    let tabs = Tabs::new(titles.to_vec())
        .select(selected)
        .style(Style::default().fg(DIM))
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
        .divider("");
    f.render_widget(tabs, area);
}

/// Fixed cost of a track row: 2 for the play and selection markers, 1 space
/// after each of the three text columns, 6 for the duration, 2 for the borders.
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

/// One track row. `playing` marks it `>`, `marked` (in the selection) `+`, and
/// `cursor` gives it the cursor row's dimmed colours.
fn track_line(
    t: &Track,
    width: u16,
    playing: bool,
    marked: bool,
    cursor: bool,
) -> ListItem<'static> {
    let (aw, alw, tw) = columns(width);
    let dim = if cursor { DIM_SELECTED } else { DIM };
    let dur = t
        .duration_ms
        .map(|ms| fmt_time(std::time::Duration::from_millis(ms.max(0) as u64)))
        .unwrap_or_else(|| "-".into());
    let line = Line::from(vec![
        Span::styled(if playing { ">" } else { " " }, Style::default().fg(ACCENT)),
        Span::styled(
            if marked { "+" } else { " " },
            Style::default().fg(Color::Yellow),
        ),
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
    marked: impl Fn(&Track) -> bool,
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
            track_line(t, area.width, playing(i, t), marked(t), selected == Some(i))
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

    let selected: std::collections::HashSet<&str> =
        app.selection.iter().map(|t| t.path.as_str()).collect();
    draw_tracks(
        f,
        area,
        &title,
        tracks,
        app.library_state,
        |_, t| current.is_some_and(|p| p.as_os_str() == t.path.as_str()),
        |t| selected.contains(t.path.as_str()),
    );

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

fn draw_selection(app: &mut Screen<'_>, f: &mut Frame, area: Rect) {
    let status = &app.snapshot.status;
    let current = status.current();
    // Every row here is selected, so no row is marked.
    draw_tracks(
        f,
        area,
        "",
        app.selection,
        app.selection_state,
        |_, t| current.is_some_and(|p| p.as_os_str() == t.path.as_str()),
        |_| false,
    );

    if app.selection.is_empty() {
        let inner = area.inner(ratatui::layout::Margin {
            horizontal: 2,
            vertical: 1,
        });
        f.render_widget(
            Paragraph::new("Selection is empty. Press a in the library to add tracks, then s to save them as a playlist.")
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
            Paragraph::new("No playlists. Build a selection, then press s to save it.")
                .style(Style::default().fg(DIM)),
            inner,
        );
    }
}

/// Now-playing line, progress bar, and the key hints.
fn draw_bar(app: &Screen<'_>, f: &mut Frame, area: Rect) {
    let status = &app.snapshot.status;
    let pos = app.snapshot.position;

    let [now, progress, ticks, hints] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 0,
    }));

    // Prefer the tagged title from the playing list over the bare file name.
    let label = app
        .playing
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
    draw_marks(app, f, ticks);

    // A prompt needs the whole line; anything else shares it with the indicators.
    let prompt = match app.input {
        Input::SavePlaylist(name) => Some(Line::from(vec![
            Span::styled("save playlist as: ", Style::default().fg(ACCENT)),
            Span::raw(format!("{name}_")),
        ])),
        Input::RenamePlaylist { from, name } => Some(Line::from(vec![
            Span::styled(
                format!("rename \"{}\" to: ", from.name),
                Style::default().fg(ACCENT),
            ),
            Span::raw(format!("{name}_")),
        ])),
        Input::Command(line) => Some(Line::from(vec![
            Span::styled(":", Style::default().fg(ACCENT)),
            Span::raw(format!("{}_", line.text)),
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
    // Whatever key opens the key list in this view, if one does.
    let help_key = app
        .keys
        .bindings()
        .iter()
        .filter(|b| b.action == Some(crate::ui::action::Action::Help))
        .map(|b| b.key)
        .find(|&key| app.keys.lookup(key, app.view) == Some(&crate::ui::action::Action::Help))
        .map(|key| key.to_string());
    let indicators = indicators(status.semitones, app.snapshot, playing, help_key);
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

/// Marks as `^` under the progress bar, each where it falls in the track.
fn draw_marks(app: &Screen<'_>, f: &mut Frame, area: Rect) {
    let marks = &app.snapshot.marks;
    let Some(total) = app.snapshot.status.duration.filter(|d| !d.is_zero()) else {
        return;
    };
    let width = area.width as usize;
    if marks.is_empty() || width == 0 {
        return;
    }
    let mut row = vec![' '; width];
    for mark in marks {
        // The same scale as the gauge: the start at the first cell, the end at the last.
        let fraction = (mark.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0);
        row[(fraction * (width - 1) as f64).round() as usize] = '^';
    }
    let row: String = row.into_iter().collect();
    f.render_widget(
        Paragraph::new(Span::styled(row, Style::default().fg(Color::Yellow))),
        area,
    );
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
fn indicators(
    semitones: i32,
    snapshot: &Snapshot,
    playing: bool,
    help_key: Option<String>,
) -> Line<'static> {
    let volume = snapshot.volume;
    let mut spans = Vec::new();
    if playing {
        spans.extend(level_readout(snapshot.loudness, snapshot.peak));
    }
    // Mode and speed show only when not normal, so the usual case stays uncluttered.
    let mode = snapshot.status.mode;
    if mode != playr_core::audio::Mode::Normal {
        spans.push(Span::styled(
            format!("{}  ", mode.name()),
            Style::default().fg(Color::Yellow),
        ));
    }
    if semitones != 0 {
        let speed = playr_core::audio::speed_for(semitones);
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
    if let Some(key) = help_key {
        spans.push(Span::styled(
            format!("  {key} help"),
            Style::default().fg(ACCENT),
        ));
    }
    Line::from(spans)
}
