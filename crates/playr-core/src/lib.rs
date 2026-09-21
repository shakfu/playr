//! playr-core: the parts of playr with no interface.
//!
//! The audio engine, the library database, the scanner, sample export,
//! waveform peaks and spectrogram, and the settings file's own keys. Nothing here depends on a
//! terminal or any other frontend; `docs/architecture.md` sets out the split.

pub mod audio;
pub mod db;
pub mod event;
pub mod notice;
pub mod samples;
pub mod scan;
pub mod session;
pub mod settings;
pub mod spectrum;
pub mod wave;
