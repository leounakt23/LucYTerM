//! Multi-exec toolbar (Prompt 5.1): arming toggle, quick-select, stagger,
//! host filter, template sender, destructive-confirm, and view opener.
//!
//! Pure view over `MultiExecMode` + draft fields — no state, no I/O. The
//! strip stays compact when disarmed (toggle + count) and expands to full
//! controls while broadcasting.

use iced::widget::{button, column, container, row, text, text_input};

use crate::app::messages::{Message, UiMsg};
use crate::app::state::AppState;

/// Toolbar strip for multi-execution.
pub fn view(app: &AppState) -> iced::Element<'_, Message> {
    let mode = &app.multi_exec_mode;
    let toggle_label = if mode.enabled {
        format!("⏺ multi-exec ON ({} target(s))", mode.targets.len())
    } else {
        "○ multi-exec (Ctrl+Shift+M)".to_string()
    };
    let mut strip = row![crate::ui::theme::chrome_button(
        app.theme.clone(),
        "multi-exec toggle",
        button(text(toggle_label).size(12))
    )
    .on_press(Message::Ui(UiMsg::MultiExecToggle))
    .padding([3, 8]),]
    .spacing(8);

    if mode.enabled {
        strip = strip.push(
            crate::ui::theme::chrome_button(
                app.theme.clone(),
                "Combined output",
                button(text("Combined output").size(11)),
            )
            .on_press(Message::Ui(UiMsg::MultiExecOpenView))
            .padding([3, 8]),
        );
        strip = strip.push(quick_select_row(&app.theme));
        strip = strip.push(
            crate::ui::theme::chrome_button(
                app.theme.clone(),
                "stagger",
                button(text(format!("stagger {}ms", mode.stagger_ms)).size(11)),
            )
            .on_press(Message::Ui(UiMsg::MultiExecStaggerChanged(next_stagger(
                mode.stagger_ms,
            ))))
            .padding([3, 8]),
        );
    }

    let mut body = column![strip].spacing(4);

    if mode.enabled {
        body = body.push(
            row![
                text_input("host filter (empty = all)…", &mode.host_filter)
                    .on_input(|value| Message::Ui(UiMsg::MultiExecFilterChanged(value)))
                    .padding(4)
                    .width(iced::Fill),
                text_input("template ($SESSION_HOST…)…", &app.multi_exec_template)
                    .on_input(|value| Message::Ui(UiMsg::MultiExecTemplateChanged(value)))
                    .on_submit(Message::Ui(UiMsg::MultiExecTemplateSend))
                    .padding(4)
                    .width(iced::Fill),
                crate::ui::theme::chrome_button(
                    app.theme.clone(),
                    "Send template",
                    button(text("Send template").size(11))
                )
                .on_press(Message::Ui(UiMsg::MultiExecTemplateSend))
                .padding([3, 8]),
            ]
            .spacing(8),
        );

        // Destructive broadcast held for confirmation (safety gate).
        if app.multi_exec_pending.is_some() {
            body = body.push(
                row![
                    text("⚠ destructive broadcast held — confirm to send").size(12),
                    crate::ui::theme::chrome_button(
                        app.theme.clone(),
                        "Confirm send",
                        button(text("Confirm send").size(11))
                    )
                    .on_press(Message::Ui(UiMsg::MultiExecConfirmBroadcast))
                    .padding([3, 8]),
                    crate::ui::theme::chrome_button(
                        app.theme.clone(),
                        "Discard",
                        button(text("Discard").size(11))
                    )
                    .on_press(Message::Ui(UiMsg::MultiExecDiscardBroadcast))
                    .padding([3, 8]),
                ]
                .spacing(8),
            );
        }

        if let Some(last) = app.multi_exec_history.last() {
            body = body.push(text(format!("last sent: {last}")).size(11));
        }
    }

    container(body).width(iced::Fill).padding([4, 8]).into()
}

fn quick_select_row(theme: &crate::ui::theme::AppTheme) -> iced::Element<'static, Message> {
    row![
        crate::ui::theme::chrome_button(theme.clone(), "All", button(text("All").size(11)))
            .on_press(Message::Ui(UiMsg::MultiExecSelectAll))
            .padding([3, 8]),
        crate::ui::theme::chrome_button(theme.clone(), "SSH", button(text("SSH").size(11)))
            .on_press(Message::Ui(UiMsg::MultiExecSelectSsh))
            .padding([3, 8]),
        crate::ui::theme::chrome_button(theme.clone(), "Tabs", button(text("Tabs").size(11)))
            .on_press(Message::Ui(UiMsg::MultiExecSelectTabs))
            .padding([3, 8]),
        crate::ui::theme::chrome_button(theme.clone(), "Clear", button(text("Clear").size(11)))
            .on_press(Message::Ui(UiMsg::MultiExecClearTargets))
            .padding([3, 8]),
    ]
    .spacing(4)
    .into()
}

/// Stagger cycle: off → 50 → 100 → 250 ms → off.
fn next_stagger(current_ms: u64) -> u64 {
    match current_ms {
        0 => 50,
        1..=50 => 100,
        51..=100 => 250,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> AppState {
        let base = std::env::temp_dir().join(format!("mbxt-met-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        AppState::new(crate::utils::config::AppConfig::default(), paths).0
    }

    #[test]
    fn toolbar_renders_disarmed_and_armed() {
        let app = test_app();
        let _ = view(&app);

        let mut armed = test_app();
        armed.multi_exec_mode.enabled = true;
        armed.multi_exec_mode.targets = vec![1, 2];
        armed.multi_exec_pending = Some(b"rm -rf /".to_vec());
        armed.multi_exec_history.push("uptime".to_string());
        let _ = view(&armed);
    }

    #[test]
    fn stagger_cycles_without_sticking() {
        assert_eq!(next_stagger(0), 50);
        assert_eq!(next_stagger(50), 100);
        assert_eq!(next_stagger(100), 250);
        assert_eq!(next_stagger(250), 0);
        assert_eq!(next_stagger(9999), 0);
    }
}
