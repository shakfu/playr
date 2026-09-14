//! playr-core: the parts of playr with no interface.
//!
//! The audio engine, the library database, the scanner, sample export and
//! waveform peaks. Nothing here depends on a terminal or any other frontend;
//! `docs/dev/architecture.md` sets out the split and what moves here next.

pub mod audio;
pub mod db;
pub mod event;
pub mod notice;
pub mod samples;
pub mod scan;
pub mod session;
pub mod wave;
