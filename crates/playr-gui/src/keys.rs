//! Keyboard events as the keys bindings name.

use eframe::egui;
use playr_app::action::{Key, KeyCode, Modifiers};

/// The key an egui event names, or `None` for an event no binding can name.
///
/// A character comes from its text event, which already heeds Shift and the
/// keyboard layout: `J`, `?`. With Ctrl or Alt held there is no text, so the
/// character comes from the key event instead.
pub fn key_of(event: &egui::Event) -> Option<Key> {
    match event {
        egui::Event::Text(text) => {
            let mut chars = text.chars();
            let c = chars.next()?;
            // Several characters at once are an input method's, not a key's.
            if chars.next().is_some() {
                return None;
            }
            Some(Key::new(KeyCode::Char(c), Modifiers::default()))
        }
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => {
            use egui::Key as E;
            let code = match key {
                E::ArrowDown => KeyCode::Down,
                E::ArrowUp => KeyCode::Up,
                E::ArrowLeft => KeyCode::Left,
                E::ArrowRight => KeyCode::Right,
                E::Escape => KeyCode::Esc,
                E::Tab if modifiers.shift => KeyCode::BackTab,
                E::Tab => KeyCode::Tab,
                E::Backspace => KeyCode::Backspace,
                E::Enter => KeyCode::Enter,
                E::Insert => KeyCode::Insert,
                E::Delete => KeyCode::Delete,
                E::Home => KeyCode::Home,
                E::End => KeyCode::End,
                E::PageUp => KeyCode::PageUp,
                E::PageDown => KeyCode::PageDown,
                _ => match function_key(*key) {
                    Some(n) => KeyCode::F(n),
                    None if modifiers.ctrl || modifiers.alt => KeyCode::Char(character(*key)?),
                    None => return None,
                },
            };
            let mods = Modifiers {
                ctrl: modifiers.ctrl,
                alt: modifiers.alt,
                shift: modifiers.shift,
            };
            Some(Key::new(code, mods))
        }
        _ => None,
    }
}

/// The number of a function key from F1 to F12.
fn function_key(key: egui::Key) -> Option<u8> {
    let n: u8 = key.name().strip_prefix('F')?.parse().ok()?;
    (1..=12).contains(&n).then_some(n)
}

/// The character a character key types without Shift.
fn character(key: egui::Key) -> Option<char> {
    use egui::Key as E;
    let c = match key {
        E::Space => ' ',
        E::Colon => ':',
        E::Comma => ',',
        E::Backslash => '\\',
        E::Slash => '/',
        E::Pipe => '|',
        E::Questionmark => '?',
        E::Exclamationmark => '!',
        E::OpenBracket => '[',
        E::CloseBracket => ']',
        E::OpenCurlyBracket => '{',
        E::CloseCurlyBracket => '}',
        E::Backtick => '`',
        E::Minus => '-',
        E::Period => '.',
        E::Plus => '+',
        E::Equals => '=',
        E::Semicolon => ';',
        E::Quote => '\'',
        // Letters and digits are named by their character: `A`, `1`.
        _ => {
            let mut chars = key.name().chars();
            let c = chars.next()?;
            if chars.next().is_some() || !c.is_ascii_alphanumeric() {
                return None;
            }
            c.to_ascii_lowercase()
        }
    };
    Some(c)
}
