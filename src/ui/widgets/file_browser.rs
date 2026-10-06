//! `FileBrowser` widget (Prompt 3.3): remote directory pane with breadcrumb
//! navigation, sorting, filtering, bulk selection, context actions, inline
//! forms, preview, and lazy pagination.
//!
//! Pure view over [`FileBrowserState`] snapshots — no state, no I/O. Without
//! the `ssh` feature this module renders the legacy placeholder (same build
//! matrix as the SFTP stack).

use iced::widget::{column, container, text};

use crate::app::messages::Message;
use crate::app::state::AppState;

#[cfg(feature = "ssh")]
use iced::widget::{button, row, scrollable, text_input};

/// File browser pane for the active session (or the placeholder).
pub fn view(app: &AppState) -> iced::Element<'_, Message> {
    #[cfg(feature = "ssh")]
    {
        let session = app.active_session_id;
        let mut body = column![
            text("File browser").size(16),
            browser_body(app, session),
            text("Transfers").size(14),
            super::transfer_view::view(app),
        ]
        .spacing(6);

        let transfer_line = match app.transfers.iter().next() {
            Some((session, t)) => format!(
                "#{session}: {} → {} ({} / {} bytes)",
                t.local_path, t.remote_path, t.done, t.total
            ),
            None => "no active transfers".to_string(),
        };
        body = body.push(text(transfer_line).size(12));

        container(body)
            .width(iced::Fill)
            .height(iced::Fill)
            .padding(12)
            .into()
    }
    #[cfg(not(feature = "ssh"))]
    {
        let transfer_line = match app.transfers.iter().next() {
            Some((session, t)) => format!(
                "#{session}: {} → {} ({} / {} bytes)",
                t.local_path, t.remote_path, t.done, t.total
            ),
            None => "no active transfers".to_string(),
        };

        let body = column![
            text("File browser").size(16),
            text("sftp panel placeholder — dual-pane browser lands in prompt 1.5").size(12),
            text(transfer_line).size(12),
        ]
        .spacing(6);

        container(body)
            .width(iced::Fill)
            .height(iced::Fill)
            .padding(12)
            .into()
    }
}

#[cfg(feature = "ssh")]
fn browser_body(
    app: &AppState,
    session: Option<mbxt_core::SessionId>,
) -> iced::Element<'_, Message> {
    use crate::app::messages::SftpMsg;
    use crate::connection::sftp::breadcrumbs;

    let Some(session) = session else {
        return text("select a session to browse").size(12).into();
    };
    let Some(browser) = app.browsers.get(&session) else {
        return column![
            text("file browser opens when the session connects").size(12),
            button(text("Open now").size(12))
                .on_press(Message::Sftp(SftpMsg::BrowseRequested(
                    session,
                    ".".to_string()
                )))
                .padding([2, 8]),
        ]
        .spacing(6)
        .into();
    };

    // Breadcrumb bar + refresh.
    let mut crumbs = row![].spacing(2);
    for (label, path) in breadcrumbs(&browser.cwd) {
        crumbs = crumbs.push(
            button(text(label).size(12))
                .on_press(Message::Sftp(SftpMsg::BrowseRequested(session, path)))
                .padding([2, 6]),
        );
    }
    let nav = row![
        crumbs,
        button(text("Refresh").size(12))
            .on_press(Message::Sftp(SftpMsg::BrowserRefresh(session)))
            .padding([2, 8]),
    ]
    .spacing(8);

    // Filter + sort controls.
    let sort_label = format!(
        "Sort: {} {}",
        match browser.sort {
            crate::connection::sftp::BrowserSort::Name => "name",
            crate::connection::sftp::BrowserSort::Size => "size",
            crate::connection::sftp::BrowserSort::Modified => "date",
        },
        match browser.sort_dir {
            crate::connection::sftp::SortDir::Asc => "↑",
            crate::connection::sftp::SortDir::Desc => "↓",
        }
    );
    let controls = row![
        text_input("Filter (*.rs, report*)…", &browser.filter)
            .on_input(move |value| Message::Sftp(SftpMsg::BrowserFilterChanged(session, value)))
            .padding(4)
            .width(iced::Fill),
        button(text(sort_label).size(12))
            .on_press(Message::Sftp(cycle_sort(session, browser)))
            .padding([2, 8]),
    ]
    .spacing(8);

    let mut body = column![nav, controls].spacing(6);

    if browser.loading {
        body = body.push(text("loading…").size(12));
    }
    if let Some(error) = browser.error.as_deref() {
        body = body.push(text(format!("error: {error}")).size(12));
    }

    // Bulk bar.
    if !browser.selected.is_empty() {
        body = body.push(
            row![
                text(format!("{} selected", browser.selected.len())).size(12),
                button(text("Download").size(11))
                    .on_press(Message::Sftp(SftpMsg::BrowserDownloadSelected(session)))
                    .padding([2, 8]),
                button(text("Delete").size(11))
                    .on_press(Message::Sftp(SftpMsg::BrowserDeleteSelected(session)))
                    .padding([2, 8]),
                button(text("Permissions…").size(11))
                    .on_press(Message::Sftp(SftpMsg::BrowserChmodRequest(session)))
                    .padding([2, 8]),
                button(text("Clear").size(11))
                    .on_press(Message::Sftp(SftpMsg::BrowserClearSelection(session)))
                    .padding([2, 8]),
            ]
            .spacing(6),
        );
    }

    // Entry list (lazy pages).
    let visible = browser.visible();
    let total = visible.len();
    let page = browser.page(&visible);
    let mut list = column![].spacing(2);
    for entry in page {
        list = list.push(entry_row(session, browser, entry));
    }
    body = body.push(scrollable(list).height(iced::Fill));
    body = body.push(text(format!("showing {} of {total}", page.len())).size(11));
    if page.len() < total {
        body = body.push(
            button(text(format!("Show more ({} remaining)", total - page.len())).size(12))
                .on_press(Message::Sftp(SftpMsg::BrowserShowMore(session)))
                .padding([2, 8]),
        );
    }

    // Context menu / pending form / preview.
    if let Some(menu) = context_menu(session, browser) {
        body = body.push(menu);
    }
    if let Some(form) = pending_form(session, browser) {
        body = body.push(form);
    }
    if let Some((path, preview)) = browser.preview.as_ref() {
        body = body.push(
            column![
                row![
                    text(format!("Preview: {path}")).size(12).width(iced::Fill),
                    button(text("Close").size(11))
                        .on_press(Message::Sftp(SftpMsg::BrowserPreviewClosed(session)))
                        .padding([2, 8]),
                ]
                .spacing(8),
                scrollable(text(preview.clone()).size(11).font(iced::Font::MONOSPACE)).height(120),
            ]
            .spacing(4),
        );
    }

    // Selection helpers.
    body = body.push(
        row![
            button(text("Select all").size(11))
                .on_press(Message::Sftp(SftpMsg::BrowserSelectAll(session)))
                .padding([2, 8]),
            button(text("Background menu").size(11))
                .on_press(Message::Sftp(SftpMsg::BrowserContextOpened(session, None)))
                .padding([2, 8]),
        ]
        .spacing(6),
    );

    body.into()
}

