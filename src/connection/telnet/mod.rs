//! Telnet support (Prompt 4.4): protocol parser, direct session, errors.
//!
//! Layout (prompt-mandated):
//! - `parser` — IAC state machine (shared wire core lives in
//!   `mbxt-connections::telnet`, used by the actor transport).
//! - `session` — [`TelnetSession`] direct handle (embedding/tests).
//!
//! Plain Telnet carries no encryption: credentials cross the wire in the
//! clear. Prefer SSH; `use_tls` is tracked as follow-up (would need a TLS
//! connector — not wired in this prompt).

pub mod parser;
pub mod session;

pub use parser::{escape_iac, feed_terminal, TelnetParser, DEFAULT_TELNET_PORT};
pub use session::TelnetSession;

/// Telnet failures (user-actionable messages, architecture §7).
#[derive(Debug, thiserror::Error)]
pub enum TelnetError {
    /// TCP transport failure.
    #[error("telnet I/O error: {0}")]
    Io(String),
    /// Unusable endpoint (empty host, …).
    #[error("invalid telnet target: {0}")]
    InvalidTarget(String),
    /// Peer closed the connection.
    #[error("remote side closed the connection")]
    RemoteClosed,
}

impl TelnetError {
    /// Build an I/O error from any displayable failure.
    pub fn io(error: impl std::fmt::Display) -> Self {
        Self::Io(error.to_string())
    }

    /// User-facing message (notification path).
    pub fn user_message(&self) -> String {
        match self {
            Self::Io(detail) => format!("Telnet connection failed: {detail}"),
            Self::InvalidTarget(detail) => format!("Invalid telnet target: {detail}"),
            Self::RemoteClosed => "The remote side closed the connection.".to_string(),
        }
    }
}

impl From<std::io::Error> for TelnetError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_map_to_actionable_messages() {
        assert!(TelnetError::RemoteClosed.user_message().contains("closed"));
        assert!(TelnetError::InvalidTarget("empty host".into())
            .user_message()
            .contains("empty host"));
    }
}
