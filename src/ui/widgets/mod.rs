//! Custom widgets (prompt 1.4). Each widget is a pure function from the
//! relevant `AppState` slice to an `Element<Message>` — no state, no I/O.

pub mod context_menu;
pub mod drag_drop;
pub mod feedback;
pub mod file_browser;
pub mod macro_editor;
pub mod macro_panel;
pub mod multi_exec_toolbar;
pub mod multi_exec_view;
pub mod new_session_dialog;
pub mod session_list;
pub mod settings;
pub mod status_bar;
pub mod terminal_view;
pub mod terminal_widget;
pub mod toolbar;
/// Transfer list (Prompt 3.2); rides the ssh-gated SFTP stack.
#[cfg(feature = "ssh")]
pub mod transfer_view;
pub mod tunnels_panel;
/// VNC viewer (Prompt 4.3); needs the vnc-gated RFB stack.
#[cfg(feature = "vnc")]
pub mod vnc_view;

use crate::app::notifications::Level;

/// Map a notification level to a render color (moved here from ui root so
/// `update` stays free of widget concepts).
pub fn level_color(level: Level) -> iced::Color {
    match level {
        Level::Info => iced::Color::from_rgb(0.47, 0.75, 1.0),
        Level::Success => iced::Color::from_rgb(0.49, 0.9, 0.53),
        Level::Warning => iced::Color::from_rgb(1.0, 0.65, 0.34),
        Level::Error => iced::Color::from_rgb(1.0, 0.4, 0.4),
    }
}