#[cfg(feature = "ssh")]
fn cycle_sort(
    session: mbxt_core::SessionId,
    browser: &crate::connection::sftp::FileBrowserState,
) -> crate::app::messages::SftpMsg {
    use crate::app::messages::SftpMsg;
    use crate::connection::sftp::{BrowserSort, SortDir};
    let (sort, dir) = match (browser.sort, browser.sort_dir) {
        (BrowserSort::Name, SortDir::Asc) => (BrowserSort::Name, SortDir::Desc),
        (BrowserSort::Name, SortDir::Desc) => (BrowserSort::Size, SortDir::Asc),
        (BrowserSort::Size, SortDir::Asc) => (BrowserSort::Size, SortDir::Desc),
        (BrowserSort::Size, SortDir::Desc) => (BrowserSort::Modified, SortDir::Asc),
        (BrowserSort::Modified, SortDir::Asc) => (BrowserSort::Modified, SortDir::Desc),
        (BrowserSort::Modified, SortDir::Desc) => (BrowserSort::Name, SortDir::Asc),
    };
    SftpMsg::BrowserSortChanged(session, sort, dir)
}

#[cfg(feature = "ssh")]
fn entry_row(
    session: mbxt_core::SessionId,
    browser: &crate::connection::sftp::FileBrowserState,
    entry: &crate::connection::sftp::FileInfo,
) -> iced::Element<'static, Message> {
    use crate::app::messages::SftpMsg;

    let selected = browser.selected.contains(&entry.name);
    let marker = if selected { "[x]" } else { "[ ]" };
    let kind = if entry.is_dir() { "📁" } else { "📄" };
    let name = entry.name.clone();
    let open = entry.name.clone();
    let menu_for = entry.name.clone();

    let activate = if entry.is_dir() {
        let path = entry.path.clone();
        Message::Sftp(SftpMsg::BrowseRequested(session, path))
    } else {
        Message::Sftp(SftpMsg::BrowserPreview(session, open))
    };

    row![
        button(text(marker).size(12))
            .on_press(Message::Sftp(SftpMsg::BrowserToggled(session, name)))
            .padding([2, 6]),
        button(text(format!("{kind} {}", entry.name)).size(12))
            .on_press(activate)
            .padding([2, 6]),
        text(format!("{} B", entry.size)).size(11).width(90),
        button(text("⋮").size(12))
            .on_press(Message::Sftp(SftpMsg::BrowserContextOpened(
                session,
                Some(menu_for)
            )))
            .padding([2, 6]),
    ]
    .spacing(4)
    .into()
}

