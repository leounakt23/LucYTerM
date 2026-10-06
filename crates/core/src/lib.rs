//! Shared domain vocabulary: sessions, protocols, auth methods, events,
//! actions, and the root error type. Pure data — **no I/O, no frameworks**
//! (architecture §2: everything else depends on this crate).

use serde::{Deserialize, Serialize};

/// Stable identifier for a stored session.
pub type SessionId = u64;

/// Supported connection protocols (feature matrix §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Protocol {
    Ssh,
    Telnet,
    Ftp,
    Sftp,
    Rdp,
    Vnc,
    X11,
    Serial,
    /// Arbitrary command session (`docker attach`, `socat`, local shell, …).
    Custom,
}

/// Authentication strategy selector (Strategy pattern, architecture §3.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMethod {
    Password,
    /// OpenSSH-format private key file, optionally with passphrase.
    KeyFile {
        path: String,
    },
    /// Use `SSH_AUTH_SOCK` agent (optionally forwarded to the remote).
    Agent {
        forward: bool,
    },
    KeyboardInteractive,
}

/// Everything needed to open one connection (one storage row).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSpec {
    pub name: String,
    pub protocol: Protocol,
    pub host: Option<String>,
    pub port: Option<u16>,
    /// Login user, when the protocol carries one (SSH/FTP/RDP/VNC).
    #[serde(default)]
    pub username: Option<String>,
    pub auth: AuthMethod,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: String,
    /// SSH X11 forwarding (Prompt 4.1): request `x11` on the shell channel
    /// and proxy server-opened X11 channels to the local X server.
    #[serde(default)]
    pub x11_forwarding: bool,
    /// Serial line parameters (Prompt 4.4, `Protocol::Serial` only).
    #[serde(default)]
    pub serial: Option<SerialParams>,
    /// Port-forward definitions (Prompt 5.3, SSH sessions).
    #[serde(default)]
    pub forwards: Vec<ForwardDef>,
}

/// Port-forwarding shape (Prompt 5.3 spec): local (`-L`), remote (`-R`),
/// and dynamic SOCKS (`-D`) tunnels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForwardType {
    Local {
        local_host: String,
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    },
    Remote {
        remote_host: String,
        remote_port: u16,
        local_host: String,
        local_port: u16,
    },
    Dynamic {
        local_host: String,
        local_port: u16,
    },
}

impl ForwardType {
    /// Short label for the tunnels panel (`L 127.0.0.1:8080 → web:80`).
    pub fn label(&self) -> String {
        match self {
            Self::Local {
                local_host,
                local_port,
                remote_host,
                remote_port,
            } => {
                format!("L {local_host}:{local_port} → {remote_host}:{remote_port}")
            },
            Self::Remote {
                remote_host,
                remote_port,
                local_host,
                local_port,
            } => {
                format!("R {remote_host}:{remote_port} → {local_host}:{local_port}")
            },
            Self::Dynamic {
                local_host,
                local_port,
            } => {
                format!("D {local_host}:{local_port} (SOCKS)")
            },
        }
    }

    /// Local bind endpoint for `Local`/`Dynamic` (listener side).
    pub fn local_bind(&self) -> Option<(&str, u16)> {
        match self {
            Self::Local {
                local_host,
                local_port,
                ..
            }
            | Self::Dynamic {
                local_host,
                local_port,
            } => Some((local_host, *local_port)),
            Self::Remote { .. } => None,
        }
    }
}

/// One persisted port-forward definition (runtime status/stats live in the
/// forward manager, never in the store).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardDef {
    pub id: uuid::Uuid,
    pub name: Option<String>,
    pub forward_type: ForwardType,
    pub auto_start: bool,
}

impl ForwardDef {
    pub fn new(name: Option<String>, forward_type: ForwardType, auto_start: bool) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            name,
            forward_type,
            auto_start,
        }
    }

    /// Display name (explicit name or the type label).
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| self.forward_type.label())
    }
}

/// Serial line parameters (Prompt 4.4): plain data, validated by
/// [`SerialParams::validate`]; transport mapping lives in the connection
/// crates so this core type stays dependency-free.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerialParams {
    /// Device path (`/dev/ttyUSB0`, `/dev/ttyS0`, …).
    pub device: String,
    /// Baud rate (9600, 115200, …).
    pub baud_rate: u32,
    /// Data bits (5–8).
    pub data_bits: u8,
    /// Parity (`none`, `odd`, `even`).
    pub parity: SerialParity,
    /// Stop bits (1–2).
    pub stop_bits: u8,
    /// Flow control (`none`, `software`, `hardware`).
    pub flow_control: SerialFlowControl,
}

/// Serial parity (serializable vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SerialParity {
    #[default]
    None,
    Odd,
    Even,
}

/// Serial flow control (serializable vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SerialFlowControl {
    #[default]
    None,
    Software,
    Hardware,
}

/// Common baud rates for the device picker.
pub const COMMON_BAUD_RATES: [u32; 12] = [
    300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600,
];

impl Default for SerialParams {
    fn default() -> Self {
        Self {
            device: String::new(),
            baud_rate: 115200,
            data_bits: 8,
            parity: SerialParity::None,
            stop_bits: 1,
            flow_control: SerialFlowControl::None,
        }
    }
}

impl SerialParams {
    /// Validate for `open` (empty device, wild baud/data/stop bits rejected
    /// with actionable messages for the dialog).
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.device.trim().is_empty() {
            return Err("serial device path is empty");
        }
        if !(50..=4_000_000).contains(&self.baud_rate) {
            return Err("baud rate out of range (50–4000000)");
        }
        if !(5..=8).contains(&self.data_bits) {
            return Err("data bits must be 5–8");
        }
        if !(1..=2).contains(&self.stop_bits) {
            return Err("stop bits must be 1–2");
        }
        Ok(())
    }
}

/// Pure X11 display/xauth parsing (Prompt 4.1).
///
/// No I/O here (charter: pure data) — environment probing (`$DISPLAY`) and
/// `~/.Xauthority` file reads live in the app-level `connection::x11::display`
/// module, which builds on these parsers.
pub mod x11;

/// Live connection state as reported by a session actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Disconnected,
    Connecting,
    Connected,
    Failed(String),
}

/// A stored session plus its current state (UI model unit).
#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub spec: SessionSpec,
    pub state: SessionState,
}

/// Events pushed from the runtime to the UI via the event bus
/// (Observer pattern, architecture §3.3).
#[derive(Debug, Clone)]
pub enum UiEvent {
    SessionStateChanged {
        session: SessionId,
        state: SessionState,
    },
    /// Coalesced terminal output; emitted at most once per frame.
    TerminalOutput {
        session: SessionId,
        bytes: Vec<u8>,
    },
    TransferProgress {
        session: SessionId,
        done: u64,
        total: u64,
    },
    Error(String),
}

/// Queued unit of async work (Command pattern, architecture §3.2).
#[derive(Debug, Clone)]
pub enum Action {
    Connect(SessionId),
    Disconnect(SessionId),
    StartTransfer {
        session: SessionId,
        remote_path: String,
        local_path: String,
    },
    MultiExec {
        sessions: Vec<SessionId>,
        bytes: Vec<u8>,
    },
}

/// Root library error.
#[derive(Debug, thiserror::Error)]
pub enum MbxtError {
    #[error("session {0} not found")]
    SessionNotFound(SessionId),
    #[error("unsupported operation: {0}")]
    Unsupported(&'static str),
    #[error("{0}")]
    Other(String),
}
