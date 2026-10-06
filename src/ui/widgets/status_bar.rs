//! `StatusBar` widget: connection status, notification feed, report-issue
//! affordance (prompt 1.4 bottom bar).

use iced::widget::{button, column, row, scrollable, text};

use crate::app::messages::{Message, UiMsg};
use crate::app::state::{AppState, ConnectionStatus};
use mbxt_core::SessionState;

/// Bottom status bar.
pub fn view(app: &AppState) -> iced::Element<'_, Message> {
    let connection = match (&app.connection_status, app.active_session_id) {
        (ConnectionStatus::Disconnected, _) => "disconnected".to_string(),
        (ConnectionStatus::Connecting { progress }, _) => match progress {
            Some(pct) => format!("connecting… {pct}%"),
            None => "connecting…".to_string(),
        },
        (ConnectionStatus::Connected { .. }, Some(id)) => {
            format!("connected: {}", session_label(app, id))
        },
        (ConnectionStatus::Connected { .. }, None) => "connected".to_string(),
        (ConnectionStatus::Failed { reason }, _) => format!("failed: {reason}"),
    };

    let lock_state = if app.sessions_unlocked {
        "store unlocked"
    } else if app.secure.has_store() {
        "store LOCKED — master password required"
    } else {
        "no session store yet"
    };

    let mut notifications = column![].spacing(2);
    for line in app.notifications.display() {
        notifications = notifications.push(text(line).size(11));
    }

    let mut bar = row![
        text(connection).size(12).width(220),
        text(lock_state).size(12).width(240),
    ]
    .spacing(12);
    // X11 status indicator (Prompt 4.1): ⚡ while forwarding is up, ⚠ when
    // the session wants it but the local display is unreachable.
    #[cfg(feature = "x11")]
    if let Some(segment) = x11_segment(app) {
        bar = bar.push(text(segment).size(12).width(150));
    }
    bar = bar.push(
        scrollable(notifications)
            .width(iced::Fill)
            .height(iced::Shrink),
    );
    bar = bar.push(
        button(text("Send feedback").size(12))
            .on_press(Message::Ui(UiMsg::OpenFeedback))
            .padding(4),
    );
    bar.padding([4, 8]).into()
}

/// X11 indicator for the active session, if it requested forwarding.
#[cfg(feature = "x11")]
fn x11_segment(app: &AppState) -> Option<String> {
    let id = app.active_session_id?;
    let session = app.session(id)?;
    if !session.spec.x11_forwarding {
        return None;
    }
    if !matches!(app.session_states.get(&id), Some(SessionState::Connected)) {
        return Some("X11 off".to_string());
    }
    if crate::connection::x11::X11Manager::shared().is_active(id) {
        return Some("X11 ⚡".to_string());
    }
    Some("X11 ⚠ no local display".to_string())
}

fn session_label(app: &AppState, id: mbxt_core::SessionId) -> String {
    let name = app
        .session(id)
        .map(|s| s.spec.name.as_str())
        .unwrap_or("<deleted>");
    match app.session_states.get(&id) {
        Some(SessionState::Connected) => format!("{name} (live)"),
        _ => name.to_string(),
    }
}
