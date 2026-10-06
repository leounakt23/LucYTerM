//! `SessionList` widget: search bar + grouped session tree (feature matrix
//! #12–14). Placeholder rendering (flat grouped list) until the full tree
//! widget lands with the terminal grid work.

use iced::widget::{button, column, container, row, scrollable, text, text_input};

use crate::app::messages::{Message, UiMsg};
use crate::app::state::AppState;

/// Sidebar contents: search + grouped session list.
pub fn view(app: &AppState) -> iced::Element<'_, Message> {
    let search = text_input("Search sessions…", &app.ui_state.search_query)
        .on_input(|query| Message::Ui(UiMsg::SearchChanged(query)))
        .padding(6);

    let mut list = column![].spacing(2);

    for (group, sessions) in grouped_sessions(app) {
        list = list.push(text(group).size(12).width(iced::Fill));
        for (id, name, connected, x11) in sessions {
            #[cfg(not(feature = "x11"))]
            let _ = x11;
            let label = if connected {
                format!("● {name}")
            } else {
                format!("○ {name}")
            };
            let mut session_row = row![button(text(label).size(14).width(iced::Fill))
                .on_press(Message::Session(
                    crate::app::messages::SessionMsg::Selected(id),
                ))
                .width(iced::Fill),]
            .spacing(4);
            // X11 forwarding toggle (Prompt 4.1 checkbox equivalent; only
            // SSH-family sessions honour it — others reject the toggle).
            // Without the `x11` feature the button is hidden and the flag
            // stays inert data.
            #[cfg(feature = "x11")]
            {
                let bolt = if crate::connection::x11::X11Manager::shared().is_active(id) {
                    " ⚡"
                } else {
                    ""
                };
                let check = if x11 { "☑" } else { "☐" };
                session_row = session_row.push(
                    button(text(format!("X11 {check}{bolt}")).size(11))
                        .on_press(Message::Session(
                            crate::app::messages::SessionMsg::X11Toggled(id),
                        ))
                        .padding([2, 6]),
                );
            }
            // Multi-exec target checkbox, visible while broadcasting is
            // armed (Prompt 5.1 session selection UI).
            if app.multi_exec_mode.enabled {
                let targeted = app.multi_exec_mode.targets.contains(&id);
                let mark = if targeted { "[x]" } else { "[ ]" };
                session_row = session_row.push(
                    button(text(mark).size(11))
                        .on_press(Message::Ui(
                            crate::app::messages::UiMsg::MultiExecToggleTarget(id),
                        ))
                        .padding([2, 6]),
                );
            }
            list = list.push(session_row);
        }
    }

    container(scrollable(column![search, list].spacing(8).padding(4)))
        .width(iced::Fill)
        .height(iced::Fill)
        .into()
}

/// One grouped row: session id, display name, connected flag, X11 flag.
type SessionRow = (mbxt_core::SessionId, String, bool, bool);
/// Sessions grouped by tag for the sidebar tree.
type SessionGroups = Vec<(String, Vec<SessionRow>)>;

/// Group sessions by their first tag (placeholder tree behavior #12);
/// untagged sessions land under "sessions".
fn grouped_sessions(app: &AppState) -> SessionGroups {
    let query = app.ui_state.search_query.to_lowercase();
    let mut groups: std::collections::BTreeMap<String, Vec<SessionRow>> =
        std::collections::BTreeMap::new();

    for session in &app.sessions {
        if !query.is_empty()
            && !session.spec.name.to_lowercase().contains(&query)
            && !session
                .spec
                .tags
                .iter()
                .any(|t| t.to_lowercase().contains(&query))
        {
            continue;
        }
        let group = session
            .spec
            .tags
            .first()
            .cloned()
            .unwrap_or_else(|| "sessions".to_string());
        let connected = matches!(
            app.session_states.get(&session.id),
            Some(mbxt_core::SessionState::Connected)
        );
        groups.entry(group).or_default().push((
            session.id,
            session.spec.name.clone(),
            connected,
            session.spec.x11_forwarding,
        ));
    }

    groups.into_iter().collect()
}

/// Quick-connect row shown above the list (toolbar-adjacent affordance).
pub fn quick_actions() -> iced::Element<'static, Message> {
    row![
        button(text("+ session").size(12))
            .on_press(Message::Ui(UiMsg::NewSession))
            .padding(4),
        button(text("quick connect").size(12))
            .on_press(Message::Ui(UiMsg::QuickConnect))
            .padding(4),
    ]
    .spacing(6)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbxt_core::{AuthMethod, Protocol, Session, SessionSpec, SessionState};

    fn app_with_sessions() -> AppState {
        // Lightweight state without touching the real store.
        let base = std::env::temp_dir().join(format!("mbxt-ui-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        let (mut state, _) = AppState::new(crate::utils::config::AppConfig::default(), paths);
        for (i, name) in ["web-1", "db-1"].into_iter().enumerate() {
            state.sessions.push(Session {
                id: i as u64 + 1,
                spec: SessionSpec {
                    name: name.into(),
                    protocol: Protocol::Ssh,
                    host: Some("h".into()),
                    port: Some(22),
                    username: None,
                    auth: AuthMethod::Password,
                    tags: vec![if i == 0 { "prod".into() } else { "db".into() }],
                    notes: String::new(),
                    x11_forwarding: false,
                    serial: None,
                    forwards: Vec::new(),
                },
                state: SessionState::Disconnected,
            });
        }
        state
    }

    #[test]
    fn search_filters_by_name() {
        let mut app = app_with_sessions();
        app.ui_state.search_query = "web".into();
        let groups = grouped_sessions(&app);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "prod");
    }

    #[test]
    fn untagged_sessions_fall_into_default_group() {
        let mut app = app_with_sessions();
        app.sessions[0].spec.tags.clear();
        let groups = grouped_sessions(&app);
        assert!(groups.iter().any(|(g, _)| g == "sessions"));
    }

    #[test]
    fn view_builds_without_panicking() {
        let app = app_with_sessions();
        let _element = view(&app);
        let _actions = quick_actions();
    }
}
