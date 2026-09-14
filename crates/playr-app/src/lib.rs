//! playr-app: what playr's Rust frontends share.
//!
//! A frontend draws and reads input its own way; this crate holds what it
//! means. [`action::Action`] is everything a key or a `:` command can do, and
//! [`dispatch::dispatch`] does it, through a [`dispatch::Frontend`] the
//! frontend implements over its own cursors and prompts. The `:` command
//! language and key bindings over a key type of its own are here too, with
//! the `[keys]` tables of the settings file, so a terminal and a GUI read the
//! same commands and bindings.
//! `docs/architecture.md` sets out the design.

pub mod action;
pub mod command;
pub mod config;
pub mod dispatch;
pub mod message;

/// A part of the interface that scopes key bindings and commands. A terminal
/// shows one at a time; a GUI maps its panels or focus onto them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Library,
    Selection,
    Playlists,
    /// The playing track's waveform, for marking and slicing it.
    Sampler,
}

impl View {
    /// The view `next-view` switches to after this one.
    pub fn next(self) -> Self {
        match self {
            View::Library => View::Selection,
            View::Selection => View::Playlists,
            View::Playlists => View::Sampler,
            View::Sampler => View::Library,
        }
    }
}

/// How a waveform is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    /// RMS inside peak, on a linear scale.
    #[default]
    Envelope,
    /// RMS inside peak, on a dB scale.
    Decibels,
    /// The waveform around a centre line.
    Braille,
}

impl Display {
    /// The name `:display` takes.
    pub fn name(self) -> &'static str {
        match self {
            Display::Envelope => "envelope",
            Display::Decibels => "db",
            Display::Braille => "braille",
        }
    }

    /// The display `:display` switches to after this one.
    pub fn next(self) -> Display {
        match self {
            Display::Envelope => Display::Decibels,
            Display::Decibels => Display::Braille,
            Display::Braille => Display::Envelope,
        }
    }
}
