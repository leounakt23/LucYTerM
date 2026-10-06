//! Keyboard shortcuts (prompt 1.4).
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
//!
//! Wired as an iced keyboard subscription from `app::subscriptions`.

use iced::keyboard::{Key, Modifiers};

use crate::app::messages::{Message, UiMsg};

/// Keyboard subscription.
pub fn subscription() -> iced::Subscription<Message> {
    iced::keyboard::on_key_press(shortcut)
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
}
