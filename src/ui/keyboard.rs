//! Keyboard input (prompt 1.4 + terminal typing).
//!
//! Mapping (all Ctrl-based on Linux; Meta reserved for window management):
//! - `Ctrl+T`  New tab
//! - `Ctrl+W`  Close active tab
//! - `Ctrl+B`  Toggle sidebar
//! - `Ctrl+K`  Focus/switch to session list
//! - `Ctrl+,`  Open settings
//! - `Ctrl+Shift+M`  Toggle multi-exec broadcasting
//! - `Ctrl+Shift+R`  Start/stop macro recording
//! - `F5`–`F8`  Play the macro bound to that hotkey
//! - `Ctrl+Shift+C`  Copy the active terminal selection to the clipboard
//! - `Ctrl+Shift+V`  Paste the clipboard into the active terminal
//! - `Ctrl+Shift+A`  Select the whole visible grid of the active terminal
//! - everything else (while a terminal tab is focused): typed into the
//!   session (`Ctrl+C` without Shift still sends ETX/SIGINT, as in xterm)
//!
//! Wired as a single iced event subscription from `app::subscriptions`.
//! `listen_with` (not `on_key_press`) preserves the produced key text, and
//! only `Ignored` events are routed — keystrokes captured by a dialog field
//! or the search box never leak into the PTY.

use iced::event::Status;
use iced::keyboard::{Key, Modifiers};
use iced::window;
use mbxt_terminal::input::Key as TermKey;

use crate::app::messages::{Message, TerminalMsg, UiMsg};

/// Unified keyboard subscription: shortcuts, terminal copy/paste, typing.
pub fn subscription() -> iced::Subscription<Message> {
    iced::event::listen_with(route_event)
}

/// Route one iced event; `None` for everything but ignored key presses.
fn route_event(event: iced::Event, status: Status, _window: window::Id) -> Option<Message> {
    if status != Status::Ignored {
        return None;
    }
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key,
        text,
        modifiers,
        ..
    }) = event
    else {
        return None;
    };
    route_key_press(key, text.as_deref(), modifiers)
}

/// Route one ignored key press: terminal clipboard bindings, then the
/// classic shortcuts, then terminal typing.
fn route_key_press(key: Key, text: Option<&str>, modifiers: Modifiers) -> Option<Message> {
    // Window-manager key: never an app action or terminal input.
    if modifiers.logo() {
        return None;
    }
    // Terminal clipboard bindings win over everything (checked before
    // `shortcut()`; none of C/V/A collide with existing bindings).
    if modifiers.control() && modifiers.shift() && !modifiers.alt() {
        if let Key::Character(c) = &key {
            match c.as_str() {
                "c" | "C" => return Some(Message::Terminal(TerminalMsg::CopyActive)),
                "v" | "V" => return Some(Message::Terminal(TerminalMsg::PasteActive)),
                "a" | "A" => return Some(Message::Terminal(TerminalMsg::SelectAllActive)),
                _ => {},
            }
        }
    }
    if let Some(shortcut) = shortcut(key.clone(), modifiers) {
        return Some(shortcut);
    }
    let term_key = map_key(&key)?;
    Some(Message::Terminal(TerminalMsg::KeyInput {
        key: term_key,
        text: text.map(str::to_string),
        ctrl: modifiers.control(),
        alt: modifiers.alt(),
    }))
}

