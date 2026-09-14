//! egui key events as the keys bindings name.

use eframe::egui::{self, Event, Modifiers};
use playr_app::action::Key;
use playr_gui::keys::key_of;

fn key(key: egui::Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn named(text: &str) -> Option<Key> {
    Some(Key::parse(text).unwrap())
}

#[test]
fn characters_come_from_text_and_named_keys_from_key_events() {
    assert_eq!(key_of(&Event::Text("G".into())), named("G"));
    assert_eq!(key_of(&Event::Text("?".into())), named("?"));
    assert_eq!(key_of(&Event::Text(" ".into())), named("space"));
    // An input method's text is not a key.
    assert_eq!(key_of(&Event::Text("ka".into())), None);

    assert_eq!(
        key_of(&key(egui::Key::Enter, Modifiers::NONE)),
        named("enter")
    );
    assert_eq!(
        key_of(&key(egui::Key::PageDown, Modifiers::NONE)),
        named("pagedown")
    );
    assert_eq!(key_of(&key(egui::Key::F5, Modifiers::NONE)), named("f5"));
    assert_eq!(
        key_of(&key(egui::Key::ArrowRight, Modifiers::SHIFT)),
        named("shift-right")
    );
    assert_eq!(
        key_of(&key(egui::Key::Tab, Modifiers::SHIFT)),
        named("backtab")
    );
    // A character key without Ctrl or Alt also sends text, which names it.
    assert_eq!(key_of(&key(egui::Key::J, Modifiers::NONE)), None);
    let released = Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    assert_eq!(key_of(&released), None);
}

#[test]
fn chords_come_from_key_events() {
    assert_eq!(key_of(&key(egui::Key::S, Modifiers::CTRL)), named("ctrl-s"));
    assert_eq!(key_of(&key(egui::Key::X, Modifiers::ALT)), named("alt-x"));
    assert_eq!(
        key_of(&key(egui::Key::Minus, Modifiers::CTRL)),
        named("ctrl--")
    );
    assert_eq!(
        key_of(&key(egui::Key::Num1, Modifiers::CTRL)),
        named("ctrl-1")
    );
}
