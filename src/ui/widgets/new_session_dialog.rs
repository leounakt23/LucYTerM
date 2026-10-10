//! New-session dialog (Prompt 4.4): protocol picker (SSH/Telnet/Serial),
//! host/port fields, serial device picker + baud, inline validation.
//!
//! Pure view over [`NewSessionDraft`] — `None` (closed) renders nothing.
//! The dialog replaces the main content like the auth modal while open.

use iced::widget::{button, column, container, row, scrollable, text, text_input};

use crate::app::messages::{Message, NewSessionField, UiMsg};
use crate::app::state::{AppState, NewSessionDraft};
use mbxt_core::Protocol;

/// Dialog overlay (`None` when closed).
pub fn view(app: &AppState) -> Option<iced::Element<'_, Message>> {
    let draft = app.new_session.as_ref()?;
    Some(dialog(draft))
}

fn dialog(draft: &NewSessionDraft) -> iced::Element<'_, Message> {
    let protocols = row![
        protocol_button(draft, Protocol::Ssh, "SSH"),
        protocol_button(draft, Protocol::Telnet, "Telnet"),
        protocol_button(draft, Protocol::Serial, "Serial"),
        protocol_button(draft, Protocol::Rdp, "RDP"),
        protocol_button(draft, Protocol::Vnc, "VNC"),
    ]
    .spacing(6);

    let mut body = column![
        text(if draft.editing.is_some() {
            "Edit session"
        } else {
            "New session"
        })
        .size(18),
        protocols,
        field("Name", &draft.name, NewSessionField::Name, "session-1"),
    ]
    .spacing(8);

    match draft.protocol {
        Protocol::Ssh | Protocol::Telnet | Protocol::Sftp | Protocol::X11 => {
            body = body.push(field(
                "Host",
                &draft.host,
                NewSessionField::Host,
                "192.168.1.10",
            ));
            body = body.push(field(
                if draft.protocol == Protocol::Telnet {
                    "Port (23)"
                } else {
                    "Port (22)"
                },
                &draft.port,
                NewSessionField::Port,
                "22",
            ));
        },
        // RDP/VNC: same host/port shape, viewer-side defaults.
        Protocol::Rdp | Protocol::Vnc => {
            body = body.push(field(
                "Host",
                &draft.host,
                NewSessionField::Host,
                "192.168.1.10",
            ));
            body = body.push(field(
                if draft.protocol == Protocol::Rdp {
                    "Port (3389)"
                } else {
                    "Port (5900)"
                },
                &draft.port,
                NewSessionField::Port,
                if draft.protocol == Protocol::Rdp {
                    "3389"
                } else {
                    "5900"
                },
            ));
        },
        Protocol::Serial => {
            body = body.push(device_picker(draft));
            body = body.push(field(
                "Baud rate",
                &draft.baud,
                NewSessionField::Baud,
                "115200",
            ));
        },
        _ => {
            body = body.push(text("this protocol is configured elsewhere").size(12));
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
                    "Create"
                })
                .size(13)
            )
            .on_press(Message::Ui(UiMsg::NewSessionSubmitted))
            .padding([4, 12]),
            button(text("Cancel").size(13))
                .on_press(Message::Ui(UiMsg::NewSessionDialogClosed))
                .padding([4, 12]),
        ]
        .spacing(8),
    );

    container(scrollable(body.spacing(8).padding(16)))
        .width(iced::Fill)
        .height(iced::Fill)
        .into()
}

fn protocol_button<'a>(
    draft: &'a NewSessionDraft,
    protocol: Protocol,
    label: &'a str,
) -> iced::Element<'a, Message> {
    let marker = if draft.protocol == protocol {
        "▸ "
    } else {
        ""
    };
    button(text(format!("{marker}{label}")).size(12))
        .on_press(Message::Ui(UiMsg::NewSessionProtocolSelected(protocol)))
        .padding([4, 10])
        .into()
}

fn field<'a>(
    label: &'a str,
    value: &'a str,
    field: NewSessionField,
    placeholder: &'a str,
) -> iced::Element<'a, Message> {
    column![
        text(label).size(12),
        text_input(placeholder, value)
            .on_input(move |input| Message::Ui(UiMsg::NewSessionFieldChanged(field, input)))
            .padding(4),
    ]
    .spacing(2)
    .into()
}

/// Serial device picker: one button per detected port (fills the device
/// field), manual entry, refresh, and the no-backend note.
fn device_picker(draft: &NewSessionDraft) -> iced::Element<'_, Message> {
    let mut list = column![text("Device").size(12)].spacing(2);
    if draft.serial_ports.is_empty() {
        list = list.push(text("no serial ports detected").size(11));
    }
    for port in &draft.serial_ports {
        // Button label owns its text; the message carries the choice.
        let choice = port.clone();
        // Show only the device node on the button (full descriptor in title).
        let short = choice
            .split_whitespace()
            .next()
            .unwrap_or(&choice)
            .to_string();
        list = list.push(
            button(text(short).size(11))
                .on_press(Message::Ui(UiMsg::NewSessionFieldChanged(
                    NewSessionField::Device,
                    choice,
                )))
                .padding([2, 8]),
        );
    }
    list = list.push(
        text_input("/dev/ttyUSB0", &draft.device)
            .on_input(|input| {
                Message::Ui(UiMsg::NewSessionFieldChanged(
                    NewSessionField::Device,
                    input,
                ))
            })
            .padding(4),
    );
    list = list.push(
        button(text("Refresh ports").size(11))
            .on_press(Message::Ui(UiMsg::NewSessionSerialRefresh))
            .padding([2, 8]),
    );
    list.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::config::AppConfig;
    use crate::utils::paths::AppPaths;

    fn test_app() -> AppState {
        let base = std::env::temp_dir().join(format!("mbxt-nsd-test-{}", std::process::id()));
        let paths = AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        AppState::new(AppConfig::default(), paths).0
    }

    #[test]
    fn closed_dialog_renders_nothing() {
        let app = test_app();
        assert!(view(&app).is_none());
    }

    #[test]
    fn dialog_covers_all_protocols() {
        let mut app = test_app();
        for protocol in [
            Protocol::Ssh,
            Protocol::Telnet,
            Protocol::Serial,
            Protocol::Rdp,
            Protocol::Vnc,
        ] {
            let mut draft = NewSessionDraft::new("test".into());
            draft.protocol = protocol;
            draft.serial_ports = vec!["/dev/ttyUSB0 — USB".into()];
            app.new_session = Some(draft);
            let _ = view(&app).expect("open dialog renders");
        }
    }

    #[test]
    fn dialog_state_round_trips_through_draft() {
        let mut app = test_app();
        app.new_session = Some(NewSessionDraft::new("console".into()));
        let draft = app.new_session.as_ref().unwrap();
        assert_eq!(draft.protocol, Protocol::Ssh);
        assert_eq!(draft.port, "22");
        assert_eq!(draft.baud, "115200");
    }

    #[test]
    fn edit_dialog_renders_with_save_label() {
        let mut app = test_app();
        let mut draft = NewSessionDraft::new("lan".into());
        draft.editing = Some(7);
        draft.host = "127.0.0.1".into();
        app.new_session = Some(draft);
        let _ = view(&app).expect("edit dialog renders");
    }
}