/// Map an iced key to the framework-independent terminal key.
/// Printable characters arrive as `Char`; unmapped specials are `None`.
fn map_key(key: &Key) -> Option<TermKey> {
    use iced::keyboard::key::Named;
    match key {
        Key::Character(c) => c.chars().next().map(TermKey::Char),
        Key::Named(named) => match named {
            Named::Enter => Some(TermKey::Enter),
            Named::Tab => Some(TermKey::Tab),
            Named::Space => Some(TermKey::Char(' ')),
            Named::Backspace => Some(TermKey::Backspace),
            Named::Escape => Some(TermKey::Escape),
            Named::ArrowUp => Some(TermKey::Up),
            Named::ArrowDown => Some(TermKey::Down),
            Named::ArrowLeft => Some(TermKey::Left),
            Named::ArrowRight => Some(TermKey::Right),
            Named::Home => Some(TermKey::Home),
            Named::End => Some(TermKey::End),
            Named::Insert => Some(TermKey::Insert),
            Named::Delete => Some(TermKey::Delete),
            Named::PageUp => Some(TermKey::PageUp),
            Named::PageDown => Some(TermKey::PageDown),
            Named::F1 => Some(TermKey::Function(1)),
            Named::F2 => Some(TermKey::Function(2)),
            Named::F3 => Some(TermKey::Function(3)),
            Named::F4 => Some(TermKey::Function(4)),
            Named::F5 => Some(TermKey::Function(5)),
            Named::F6 => Some(TermKey::Function(6)),
            Named::F7 => Some(TermKey::Function(7)),
            Named::F8 => Some(TermKey::Function(8)),
            Named::F9 => Some(TermKey::Function(9)),
            Named::F10 => Some(TermKey::Function(10)),
            Named::F11 => Some(TermKey::Function(11)),
            Named::F12 => Some(TermKey::Function(12)),
            _ => None,
        },
        Key::Unidentified => None,
    }
}

