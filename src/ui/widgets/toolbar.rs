//! `Toolbar` widget: top action row (quick connect, new session, new tab,
//! settings) per prompt 1.4 layout.

use iced::widget::{button, row, text};

use crate::app::messages::{Message, UiMsg};
use crate::app::state::AppState;

/// Top toolbar.
pub fn view(_app: &AppState) -> iced::Element<'_, Message> {
    row![
        button(text("+ session").size(13))
            .on_press(Message::Ui(UiMsg::NewSession))
            .padding(6),
        button(text("quick connect").size(13))
            .on_press(Message::Ui(UiMsg::QuickConnect))
            .padding(6),
        button(text("+ tab (Ctrl+T)").size(13))
            .on_press(Message::Ui(UiMsg::NewTab))
            .padding(6),
        button(text("settings (Ctrl+,)").size(13))
            .on_press(Message::Ui(UiMsg::OpenSettings))
            .padding(6),
        button(text("sidebar (Ctrl+B)").size(13))
            .on_press(Message::Ui(UiMsg::ToggleSidebar))
            .padding(6),
        button(text("macros").size(13))
            .on_press(Message::Ui(UiMsg::OpenMacrosView))
            .padding(6),
        button(text("tunnels").size(13))
            .on_press(Message::Ui(UiMsg::OpenTunnelsView))
            .padding(6),
        button(text("send feedback").size(13))
            .on_press(Message::Ui(UiMsg::OpenFeedback))
            .padding(6),
    ]
    .spacing(8)
    .padding([4, 8])
    .into()
}
