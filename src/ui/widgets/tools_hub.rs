//! Network-tools hub tab (Prompt 5.4): tool picker, target + params form,
//! local/remote scope, streaming run history with search.
//!
//! Pure view over [`ToolDraft`] + `tool_runs`; runs stream through
//! `Task::stream` into `ToolMsg::Event`. No state, no I/O here.

use iced::widget::{button, column, container, row, scrollable, text, text_input};

use crate::app::messages::{Message, ToolMsg};
use crate::app::state::AppState;
use crate::tools::{OutputLevel, RunStatus, ToolKind, ToolParams};

/// Hub tab content.
pub fn view(app: &AppState) -> iced::Element<'_, Message> {
    let mut body = column![text("Network tools").size(16)].spacing(8);
    body = body.push(kind_picker(app));
    body = body.push(target_row(app));
    body = body.push(params_form(app));
    if let Some(error) = app.tool_draft.error.as_deref() {
        body = body.push(text(error).size(12));
    }
    body = body.push(history(app));
    container(scrollable(body.spacing(8).padding(12)))
        .width(iced::Fill)
        .height(iced::Fill)
        .into()
}

fn kind_picker(app: &AppState) -> iced::Element<'_, Message> {
    let mut row = row![text("Tool:").size(12)].spacing(4);
    for kind in ToolKind::ALL {
        let label = if kind == app.tool_draft.kind {
            format!("[{}]", kind.name())
        } else {
            kind.name().to_string()
        };
        row = row.push(
            button(text(label).size(11))
                .on_press(Message::Tool(ToolMsg::KindSelected(kind)))
                .padding([2, 6]),
        );
    }
    row.spacing(4).wrap().into()
}

fn target_row(app: &AppState) -> iced::Element<'_, Message> {
    let draft = &app.tool_draft;
    let placeholder = match draft.kind {
        ToolKind::Http => "http://host:port",
        ToolKind::Subnet => "192.168.1.0/24",
        _ => "host, domain, or IP",
    };
    let mut body = column![text_input(placeholder, &draft.target)
        .on_input(|value| Message::Tool(ToolMsg::TargetChanged(value)))
        .padding(4),]
    .spacing(4);
    let mut scope = row![
        button(text(if draft.remote.is_none() {
            "[local]"
        } else {
            "local"
        }))
        .on_press(Message::Tool(ToolMsg::ScopeLocal))
        .padding([2, 8]),
        text("run on:").size(11),
    ]
    .spacing(4);
    for session in &app.sessions {
        let label = if draft.remote == Some(session.id) {
            format!("[{}]", session.spec.name)
        } else {
            session.spec.name.clone()
        };
        let id = session.id;
        scope = scope.push(
            button(text(label).size(11))
                .on_press(Message::Tool(ToolMsg::ScopeRemote(id)))
                .padding([2, 8]),
        );
    }
    body = body.push(scope.spacing(4).wrap());
    body = body.push(
        button(text("Run").size(12))
            .on_press(Message::Tool(ToolMsg::RunRequested))
            .padding([4, 16]),
    );
    body.into()
}

fn field<'a>(
    label: &'a str,
    value: &'a str,
    message: impl Fn(String) -> Message + 'a,
) -> iced::Element<'a, Message> {
    row![
        text(label).size(11).width(110),
        text_input("", value).on_input(message).padding(4),
    ]
    .spacing(4)
    .into()
}

fn bool_field<'a>(
    label: &'a str,
    value: bool,
    message: impl Fn(String) -> Message + 'a,
) -> iced::Element<'a, Message> {
    row![
        text(label).size(11).width(110),
        button(text(if value { "[true]" } else { "false" }).size(11))
            .on_press(message((!value).to_string()))
            .padding([2, 8]),
    ]
    .spacing(4)
    .into()
}

fn text_of<'a>(draft: &'a crate::app::state::ToolDraft, field: &str) -> &'a str {
    draft
        .field_text
        .get(field)
        .map(String::as_str)
        .unwrap_or("")
}

