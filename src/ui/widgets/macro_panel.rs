//! Macro list panel (Prompt 5.2): library rows with Play/Edit/Duplicate/
//! Delete/Export actions, recording indicator, variable prompt form,
//! pending-play confirmation, and footer (counts, load errors, macros dir).
//!
//! Pure view over `AppState::macros` + recorder/player/prompt state.

use iced::widget::{button, column, container, row, scrollable, text, text_input};

use crate::app::messages::{MacroMsg, Message};
use crate::app::state::AppState;

/// Library tab content: panel + editor side by side.
pub fn tab_view(app: &AppState) -> iced::Element<'_, Message> {
    row![
        container(panel(app)).width(360).height(iced::Fill),
        iced::widget::vertical_rule(2),
        container(super::macro_editor::view(app))
            .width(iced::Fill)
            .height(iced::Fill),
    ]
    .spacing(4)
    .into()
}

/// Macro list panel.
pub fn panel(app: &AppState) -> iced::Element<'_, Message> {
    let mut list = column![].spacing(4);

    // Recorder status + controls.
    list = list.push(record_row(app));

    // Pending destructive play confirmation.
    if let Some(pending) = app.macro_pending_play.as_ref() {
        let name = app
            .macros
            .iter()
            .find(|m| m.id == pending.macro_id)
            .map(|m| m.name.as_str())
            .unwrap_or("?");
        list = list.push(
            column![
                text(format!("⚠ play destructive macro \"{name}\"?")).size(12),
                row![
                    button(text("Confirm").size(11))
                        .on_press(Message::Macro(MacroMsg::ConfirmPlay))
                        .padding([2, 8]),
                    button(text("Discard").size(11))
                        .on_press(Message::Macro(MacroMsg::DiscardPlay))
                        .padding([2, 8]),
                ]
                .spacing(6),
            ]
            .spacing(4),
        );
    }

    // Variable prompt form.
    if let Some(prompt) = app.macro_prompt.as_ref() {
        list = list.push(variable_prompt(app, prompt));
    }

    // Running player progress.
    if let Some(running) = app.macro_player.as_ref() {
        let (done, total) = running.player.progress();
        list = list.push(
            row![
                text(format!(
                    "▶ {} ({done}/{total})",
                    running.player.macro_name()
                ))
                .size(12),
                button(text("Stop").size(11))
                    .on_press(Message::Macro(MacroMsg::Stop))
                    .padding([2, 8]),
            ]
            .spacing(8),
        );
    }

    // Library rows.
    if app.macros.is_empty() {
        list = list.push(text("no macros yet — record one (Ctrl+Shift+R)").size(12));
    }
    for macro_ in &app.macros {
        list = list.push(macro_row(app, macro_));
    }

    // Footer: counts, load errors, storage location, rescan.
    let mut footer = column![
        text(format!(
            "{} macro(s), {} failed to load",
            app.macros.len(),
            app.macro_load_errors.len()
        ))
        .size(11),
        row![
            button(text("New").size(11))
                .on_press(Message::Macro(MacroMsg::EditorOpened(None)))
                .padding([2, 8]),
            button(text("Refresh").size(11))
                .on_press(Message::Macro(MacroMsg::Refresh))
                .padding([2, 8]),
        ]
        .spacing(6),
    ]
    .spacing(4);
    for error in app.macro_load_errors.iter().take(3) {
        footer = footer.push(text(format!("load error: {error}")).size(10));
    }
    footer = footer.push(
        text(format!(
            "macros dir: {}",
            app.paths.config_dir.join("macros").display()
        ))
        .size(10),
    );
    list = list.push(footer);

    container(scrollable(list.spacing(6).padding(8)))
        .width(iced::Fill)
        .height(iced::Fill)
        .into()
}

