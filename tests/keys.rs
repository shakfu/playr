//! Terminal key events, as the key names bindings use.

use playr::ui::key_of;
use playr_app::action::Key;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(text: &str) -> Key {
    Key::parse(text).unwrap()
}

fn event(code: KeyCode, mods: KeyModifiers) -> Key {
    key_of(&KeyEvent::new(code, mods)).unwrap()
}

#[test]
fn a_key_press_matches_the_name_a_binding_uses() {
    // Terminals report the shifted character, often with Shift set as well.
    assert_eq!(event(KeyCode::Char('J'), KeyModifiers::SHIFT), key("J"));
    assert_eq!(event(KeyCode::Char('?'), KeyModifiers::SHIFT), key("?"));
    assert_eq!(event(KeyCode::Char('<'), KeyModifiers::SHIFT), key("<"));
    assert_ne!(key("<"), key(","), "shift-comma is its own key");
    assert_eq!(event(KeyCode::Char(' '), KeyModifiers::NONE), key("space"));
    assert_eq!(
        event(KeyCode::Char('S'), KeyModifiers::CONTROL),
        key("ctrl-s")
    );
    assert_eq!(event(KeyCode::BackTab, KeyModifiers::SHIFT), key("backtab"));
    assert_eq!(event(KeyCode::Down, KeyModifiers::SHIFT), key("shift-down"));
    assert_ne!(event(KeyCode::Down, KeyModifiers::SHIFT), key("down"));
    assert_ne!(event(KeyCode::Char('m'), KeyModifiers::CONTROL), key("m"));
}

#[test]
fn a_key_no_binding_can_name_has_no_key() {
    assert_eq!(
        key_of(&KeyEvent::new(KeyCode::CapsLock, KeyModifiers::NONE)),
        None
    );
    assert_eq!(event(KeyCode::F(5), KeyModifiers::ALT), key("alt-f5"));
}
