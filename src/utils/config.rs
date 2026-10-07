//! Configuration schemas: `AppConfig` (settings) and `SessionEntry`
//! (encrypted-at-rest session record).
//!
//! - `AppConfig` lives in `~/.config/remote-app/config.ron` — human-readable,
//!   **non-sensitive** (security §6.3: secrets never live here).
//! - `SessionEntry` is the serde schema for `sessions.enc` (handled by
//!   `secure_storage`); every field has `#[serde(default)]` so older files
//!   load gracefully and new fields are backward-compatible.

use serde::{Deserialize, Serialize};

use mbxt_core::{AuthMethod, Protocol, SessionId, SessionSpec};

// ---------------------------------------------------------------------------
// SessionEntry (encrypted store schema)
// ---------------------------------------------------------------------------

/// One stored session as persisted in `sessions.enc` (feature matrix #10–13).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionEntry {
    pub id: SessionId,
    pub name: String,
    pub protocol: Protocol,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub auth: AuthMethod,
    pub tags: Vec<String>,
    pub notes: String,
    /// SSH X11 forwarding (Prompt 4.1).
    #[serde(default)]
    pub x11_forwarding: bool,
    /// Serial line parameters (Prompt 4.4, `Protocol::Serial` only).
    #[serde(default)]
    pub serial: Option<mbxt_core::SerialParams>,
    /// Port-forward definitions (Prompt 5.3, SSH sessions).
    #[serde(default)]
    pub forwards: Vec<mbxt_core::ForwardDef>,
    /// Creation time (unix secs) — for history sorting.
    pub created_secs: u64,
}

impl Default for SessionEntry {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            protocol: Protocol::Ssh,
            host: None,
            port: None,
            username: None,
            auth: AuthMethod::Agent { forward: false },
            tags: Vec::new(),
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
            created_secs: 0,
        }
    }
}

impl SessionEntry {
    /// Build a store entry from the runtime model.
    pub fn from_session(session: &mbxt_core::Session) -> Self {
        Self {
            id: session.id,
            name: session.spec.name.clone(),
            protocol: session.spec.protocol,
            host: session.spec.host.clone(),
            port: session.spec.port,
            username: session.spec.username.clone(),
            auth: session.spec.auth.clone(),
            tags: session.spec.tags.clone(),
            notes: session.spec.notes.clone(),
            x11_forwarding: session.spec.x11_forwarding,
            serial: session.spec.serial.clone(),
            forwards: session.spec.forwards.clone(),
            created_secs: crate::app::state::now_secs(),
        }
    }

    /// Convert back to the runtime spec (Loaded message path).
    pub fn to_spec(&self) -> SessionSpec {
        SessionSpec {
            name: self.name.clone(),
            protocol: self.protocol,
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            auth: self.auth.clone(),
            tags: self.tags.clone(),
            notes: self.notes.clone(),
            x11_forwarding: self.x11_forwarding,
            serial: self.serial.clone(),
            forwards: self.forwards.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// AppConfig sections
// ---------------------------------------------------------------------------

/// Root settings schema (`config.ron`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Schema version for forward migrations.
    pub version: u32,
    pub general: GeneralSettings,
    pub appearance: AppearanceSettings,
    pub terminal: TerminalSettings,
    pub network: NetworkSettings,
    pub security: SecuritySettings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            general: GeneralSettings::default(),
            appearance: AppearanceSettings::default(),
            terminal: TerminalSettings::default(),
            network: NetworkSettings::default(),
            security: SecuritySettings::default(),
        }
    }
}

/// Application update stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseChannel {
    Stable,
    Beta,
    Nightly,
}

impl Default for ReleaseChannel {
    fn default() -> Self {
        match option_env!("MBXT_DEFAULT_CHANNEL") {
            Some("nightly") => Self::Nightly,
            Some("beta") => Self::Beta,
            _ => Self::Stable,
        }
    }
}

impl std::fmt::Display for ReleaseChannel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Nightly => "nightly",
        })
    }
}

