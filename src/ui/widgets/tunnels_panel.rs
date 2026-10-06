//! Tunnels panel (Prompt 5.3): per-session forward list with live status,
//! statistics, traffic tail, and the add/edit form.
//!
//! Pure view over session specs (definitions) plus `ForwardManager`
//! snapshots (status/stats/logs). Without the `ssh` feature this renders a
//! placeholder (same build matrix as the SFTP stack).

#[cfg(feature = "ssh")]
use iced::widget::column;
use iced::widget::{container, text};

use crate::app::messages::Message;
use crate::app::state::AppState;

/// Tunnels tab content.
pub fn view(app: &AppState) -> iced::Element<'_, Message> {
    #[cfg(feature = "ssh")]
    {
        full_view(app)
    }
    #[cfg(not(feature = "ssh"))]
    {
        let _ = app;
        container(text("tunnels need the ssh feature").size(12))
            .width(iced::Fill)
            .padding(12)
            .into()
    }
}

#[cfg(feature = "ssh")]
fn full_view(app: &AppState) -> iced::Element<'_, Message> {
    use crate::app::messages::TunnelMsg;
    use crate::connection::forward::ForwardManager;

    let mut body = column![text("SSH tunnels").size(16)].spacing(8);

    if app.sessions.is_empty() {
        body = body.push(text("no sessions yet — create one first").size(12));
    }
    for session in &app.sessions {
        let snapshots = ForwardManager::shared().snapshots_for(session.id);
        let mut section =
            column![text(format!("{} (#{})", session.spec.name, session.id)).size(13)].spacing(4);
        if snapshots.is_empty() {
            section = section.push(text("no tunnels defined").size(11));
        }
        for snapshot in &snapshots {
            section = section.push(forward_row(snapshot));
        }
        section = section.push(
            iced::widget::button(text("Add tunnel").size(11))
                .on_press(Message::Tunnel(TunnelMsg::OpenForm(session.id)))
                .padding([2, 8]),
        );
        body = body.push(section);
    }

    if let Some(form) = tunnel_form(app) {
        body = body.push(form);
    }

    container(iced::widget::scrollable(body.spacing(8).padding(12)))
        .width(iced::Fill)
        .height(iced::Fill)
        .into()
}

#[cfg(feature = "ssh")]
fn forward_row(
    snapshot: &crate::connection::forward::PortForward,
) -> iced::Element<'static, Message> {
    use crate::app::messages::TunnelMsg;
    use crate::connection::forward::ForwardStatus;
    use iced::widget::{button, row};

    let id = snapshot.id;
    let status = match &snapshot.status {
        ForwardStatus::Stopped => "stopped".to_string(),
        ForwardStatus::Starting => "starting…".to_string(),
        ForwardStatus::Running => "● running".to_string(),
        ForwardStatus::Failed(reason) => format!("failed: {reason}"),
    };
    let stats = &snapshot.statistics;
    let bound = snapshot
        .bound_port
        .map(|port| format!(" (:{port})"))
        .unwrap_or_default();
    let header = row![
        text(format!("{}{}", snapshot_name(snapshot), bound))
            .size(12)
            .width(iced::Fill),
        text(status).size(11),
    ]
    .spacing(8);
    let counters = text(format!(
        "{} ↑ {} ↓ · {} active / {} total",
        stats.bytes_sent, stats.bytes_received, stats.active_connections, stats.total_connections
    ))
    .size(11);

    let mut controls = row![].spacing(4);
    if snapshot.status == ForwardStatus::Running {
        controls = controls.push(
            button(text("Stop").size(11))
                .on_press(Message::Tunnel(TunnelMsg::Stop(id)))
                .padding([2, 8]),
        );
    } else {
        controls = controls.push(
            button(text("Start").size(11))
                .on_press(Message::Tunnel(TunnelMsg::Start(id)))
                .padding([2, 8]),
        );
    }
    controls = controls.push(
        button(text("Edit").size(11))
            .on_press(Message::Tunnel(TunnelMsg::Edit(id)))
            .padding([2, 8]),
    );
    controls = controls.push(
        button(text("Delete").size(11))
            .on_press(Message::Tunnel(TunnelMsg::Delete(id)))
            .padding([2, 8]),
    );

    let mut card = column![header, counters, controls].spacing(2);
    for line in crate::connection::forward::ForwardManager::shared().log_tail(id, 3) {
        card = card.push(text(line).size(10));
    }
    card.into()
}

#[cfg(feature = "ssh")]
fn snapshot_name(snapshot: &crate::connection::forward::PortForward) -> String {
    snapshot
        .name
        .clone()
        .unwrap_or_else(|| snapshot.forward_type.label())
}