#[cfg(feature = "ssh")]
fn context_menu(
    session: mbxt_core::SessionId,
    browser: &crate::connection::sftp::FileBrowserState,
) -> Option<iced::Element<'static, Message>> {
    use crate::app::messages::SftpMsg;
    use crate::ui::widgets::context_menu::ContextAction;

    let target = browser.context.clone()?;
    let actions = match &target {
        crate::connection::sftp::ContextTarget::Entry(name) => {
            let is_dir = browser
                .entries
                .iter()
                .find(|e| &e.name == name)
                .is_some_and(|e| e.is_dir());
            ContextAction::for_entry(is_dir)
        },
        crate::connection::sftp::ContextTarget::Background => ContextAction::for_background(),
    };
    let entry = match &target {
        crate::connection::sftp::ContextTarget::Entry(name) => Some(name.clone()),
        crate::connection::sftp::ContextTarget::Background => None,
    };
    let mut items = row![].spacing(4);
    for action in actions {
        items = items.push(
            button(text(action.label()).size(11))
                .on_press(action.to_message(session, entry.as_deref()))
                .padding([2, 8]),
        );
    }
    Some(
        column![
            items,
            button(text("Close menu").size(11))
                .on_press(Message::Sftp(SftpMsg::BrowserContextClosed(session)))
                .padding([2, 8]),
        ]
        .spacing(4)
        .into(),
    )
}

#[cfg(feature = "ssh")]
fn pending_form(
    session: mbxt_core::SessionId,
    browser: &crate::connection::sftp::FileBrowserState,
) -> Option<iced::Element<'_, Message>> {
    use crate::app::messages::SftpMsg;
    use crate::connection::sftp::PendingOp;

    let pending = browser.pending.clone()?;
    let (title, placeholder) = match &pending {
        PendingOp::Mkdir => ("New folder", "folder name"),
        PendingOp::Rename { .. } => ("Rename to", "new name"),
        PendingOp::Chmod => ("Permissions (octal)", "755"),
        PendingOp::Upload => ("Upload local file", "/path/to/file"),
    };
    Some(
        column![
            text(title).size(12),
            text_input(placeholder, &browser.input)
                .on_input(move |value| Message::Sftp(SftpMsg::BrowserInputChanged(session, value)))
                .on_submit(Message::Sftp(SftpMsg::BrowserInputConfirmed(session)))
                .padding(4),
            row![
                button(text("Confirm").size(11))
                    .on_press(Message::Sftp(SftpMsg::BrowserInputConfirmed(session)))
                    .padding([2, 8]),
                button(text("Cancel").size(11))
                    .on_press(Message::Sftp(SftpMsg::BrowserInputCancelled(session)))
                    .padding([2, 8]),
            ]
            .spacing(6),
        ]
        .spacing(4)
        .into(),
    )
}

#[cfg(test)]
#[cfg(feature = "ssh")]
mod tests {
    use super::*;
    use crate::connection::sftp::{FileBrowserState, FileInfo};
    use russh_sftp::protocol::FileAttributes;

    fn test_app() -> AppState {
        let base = std::env::temp_dir().join(format!("mbxt-fb-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        AppState::new(crate::utils::config::AppConfig::default(), paths).0
    }

    fn entry(name: &str) -> FileInfo {
        FileInfo::from_metadata(
            &format!("/home/ops/{name}"),
            &FileAttributes {
                size: Some(128),
                uid: None,
                user: None,
                gid: None,
                group: None,
                permissions: Some(0o644),
                atime: None,
                mtime: None,
            },
        )
    }

    #[test]
    fn full_browser_view_builds() {
        let mut app = test_app();
        // No session selected: hint text.
        let _ = view(&app);
        // Browser with entries, selection, menu, form, and preview.
        app.active_session_id = Some(9);
        let mut browser = FileBrowserState::new(9, "/home/ops");
        browser.entries = vec![entry("a.txt"), entry("docs")];
        browser.selected.insert("a.txt".to_string());
        browser.context = Some(crate::connection::sftp::ContextTarget::Entry(
            "a.txt".to_string(),
        ));
        browser.pending = Some(crate::connection::sftp::PendingOp::Mkdir);
        browser.input = "new".to_string();
        browser.preview = Some(("/home/ops/a.txt".to_string(), "hello".to_string()));
        app.browsers.insert(9, browser);
        let _ = view(&app);
        let _ = browser_body(&app, Some(9));
    }

    #[test]
    fn sort_cycles_through_all_columns() {
        use crate::app::messages::SftpMsg;
        let mut browser = FileBrowserState::new(1, "/");
        let mut seen = Vec::new();
        for _ in 0..6 {
            match cycle_sort(1, &browser) {
                SftpMsg::BrowserSortChanged(_, sort, dir) => {
                    assert!(!seen.contains(&(sort, dir)), "no repeat before full cycle");
                    seen.push((sort, dir));
                    browser.sort = sort;
                    browser.sort_dir = dir;
                },
                other => panic!("expected sort message, got {other:?}"),
            }
        }
        assert_eq!(seen.len(), 6, "all column/dir combos reachable");
    }
}
