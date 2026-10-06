//! SFTP subsystem (Prompt 3.1): session, transfers, file metadata.
//!
//! Layout (prompt-mandated):
//! - `file_info` — [`FileInfo`] + POSIX path helpers (pure, headless tests).
//! - `transfer` — resume arithmetic, chunked streaming, progress, cancel.
//! - `session` — live [`SftpSession`] over `russh-sftp`.
//!
//! Lifecycle: [`SftpManager`] ties one SFTP channel per SSH session. When an
//! SSH session connects, the app establishes SFTP alongside it (same auth);
//! on SSH disconnect/reconnect the SFTP handle is dropped so the next
//! operation reconnects — a stale channel is never reused across a reconnect.

pub mod browser;
pub mod file_info;
pub mod session;
pub mod transfer;
pub mod transfer_manager;

pub use browser::{
    apply_sort, breadcrumbs, matches_filter, BrowserSort, ContextTarget, DirCache,
    FileBrowserState, PendingOp, SortDir, CACHE_TTL_SECS, PAGE_SIZE,
};
pub use file_info::{join_remote, parent_of, sort_entries, FileInfo, FileType};
pub use session::SftpSession;
pub use transfer::{
    copy_stream_limited, next_transfer_id, throttle_delay, Backoff, CancelToken, Direction,
    Transfer, TransferId, TransferOptions, TransferPlan, TransferProgress, TransferStatus,
};
pub use transfer_manager::{FinishOutcome, TransferConfig, TransferFilter, TransferManager};

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use mbxt_core::{SessionId, SessionSpec};

use super::ConnectionAuth;

/// SFTP failures. Every variant maps to [`crate::utils::error::AppError::Sftp`]
/// with a user-actionable message (architecture §7: no raw chains on screen).
#[derive(Debug, thiserror::Error)]
pub enum SftpError {
    /// Remote status / protocol failure (mapped from SFTP status codes).
    #[error("sftp error: {0}")]
    Protocol(String),
    /// Local or transport I/O failure.
    #[error("sftp I/O error: {0}")]
    Io(String),
    /// Cooperative cancellation (user pressed cancel).
    #[error("transfer cancelled")]
    Cancelled,
    /// Permanent failure (permission denied, no such file, …) — never retried.
    #[error("sftp error: {0}")]
    Permanent(String),
    /// No live SFTP channel for the session (reconnect first).
    #[error("sftp is not connected")]
    NotConnected,
    /// Underlying SSH failure while dialing the subsystem.
    #[error("ssh error: {0}")]
    Ssh(String),
}

impl SftpError {
    /// Build a protocol error from any displayable remote failure.
    pub fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol(message.into())
    }

    /// Build an I/O error from any displayable local failure.
    pub fn io(error: impl std::fmt::Display) -> Self {
        Self::Io(error.to_string())
    }

    /// Map a `russh-sftp` client error (status codes included).
    pub fn from_sftp(error: impl std::fmt::Display) -> Self {
        Self::Protocol(error.to_string())
    }

    /// User-facing message (notification path).
    pub fn user_message(&self) -> String {
        match self {
            Self::Protocol(detail) => format!("Remote file operation failed: {detail}"),
            Self::Io(detail) => format!("File operation failed: {detail}"),
            Self::Cancelled => "Transfer cancelled.".to_string(),
            Self::Permanent(detail) => format!("Remote file operation failed: {detail}"),
            Self::NotConnected => {
                "File browser is not connected. Reconnect the session first.".to_string()
            },
            Self::Ssh(_) => {
                "Connection to the remote host failed. Check host, port, and credentials."
                    .to_string()
            },
        }
    }
}

impl From<std::io::Error> for SftpError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// One SFTP channel per SSH session, tied to the SSH lifecycle.
#[derive(Debug, Default)]
pub struct SftpManager {
    sessions: Mutex<HashMap<SessionId, SftpSession>>,
}

