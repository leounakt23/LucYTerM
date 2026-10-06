//! System integration layer: OS signals, desktop keyring, clipboard,
//! notifications. All dbus/nix/syscall usage is isolated here (architecture
//! §1 layer rules).

pub mod clipboard;
pub mod keyring;
pub mod signals;

/// System-layer errors.
#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    #[error("keyring unavailable: {0}")]
    Keyring(#[from] ::keyring::Error),
    #[error("clipboard backend unavailable")]
    ClipboardUnavailable,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
