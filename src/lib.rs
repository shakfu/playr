//! playr: a minimal TUI music player.
//!
//! The library half (scan, database, queue, audio) is exposed so it can be
//! tested and driven headlessly; `main.rs` adds the CLI and TUI.

pub mod audio;
pub mod db;
pub mod scan;
pub mod ui;