fn record_row(app: &AppState) -> iced::Element<'_, Message> {
    use crate::macros::RecorderState;
    match app.macro_recorder.state() {
        RecorderState::Idle => row![button(text("● Record (Ctrl+Shift+R)").size(12))
            .on_press(Message::Macro(MacroMsg::RecordToggle))
            .padding([3, 8]),]
        .spacing(8)
        .into(),
        RecorderState::Recording => row![
            text("⏺ recording…").size(12),
            button(text("Pause").size(11))
                .on_press(Message::Macro(MacroMsg::RecordPause))
                .padding([2, 8]),
            button(text("Stop & keep").size(11))
                .on_press(Message::Macro(MacroMsg::RecordToggle))
                .padding([2, 8]),
        ]
        .spacing(8)
        .into(),
        RecorderState::Paused => row![
            text("⏸ paused").size(12),
            button(text("Resume").size(11))
                .on_press(Message::Macro(MacroMsg::RecordPause))
                .padding([2, 8]),
            button(text("Stop & keep").size(11))
                .on_press(Message::Macro(MacroMsg::RecordToggle))
                .padding([2, 8]),
        ]
        .spacing(8)
        .into(),
    }
}

fn variable_prompt<'a>(
    app: &'a AppState,
    prompt: &'a crate::macros::MacroPrompt,
) -> iced::Element<'a, Message> {
    column![
        text(format!("{}:", prompt.prompt)).size(12),
        text_input("value…", &app.macro_prompt_input)
            .on_input(|value| Message::Macro(MacroMsg::PromptInputChanged(value)))
            .on_submit(Message::Macro(MacroMsg::PromptAnswer(
                app.macro_prompt_input.clone()
            )))
            .padding(4),
        row![
            button(text("OK").size(11))
                .on_press(Message::Macro(MacroMsg::PromptAnswer(
                    app.macro_prompt_input.clone()
                )))
                .padding([2, 8]),
            button(text("Cancel").size(11))
                .on_press(Message::Macro(MacroMsg::PromptCancel))
                .padding([2, 8]),
        ]
        .spacing(6),
    ]
    .spacing(4)
    .into()
}

fn macro_row<'a>(
    app: &'a AppState,
    macro_: &'a crate::macros::Macro,
) -> iced::Element<'a, Message> {
    let id = macro_.id;
    let hotkey = macro_
        .hotkey
        .as_deref()
        .map(|h| format!(" [{h}]"))
        .unwrap_or_default();
    let editing = app
        .macro_editor
        .as_ref()
        .is_some_and(|editor| editor.draft.id == id);
    let marker = if editing { "▸ " } else { "" };
    column![
        row![
            text(format!(
                "{marker}{} ({} steps){hotkey}",
                macro_.name,
                macro_.steps.len()
            ))
            .size(13)
            .width(iced::Fill),
            button(text("⋮").size(11))
                .on_press(Message::Macro(MacroMsg::EditorOpened(Some(id))))
                .padding([2, 6]),
        ]
        .spacing(4),
        row![
            button(text("Play").size(11))
                .on_press(Message::Macro(MacroMsg::Play(id)))
                .padding([2, 8]),
            button(text("Multi").size(11))
                .on_press(Message::Macro(MacroMsg::PlayOnTargets(id)))
                .padding([2, 8]),
            button(text("Dup").size(11))
                .on_press(Message::Macro(MacroMsg::EditorDuplicate(id)))
                .padding([2, 8]),
            button(text("Del").size(11))
                .on_press(Message::Macro(MacroMsg::EditorDelete(id)))
                .padding([2, 8]),
            button(text("Export").size(11))
                .on_press(Message::Macro(MacroMsg::EditorExport(id)))
                .padding([2, 8]),
        ]
        .spacing(4),
    ]
    .spacing(2)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> AppState {
        let base = std::env::temp_dir().join(format!("mbxt-mp-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        AppState::new(crate::utils::config::AppConfig::default(), paths).0
    }

    #[test]
    fn panel_renders_empty_and_full() {
        let app = test_app();
        let _ = panel(&app);
        let _ = tab_view(&app);

        let mut full = test_app();
        let mut macro_ = crate::macros::Macro::new("demo".into());
        macro_.hotkey = Some("F5".into());
        macro_.steps = vec![crate::macros::MacroStep::SendInput {
            data: "ls\n".into(),
        }];
        full.macros.push(macro_);
        full.macro_recorder.start(1);
        full.macro_pending_play = Some(crate::macros::PendingPlay {
            macro_id: full.macros[0].id,
            session: Some(1),
            vars: crate::macros::Context::new(),
        });
        full.macro_prompt = Some(crate::macros::MacroPrompt {
            var_name: "pw".into(),
            prompt: "Password".into(),
            default: None,
            secret: true,
            session: 1,
        });
        let _ = panel(&full);
        let _ = tab_view(&full);
    }
}