impl std::str::FromStr for ReleaseChannel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "stable" => Ok(Self::Stable),
            "beta" => Ok(Self::Beta),
            "nightly" => Ok(Self::Nightly),
            _ => Err(format!("unknown release channel: {value}")),
        }
    }
}

/// General behavior and explicitly opted-in diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    /// Session name to auto-connect on launch, if any.
    pub start_session: Option<String>,
    pub confirm_quit: bool,
    pub check_for_updates: bool,
    pub release_channel: ReleaseChannel,
    /// Anonymous allow-listed metrics. Off unless the user enables it.
    pub telemetry_enabled: bool,
    /// Sentry panic and minidump reporting. Off unless the user enables it.
    pub crash_reporting_enabled: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            start_session: None,
            confirm_quit: false,
            check_for_updates: false,
            release_channel: ReleaseChannel::Stable,
            telemetry_enabled: false,
            crash_reporting_enabled: false,
        }
    }
}

/// Look & feel (feature matrix #43).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    pub dark_theme: bool,
    /// UI scale factor (Wayland HiDPI friendliness).
    pub scale_factor: f32,
    /// Theme name: light/dark/solarized-light/solarized-dark, a custom
    /// theme file name, or empty (follow the legacy `dark_theme` flag).
    pub theme: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            dark_theme: true,
            scale_factor: 1.0,
            theme: String::new(),
        }
    }
}

/// Terminal defaults (feature matrix §2.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalSettings {
    pub font_family: String,
    pub font_size: u8,
    /// Scrollback cap (#21).
    pub scrollback_lines: usize,
    /// Alternate-screen wheel scrolls instead of scrollback.
    pub copy_on_select: bool,
    pub blinking_cursor: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            font_family: "monospace".to_string(),
            font_size: 13,
            scrollback_lines: 10_000,
            copy_on_select: true,
            blinking_cursor: false,
        }
    }
}

/// Network behavior (feature matrix #39/#34–36).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkSettings {
    pub connect_timeout_secs: u32,
    /// SSH keepalive interval (#39).
    pub keepalive_secs: u32,
    /// Reconnect backoff cap in seconds.
    pub reconnect_backoff_max_secs: u32,
    /// SFTP request pipeline depth (throughput knob, tech_stack R5).
    pub sftp_pipeline_depth: u32,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            connect_timeout_secs: 10,
            keepalive_secs: 30,
            reconnect_backoff_max_secs: 60,
            sftp_pipeline_depth: 20,
        }
    }
}

/// Security policy (feature matrix §2.7 / architecture §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SecuritySettings {
    /// Require the master password at startup even when the keyring has it.
    pub require_master_password: bool,
    /// Opt-in: cache the master password in the desktop keyring (#49).
    pub keyring_enabled: bool,
    /// Auto-lock after idle seconds (0 = never).
    pub lock_after_idle_secs: u32,
    /// Clear the clipboard mirror when the screen locks (privacy, §6.2).
    pub clear_clipboard_on_lock: bool,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            require_master_password: false,
            keyring_enabled: false,
            lock_after_idle_secs: 0,
            clear_clipboard_on_lock: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Load / save
// ---------------------------------------------------------------------------

