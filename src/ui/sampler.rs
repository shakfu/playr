//! Drawing the sampler view's waveform in terminal cells.
//!
//! Three displays show the same peaks. The envelope draws each column's RMS
//! level inside its peak level, rising from a baseline in eighth blocks, 8
//! levels a row, and scaled to the loudest sample in the track. The dB display
//! draws the same bars from `DB_FLOOR` to full scale, which spreads out the
//! quiet and middle levels a linear scale squeezes into the bottom rows. The
//! Braille display draws the waveform around a centre line, 2 dots across and
//! 4 down a cell, which shows its shape.
//! Only these glyphs are not ASCII; everything else in the view is. The view's
//! state and column geometry are in `playr_app::sampler`.

/// Eighth blocks from empty to full.
const EIGHTHS: [char; 9] = [
    ' ', '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
    '\u{2588}',
];

/// What part of the envelope a cell, or the space behind its glyph, shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    Empty,
    Rms,
    Peak,
}

/// One cell of the envelope: a glyph in the colour of `fg`, over `behind`.
/// A cell where the RMS bar ends under more of the peak bar needs both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub glyph: char,
    pub fg: Fill,
    pub behind: Fill,
}

/// Rows, top first, drawing each column's `(rms, peak)`, from 0 to 1, as an
/// RMS bar inside a peak bar, both rising from the bottom of `height` rows.
/// Where the peak bar ends in the same cell as the RMS bar, the cell shows the
/// RMS bar alone: a cell has one glyph, so it cannot draw both ends.
pub fn envelope_rows(columns: &[(f32, f32)], height: usize) -> Vec<Vec<Cell>> {
    let eighths = |v: f32| (v.clamp(0.0, 1.0) * (8 * height) as f32).round() as usize;
    let steps: Vec<(usize, usize)> = columns
        .iter()
        .map(|&(rms, peak)| {
            let rms = eighths(rms);
            (rms, eighths(peak).max(rms))
        })
        .collect();
    (0..height)
        .map(|row| {
            let below = 8 * (height - 1 - row);
            steps
                .iter()
                .map(|&(rms, peak)| {
                    let cell = |glyph, fg, behind| Cell { glyph, fg, behind };
                    if rms >= below + 8 {
                        cell(EIGHTHS[8], Fill::Rms, Fill::Empty)
                    } else if rms > below {
                        let behind = if peak >= below + 8 {
                            Fill::Peak
                        } else {
                            Fill::Empty
                        };
                        cell(EIGHTHS[rms - below], Fill::Rms, behind)
                    } else if peak > below {
                        cell(EIGHTHS[(peak - below).min(8)], Fill::Peak, Fill::Empty)
                    } else {
                        cell(' ', Fill::Empty, Fill::Empty)
                    }
                })
                .collect()
        })
        .collect()
}

/// Rows, top first, drawing `extents`, (min, max) pairs from -1 to 1, as dot
/// columns two to a cell around the centre of `height` rows.
pub fn braille_rows(extents: &[(f32, f32)], height: usize) -> Vec<String> {
    let dots = 4 * height;
    // Dot row 0 is the top; +1 maps to the top row and -1 to the bottom.
    let row_of = |v: f32| ((1.0 - v.clamp(-1.0, 1.0)) / 2.0 * (dots - 1) as f32).round() as usize;
    // Bit for the dot at (column 0 or 1, row 0 to 3) within a cell.
    const BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
    let cells = extents.len().div_ceil(2);
    let mut grid = vec![vec![0u32; cells]; height];
    for (i, &(lo, hi)) in extents.iter().enumerate() {
        if lo > hi {
            continue;
        }
        for dot in row_of(hi)..=row_of(lo) {
            grid[dot / 4][i / 2] |= BITS[i % 2][dot % 4];
        }
    }
    grid.iter()
        .map(|row| {
            row.iter()
                .map(|&bits| char::from_u32(0x2800 + bits).expect("in the Braille block"))
                .collect()
        })
        .collect()
}
