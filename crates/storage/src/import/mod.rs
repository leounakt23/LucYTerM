//! Third-party session import (v1.1 roadmap scope).
//!
//! One-way only: foreign formats (MobaXterm XML, PuTTY/KiTTY `.reg`,
//! OpenSSH `ssh_config`) parse into [`ImportedSession`] drafts plus an
//! [`ImportReport`]. Parsers are total, side-effect-free, and never
//! execute anything — `ProxyCommand` and friends arrive as quarantined
//! text. Persistence stays with the caller (`SecureStorage` /
//! `SessionStore`); [`merge`] applies the no-overwrite policy.
//!
//! Security contracts (binding on every parser in this module):
//! - input size/session/line caps enforced before parsing;
//! - no `unwrap`/`expect` on untrusted input;
//! - passwords and password-shaped values are dropped with a warning,
//!   never stored;
//! - unknown fields are reported, never guessed;
//! - proxy/auto commands are quarantined as disabled-by-default text.

pub mod merge;
pub mod mobaxterm;
pub mod model;
pub mod putty;
pub mod report;
pub mod ssh_config;

pub use merge::{apply, MergeOutcome, SkippedSession};
pub use model::{ImportWarning, ImportedSession, WarningCode};
pub use report::ImportReport;

/// Import size/session/line caps (all three parsers).
pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SESSIONS: usize = 2000;
pub const MAX_LINE_BYTES: usize = 16 * 1024;
pub const MAX_WARNINGS_ENUMERATED: usize = 50;

/// Typed import failure (malformed input, caps). Unknown *fields* are
/// warnings, never errors — only whole-input problems fail here.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    #[error("input too large ({0} bytes, cap is {1})")]
    TooLarge(usize, usize),
    #[error("too many sessions (cap is {0})")]
    TooManySessions(usize),
    #[error("malformed input at line {0}: {1}")]
    Malformed(usize, String),
    #[error("include depth exceeded")]
    IncludeDepthExceeded,
}

/// Reject oversize input before parsing (every parser calls this first).
pub fn check_size(text: &str) -> Result<(), ImportError> {
    if text.len() > MAX_INPUT_BYTES {
        return Err(ImportError::TooLarge(text.len(), MAX_INPUT_BYTES));
    }
    Ok(())
}

/// Source of an imported session (provenance tag + report grouping).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    MobaXterm,
    Putty,
    SshConfig,
}

impl Source {
    pub fn tag(self) -> &'static str {
        match self {
            Source::MobaXterm => "mobaxterm",
            Source::Putty => "putty",
            Source::SshConfig => "ssh-config",
        }
    }
}
