//! playr: a local terminal music player with a sampler, a tape looper and DJ decks.
//!
//! The terminal interface, over the engine and library in `playr-core`. It is a
//! library as well as a binary so the interface can be tested headlessly;
//! `main.rs` adds the command line.

pub mod ui;
