//! Shared utilities: config schemas, crypto, secure storage, logging,
//! paths, single-instance guard, keyring bridge, embedded resources.

pub mod config;
pub mod crypto;
pub mod error;
pub mod i18n;
pub mod keyring_bridge;
pub mod logging;
pub mod paths;
pub mod profiling;
pub mod secrets;
pub mod secure_storage;
pub mod security;
pub mod single_instance;

/// Resources embedded into the binary at compile time by `build.rs`.
pub mod resources {
    /// `.desktop` entry (installed by `scripts/build.sh` / packaging).
    pub const DESKTOP_ENTRY: &str = include_str!("../../assets/remote-app.desktop");
    /// Placeholder application icon (SVG).
    pub const ICON_SVG: &str = include_str!("../../assets/icon.svg");
}
