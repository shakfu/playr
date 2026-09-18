//! Media keys and the now-playing panel.
//!
//! The keyboard's play, pause, next and previous keys, MPRIS on Linux, and the
//! macOS and Windows now-playing panels, through one crate. Both frontends have
//! them because [`crate::model::Model`] holds this, as the parity rule in
//! `docs/dev/gui.md` requires.
//!
//! None of it is load bearing. A machine with no session bus, no panel, or a
//! terminal on Windows, where the panel needs a window handle playr has not
//! got, leaves [`Media::new`] with nothing attached and every call a no-op.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};

use crate::action::Action;

/// Seconds a panel's plain seek moves, when it names no amount.
const SEEK_BY: i64 = 10;

/// What was last handed to the panel, so an unchanged frame sends nothing.
#[derive(Debug, Default, PartialEq)]
struct Shown {
    title: String,
    artist: String,
    album: String,
    duration: Option<Duration>,
    playing: Option<bool>,
    /// Whole seconds: a panel redraws from its own clock between them.
    position: u64,
}

/// The system's media controls, or nothing where there are none.
pub struct Media {
    controls: Option<MediaControls>,
    events: Receiver<MediaControlEvent>,
    shown: Shown,
}

impl Media {
    /// Registers playr with the system's media controls, calling `wake` from
    /// the handler's thread each time one arrives, so a frontend that sleeps
    /// between frames draws the result.
    ///
    /// Failure is not an error: playr runs without the panel.
    pub fn new(wake: impl Fn() + Send + 'static) -> Media {
        let (send, events) = mpsc::channel();
        Media {
            controls: attach(send, wake),
            events,
            shown: Shown::default(),
        }
    }

    /// Media controls playr is not registered with, for a frontend that wants
    /// none and for tests.
    pub fn none() -> Media {
        let (_, events) = mpsc::channel();
        Media {
            controls: None,
            events,
            shown: Shown::default(),
        }
    }

    /// What the panel has asked for since the last frame, as actions.
    pub fn drain(&self, playing: bool) -> Vec<Action> {
        self.events
            .try_iter()
            .filter_map(|e| action(e, playing))
            .collect()
    }

    /// Tells the panel what is playing and where. Sends only what changed:
    /// each call crosses the bus, and the position changes every frame.
    pub fn publish(
        &mut self,
        track: Option<(&str, &str, &str, Option<Duration>)>,
        playing: Option<bool>,
        position: Duration,
    ) {
        let Some(controls) = self.controls.as_mut() else {
            return;
        };
        let (title, artist, album, duration) = track.unwrap_or_default();
        let next = Shown {
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            duration,
            playing,
            position: position.as_secs(),
        };
        if next.title != self.shown.title
            || next.artist != self.shown.artist
            || next.album != self.shown.album
            || next.duration != self.shown.duration
        {
            let _ = controls.set_metadata(MediaMetadata {
                title: Some(&next.title).filter(|t| !t.is_empty()).map(|t| &**t),
                artist: Some(&next.artist).filter(|a| !a.is_empty()).map(|a| &**a),
                album: Some(&next.album).filter(|a| !a.is_empty()).map(|a| &**a),
                duration: next.duration,
                cover_url: None,
            });
        }
        if next.playing != self.shown.playing || next.position != self.shown.position {
            let progress = Some(MediaPosition(position));
            let _ = controls.set_playback(match next.playing {
                Some(true) => MediaPlayback::Playing { progress },
                Some(false) => MediaPlayback::Paused { progress },
                None => MediaPlayback::Stopped,
            });
        }
        self.shown = next;
    }
}

/// Registers with the system's controls, or gives nothing back when there are
/// none to register with.
fn attach(
    send: Sender<MediaControlEvent>,
    wake: impl Fn() + Send + 'static,
) -> Option<MediaControls> {
    let config = PlatformConfig {
        // The MPRIS bus name, which is `org.mpris.MediaPlayer2.playr`.
        dbus_name: "playr",
        display_name: "playr",
        // Windows shows the panel against a window; the terminal has none, so
        // there it attaches only from the window build.
        hwnd: None,
    };
    let mut controls = MediaControls::new(config).ok()?;
    controls
        .attach(move |event| {
            if send.send(event).is_ok() {
                wake();
            }
        })
        .ok()?;
    Some(controls)
}

/// The action a panel's request means here, given whether a track is playing.
///
/// playr has one pause key, so play and pause both become a toggle, and each
/// is dropped when it would ask for the state playr is already in: a panel
/// sends Play when it thinks playback stopped, which may be a frame behind.
pub fn action(event: MediaControlEvent, playing: bool) -> Option<Action> {
    match event {
        MediaControlEvent::Play if !playing => Some(Action::TogglePause),
        MediaControlEvent::Pause if playing => Some(Action::TogglePause),
        MediaControlEvent::Play | MediaControlEvent::Pause => None,
        MediaControlEvent::Toggle => Some(Action::TogglePause),
        MediaControlEvent::Next => Some(Action::Next),
        MediaControlEvent::Previous => Some(Action::Prev),
        MediaControlEvent::Stop | MediaControlEvent::Quit => Some(Action::Stop),
        MediaControlEvent::Seek(SeekDirection::Forward) => Some(Action::SeekBy(SEEK_BY)),
        MediaControlEvent::Seek(SeekDirection::Backward) => Some(Action::SeekBy(-SEEK_BY)),
        MediaControlEvent::SeekBy(SeekDirection::Forward, by) => {
            Some(Action::SeekBy(by.as_secs() as i64))
        }
        MediaControlEvent::SeekBy(SeekDirection::Backward, by) => {
            Some(Action::SeekBy(-(by.as_secs() as i64)))
        }
        MediaControlEvent::SetPosition(MediaPosition(at)) => Some(Action::SeekTo(at)),
        MediaControlEvent::SetVolume(v) => Some(Action::SetVolume(v as f32)),
        // Opening a URI and raising a window are not playr's to do.
        MediaControlEvent::OpenUri(_) | MediaControlEvent::Raise => None,
    }
}
