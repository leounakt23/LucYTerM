//! Root application error type (prompt 1.3): one enum, `thiserror`-derived
//! Display, `From` conversions so `?` works at every boundary, and
//! user-friendly message mapping for the notification path (architecture §7).

use thiserror::Error;

/// Root error enum covering every subsystem.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SSH error: {0}")]
    Ssh(String),

    #[error("X11 forwarding error: {0}")]
    X11(String),

    #[error("SFTP error: {0}")]
    Sftp(String),

    #[error("cryptographic error: {0}")]
    Crypto(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("interface error: {0}")]
    Ui(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("operation timed out after {secs}s")]
    Timeout { secs: u64 },

    #[error("{0}")]
    Other(String),
}

/// Result alias used across the application.
pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    // -- constructors (keeps call sites terse and grep-able) --------------

    pub fn ssh(message: impl Into<String>) -> Self {
        Self::Ssh(message.into())
    }
    pub fn x11(message: impl Into<String>) -> Self {
        Self::X11(message.into())
    }
    pub fn sftp(message: impl Into<String>) -> Self {
        Self::Sftp(message.into())
    }
    pub fn crypto(message: impl Into<String>) -> Self {
        Self::Crypto(message.into())
    }
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
    }
    pub fn ui(message: impl Into<String>) -> Self {
        Self::Ui(message.into())
    }
    pub fn network(message: impl Into<String>) -> Self {
        Self::Network(message.into())
    }
    pub fn timeout(secs: u64) -> Self {
        Self::Timeout { secs }
    }

    /// Short category tag for tracing span/log attributes.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Ssh(_) => "ssh",
            Self::X11(_) => "x11",
            Self::Sftp(_) => "sftp",
            Self::Crypto(_) => "crypto",
            Self::Config(_) => "config",
            Self::Ui(_) => "ui",
            Self::Network(_) => "network",
            Self::Timeout { .. } => "timeout",
            Self::Other(_) => "other",
        }
    }

    /// User-facing message: actionable, never leaks raw internals
    /// (the full chain goes to the log file, not the screen).
    pub fn user_message(&self) -> String {
        match self {
            Self::Io(err) => format!("File operation failed: {err}"),
            Self::Ssh(_) => {
                "Connection to the remote host failed. Check host, port, and credentials."
                    .to_string()
            },
            Self::X11(_) => {
                "X11 forwarding failed. Check the local X server and reconnect.".to_string()
            },
            Self::Sftp(_) => {
                "Remote file operation failed. Check the path and permissions, then retry."
                    .to_string()
            },
            Self::Crypto(_) => {
                "Decryption failed — the master password may be wrong or the data corrupted."
                    .to_string()
            },
            Self::Config(err) => format!("Configuration problem: {err}"),
            Self::Ui(err) => format!("Interface problem: {err}"),
            Self::Network(_) => {
                "Network unreachable. Check your connection or proxy settings.".to_string()
            },
            Self::Timeout { secs } => format!("The operation did not complete within {secs}s."),
            Self::Other(message) => message.clone(),
        }
    }
}

// -- bridges between the notification path (String) and typed errors ------
// UI-facing handlers degrade to friendly `String`s by design; these impls
// keep both worlds composable with `?`.

impl From<String> for AppError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        Self::Other(message.to_string())
    }
}

impl From<AppError> for String {
    fn from(err: AppError) -> Self {
        err.user_message()
    }
}

impl From<mbxt_core::MbxtError> for AppError {
    fn from(err: mbxt_core::MbxtError) -> Self {
        match err {
            mbxt_core::MbxtError::SessionNotFound(id) => {
                Self::Other(format!("session #{id} not found"))
            },
            mbxt_core::MbxtError::Unsupported(what) => Self::Ui(format!("{what} is not supported")),
            mbxt_core::MbxtError::Other(message) => Self::Other(message),
        }
    }
}