impl SftpManager {
    /// Process-wide singleton (mirrors `SessionManager`).
    pub fn shared() -> &'static Self {
        static INSTANCE: OnceLock<SftpManager> = OnceLock::new();
        INSTANCE.get_or_init(SftpManager::default)
    }

    /// Live handle for `session`, if its SFTP channel is up.
    pub fn get(&self, session: SessionId) -> Option<SftpSession> {
        self.sessions
            .lock()
            .expect("sftp registry poisoned")
            .get(&session)
            .cloned()
    }

    /// `true` while a live SFTP channel exists for `session`.
    pub fn is_connected(&self, session: SessionId) -> bool {
        self.sessions
            .lock()
            .expect("sftp registry poisoned")
            .contains_key(&session)
    }

    /// Drop the SFTP channel (SSH disconnect/reconnect path — a stale
    /// channel must never survive a reconnect).
    pub fn note_ssh_disconnected(&self, session: SessionId) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(&session);
        }
    }

    /// Establish the SFTP subsystem for `session` with the same auth the SSH
    /// shell used, and store the live channel. Replaces any previous handle
    /// (reconnect path).
    pub async fn connect(
        &'static self,
        session: SessionId,
        spec: SessionSpec,
        auth: ConnectionAuth,
    ) -> Result<(), SftpError> {
        let live = dial_sftp(spec, auth).await?;
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert(session, live);
        }
        Ok(())
    }

    /// Establish SFTP in the background (update-path: never blocks the UI).
    pub fn connect_in_background(
        &'static self,
        session: SessionId,
        spec: SessionSpec,
        auth: ConnectionAuth,
    ) {
        tokio::spawn(async move {
            if let Err(err) = Self::shared().connect(session, spec, auth).await {
                tracing::warn!(session, %err, "background sftp connect failed");
            }
        });
    }
}

#[derive(Debug)]
struct SftpClientHandler {
    host: String,
    port: u16,
}

#[async_trait::async_trait]
impl russh::client::Handler for SftpClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        match russh::keys::check_known_hosts(&self.host, self.port, server_public_key) {
            Ok(accepted) => Ok(accepted),
            Err(error) => {
                tracing::warn!(%error, host = %self.host, "known_hosts check failed");
                Ok(false)
            },
        }
    }
}

/// Dial SSH with `auth`, open the `sftp` subsystem, and handshake.
///
/// Separate control connection from the shell channel (one auth, two
/// channels over independent connections): if either drops, the other is
/// torn down and both reconnect together.
async fn dial_sftp(spec: SessionSpec, auth: ConnectionAuth) -> Result<SftpSession, SftpError> {
    use russh::client;
    use std::sync::Arc;

    let host = spec
        .host
        .clone()
        .ok_or_else(|| SftpError::protocol("SSH host is missing"))?;
    let user = spec
        .username
        .clone()
        .ok_or_else(|| SftpError::protocol("SSH username is missing"))?;
    let port = spec.port.unwrap_or(22);

    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        ..Default::default()
    });
    let mut handle = client::connect(
        config,
        (host.as_str(), port),
        SftpClientHandler {
            host: host.clone(),
            port,
        },
    )
    .await
    .map_err(|err| SftpError::Ssh(err.to_string()))?;

    let authenticated = match auth {
        ConnectionAuth::Password(password) => handle
            .authenticate_password(user.clone(), password.as_str())
            .await
            .map_err(|err| SftpError::Ssh(err.to_string()))?,
        ConnectionAuth::KeyFile { path, passphrase } => {
            let key = russh::keys::load_secret_key(path, passphrase.as_deref().map(String::as_str))
                .map_err(|err| SftpError::Ssh(err.to_string()))?;
            handle
                .authenticate_publickey(user.clone(), Arc::new(key))
                .await
                .map_err(|err| SftpError::Ssh(err.to_string()))?
        },
        ConnectionAuth::KeyboardInteractive(response) => {
            use client::KeyboardInteractiveAuthResponse as Response;
            let mut state = handle
                .authenticate_keyboard_interactive_start(user.clone(), None::<String>)
                .await
                .map_err(|err| SftpError::Ssh(err.to_string()))?;
            loop {
                match state {
                    Response::Success => break true,
                    Response::Failure => break false,
                    Response::InfoRequest { prompts, .. } => {
                        state = handle
                            .authenticate_keyboard_interactive_respond(
                                prompts.iter().map(|_| response.to_string()).collect(),
                            )
                            .await
                            .map_err(|err| SftpError::Ssh(err.to_string()))?;
                    },
                }
            }
        },
        ConnectionAuth::Agent => return Err(SftpError::Ssh("ssh-agent authentication".into())),
    };
    if !authenticated {
        return Err(SftpError::Ssh("server rejected authentication".into()));
    }

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|err| SftpError::Ssh(err.to_string()))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|err| SftpError::Ssh(err.to_string()))?;
    let stream = channel.into_stream();
    let session = SftpSession::new(stream).await?;
    tracing::info!(session = %spec.name, %host, port, "SFTP subsystem opened");
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_tracks_lifecycle_without_network() {
        // Lifecycle bookkeeping is headless: no dialing happens here.
        let manager = SftpManager::shared();
        assert!(!manager.is_connected(999_001));
        manager.note_ssh_disconnected(999_001);
        assert!(!manager.is_connected(999_001));
    }

    #[test]
    fn sftp_errors_map_to_actionable_messages() {
        assert!(SftpError::Cancelled.user_message().contains("cancelled"));
        assert!(SftpError::NotConnected.user_message().contains("Reconnect"));
        assert!(SftpError::protocol("no such file")
            .user_message()
            .contains("no such file"));
    }
}
