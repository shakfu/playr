//! Drawing. Reads app state, writes widgets, changes nothing.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Tabs};
use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

use super::sampler::{self as glyphs, Fill};
use super::{
    confirm_prompt, state_glyph, view_title, Drawn, Input, Screen, Scroll, Snapshot, View,
};
use playr_app::action::Action;
use playr_app::command::{self, view_name};
use playr_app::message::fmt_time;
use playr_app::sampler::{self, Display};
use playr_app::{meter, model};
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

/// Draws a frame, and returns the scroll positions, cursors, help scroll and
/// zoom it settled on, for the next frame to start from.
pub fn draw(app: &Screen<'_>, f: &mut Frame) -> Drawn {
    let [tabs_area, body, bar] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(4),
    ])
    .areas(f.area());

    let mut drawn = Drawn {
        lists: app.lists,
        help_scroll: app.help_scroll,
        zoom: app.sampler.zoom,
    };
    draw_tabs(app, f, tabs_area);
    match app.view {
        View::Library => drawn.lists.library = draw_library(app, f, body),
        View::Selection => drawn.lists.selection = draw_selection(app, f, body),
        View::Playlists => drawn.lists.playlists = draw_playlists(app, f, body),
        View::Sampler => drawn.zoom = draw_sampler(app, f, body),
    }
    draw_bar(app, f, bar);
    drawn.help_scroll = match app.input {
        Input::Help => {
            let rows = command::key_rows(app.keys, app.view);
            let rows: Vec<(&str, &str)> =
                rows.iter().map(|(k, c)| (k.as_str(), c.as_str())).collect();
            let name = format!("Keys in the {} view", view_name(app.view));
            draw_help(f, f.area(), &name, &rows, app.help_scroll)
        }
        Input::CommandHelp => {
            let rows = command::command_rows();
            let rows: Vec<(&str, &str)> =
                rows.iter().map(|(k, h)| (k.as_str(), h.as_str())).collect();
            draw_help(f, f.area(), "Commands", &rows, app.help_scroll)
        }
        _ => app.help_scroll,
    };
    drawn
}

/// A list of keys or commands and what they do, centred over everything else.
/// A row with no description is a heading. Returns `scroll` clamped to the
/// rows that can scroll into view.
fn draw_help(f: &mut Frame, area: Rect, name: &str, rows: &[(&str, &str)], scroll: usize) -> usize {
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
    let scroll = scroll.min(rows.len().saturating_sub(visible));
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
            .scroll((scroll as u16, 0))
            .block(list_block(&title)),
        popup,
    );
    scroll
}

/// The playing track's waveform, with its region, marks, playhead and any
/// slices planned, above an axis row marking them and a line of detail.
/// Returns the zoom, clamped to what the track and width allow.
fn draw_sampler(app: &Screen<'_>, f: &mut Frame, area: Rect) -> u32 {
    let current = app.snapshot.status.current();
    let name = current
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    let peaks = match sampler::peaks_of(app.sampler, current) {
        Ok(peaks) => peaks,
        Err(hint) => {
            let block = list_block(name.as_deref().unwrap_or(""));
            let inner = block.inner(area).inner(ratatui::layout::Margin {
                horizontal: 1,
                vertical: 0,
            });
            f.render_widget(block, area);
            f.render_widget(Paragraph::new(hint).style(Style::default().fg(DIM)), inner);
            return app.sampler.zoom;
        }
    };

    let inner_width = area.width.saturating_sub(2).max(1) as u64;
    let layout = sampler::Layout::new(
        peaks,
        inner_width,
        app.sampler.zoom,
        app.snapshot.position,
        &app.snapshot.marks,
    );
    let zoom = layout.zoom;
    // The file name last: it is the longest part, and the bar below names the
    // track too, so a narrow terminal cuts it rather than the view's scale.
    let title = format!(
        "{}  {}  1 col = {}  {}",
        app.sampler.display.name(),
        layout.shown(),
        layout.scale(),
        name.as_deref().unwrap_or(""),
    );
    let block = list_block(&title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return zoom;
    }
    let width = inner.width as usize;
    let rows = inner.height.saturating_sub(2) as usize;

    let playhead = layout.playhead();
    // Where a column is: under the playhead, in the region, or outside it.
    let colours = |c: usize| {
        if Some(c) == playhead {
            (Color::Yellow, Color::Indexed(136))
        } else if layout.in_region(c) {
            (ACCENT, Color::Indexed(30))
        } else {
            (Color::Gray, DIM)
        }
    };
    let mut lines: Vec<Line> = match app.sampler.display {
        Display::Envelope | Display::Decibels => {
            let columns: Vec<(f32, f32)> = (0..width)
                .map(|c| layout.heights(app.sampler.display, c))
                .collect();
            glyphs::envelope_rows(&columns, rows)
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
                    let (a, b) = layout.span_of(c);
                    let mid = a + (b - a).div_ceil(2);
                    [(a, mid), (mid, b)]
                })
                .map(|(a, b)| layout.extent(a, b))
                .collect();
            glyphs::braille_rows(&extents, rows)
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
        for c in sampler::edges(pending).filter_map(|e| layout.column_of(e)) {
            axis[c] = ('+', Style::default().fg(Color::Magenta));
        }
    }
    for c in layout.marks.iter().filter_map(|&m| layout.column_of(m)) {
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

    // First, so a narrow terminal cuts the region detail rather than the keys.
    let mut plan = sampler::plan_text(app.sampler);
    if !plan.is_empty() {
        plan.push_str("  ");
    }
    lines.push(Line::from(vec![
        Span::styled(plan, Style::default().fg(Color::Magenta)),
        Span::styled(layout.region_text(), Style::default().fg(DIM)),
    ]));
    // A short view keeps the axis and detail lines and loses waveform rows.
    let skip = lines.len().saturating_sub(inner.height as usize);
    f.render_widget(
        Paragraph::new(lines.split_off(skip.min(lines.len()))),
        inner,
    );
    zoom
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
            View::Sampler => return format!(" {} ", view_title(v)),
        };
        format!(" {} {} ", view_title(v), n)
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

