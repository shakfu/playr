//! playr: a minimal TUI music player.
//!
//! The terminal interface, over the engine and library in `playr-core`. It is a
//! library as well as a binary so the interface can be tested headlessly;
//! `main.rs` adds the command line.

pub mod ui;
