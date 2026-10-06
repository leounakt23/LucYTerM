//! VNC support (Prompt 4.3): RFB client, input mapping, per-session manager.
//!
//! Layout (prompt-mandated):
//! - `framebuffer` — pixel formats + Raw/CopyRect/RRE/Hextile decoders.
//! - `input` — keysyms, pointer events, scaling, clipboard codecs.
//! - `session` — handshake/auth/update loop over TCP or SSH-tunneled streams.
//!
//! Integration strategy: embedded pure-Rust RFB (tight integration, shared
//! update loop); the `vnc` cargo feature additionally keeps the spawns-an-
//! external-viewer transport (`SpawnConn`) as a fallback for servers whose
//! encodings fall outside the negotiated set. Lifecycle mirrors SFTP/X11:
//! one live session per id, dropped on SSH disconnect/reconnect.

pub mod framebuffer;
pub mod input;
pub mod session;

pub use framebuffer::{FrameSnapshot, Framebuffer, PixelFormat};
pub use input::{
    decode_cut_text, encode_cut_text, scale_point, KeyPress, PointerEvent, ScalingMode,
};
pub use session::{SshTunnel, VncConfig, VncSession};

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use mbxt_core::SessionId;

/// VNC failures (mapped to [`crate::utils::error::AppError`] via `Other`
/// with the user message — VNC has no dedicated top-level variant).
#[derive(Debug, thiserror::Error)]
pub enum VncError {
    /// Local or transport I/O failure.
    #[error("VNC I/O error: {0}")]
    Io(String),
    /// Malformed server data / unsupported configuration.
    #[error("VNC protocol error: {0}")]
    Protocol(String),
    /// Authentication rejected (no secrets in the message).
    #[error("VNC authentication failed")]
    Auth(String),
    /// Security handshake failure (no common type, refused, …).
    #[error("VNC security negotiation failed: {0}")]
    Security(String),
    /// Server sent an encoding outside the negotiated set.
    #[error("unsupported VNC encoding: {0}")]
    UnsupportedEncoding(i32),
    /// Underlying SSH failure (tunnel dial).
    #[error("SSH error: {0}")]
    Ssh(String),
    /// No live session (reconnect first).
    #[error("VNC is not connected")]
    NotConnected,
}

impl VncError {
    /// Build a protocol error from any displayable failure.
    pub fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol(message.into())
    }

    /// Build an I/O error from any displayable failure.
    pub fn io(error: impl std::fmt::Display) -> Self {
        Self::Io(error.to_string())
    }

    /// User-facing message (notification path; auth messages never name the
    /// credential that failed).
    pub fn user_message(&self) -> String {
        match self {
            Self::Io(_) => "Lost the remote desktop connection.".to_string(),
            Self::Protocol(detail) => format!("Remote desktop error: {detail}"),
            Self::Auth(_) => "The VNC server rejected the password.".to_string(),
            Self::Security(detail) => format!("VNC security setup failed: {detail}"),
            Self::UnsupportedEncoding(id) => {
                format!("The server sent an unsupported encoding ({id}).")
            },
            Self::Ssh(_) => {
                "Connection to the remote host failed. Check host, port, and credentials."
                    .to_string()
            },
            Self::NotConnected => "Remote desktop is not connected. Reconnect first.".to_string(),
        }
    }
}

impl From<std::io::Error> for VncError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// One live VNC session per id, tied to the surrounding session lifecycle.
#[derive(Debug, Default)]
pub struct VncManager {
    sessions: Mutex<HashMap<SessionId, Arc<VncSession>>>,
}

impl VncManager {
    /// Process-wide singleton (mirrors `SessionManager`/`SftpManager`).
    pub fn shared() -> &'static Self {
        static INSTANCE: OnceLock<VncManager> = OnceLock::new();
        INSTANCE.get_or_init(VncManager::default)
    }

    /// Live handle, if connected.
    pub fn get(&self, session: SessionId) -> Option<Arc<VncSession>> {
        self.sessions
            .lock()
            .expect("vnc registry poisoned")
            .get(&session)
            .cloned()
    }

    /// `true` while a live VNC session exists (viewer badge).
    pub fn is_connected(&self, session: SessionId) -> bool {
        self.sessions
            .lock()
            .expect("vnc registry poisoned")
            .contains_key(&session)
    }

    /// Store a connected session (replaces any previous handle).
    pub fn insert(&self, session: SessionId, handle: Arc<VncSession>) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert(session, handle);
        }
    }

    /// Latest published frame for the viewer widget (sync poll).
    pub fn frame_snapshot(&self, session: SessionId) -> Option<Arc<framebuffer::FrameSnapshot>> {
        self.get(session)?.latest_frame()
    }

    /// Remote desktop size, if a frame has landed yet.
    pub fn desktop_size(&self, session: SessionId) -> Option<(u16, u16)> {
        self.get(session).map(|handle| handle.desktop_size())
    }

    /// Last remote clipboard text, if any.
    pub fn last_cut_text(&self, session: SessionId) -> Option<String> {
        self.get(session)?.last_cut_text()
    }

    /// `true` while any viewer is connected (frame-tick subscription gate).
    pub fn has_any(&self) -> bool {
        self.sessions
            .lock()
            .map(|sessions| !sessions.is_empty())
            .unwrap_or(false)
    }

    /// Drop the session (disconnect/reconnect path — never reuse a stale
    /// desktop across reconnects).
    pub fn note_disconnected(&self, session: SessionId) {
        if let Ok(mut sessions) = self.sessions.lock() {
            if let Some(previous) = sessions.remove(&session) {
                let handle = previous;
                tokio::spawn(async move { handle.disconnect().await });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_tracks_lifecycle_without_network() {
        let manager = VncManager::shared();
        assert!(!manager.is_connected(777_001));
        manager.note_disconnected(777_001);
        assert!(!manager.is_connected(777_001));
    }

    #[test]
    fn vnc_errors_map_to_actionable_messages() {
        assert!(VncError::NotConnected.user_message().contains("Reconnect"));
        // Auth messages never echo credentials.
        assert!(!VncError::Auth("hunter2".into())
            .user_message()
            .contains("hunter2"));
        assert!(VncError::UnsupportedEncoding(7)
            .user_message()
            .contains('7'));
    }
}
