//! Transfer list widget (Prompt 3.2): active transfers with progress bars
//! and pause/resume/cancel controls.
//!
//! Pure view over [`TransferManager`] snapshots — no state, no I/O. Progress
//! bars render `fraction()` (0.0–1.0, unknown totals show an indeterminate
//! label instead); buttons emit the `SftpMsg::{Pause,Resume,Cancel}Requested`
//! variants handled in `app::update`.

use iced::widget::{button, column, container, progress_bar, row, text};

use crate::app::messages::{Message, SftpMsg};
use crate::app::state::AppState;
use crate::connection::sftp::{Transfer, TransferFilter, TransferManager, TransferStatus};

/// Transfers pane: every known transfer (active first) with controls.
pub fn view(app: &AppState) -> iced::Element<'_, Message> {
    let _ = app;
    view_transfers(TransferManager::shared().list_transfers(TransferFilter::All))
}

/// Session-scoped pane (SFTP panel side of the file browser).
pub fn view_for_session(
    app: &AppState,
    session: mbxt_core::SessionId,
) -> iced::Element<'_, Message> {
    let _ = app;
    view_transfers(TransferManager::shared().list_transfers(TransferFilter::BySession(session)))
}

/// Render owned snapshots (all row content is owned, so no borrow escapes).
fn view_transfers(transfers: Vec<Transfer>) -> iced::Element<'static, Message> {
    if transfers.is_empty() {
        return container(text("no transfers yet").size(12))
            .width(iced::Fill)
            .padding(8)
            .into();
    }

    let mut list = column![].spacing(6);
    for transfer in &transfers {
        list = list.push(transfer_row(transfer));
    }
    container(list).width(iced::Fill).padding(8).into()
}

fn transfer_row(transfer: &Transfer) -> iced::Element<'static, Message> {
    let header = row![
        text(transfer.summary()).size(12).width(iced::Fill),
        controls_for(transfer),
    ]
    .spacing(8);

    let progress: iced::Element<'static, Message> = match transfer.fraction() {
        Some(fraction) => progress_bar(0.0..=1.0, fraction.clamp(0.0, 1.0) as f32)
            .height(8)
            .into(),
        None => text(format!("{} bytes so far", transfer.done))
            .size(11)
            .into(),
    };

    column![header, progress].spacing(4).into()
}

fn controls_for(transfer: &Transfer) -> iced::Element<'static, Message> {
    let id = transfer.id.0;
    match transfer.status {
        TransferStatus::Pending | TransferStatus::InProgress => row![
            button(text("Pause").size(11))
                .on_press(Message::Sftp(SftpMsg::PauseRequested(id)))
                .padding([2, 8]),
            button(text("Cancel").size(11))
                .on_press(Message::Sftp(SftpMsg::CancelRequested(id)))
                .padding([2, 8]),
        ]
        .spacing(4)
        .into(),
        TransferStatus::Paused => row![
            button(text("Resume").size(11))
                .on_press(Message::Sftp(SftpMsg::ResumeRequested(id)))
                .padding([2, 8]),
            button(text("Cancel").size(11))
                .on_press(Message::Sftp(SftpMsg::CancelRequested(id)))
                .padding([2, 8]),
        ]
        .spacing(4)
        .into(),
        TransferStatus::Completed | TransferStatus::Failed(_) | TransferStatus::Cancelled => {
            text(transfer.status.label()).size(11).into()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::sftp::Direction;
    use std::path::Path;

    fn test_app() -> AppState {
        let base = std::env::temp_dir().join(format!("mbxt-tv-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        AppState::new(crate::utils::config::AppConfig::default(), paths).0
    }

    #[test]
    fn empty_and_populated_views_build() {
        let app = test_app();
        let _ = view(&app);
        let _ = view_for_session(&app, 1);

        let manager = TransferManager::shared();
        let id = manager.submit(
            4242,
            Direction::Download,
            Path::new("/tmp/x.bin"),
            "/remote/x.bin",
            Some(100),
            None,
        );
        manager.on_progress(id, 25, Some(100));
        let _ = view(&app);
        let _ = view_for_session(&app, 4242);
        manager.cancel(id).expect("cleanup");
        manager.clear_finished();
    }

    #[test]
    fn controls_emit_pause_resume_cancel_messages() {
        let pending = Transfer {
            id: crate::connection::sftp::TransferId(1),
            session: 1,
            direction: Direction::Upload,
            local_path: Path::new("/tmp/a").to_path_buf(),
            remote_path: "/r/a".into(),
            status: TransferStatus::InProgress,
            done: 0,
            total: None,
            attempts: 1,
            throttle_bps: None,
            started: None,
            updated: std::time::Instant::now(),
            cancel: crate::connection::sftp::CancelToken::new(),
        };
        // Exercised through the row constructor (button messages compile).
        let _ = transfer_row(&pending);
        let paused = Transfer {
            status: TransferStatus::Paused,
            ..pending.clone()
        };
        let _ = transfer_row(&paused);
        let done = Transfer {
            status: TransferStatus::Completed,
            ..pending
        };
        let _ = transfer_row(&done);
    }
}
