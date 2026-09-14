//! Messages about what an action did, as data.
//!
//! A core operation reports a [`Notice`]; the rest are about the interface
//! itself: prompts, key bindings and views. Each frontend words them; the
//! terminal does in `ui::notice::text`.

use playr_core::notice::{Notice, Outcome, Refusal};

use crate::action::{Action, Key};
use crate::{Display, View};

/// A message for the person using the interface.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Core(Notice),
    /// A confirmation was answered with anything but `y`.
    Cancelled,
    NoMatches,
    NoPlaylistUnderCursor,
    Display(Display),
    /// A `map` command took effect; holds the `Action::Map`.
    Mapped(Action),
    Unmapped(Key),
    NotBound {
        key: Key,
        view: Option<View>,
    },
    NoSlicesPlanned,
    SlicesDiscarded,
    /// A `:` command could not be parsed or run; the parser's own words.
    Command(String),
}

impl From<Notice> for Message {
    fn from(notice: Notice) -> Self {
        Message::Core(notice)
    }
}

impl From<Outcome> for Message {
    fn from(outcome: Outcome) -> Self {
        Message::Core(Notice::Done(outcome))
    }
}

impl From<Refusal> for Message {
    fn from(refusal: Refusal) -> Self {
        Message::Core(Notice::Refused(refusal))
    }
}
