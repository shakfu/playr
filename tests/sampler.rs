//! Drawing the sampler view's waveform in terminal cells.

use playr::ui::sampler::{braille_rows, envelope_rows};

#[test]
fn the_envelope_draws_rms_inside_peak_in_eighths() {
    use playr::ui::sampler::{Cell, Fill};
    // Two rows, 16 eighths. Columns as (rms, peak).
    let rows = envelope_rows(
        &[
            (0.0, 0.0),
            (0.0, 1.0 / 16.0),
            (0.25, 1.0),
            (0.5, 0.75),
            (1.0, 1.0),
            (0.8, 0.2),
        ],
        2,
    );
    let text = |row: &Vec<Cell>| row.iter().map(|c| c.glyph).collect::<String>();
    assert_eq!(text(&rows[0]), "  \u{2588}\u{2584}\u{2588}\u{2585}");
    assert_eq!(text(&rows[1]), " \u{2581}\u{2584}\u{2588}\u{2588}\u{2588}");
    let cell = |glyph, fg, behind| Cell { glyph, fg, behind };
    // A peak alone.
    assert_eq!(rows[1][1], cell('\u{2581}', Fill::Peak, Fill::Empty));
    // RMS a quarter up, peak to the top: the RMS glyph over the peak's colour,
    // then the peak filling the row above.
    assert_eq!(rows[1][2], cell('\u{2584}', Fill::Rms, Fill::Peak));
    assert_eq!(rows[0][2], cell('\u{2588}', Fill::Peak, Fill::Empty));
    // RMS to half, peak ending in the next cell: that cell draws the peak alone.
    assert_eq!(rows[1][3], cell('\u{2588}', Fill::Rms, Fill::Empty));
    assert_eq!(rows[0][3], cell('\u{2584}', Fill::Peak, Fill::Empty));
    // A peak below the RMS is drawn as the RMS: no peak shows.
    assert_eq!(rows[0][5], cell('\u{2585}', Fill::Rms, Fill::Empty));
}

#[test]
fn braille_draws_each_dot_column_between_its_extremes() {
    // One row: four dot rows, top +1 and bottom -1. A column from -1 to +1
    // fills all four dots; one at the top fills only the top dot.
    let rows = braille_rows(&[(-1.0, 1.0), (1.0, 1.0)], 1);
    // Left column all dots (1,2,3,7), right column top dot (4).
    assert_eq!(rows, ["\u{284f}"]);

    // Two rows, eight dot rows, with no row exactly at 0. A quiet column from
    // -0.1 to 0.1 spans the two middle dot rows: the bottom dot of the top cell
    // (0x40) and the top dot of the bottom cell (0x01). Silence rounds to the
    // lower of the two (0x08, right column), and -1 to the bottom dot (0x40).
    let rows = braille_rows(&[(-0.1, 0.1), (0.0, 0.0), (-1.0, -1.0)], 2);
    assert_eq!(rows[0], "\u{2840}\u{2800}");
    assert_eq!(rows[1], "\u{2809}\u{2840}");
    // An empty column, as past the end of the track, draws nothing.
    assert_eq!(braille_rows(&[(1.0, -1.0)], 1), ["\u{2800}"]);
}