/// Draws the rows of `tracks` that fit in `area`, and returns `scroll` with its
/// row kept inside the list and on screen.
///
/// Only those rows are formatted. Handing `List` every track would format the
/// whole library on every frame, which is 200,000 strings at 50,000 tracks.
fn draw_tracks(
    f: &mut Frame,
    area: Rect,
    title: &str,
    tracks: &[Track],
    scroll: Scroll,
    playing: impl Fn(usize, &Track) -> bool,
    marked: impl Fn(&Track) -> bool,
) -> Scroll {
    let len = tracks.len();
    let selected = scroll.row.map(|s| s.min(len.saturating_sub(1)));
    let rows = area.height.saturating_sub(2) as usize;
    let offset = scroll_offset(scroll.offset, selected, rows, len);

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
    Scroll {
        row: if len == 0 { None } else { selected },
        offset,
    }
}

fn draw_library(app: &Screen<'_>, f: &mut Frame, area: Rect) -> Scroll {
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
    let scroll = draw_tracks(
        f,
        area,
        &title,
        tracks,
        app.lists.library,
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
    scroll
}

fn draw_selection(app: &Screen<'_>, f: &mut Frame, area: Rect) -> Scroll {
    let status = &app.snapshot.status;
    let current = status.current();
    // Every row here is selected, so no row is marked.
    let scroll = draw_tracks(
        f,
        area,
        "",
        app.selection,
        app.lists.selection,
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
    scroll
}

fn draw_playlists(app: &Screen<'_>, f: &mut Frame, area: Rect) -> Scroll {
    let selected = app.lists.playlists.row;
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
    let mut state = ListState::default()
        .with_selected(selected)
        .with_offset(app.lists.playlists.offset);
    f.render_stateful_widget(list, area, &mut state);

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
    Scroll {
        row: state.selected(),
        offset: state.offset(),
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

    let label = model::now_playing(app.playing, status).unwrap_or_else(|| "nothing playing".into());

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
            confirm_prompt(c),
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
        .filter(|b| b.action == Some(Action::Help))
        .map(|b| b.key)
        .find(|&key| app.keys.lookup(key, app.view) == Some(&Action::Help))
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

/// Cells in the volume bar.
const VOLUME_CELLS: usize = 10;

/// Cells of `cells` that a level of `db` fills, from the floor to full scale.
fn meter_cells(db: f32, cells: usize) -> usize {
    (meter::fraction(db) * cells as f32).round() as usize
}

/// Colour of cell `i` of `cells`, by the level at its centre, as on an LED meter.
fn zone_colour(i: usize, cells: usize) -> Color {
    match meter::zone(meter::level_at((i as f32 + 0.5) / cells as f32)) {
        meter::Zone::Red => Color::Red,
        meter::Zone::Yellow => Color::Yellow,
        meter::Zone::Green => Color::Green,
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
    let clipping = peak.is_some_and(meter::clipping);
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