fn params_form(app: &AppState) -> iced::Element<'_, Message> {
    let draft = &app.tool_draft;
    let params = &draft.params;
    let edit = |field: &'static str| {
        move |value: String| Message::Tool(ToolMsg::ParamChanged(field.to_string(), value))
    };
    let mut form = column![].spacing(4);
    match params {
        ToolParams::Ping { v6, .. } => {
            form = form.push(field("count", text_of(draft, "count"), edit("count")));
            form = form.push(field(
                "interval ms",
                text_of(draft, "interval_ms"),
                edit("interval_ms"),
            ));
            form = form.push(field("size", text_of(draft, "size"), edit("size")));
            form = form.push(bool_field("IPv6", *v6, edit("v6")));
        },
        ToolParams::Traceroute { .. } => {
            form = form.push(field(
                "max hops",
                text_of(draft, "max_hops"),
                edit("max_hops"),
            ));
            form = form.push(field(
                "timeout ms",
                text_of(draft, "timeout_ms"),
                edit("timeout_ms"),
            ));
        },
        ToolParams::Dns { .. } => {
            if app.tool_draft.kind == ToolKind::Dns {
                form = form.push(field(
                    "record type",
                    text_of(draft, "record_type"),
                    edit("record_type"),
                ));
            }
            form = form.push(field(
                "server (empty = system)",
                text_of(draft, "server"),
                edit("server"),
            ));
        },
        ToolParams::ReverseDns => {
            form = form.push(text("PTR lookup via the system resolver").size(11));
        },
        ToolParams::Whois { .. } => {
            form = form.push(field(
                "server (empty = iana)",
                text_of(draft, "server"),
                edit("server"),
            ));
        },
        ToolParams::PortScan { banner, .. } => {
            form = form.push(field(
                "ports (22,80-82)",
                text_of(draft, "ports"),
                edit("ports"),
            ));
            form = form.push(field(
                "concurrency",
                text_of(draft, "concurrency"),
                edit("concurrency"),
            ));
            form = form.push(field(
                "timeout ms",
                text_of(draft, "timeout_ms"),
                edit("timeout_ms"),
            ));
            form = form.push(bool_field("banner grab", *banner, edit("banner")));
        },
        ToolParams::Http { .. } => {
            form = form.push(field("method", text_of(draft, "method"), edit("method")));
            form = form.push(field("path", text_of(draft, "path"), edit("path")));
            form = form.push(field(
                "headers (Name: v)",
                text_of(draft, "headers"),
                edit("headers"),
            ));
            form = form.push(field("body", text_of(draft, "body"), edit("body")));
        },
        ToolParams::Subnet { .. } => {
            form = form.push(field("CIDR", text_of(draft, "cidr"), edit("cidr")));
        },
        ToolParams::Bandwidth { .. } => {
            form = form.push(field(
                "mode (send/receive)",
                text_of(draft, "mode"),
                edit("mode"),
            ));
            form = form.push(field("port", text_of(draft, "port"), edit("port")));
            form = form.push(field("seconds", text_of(draft, "seconds"), edit("seconds")));
        },
    }
    form.into()
}

fn history(app: &AppState) -> iced::Element<'_, Message> {
    let mut body = column![
        row![
            text("History").size(14).width(iced::Fill),
            button(text("Clear finished").size(11))
                .on_press(Message::Tool(ToolMsg::ClearHistory))
                .padding([2, 8]),
        ]
        .spacing(8),
        text_input("search output", &app.tool_draft.search)
            .on_input(|value| Message::Tool(ToolMsg::SearchChanged(value)))
            .padding(4),
    ]
    .spacing(4);
    let mut runs: Vec<_> = app.tool_runs.iter().collect();
    runs.reverse();
    if runs.is_empty() {
        body = body.push(text("no runs yet").size(11));
    }
    for run in runs {
        body = body.push(run_card(app, run));
    }
    body.into()
}

fn run_card<'a>(app: &'a AppState, run: &'a crate::tools::ToolRun) -> iced::Element<'a, Message> {
    let status = match &run.status {
        RunStatus::Running => "● running".to_string(),
        RunStatus::Completed => "done".to_string(),
        RunStatus::Failed(error) => format!("failed: {error}"),
        RunStatus::Cancelled => "cancelled".to_string(),
    };
    let mut card = column![row![
        text(format!("{} — {}", run.kind.name(), run.target_label))
            .size(12)
            .width(iced::Fill),
        text(status).size(11),
    ]
    .spacing(8)]
    .spacing(2);
    let shown: Vec<_> = if app.tool_draft.search.is_empty() {
        run.lines.iter().collect()
    } else {
        crate::tools::filter_lines(&run.lines, &app.tool_draft.search)
            .into_iter()
            .map(|(_, line)| line)
            .collect()
    };
    for (level, line) in shown.iter().rev().take(30).rev() {
        let prefix = match level {
            OutputLevel::Success => "+ ",
            OutputLevel::Warning => "! ",
            OutputLevel::Error => "× ",
            OutputLevel::Info => "",
        };
        card = card.push(text(format!("{prefix}{line}")).size(11));
    }
    if run.status == RunStatus::Running {
        let id = run.id;
        card = card.push(
            button(text("Cancel").size(11))
                .on_press(Message::Tool(ToolMsg::CancelRequested(id)))
                .padding([2, 8]),
        );
    }
    container(card.spacing(2).padding(6)).into()
}
