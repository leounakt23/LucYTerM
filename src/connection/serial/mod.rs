//! Serial support (Prompt 4.4): validated config, direct session, errors.
//!
//! Layout (prompt-mandated):
//! - `config` — [`SerialConfig`] validation + device picker list.
//! - `session` — [`SerialSession`] direct handle (embedding/tests).
//!
//! The actor transport (`mbxt-connections::serial::SerialConn`) is what the
//! UI connects through; both layers share [`mbxt_core::SerialParams`].

#[cfg(feature = "serial")]
pub mod config;
#[cfg(feature = "serial")]
pub mod session;

#[cfg(feature = "serial")]
pub use config::{SerialConfig, COMMON_BAUD_RATES};
#[cfg(feature = "serial")]
pub use session::SerialSession;

/// Serial failures (user-actionable messages, architecture §7).
#[derive(Debug, thiserror::Error)]
pub enum SerialError {
    /// Port I/O failure (unplugged adapter, permission denied, …).
    #[error("serial I/O error: {0}")]
    Io(String),
    /// Invalid settings (empty device, wild baud rate, …).
    #[error("invalid serial settings: {0}")]
    InvalidConfig(String),
    /// Backend not compiled in.
    #[error("serial support not compiled in (enable the `serial` feature)")]
    Unsupported,
}

impl SerialError {
    /// Build an I/O error from any displayable failure.
    pub fn io(error: impl std::fmt::Display) -> Self {
        Self::Io(error.to_string())
    }

    /// User-facing message (notification path).
    pub fn user_message(&self) -> String {
        match self {
            Self::Io(detail) => format!("Serial port error: {detail}"),
            Self::InvalidConfig(detail) => format!("Invalid serial settings: {detail}"),
            Self::Unsupported => "Serial support is not compiled in for this build.".to_string(),
        }
    }
}

impl From<std::io::Error> for SerialError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_map_to_actionable_messages() {
        assert!(SerialError::Unsupported
            .user_message()
            .contains("not compiled"));
        assert!(SerialError::InvalidConfig("empty device".into())
            .user_message()
            .contains("empty device"));
    }
}