/// Load `config.ron` from `config_dir`.
///
/// Graceful handling (quality bar): a missing file yields defaults; a
/// malformed/outdated file is moved aside to `config.ron.invalid` (never
/// silently overwritten — users can recover it) and defaults are returned.
pub fn load(config_dir: &std::path::Path) -> std::io::Result<AppConfig> {
    let path = config_dir.join("config.ron");
    match std::fs::read_to_string(&path) {
        Ok(text) => match ron::from_str(&text) {
            Ok(config) => Ok(config),
            Err(err) => {
                tracing::warn!(%err, path = %path.display(), "malformed config; moving aside");
                let backup = config_dir.join(format!(
                    "config.ron.invalid-{}",
                    crate::app::state::now_secs()
                ));
                let _ = std::fs::rename(&path, backup);
                Ok(AppConfig::default())
            },
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(err) => Err(err),
    }
}

/// Persist `config.ron` (pretty-printed RON, round-trip safe).
pub fn save(config_dir: &std::path::Path, config: &AppConfig) -> std::io::Result<()> {
    let text = ron::ser::to_string_pretty(config, ron::ser::PrettyConfig::default())
        .map_err(|e| std::io::Error::other(format!("config serialization failed: {e}")))?;
    write_private(config_dir.join("config.ron"), text.as_bytes())
}

/// Write a file restricted to owner-only permissions (unix `0600`).
pub fn write_private(path: std::path::PathBuf, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&path)?;
    #[cfg(unix)]
    {
        // SAFETY: `file` owns a valid descriptor and LOCK_EX is a scalar flag.
        if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&file), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    file.write_all(bytes)?;
    file.sync_all()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file.metadata()?.permissions();
        perms.set_mode(0o600);
        file.set_permissions(perms)?;
    }
    #[cfg(unix)]
    {
        // SAFETY: descriptor remains valid until `file` drops.
        unsafe {
            libc::flock(std::os::fd::AsRawFd::as_raw_fd(&file), libc::LOCK_UN);
        }
    }
    let _ = &path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_all_sections() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.terminal.scrollback_lines = 55_000;
        config.network.sftp_pipeline_depth = 8;
        config.security.keyring_enabled = true;
        config.appearance.dark_theme = false;
        save(dir.path(), &config).unwrap();

        let loaded = load(dir.path()).unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = load(dir.path()).unwrap();
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn malformed_config_is_moved_aside_and_defaults_apply() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config.ron"), "this is not ron !!!").unwrap();
        let config = load(dir.path()).unwrap();
        assert_eq!(config, AppConfig::default());
        // Original preserved for user recovery.
        let moved: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("config.ron.invalid"))
            .collect();
        assert_eq!(moved.len(), 1);
    }

    #[test]
    fn unknown_fields_are_tolerated() {
        // Forward compatibility: extra fields in newer files must not break.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.ron"),
            r#"(version: 99, future_field: "x")"#,
        )
        .unwrap();
        let config = load(dir.path()).unwrap();
        assert_eq!(config.version, 99);
        assert_eq!(config.general, GeneralSettings::default());
    }

    #[test]
    fn session_entry_round_trips_through_serde() {
        let entry = SessionEntry {
            id: 3,
            name: "bastion".into(),
            protocol: Protocol::Ssh,
            host: Some("10.0.0.1".into()),
            port: Some(2222),
            username: Some("ops".into()),
            auth: AuthMethod::KeyFile {
                path: "~/.ssh/id_ed25519".into(),
            },
            tags: vec!["jump".into()],
            notes: "primary bastion".into(),
            x11_forwarding: true,
            serial: Some(mbxt_core::SerialParams {
                device: "/dev/ttyUSB0".into(),
                baud_rate: 115200,
                ..mbxt_core::SerialParams::default()
            }),
            forwards: vec![mbxt_core::ForwardDef::new(
                Some("db".into()),
                mbxt_core::ForwardType::Local {
                    local_host: "127.0.0.1".into(),
                    local_port: 5433,
                    remote_host: "db.internal".into(),
                    remote_port: 5432,
                },
                true,
            )],
            created_secs: 1_700_000_000,
        };
        let packed = rmp_serde::to_vec(&entry).unwrap();
        let unpacked: SessionEntry = rmp_serde::from_slice(&packed).unwrap();
        assert_eq!(unpacked.name, "bastion");
        assert_eq!(unpacked.username.as_deref(), Some("ops"));
        assert!(
            unpacked.x11_forwarding,
            "x11 flag survives the store round trip"
        );
        assert!(unpacked.to_spec().x11_forwarding);
        assert_eq!(
            unpacked.to_spec().serial.as_ref().map(|s| s.baud_rate),
            Some(115200),
            "serial params survive the store round trip"
        );
        assert_eq!(
            unpacked.to_spec().forwards.len(),
            1,
            "forward defs survive the store round trip"
        );
        assert_eq!(
            unpacked.to_spec().forwards[0].forward_type.label(),
            "L 127.0.0.1:5433 → db.internal:5432"
        );
    }
}