/// Translate a key press into a message (None when not a shortcut).
pub fn shortcut(key: Key, modifiers: Modifiers) -> Option<Message> {
    if !modifiers.control() {
        return None;
    }
    match key {
        Key::Character(c) => match c.as_str() {
            "t" => Some(Message::Ui(UiMsg::NewTab)),
            "w" => Some(Message::Ui(UiMsg::CloseActiveTab)),
            "b" => Some(Message::Ui(UiMsg::ToggleSidebar)),
            "k" => Some(Message::Ui(UiMsg::ViewChanged(
                crate::app::messages::ViewKind::SessionList,
            ))),
            "," => Some(Message::Ui(UiMsg::OpenSettings)),
            "m" | "M" if modifiers.shift() => Some(Message::Ui(UiMsg::MultiExecToggle)),
            "r" | "R" if modifiers.shift() => {
                Some(Message::Macro(crate::app::messages::MacroMsg::RecordToggle))
            },
            _ => None,
        },
        Key::Named(named) => match named {
            iced::keyboard::key::Named::F5 => Some(Message::Macro(
                crate::app::messages::MacroMsg::HotkeyPressed("F5".to_string()),
            )),
            iced::keyboard::key::Named::F6 => Some(Message::Macro(
                crate::app::messages::MacroMsg::HotkeyPressed("F6".to_string()),
            )),
            iced::keyboard::key::Named::F7 => Some(Message::Macro(
                crate::app::messages::MacroMsg::HotkeyPressed("F7".to_string()),
            )),
            iced::keyboard::key::Named::F8 => Some(Message::Macro(
                crate::app::messages::MacroMsg::HotkeyPressed("F8".to_string()),
            )),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl() -> Modifiers {
        Modifiers::CTRL
    }

    fn ctrl_shift() -> Modifiers {
        Modifiers::CTRL | Modifiers::SHIFT
    }

    #[test]
    fn ctrl_t_opens_new_tab() {
        let message = shortcut(Key::Character("t".into()), ctrl());
        assert!(matches!(message, Some(Message::Ui(UiMsg::NewTab))));
    }

    #[test]
    fn ctrl_w_closes_tab() {
        let message = shortcut(Key::Character("w".into()), ctrl());
        assert!(matches!(message, Some(Message::Ui(UiMsg::CloseActiveTab))));
    }

    #[test]
    fn plain_keys_are_ignored() {
        assert!(shortcut(Key::Character("t".into()), Modifiers::empty()).is_none());
        assert!(shortcut(Key::Character("x".into()), ctrl()).is_none());
    }

    #[test]
    fn ctrl_comma_opens_settings() {
        let message = shortcut(Key::Character(",".into()), ctrl());
        assert!(matches!(message, Some(Message::Ui(UiMsg::OpenSettings))));
    }

    #[test]
    fn ctrl_shift_m_toggles_multi_exec() {
        let shift = Modifiers::CTRL | Modifiers::SHIFT;
        let message = shortcut(Key::Character("m".into()), shift);
        assert!(matches!(message, Some(Message::Ui(UiMsg::MultiExecToggle))));
        // Without shift there is no binding (plain typing must not toggle).
        assert!(shortcut(Key::Character("m".into()), ctrl()).is_none());
    }

    #[test]
    fn ctrl_shift_c_v_a_route_to_terminal_clipboard() {
        let message = route_key_press(Key::Character("c".into()), None, ctrl_shift());
        assert!(matches!(
            message,
            Some(Message::Terminal(TerminalMsg::CopyActive))
        ));
        let message = route_key_press(Key::Character("V".into()), None, ctrl_shift());
        assert!(matches!(
            message,
            Some(Message::Terminal(TerminalMsg::PasteActive))
        ));
        let message = route_key_press(Key::Character("a".into()), None, ctrl_shift());
        assert!(matches!(
            message,
            Some(Message::Terminal(TerminalMsg::SelectAllActive))
        ));
        // Without shift there is no clipboard binding: Ctrl+C is KeyInput.
        let message = route_key_press(Key::Character("c".into()), Some("c"), ctrl());
        assert!(matches!(
            message,
            Some(Message::Terminal(TerminalMsg::KeyInput { ctrl: true, .. }))
        ));
    }

    #[test]
    fn typing_routes_to_key_input_with_text() {
        let message = route_key_press(Key::Character("a".into()), Some("a"), Modifiers::empty());
        match message {
            Some(Message::Terminal(TerminalMsg::KeyInput {
                key,
                text,
                ctrl,
                alt,
            })) => {
                assert_eq!(key, TermKey::Char('a'));
                assert_eq!(text.as_deref(), Some("a"));
                assert!(!ctrl && !alt);
            },
            other => panic!("expected KeyInput, got {other:?}"),
        }
    }

    #[test]
    fn special_keys_map_without_text() {
        use iced::keyboard::key::Named;
        let message = route_key_press(Key::Named(Named::Enter), None, Modifiers::empty());
        assert!(matches!(
            message,
            Some(Message::Terminal(TerminalMsg::KeyInput {
                key: TermKey::Enter,
                ..
            }))
        ));
        let message = route_key_press(Key::Named(Named::ArrowUp), None, Modifiers::empty());
        assert!(matches!(
            message,
            Some(Message::Terminal(TerminalMsg::KeyInput {
                key: TermKey::Up,
                ..
            }))
        ));
    }

    #[test]
    fn shortcuts_win_over_typing() {
        // Ctrl+T is a shortcut, never terminal input.
        let message = route_key_press(Key::Character("t".into()), None, ctrl());
        assert!(matches!(message, Some(Message::Ui(UiMsg::NewTab))));
        // Ctrl+C without shift is terminal input (ETX), not copy.
        let message = route_key_press(Key::Character("c".into()), None, ctrl());
        assert!(matches!(
            message,
            Some(Message::Terminal(TerminalMsg::KeyInput { ctrl: true, .. }))
        ));
    }

    #[test]
    fn unmapped_keys_are_dropped() {
        use iced::keyboard::key::Named;
        assert!(route_key_press(Key::Unidentified, None, Modifiers::empty()).is_none());
        assert!(route_key_press(Key::Named(Named::Shift), None, Modifiers::empty()).is_none());
        // Meta-modified keys belong to the window manager.
        assert!(route_key_press(
            Key::Character("t".into()),
            Some("t"),
            Modifiers::LOGO | Modifiers::CTRL
        )
        .is_none());
    }
}
