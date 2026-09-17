//! The playing track as clients name it, and the latest state sent to pages.

use std::sync::{Condvar, Mutex};
use std::time::Duration;

use playr_app::model::Model;
use playr_core::db::Track;

/// The row of the track playing, when the list playing has one.
pub fn playing(model: &Model) -> Option<&Track> {
    model.playing().get(model.snapshot().status.index)
}

/// The playing track's title. A file played from outside the list has no
/// row, so its name stands in.
pub fn title(model: &Model) -> Option<String> {
    match playing(model) {
        Some(t) => Some(t.display_title()),
        None => model
            .snapshot()
            .status
            .current()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned()),
    }
}

/// The latest state as text, numbered so a reader waits only for a newer one.
#[derive(Default)]
pub struct Latest {
    state: Mutex<(u64, String)>,
    changed: Condvar,
}

impl Latest {
    /// Stores `text`, waking readers only when it differs from what is stored.
    pub fn set(&self, text: String) {
        let mut state = self.state.lock().unwrap();
        if state.1 != text {
            *state = (state.0 + 1, text);
            self.changed.notify_all();
        }
    }

    /// The stored text and its number once it is newer than `seen`, or `None`
    /// after `timeout`.
    pub fn newer_than(&self, seen: u64, timeout: Duration) -> Option<(u64, String)> {
        let state = self.state.lock().unwrap();
        let (state, _) = self
            .changed
            .wait_timeout_while(state, timeout, |s| s.0 <= seen)
            .unwrap();
        (state.0 > seen).then(|| state.clone())
    }
}
