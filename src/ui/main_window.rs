//! Main window composition (prompt 1.4): toolbar / sidebar+tabbed content /
//! status bar with the adjustable split.
//!
//! iced-version note: the prompt referenced `iced::Application` +
//! `type Executor = iced::executor::Default` (0.10-era). On our stack
//! (iced 0.13, tech_stack.md) the equivalent wiring lives in
//! `app::run` — `iced::application(title, update, view)` with the default
//! (Tokio) executor provisioned by the `tokio` feature; `Message` is
//! `crate::app::messages::Message` exactly as specified.

use iced::widget::{column, container, row, text};

use crate::app::messages::{Message, ViewKind};
use crate::app::state::{AppState, TabKind};

/// Root view of the main window.
pub fn view(app: &AppState) -> iced::Element<'_, Message> {
    // Blocking modal: while an auth prompt is pending, the dialog replaces
    // the main content (prompt 2.2).
    if let Some(dialog) = super::auth_dialog::view(app) {
        return dialog;
    }
    // New-session dialog (Prompt 4.4) behaves like the auth modal.
    if let Some(dialog) = super::widgets::new_session_dialog::view(app) {
        return dialog;
    }

    let toolbar = super::widgets::toolbar::view(app);

    // Tab strip (#41): one button per tab; the active one is highlighted.
    let mut tab_strip = row![].spacing(4);
    for (index, tab) in app.tabs.iter().enumerate() {
        let marker = if index == app.active_tab { "▸ " } else { "" };
        tab_strip = tab_strip.push(
            iced::widget::button(text(format!("{marker}{}", tab.title)).size(13))
                .on_press(Message::Ui(crate::app::messages::UiMsg::SelectTab(index)))
                .padding([3, 8]),
        );
    }

    let content: iced::Element<'_, Message> = if app.ui_state.view == ViewKind::Settings {
        super::widgets::settings::view(app)
    } else if app.ui_state.view == ViewKind::Feedback {
        super::widgets::feedback::view(app)
    } else {
        match active_tab_kind(app) {
            TabKind::TerminalPlaceholder(id) => super::widgets::terminal_view::view(app, id),
            TabKind::FileBrowserPlaceholder => super::widgets::file_browser::view(app),
            TabKind::Tunnels => super::widgets::tunnels_panel::view(app),
            TabKind::Tools => super::widgets::tools_hub::view(app),
            TabKind::Macros => super::widgets::macro_panel::tab_view(app),
            TabKind::MultiExecView => super::widgets::multi_exec_view::view(app),
            #[cfg(feature = "vnc")]
            TabKind::VncViewer(id) => super::widgets::vnc_view::view(app, id),
            #[cfg(not(feature = "vnc"))]
            TabKind::VncViewer(_) => container(text("VNC support not compiled in")).into(),
            TabKind::Welcome => container(column![
            text("Welcome to Remote App").size(20),
            text("Select a session on the left, or create a new one.").size(13),
            text("Shortcuts: Ctrl+T new tab · Ctrl+W close tab · Ctrl+B sidebar · Ctrl+, settings")
                .size(11),
        ])
            .padding(12)
            .into(),
        }
    };

    // Adjusting split: sidebar visibility toggle (Ctrl+B) today; drag-to-
    // resize divider lands with the pane-grid work (TODO prompt 2.x).
    let body: iced::Element<'_, Message> =
        if app.ui_state.sidebar_visible && app.ui_state.view != ViewKind::Settings {
            row![
                container(super::widgets::session_list::view(app))
                    .width(240)
                    .height(iced::Fill),
                iced::widget::vertical_rule(2),
                column![tab_strip, content].spacing(4),
            ]
            .into()
        } else {
            column![tab_strip, content].into()
        };

    column![
        toolbar,
        super::widgets::multi_exec_toolbar::view(app),
        iced::widget::horizontal_rule(1),
        container(body).height(iced::Fill),
        iced::widget::horizontal_rule(1),
        super::widgets::status_bar::view(app),
    ]
    .width(iced::Fill)
    .height(iced::Fill)
    .into()
}

/// Kind of the focused tab (clamped against the strip length).
fn active_tab_kind(app: &AppState) -> TabKind {
    app.tabs
        .get(app.active_tab)
        .map(|tab| tab.kind.clone())
        .unwrap_or(TabKind::Welcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Tab;

    #[test]
    fn active_tab_kind_clamps_out_of_range() {
        let base = std::env::temp_dir().join(format!("mbxt-mw-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        let (mut app, _) = AppState::new(crate::utils::config::AppConfig::default(), paths);
        app.active_tab = 99; // stale index
        assert_eq!(active_tab_kind(&app), TabKind::Welcome);

        app.tabs.push(Tab {
            title: "t".into(),
            kind: TabKind::FileBrowserPlaceholder,
        });
        app.active_tab = 1;
        assert_eq!(active_tab_kind(&app), TabKind::FileBrowserPlaceholder);
    }

    #[test]
    fn root_view_renders_all_regions() {
        let base = std::env::temp_dir().join(format!("mbxt-mw2-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        let (app, _) = AppState::new(crate::utils::config::AppConfig::default(), paths);
        // Building the element exercises every widget constructor.
        let _ = view(&app);
    }
}