impl From<crate::utils::crypto::CryptoError> for AppError {
    fn from(err: crate::utils::crypto::CryptoError) -> Self {
        Self::Crypto(err.to_string())
    }
}

#[cfg(feature = "ssh")]
impl From<crate::connection::x11::X11Error> for AppError {
    fn from(err: crate::connection::x11::X11Error) -> Self {
        match err {
            crate::connection::x11::X11Error::Io(detail) => Self::Io(std::io::Error::other(detail)),
            crate::connection::x11::X11Error::Ssh(detail) => Self::Ssh(detail),
            other => Self::X11(other.to_string()),
        }
    }
}

#[cfg(feature = "ssh")]
impl From<crate::connection::sftp::SftpError> for AppError {
    fn from(err: crate::connection::sftp::SftpError) -> Self {
        match err {
            crate::connection::sftp::SftpError::Io(detail) => {
                Self::Io(std::io::Error::other(detail))
            },
            crate::connection::sftp::SftpError::Ssh(detail) => Self::Ssh(detail),
            other => Self::Sftp(other.to_string()),
        }
    }
}

impl From<crate::utils::secure_storage::SecureStorageError> for AppError {
    fn from(err: crate::utils::secure_storage::SecureStorageError) -> Self {
        match err {
            crate::utils::secure_storage::SecureStorageError::Io(io) => Self::Io(io),
            crate::utils::secure_storage::SecureStorageError::Crypto(c) => {
                Self::Crypto(c.to_string())
            },
            crate::utils::secure_storage::SecureStorageError::Serialize(s) => Self::Config(s),
            crate::utils::secure_storage::SecureStorageError::Keyring(k) => {
                Self::Other(k.to_string())
            },
            other => Self::Other(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_and_kind_are_consistent() {
        assert_eq!(AppError::ssh("refused").kind(), "ssh");
        assert_eq!(AppError::sftp("no such file").kind(), "sftp");
        assert_eq!(AppError::x11("no display").kind(), "x11");
        assert_eq!(AppError::crypto("bad tag").kind(), "crypto");
        assert_eq!(AppError::config("bad ron").kind(), "config");
        assert_eq!(AppError::ui("no widget").kind(), "ui");
        assert_eq!(AppError::network("unreachable").kind(), "network");
        assert_eq!(AppError::timeout(5).kind(), "timeout");
        assert_eq!(AppError::Other("x".into()).kind(), "other");
        assert_eq!(AppError::Io(std::io::Error::other("disk")).kind(), "io");
    }

    #[test]
    fn io_errors_convert_with_question_mark() {
        fn failing() -> AppResult<()> {
            let read: std::io::Result<()> = Err(std::io::Error::other("boom"));
            read?;
            Ok(())
        }
        let err = failing().unwrap_err();
        assert!(matches!(err, AppError::Io(_)));
        assert!(err.user_message().contains("boom"));
    }

    #[test]
    fn user_messages_are_actionable_and_free_of_chains() {
        let ssh = AppError::ssh("kex: no common algorithms");
        assert!(ssh.user_message().contains("Check host, port"));
        assert!(
            !ssh.user_message().contains("kex:"),
            "raw internals stay in logs"
        );

        let timeout = AppError::timeout(30);
        assert!(timeout.user_message().contains("30s"));
    }

    #[test]
    fn string_bridge_round_trip() {
        let err: AppError = "plain failure".into();
        let message: String = err.into();
        assert_eq!(message, "plain failure");
    }

    #[test]
    fn domain_errors_convert_into_app_error() {
        let crypto = crate::utils::crypto::CryptoError::Decrypt;
        let app: AppError = crypto.into();
        assert_eq!(app.kind(), "crypto");

        let locked = crate::utils::secure_storage::SecureStorageError::Locked;
        let app: AppError = locked.into();
        assert!(matches!(app, AppError::Other(_)));

        let domain = mbxt_core::MbxtError::SessionNotFound(9);
        let app: AppError = domain.into();
        assert!(app.user_message().contains("#9"));
    }
}
