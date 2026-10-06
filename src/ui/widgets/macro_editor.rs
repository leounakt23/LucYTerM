//! Macro step editor (Prompt 5.2): metadata form, step list with
//! up/down/delete, new-step form per kind, sanitized previews, and the
//! dry-run test pane.
//!
//! Pure view over `MacroEditorState` — reordering is up/down buttons (iced
//! exposes no widget drag-drop); every destructive action stays a button.

use iced::widget::{button, column, container, row, scrollable, text, text_input};

use crate::app::messages::{MacroEditorField, MacroMsg, Message};
use crate::app::state::AppState;
use crate::macros::MacroStepKind;

/// Editor pane (`None` content when closed — callers handle that).
pub fn view(app: &AppState) -> iced::Element<'_, Message> {
    let Some(editor) = app.macro_editor.as_ref() else {
        return container(text("select a macro to edit (or New)").size(12))
            .width(iced::Fill)
            .padding(8)
            .into();
    };
    let secrets = editor_secrets(editor);

    let mut body = column![
        row![
            text("Macro editor").size(16),
            button(text("Save").size(11))
                .on_press(Message::Macro(MacroMsg::EditorSave))
                .padding([2, 8]),
            button(text("Test run").size(11))
                .on_press(Message::Macro(MacroMsg::EditorTest))
                .padding([2, 8]),
            button(text("Close").size(11))
                .on_press(Message::Macro(MacroMsg::EditorClosed))
                .padding([2, 8]),
        ]
        .spacing(8),
        meta_form(editor),
    ]
    .spacing(8);

    // Step list.
    let mut steps =
        column![text(format!("Steps ({})", editor.draft.steps.len())).size(13)].spacing(2);
    for (index, step) in editor.draft.steps.iter().enumerate() {
        let selected = editor.selected == Some(index);
        let marker = if selected { "▸ " } else { "" };
        steps = steps.push(
            row![
                button(text(format!("{marker}#{index}")).size(11))
                    .on_press(Message::Macro(MacroMsg::EditorSelected(index)))
                    .padding([2, 6]),
                text(step.preview(&secrets)).size(11).width(iced::Fill),
                button(text("↑").size(11))
                    .on_press(Message::Macro(MacroMsg::EditorMoveStep(index, true)))
                    .padding([2, 6]),
                button(text("↓").size(11))
                    .on_press(Message::Macro(MacroMsg::EditorMoveStep(index, false)))
                    .padding([2, 6]),
                button(text("✕").size(11))
                    .on_press(Message::Macro(MacroMsg::EditorDeleteStep(index)))
                    .padding([2, 6]),
            ]
            .spacing(4),
        );
    }
    body = body.push(scrollable(steps).height(iced::Fill));
    body = body.push(new_step_form(editor));

    if let Some(report) = editor.report.as_deref() {
        body = body.push(
            column![
                text("Dry-run report").size(13),
                scrollable(text(report).size(11).font(iced::Font::MONOSPACE)).height(140),
            ]
            .spacing(4),
        );
    }

    container(body.spacing(8).padding(8))
        .width(iced::Fill)
        .height(iced::Fill)
        .into()
}

/// Secret values declared by the draft (preview redaction).
fn editor_secrets(editor: &crate::macros::MacroEditorState) -> Vec<String> {
    editor
        .draft
        .variables
        .iter()
        .filter(|var| var.secret)
        .filter_map(|var| var.default_value.clone())
        .filter(|value| !value.is_empty())
        .collect()
}

fn meta_form(editor: &crate::macros::MacroEditorState) -> iced::Element<'_, Message> {
    column![
        text_input("Name", &editor.draft.name)
            .on_input(|value| Message::Macro(MacroMsg::EditorFieldChanged(
                MacroEditorField::Name,
                value
            )))
            .padding(4),
        text_input(
            "Description (optional)",
            editor.draft.description.as_deref().unwrap_or("")
        )
        .on_input(|value| Message::Macro(MacroMsg::EditorFieldChanged(
            MacroEditorField::Description,
            value
        )))
        .padding(4),
        row![
            text_input("tags (comma separated)", &editor.draft.tags.join(", "))
                .on_input(|value| Message::Macro(MacroMsg::EditorFieldChanged(
                    MacroEditorField::Tags,
                    value
                )))
                .padding(4)
                .width(iced::Fill),
            text_input(
                "hotkey (F5–F8, optional)",
                editor.draft.hotkey.as_deref().unwrap_or("")
            )
            .on_input(|value| Message::Macro(MacroMsg::EditorFieldChanged(
                MacroEditorField::Hotkey,
                value
            )))
            .padding(4)
            .width(iced::Fill),
        ]
        .spacing(8),
    ]
    .spacing(4)
    .into()
}

fn new_step_form(editor: &crate::macros::MacroEditorState) -> iced::Element<'_, Message> {
    let mut kinds = row![text("Add step:").size(12)].spacing(4);
    for kind in MacroStepKind::ALL {
        let marker = if editor.new_kind == kind { "▸" } else { "" };
        kinds = kinds.push(
            button(text(format!("{marker}{}", kind.label())).size(10))
                .on_press(Message::Macro(MacroMsg::EditorAddStep(kind)))
                .padding([2, 6]),
        );
    }
    // Kind buttons above both select AND append (single click); the fields
    // below feed the next append for kinds needing payloads.
    column![
        kinds,
        row![
            text_input("text / pattern / name", &editor.text1)
                .on_input(|value| Message::Macro(MacroMsg::EditorFieldChanged(
                    MacroEditorField::Text1,
                    value
                )))
                .padding(4)
                .width(iced::Fill),
            text_input("value / prompt", &editor.text2)
                .on_input(|value| Message::Macro(MacroMsg::EditorFieldChanged(
                    MacroEditorField::Text2,
                    value
                )))
                .padding(4)
                .width(iced::Fill),
            text_input("number (ms/count)", &editor.number.to_string())
                .on_input(|value| Message::Macro(MacroMsg::EditorFieldChanged(
                    MacroEditorField::Number,
                    value
                )))
                .padding(4)
                .width(120),
        ]
        .spacing(8),
    ]
    .spacing(4)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> AppState {
        let base = std::env::temp_dir().join(format!("mbxt-me-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        AppState::new(crate::utils::config::AppConfig::default(), paths).0
    }

    #[test]
    fn editor_renders_closed_and_open() {
        let app = test_app();
        let _ = view(&app);

        let mut open = test_app();
        let mut draft = crate::macros::Macro::new("demo".into());
        draft.steps = vec![
            crate::macros::MacroStep::SendInput {
                data: "ls\n".into(),
            },
            crate::macros::MacroStep::Wait { duration_ms: 100 },
        ];
        let mut editor = crate::macros::MacroEditorState::new(draft);
        editor.selected = Some(0);
        editor.report = Some("completed: true".into());
        open.macro_editor = Some(editor);
        let _ = view(&open);
    }
}
