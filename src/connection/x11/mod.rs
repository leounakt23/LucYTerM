//! X11 forwarding (Prompt 4.1): display detection, byte proxy, per-session
//! forwarder lifecycle.
//!
//! Layout (prompt-mandated):
//! - `display` — [`display::LocalEndpoint`] detection + xauth cookies.
//! - `proxy` — generic bidirectional pipe + unix listener.
//! - `forwarder` — [`X11Forwarder`] (enable / handle_connection /
//!   spawn_listener / disable) + counters.
//!
//! Lifecycle mirrors SFTP: [`X11Manager`] tracks one forwarder per SSH
//! session. Enabling happens alongside the shell connect (same auth
//! context); SSH disconnect/reconnect drops the forwarder so a stale proxy
//! never survives a reconnect. The transport (`SshConn`) requests `x11` on
//! the shell channel and drains server-opened channels into the forwarder.

pub mod display;
pub mod forwarder;
pub mod proxy;

pub use display::{detect_display, local_endpoint, read_cookie, LocalEndpoint, XAuthCookie};
pub use forwarder::{ForwarderStats, X11Forwarder};
pub use proxy::{dial_local, proxy_bidirectional, ProxyStats};

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use mbxt_core::SessionId;

/// X11 failures (mapped to [`crate::utils::error::AppError::X11`]).
#[derive(Debug, thiserror::Error)]
pub enum X11Error {
    /// `$DISPLAY` missing or unparsable.
    #[error("no X display found")]
    NoDisplay,
    /// Display parses but names no X server (prompt-mandated `Display` case).
    #[error("DISPLAY value is invalid: {0}")]
    Display(String),
    /// Local socket absent (X11/XWayland not reachable).
    #[error("local X server not accessible at {0:?}")]
    NoLocalServer(std::path::PathBuf),
    /// Cookie read failure (missing file itself is `Ok(None)`, not an error).
    #[error("X11 authentication error: {0}")]
    Auth(String),
    /// Transport/proxy I/O failure.
    #[error("X11 I/O error: {0}")]
    Io(String),
    /// Underlying SSH failure.
    #[error("SSH error: {0}")]
    Ssh(String),
}

impl X11Error {
    /// Build an I/O error from any displayable failure.
    pub fn io(error: impl std::fmt::Display) -> Self {
        Self::Io(error.to_string())
    }

    /// User-facing message (notification path; never leaks cookies/paths
    /// beyond the socket hint needed to fix the setup).
    pub fn user_message(&self) -> String {
        match self {
            Self::NoDisplay => {
                "No X display found. Set $DISPLAY or start XWayland first.".to_string()
            },
            Self::Display(detail) => format!("Invalid display setting: {detail}"),
            Self::NoLocalServer(_) => {
                "Local X server is not accessible — remote GUI apps cannot be shown.".to_string()
            },
            Self::Auth(_) => "Could not read the X11 authority cookie.".to_string(),
            Self::Io(_) => "Lost the X11 connection; the remote app may have exited.".to_string(),
            Self::Ssh(_) => {
                "Connection to the remote host failed. Check host, port, and credentials."
                    .to_string()
            },
        }
    }
}

/// One forwarder per SSH session, tied to the SSH lifecycle.
#[derive(Debug, Default)]
pub struct X11Manager {
    forwarders: Mutex<HashMap<SessionId, Arc<X11Forwarder>>>,
}

impl X11Manager {
    /// Process-wide singleton (mirrors `SessionManager`/`SftpManager`).
    pub fn shared() -> &'static Self {
        static INSTANCE: OnceLock<X11Manager> = OnceLock::new();
        INSTANCE.get_or_init(X11Manager::default)
    }

    /// Enable forwarding for `session` on `display` (replaces any handle —
    /// the reconnect path).
    pub fn enable(&self, session: SessionId, display: &str) -> Result<(), X11Error> {
        let forwarder = X11Forwarder::enable(display)?;
        if let Ok(mut forwarders) = self.forwarders.lock() {
            if let Some(previous) = forwarders.insert(session, forwarder) {
                previous.disable();
            }
        }
        Ok(())
    }

    /// Live forwarder for `session`, if forwarding is up.
    pub fn get(&self, session: SessionId) -> Option<Arc<X11Forwarder>> {
        self.forwarders
            .lock()
            .expect("x11 registry poisoned")
            .get(&session)
            .cloned()
    }

    /// `true` while forwarding is up for `session` (status indicator).
    pub fn is_active(&self, session: SessionId) -> bool {
        self.forwarders
            .lock()
            .expect("x11 registry poisoned")
            .contains_key(&session)
    }

    /// Drop the forwarder (SSH disconnect/reconnect path).
    pub fn note_ssh_disconnected(&self, session: SessionId) {
        if let Ok(mut forwarders) = self.forwarders.lock() {
            if let Some(previous) = forwarders.remove(&session) {
                previous.disable();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_tracks_lifecycle_without_x_server() {
        let manager = X11Manager::shared();
        assert!(!manager.is_active(888_001));
        // :99 never exists → enable fails, nothing stored.
        assert!(manager.enable(888_001, ":99").is_err());
        assert!(!manager.is_active(888_001));
        manager.note_ssh_disconnected(888_001);
    }

    #[test]
    fn x11_errors_map_to_actionable_messages() {
        assert!(X11Error::NoDisplay.user_message().contains("DISPLAY"));
        assert!(X11Error::NoLocalServer("/tmp/.X11-unix/X0".into())
            .user_message()
            .contains("not accessible"));
        assert!(X11Error::Display("bogus".into())
            .user_message()
            .contains("bogus"));
    }
}
