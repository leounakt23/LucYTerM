//! `remote_app` — Linux-native remote computing application (MobaXterm parity).
//!
//! Crate layout follows `doc/architecture.md`:
//! - `app`         — MVU composition root (AppState/Message/update) + runtime wiring
//! - `task`        — TaskManager: async task scheduling + progress event bus
//! - `ui`          — Iced views & widgets (thin: state mapping only)
//! - `connection`  — facade over `mbxt-connections` (factory, transports)
//! - `terminal`    — facade over `mbxt-terminal` (grid model, parser)
//! - `utils`       — logging, paths, single-instance guard, embedded resources
//!
//! Domain logic lives in the `mbxt-*` workspace crates; this crate is the
//! binary/composition layer only.

pub mod app;
pub mod connection;
pub mod feedback;
/// Macro recording & playback (Prompt 5.2). Lives in `src/macro/` (the
/// `macro` identifier is a Rust keyword, so the module is `macros`).
#[path = "macro/mod.rs"]
pub mod macros;
pub mod security;
pub mod task;
pub mod terminal;
pub mod test_utils;
pub mod tools;
pub mod ui;
pub mod utils;

/// Build metadata embedded by `build.rs` (git hash, timestamp, completions dir).
pub mod build_info {
    /// Crate version from Cargo.
    pub const VERSION: &str = env!("CARGO_PKG_VERSION");
    /// Short git commit hash; `unknown` when built outside a git tree.
    pub const GIT_HASH: &str = match option_env!("MBXT_GIT_HASH") {
        Some(h) => h,
        None => "unknown",
    };
    /// Build timestamp (unix seconds) set by `build.rs`.
    pub const BUILD_TIME: &str = match option_env!("MBXT_BUILD_TIME") {
        Some(t) => t,
        None => "unknown",
    };
    /// Directory where `build.rs` generated shell completion scripts.
    pub const COMPLETIONS_DIR: &str = match option_env!("MBXT_COMPLETIONS_DIR") {
        Some(d) => d,
        None => "target/completions",
    };

    /// Human-readable long version string, e.g. `0.1.0 (a1b2c3d unix:1730000000)`.
    pub fn long() -> String {
        format!("{VERSION} ({GIT_HASH} {BUILD_TIME})")
    }
}