#[cfg(feature = "ssh")]
fn tunnel_form(app: &AppState) -> Option<iced::Element<'_, Message>> {
    use crate::app::messages::{TunnelField, TunnelKind, TunnelMsg};
    use iced::widget::{button, row, text_input};

    let draft = app.tunnel_draft.as_ref()?;
    let session_name = app
        .session(draft.session)
        .map(|s| s.spec.name.as_str())
        .unwrap_or("?");
    let mut kinds = row![text("Type:").size(12)].spacing(4);
    for (kind, label) in [
        (TunnelKind::Local, "Local (-L)"),
        (TunnelKind::Remote, "Remote (-R)"),
        (TunnelKind::Dynamic, "Dynamic (-D)"),
    ] {
        let marker = if draft.kind == kind { "▸ " } else { "" };
        kinds = kinds.push(
            button(text(format!("{marker}{label}")).size(11))
                .on_press(Message::Tunnel(TunnelMsg::KindSelected(kind)))
                .padding([2, 8]),
        );
    }
    let auto = if draft.auto_start { "☑" } else { "☐" };
    let mut body = column![
        text(format!("Tunnel for {session_name}")).size(14),
        kinds,
        text_input("Name (optional)", &draft.name)
            .on_input(|value| Message::Tunnel(TunnelMsg::FieldChanged(TunnelField::Name, value)))
            .padding(4),
    ]
    .spacing(6);

    match draft.kind {
        TunnelKind::Dynamic => {
            body = body.push(bind_fields(draft, "Bind address", "Bind port"));
        },
        TunnelKind::Local => {
            body = body.push(bind_fields(draft, "Bind address (local)", "Bind port"));
            body = body.push(target_fields(draft, "Target host (remote)", "Target port"));
        },
        TunnelKind::Remote => {
            body = body.push(bind_fields(
                draft,
                "Bind address (remote side)",
                "Bind port",
            ));
            body = body.push(target_fields(draft, "Target host (local)", "Target port"));
        },
    }

    if let Some(error) = draft.error.as_deref() {
        body = body.push(text(format!("error: {error}")).size(12));
    }
    body = body.push(
        row![
            button(
                text(if draft.editing.is_some() {
                    "Save"
                } else {
                    "Add"
                })
                .size(12)
            )
            .on_press(Message::Tunnel(TunnelMsg::Submit))
            .padding([3, 10]),
            button(text(format!("Auto-start {auto}")).size(11))
                .on_press(Message::Tunnel(TunnelMsg::AutoStartToggled))
                .padding([3, 8]),
            button(text("Cancel").size(11))
                .on_press(Message::Tunnel(TunnelMsg::CloseForm))
                .padding([3, 8]),
        ]
        .spacing(8),
    );
    Some(body.into())
}

/// Bind address + port inputs (labels resolved by the caller).
#[cfg(feature = "ssh")]
fn bind_fields<'a>(
    draft: &'a crate::app::state::TunnelDraft,
    host_label: &'a str,
    port_label: &'a str,
) -> iced::Element<'a, Message> {
    use crate::app::messages::{TunnelField, TunnelMsg};
    use iced::widget::{column, row, text, text_input};
    column![
        text(host_label).size(11),
        row![
            text_input("bind address", &draft.bind_host)
                .on_input(|value| Message::Tunnel(TunnelMsg::FieldChanged(
                    TunnelField::BindHost,
                    value
                )))
                .padding(4)
                .width(iced::Fill),
            text_input(port_label, &draft.bind_port)
                .on_input(|value| Message::Tunnel(TunnelMsg::FieldChanged(
                    TunnelField::BindPort,
                    value
                )))
                .padding(4)
                .width(120),
        ]
        .spacing(8),
    ]
    .spacing(2)
    .into()
}

/// Target host + port inputs (labels resolved by the caller).
#[cfg(feature = "ssh")]
fn target_fields<'a>(
    draft: &'a crate::app::state::TunnelDraft,
    host_label: &'a str,
    port_label: &'a str,
) -> iced::Element<'a, Message> {
    use crate::app::messages::{TunnelField, TunnelMsg};
    use iced::widget::{column, row, text, text_input};
    column![
        text(host_label).size(11),
        row![
            text_input("host", &draft.target_host)
                .on_input(|value| Message::Tunnel(TunnelMsg::FieldChanged(
                    TunnelField::TargetHost,
                    value
                )))
                .padding(4)
                .width(iced::Fill),
            text_input(port_label, &draft.target_port)
                .on_input(|value| Message::Tunnel(TunnelMsg::FieldChanged(
                    TunnelField::TargetPort,
                    value
                )))
                .padding(4)
                .width(120),
        ]
        .spacing(8),
    ]
    .spacing(2)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> AppState {
        let base = std::env::temp_dir().join(format!("mbxt-tp-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        AppState::new(crate::utils::config::AppConfig::default(), paths).0
    }

    #[test]
    fn panel_renders_without_sessions() {
        let app = test_app();
        let _ = view(&app);
    }
}
