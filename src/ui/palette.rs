//! The terminal's colour roles, in two sets. [`ANSI`] names ANSI colours, so
//! the terminal's theme shades it; it serves `system` and `dark`. [`LIGHT`]
//! uses the 256-colour table, which themes rarely change, for a light
//! background whatever the theme's ANSI colours are.

use playr_app::Theme;
use ratatui::style::Color;

/// What each colour marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Titles, keys, prompts, and the waveform in the region.
    pub accent: Color,
    /// Envelope peaks behind [`Palette::accent`].
    pub accent_peak: Color,
    /// The progress bar. Its time is in the default colour, except over the
    /// filled part, where it is [`Palette::progress_text`].
    pub progress: Color,
    pub progress_text: Color,
    pub dim: Color,
    /// Background of the selected row.
    pub selected_bg: Color,
    /// Secondary text on the selected row, where [`Palette::dim`] may not show.
    pub dim_selected: Color,
    /// Artist and playlist names.
    pub name: Color,
    /// Messages, confirmations, and a mode or speed that is not normal.
    pub notice: Color,
    /// Marks, the playhead, and tracks in the selection.
    pub mark: Color,
    /// Envelope peaks behind [`Palette::mark`].
    pub mark_peak: Color,
    /// Planned slices.
    pub edge: Color,
    /// A peak at full scale.
    pub clip: Color,
    /// The waveform outside the region.
    pub outside: Color,
    pub outside_peak: Color,
    /// The level meter's zones.
    pub green: Color,
    pub yellow: Color,
    pub red: Color,
}

pub const ANSI: Palette = Palette {
    accent: Color::Cyan,
    // ANSI has no darker cyan or yellow, and bright variants equal normal
    // ones in some themes, so these two come from the 256-colour table.
    accent_peak: Color::Indexed(30),
    progress: Color::Cyan,
    progress_text: Color::Black,
    dim: Color::DarkGray,
    selected_bg: Color::DarkGray,
    // `dim` is the same colour as `selected_bg`.
    dim_selected: Color::Gray,
    name: Color::Green,
    notice: Color::Yellow,
    mark: Color::Yellow,
    mark_peak: Color::Indexed(136),
    edge: Color::Magenta,
    clip: Color::Red,
    outside: Color::Gray,
    outside_peak: Color::DarkGray,
    green: Color::Green,
    yellow: Color::Yellow,
    red: Color::Red,
};

pub const LIGHT: Palette = Palette {
    accent: Color::Indexed(24),
    accent_peak: Color::Indexed(110),
    // Light enough for black text over it, dark enough to see on white.
    progress: Color::Indexed(67),
    progress_text: Color::Indexed(16),
    dim: Color::Indexed(243),
    selected_bg: Color::Indexed(254),
    dim_selected: Color::Indexed(240),
    name: Color::Indexed(22),
    notice: Color::Indexed(94),
    mark: Color::Indexed(94),
    mark_peak: Color::Indexed(179),
    edge: Color::Indexed(127),
    clip: Color::Indexed(160),
    outside: Color::Indexed(243),
    outside_peak: Color::Indexed(250),
    green: Color::Indexed(28),
    yellow: Color::Indexed(136),
    red: Color::Indexed(160),
};

/// The spectrogram's ramp, quiet to loud: the 256-colour entries nearest
/// matplotlib's magma in OKLab, in both themes. Each is lighter than the last.
pub const MAGMA: [u8; 16] = [
    16, 232, 233, 17, 53, 54, 90, 126, 161, 167, 203, 209, 216, 222, 223, 229,
];

/// The set `theme` draws in.
pub fn of(theme: Theme) -> &'static Palette {
    match theme {
        Theme::System | Theme::Dark => &ANSI,
        Theme::Light => &LIGHT,
    }
}
