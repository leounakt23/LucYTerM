//! Auth dialog (prompt 2.2): modal for password / passphrase / OTP entry.
//!
//! Rendered in place of the main content while a prompt is pending (blocking
//! modal semantics without absolute-position overlays); the typed answer is
//! buffered in `AppState::auth_input` and submitted via `AuthSubmit` /
//! `Message::AuthResponse` — the awaiting provider's oneshot resolves inside
//! `PromptBridge`.

use iced::widget::{button, column, container, row, text, text_input};

use crate::app::messages::{Message, UiMsg};
use crate::app::state::AppState;
use crate::connection::ssh::auth::PromptKind;

/// Modal view, `Some` only while `app.auth_dialog` is pending.
pub fn view(app: &AppState) -> Option<iced::Element<'_, Message>> {
    let prompt = app.auth_dialog.as_ref()?;

    let kind_label = match prompt.kind {
        PromptKind::Password => "password",
        PromptKind::Passphrase => "key passphrase",
        PromptKind::Otp => "verification code (two-factor)",
    };

    let input = text_input(prompt.prompt.as_str(), &app.auth_input)
        .secure(prompt.masked)
        .on_input(|value| Message::Ui(UiMsg::AuthInputChanged(value)))
        .on_submit(Message::Ui(UiMsg::AuthSubmit))
        .padding(6)
        .width(320);

    let dialog = container(column![
        text(prompt.title.as_str()).size(18),
        text(prompt.prompt.as_str()).size(13),
        text(format!("authentication method: {kind_label}")).size(11),
        input,
        row![
            button(text("Cancel").size(13))
                .on_press(Message::Ui(UiMsg::AuthCancel))
                .padding(6),
            button(text("OK").size(13))
                .on_press(Message::Ui(UiMsg::AuthSubmit))
                .padding(6),
        ]
        .spacing(8),
    ])
    .padding(16)
    .width(iced::Shrink);

    Some(
        container(dialog)
            .width(iced::Fill)
            .height(iced::Fill)
            .center_x(iced::Fill)
            .padding(24)
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::ssh::auth::AuthPrompt;

    fn app_with_prompt(kind: PromptKind) -> AppState {
        let base = std::env::temp_dir().join(format!("mbxt-authdlg-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        let (mut state, _) = AppState::new(crate::utils::config::AppConfig::default(), paths);
        state.auth_dialog = Some(AuthPrompt {
            id: 1,
            kind,
            title: "Authentication".into(),
            prompt: "Password for ops:".into(),
            masked: true,
        });
        state
    }

    #[test]
    fn no_dialog_renders_nothing() {
        let base = std::env::temp_dir().join(format!("mbxt-authdlg0-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        let (state, _) = AppState::new(crate::utils::config::AppConfig::default(), paths);
        assert!(view(&state).is_none());
    }

    #[test]
    fn renders_for_each_prompt_kind() {
        for kind in [
            PromptKind::Password,
            PromptKind::Passphrase,
            PromptKind::Otp,
        ] {
            let state = app_with_prompt(kind);
            let element = view(&state);
            assert!(element.is_some(), "{kind:?} should render");
        }
    }
}
