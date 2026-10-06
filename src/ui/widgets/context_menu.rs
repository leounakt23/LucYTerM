//! Right-click context menu (Prompt 3.3).
//!
//! The menu itself renders as an inline action row in the browser (iced
//! `mouse::Click` right-button events don't reach custom widgets on 0.13, so
//! a dedicated "⋮" button per row + a background menu button opens it — same
//! actions, accessible without pointer-button plumbing). [`ContextAction`]
//! maps each item to its label and target message; background vs entry menus
//! differ exactly as a native menu would.

use crate::app::messages::{Message, SftpMsg};

/// One menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    NewFolder,
    UploadHere,
    Refresh,
    SelectAll,
    Download,
    Preview,
    Rename,
    Delete,
    Properties,
    Chmod,
}

impl ContextAction {
    /// Button label.
    pub fn label(self) -> &'static str {
        match self {
            Self::NewFolder => "New folder",
            Self::UploadHere => "Upload local file…",
            Self::Refresh => "Refresh",
            Self::SelectAll => "Select all",
            Self::Download => "Download",
            Self::Preview => "Preview",
            Self::Rename => "Rename…",
            Self::Delete => "Delete",
            Self::Properties => "Properties",
            Self::Chmod => "Permissions…",
        }
    }

    /// Background menu (no entry under the cursor).
    pub fn for_background() -> Vec<Self> {
        vec![
            Self::NewFolder,
            Self::UploadHere,
            Self::Refresh,
            Self::SelectAll,
        ]
    }

    /// Entry menu (files offer preview/download; dirs skip preview).
    pub fn for_entry(is_dir: bool) -> Vec<Self> {
        let mut actions = vec![Self::Download, Self::Rename, Self::Delete];
        if !is_dir {
            actions.insert(1, Self::Preview);
        }
        actions.extend([Self::Properties, Self::Chmod]);
        actions
    }

    /// Target message (`entry` is `Some` for entry menus).
    #[cfg(feature = "ssh")]
    pub fn to_message(self, session: mbxt_core::SessionId, entry: Option<&str>) -> Message {
        match self {
            Self::NewFolder => Message::Sftp(SftpMsg::BrowserMkdirRequest(session)),
            Self::UploadHere => Message::Sftp(SftpMsg::BrowserUploadRequest(session)),
            Self::Refresh => Message::Sftp(SftpMsg::BrowserRefresh(session)),
            Self::SelectAll => Message::Sftp(SftpMsg::BrowserSelectAll(session)),
            Self::Download => Message::Sftp(SftpMsg::BrowserDownloadSelected(session)),
            Self::Preview => Message::Sftp(SftpMsg::BrowserPreview(
                session,
                entry.unwrap_or_default().to_string(),
            )),
            Self::Rename => Message::Sftp(SftpMsg::BrowserRenameRequest(
                session,
                entry.unwrap_or_default().to_string(),
            )),
            Self::Delete => Message::Sftp(SftpMsg::BrowserDeleteSelected(session)),
            Self::Properties => Message::Sftp(SftpMsg::BrowserProperties(
                session,
                entry.unwrap_or_default().to_string(),
            )),
            Self::Chmod => Message::Sftp(SftpMsg::BrowserChmodRequest(session)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_and_entry_menus_differ() {
        let background = ContextAction::for_background();
        assert!(background.contains(&ContextAction::NewFolder));
        assert!(!background.contains(&ContextAction::Delete));

        let file = ContextAction::for_entry(false);
        assert!(file.contains(&ContextAction::Preview));
        assert!(file.contains(&ContextAction::Download));

        let dir = ContextAction::for_entry(true);
        assert!(!dir.contains(&ContextAction::Preview));
        assert!(dir.contains(&ContextAction::Download));
    }

    #[test]
    fn every_action_has_a_label() {
        for action in [
            ContextAction::NewFolder,
            ContextAction::UploadHere,
            ContextAction::Refresh,
            ContextAction::SelectAll,
            ContextAction::Download,
            ContextAction::Preview,
            ContextAction::Rename,
            ContextAction::Delete,
            ContextAction::Properties,
            ContextAction::Chmod,
        ] {
            assert!(!action.label().is_empty());
        }
    }

    #[cfg(feature = "ssh")]
    #[test]
    fn actions_map_to_browser_messages() {
        let message = ContextAction::Refresh.to_message(3, None);
        assert!(matches!(message, Message::Sftp(SftpMsg::BrowserRefresh(3))));
        let message = ContextAction::Rename.to_message(3, Some("old.txt"));
        assert!(matches!(
            message,
            Message::Sftp(SftpMsg::BrowserRenameRequest(3, _))
        ));
    }
}
