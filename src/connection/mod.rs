//! Connection facade over `mbxt-connections`.
//! The UI never sees russh/nix/dbus types — only `mbxt-core` domain types and
//! events (architecture §1 layer rule 1).

pub use mbxt_connections::{
    ConnError, Connection, ConnectionAuth, ConnectionEvent, ConnectionFactory, TerminalSize,
};

/// Authentication subsystem (prompt 2.2): providers, negotiation, prompts.
pub mod ssh;

/// SFTP subsystem (prompt 3.1): file browsing + streaming transfers.
/// Rides the SSH connection (same auth, subsystem channel).
#[cfg(feature = "ssh")]
pub mod sftp;

/// X11 forwarding (prompt 4.1): display detection, byte proxy, per-session
/// forwarder lifecycle. Rides SSH like SFTP (same auth, `x11` channels).
#[cfg(feature = "ssh")]
pub mod x11;

/// Port forwarding & tunneling (Prompt 5.3): local/remote/dynamic loops
/// plus the per-session forward manager. Rides the SSH connection.
#[cfg(feature = "ssh")]
pub mod forward;

/// Telnet support (Prompt 4.4, feature matrix #2): IAC parser + direct
/// session. The actor transport lives in `mbxt-connections::telnet`.
#[cfg(feature = "telnet")]
pub mod telnet;

/// Serial support (Prompt 4.4, feature matrix #8): validated config +
/// direct session over the `serialport` backend.
#[cfg(feature = "serial")]
pub mod serial;

/// VNC viewer (prompt 4.3): embedded RFB client over TCP or SSH tunnels.
/// Needs `ssh` for tunneling plus `des` for password auth.
#[cfg(feature = "vnc")]
pub mod vnc;

/// Credential caching (encrypted at rest via the master-password KEK).
pub mod credential_cache;

/// Session-specific glue (actors) lands in prompt 2.1+; the factory and
/// trait definitions remain the single protocol entry point.
pub mod actor;
