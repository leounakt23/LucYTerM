//! `TerminalView` widget: placeholder until the custom wgpu renderer lands
//! (tech_stack R3 staged plan). The grid model already exists in
//! `mbxt-terminal::Grid`; this widget will become the instanced-quad
//! renderer consuming grid diffs.

use iced::widget::{column, container, scrollable, text};

use crate::app::messages::Message;
use crate::app::state::AppState;
use mbxt_core::SessionId;

/// Placeholder terminal pane for `session_id`.
pub fn view(app: &AppState, session_id: SessionId) -> iced::Element<'_, Message> {
    let name = app
        .session(session_id)
        .map(|s| s.spec.name.as_str())
        .unwrap_or("<deleted session>");
    let status = app
        .session_states
        .get(&session_id)
        .map(|s| format!("{s:?}"))
        .unwrap_or_else(|| "unknown".to_string());

    let screen = app
        .terminals
        .get(&session_id)
        .map(|terminal| terminal.grid.visible_text().join("\n"))
        .unwrap_or_else(|| "Waiting for terminal output...".to_string());

    // Multi-exec leader highlight (Prompt 5.1): the terminal that sources
    // broadcasts gets a distinct title while armed.
    let title = if app.multi_exec_mode.enabled && app.multi_exec_leader == Some(session_id) {
        format!("▶ terminal: {name} (multi-exec leader)")
    } else {
        format!("terminal: {name}")
    };
    let body = column![
        text(title).size(16),
        text(format!("state: {status}")).size(12),
        scrollable(text(screen).font(iced::Font::MONOSPACE).size(13)).height(iced::Fill),
    ]
    .spacing(6);

    container(body)
        .width(iced::Fill)
        .height(iced::Fill)
        .padding(12)
        .into()
}
