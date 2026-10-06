//! Connection layer: transports, factory (Factory pattern), auth strategies
//! (Strategy pattern). Per `doc/architecture.md` §3.4, new protocols are
//! added by adding one module + one match arm; nothing upstream changes.
//!
//! Wire implementations land in prompt 0.5+; the trait surface and factory
//! are final so upstream code can be written against them now.

use async_trait::async_trait;
use mbxt_core::{AuthMethod, Protocol, SessionSpec};
use std::path::PathBuf;
use zeroize::Zeroizing;

#[derive(Clone)]
pub enum ConnectionAuth {
    Password(Zeroizing<String>),
    KeyFile {
        path: PathBuf,
        passphrase: Option<Zeroizing<String>>,
    },
    Agent,
    KeyboardInteractive(Zeroizing<String>),
}

impl std::fmt::Debug for ConnectionAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password(_) => f.write_str("Password(<redacted>)"),
            Self::KeyFile { path, .. } => f.debug_tuple("KeyFile").field(path).finish(),
            Self::Agent => f.write_str("Agent"),
            Self::KeyboardInteractive(_) => f.write_str("KeyboardInteractive(<redacted>)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionEvent {
    Output(Vec<u8>),
    ExitStatus(u32),
    Eof,
}

/// One tunneled TCP stream over the SSH connection (Prompt 5.3).
/// Produced by `direct-tcpip` opens and server `forwarded-tcpip` accepts;
/// both directions implement the async byte-stream traits.
#[cfg(feature = "ssh")]
pub type ForwardStream = russh::ChannelStream<russh::client::Msg>;

/// A server-opened `forwarded-tcpip` channel with its listen endpoint
/// (routes concurrent `-R` forwards sharing one connection).
#[cfg(feature = "ssh")]
pub struct ForwardedTcpIp {
    pub stream: ForwardStream,
    pub connected_host: String,
    pub connected_port: u32,
}

/// A live transport to a remote endpoint.
///
/// Implementations own their byte streams; the session actor wraps this and
/// pumps PTY data onto the event bus.
#[async_trait]
pub trait Connection: Send {
    /// Establish the connection and perform authentication.
    async fn start(&mut self, auth: ConnectionAuth, size: TerminalSize) -> Result<(), ConnError>;
    /// Write raw bytes to the remote side (terminal input).
    async fn write(&mut self, data: &[u8]) -> Result<(), ConnError>;
    async fn resize(&mut self, size: TerminalSize) -> Result<(), ConnError>;
    async fn next_event(&mut self) -> Result<ConnectionEvent, ConnError>;
    /// Graceful shutdown (channel close, PTY release, child reap).
    async fn shutdown(&mut self) -> Result<(), ConnError>;
    /// Run one command and collect its output (Prompt 5.4 remote tools).
    /// Transports without exec channels refuse.
    async fn exec(&mut self, _command: &str) -> Result<String, ConnError> {
        Err(ConnError::Unsupported("exec"))
    }
    /// Open a `direct-tcpip` channel to `host:port` (local/dynamic forward
    /// data path, Prompt 5.3). Transports without tunneling refuse.
    #[cfg(feature = "ssh")]
    async fn open_direct_channel(
        &mut self,
        _host: &str,
        _port: u32,
    ) -> Result<ForwardStream, ConnError> {
        Err(ConnError::Unsupported("port forwarding"))
    }
    /// Ask the server to listen on `bind:port` (`-R`, Prompt 5.3).
    /// Returns the bound port (servers may pick when `port == 0`).
    #[cfg(feature = "ssh")]
    async fn request_remote_forward(&mut self, _bind: &str, _port: u32) -> Result<u32, ConnError> {
        Err(ConnError::Unsupported("port forwarding"))
    }
    /// Release a remote listen (`-R` teardown, best-effort).
    #[cfg(feature = "ssh")]
    async fn cancel_remote_forward(&mut self, _bind: &str, _port: u32) -> Result<(), ConnError> {
        Err(ConnError::Unsupported("port forwarding"))
    }
    /// Sink for server-opened `forwarded-tcpip` channels (`-R` data path,
    /// tagged with the listen endpoint for multi-forward routing).
    /// `None` (default) drops them loudly at the handler.
    #[cfg(feature = "ssh")]
    fn set_forwarded_sink(
        &mut self,
        _sink: Option<tokio::sync::mpsc::UnboundedSender<ForwardedTcpIp>>,
    ) {
    }
}

/// Transport-level errors. Mapped to user-friendly messages in `app`
/// (error handling strategy, architecture §7).
#[derive(Debug, thiserror::Error)]
pub enum ConnError {
    #[error("protocol {0} support not compiled in (enable the corresponding cargo feature)")]
    Unsupported(&'static str),
    #[error("transport not implemented yet (skeleton)")]
    NotYetImplemented,
    #[error("authentication method {0:?} is not valid for this protocol")]
    AuthMismatch(AuthMethod),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid session: {0}")]
    InvalidSession(&'static str),
    #[cfg(feature = "ssh")]
    #[error("SSH error: {0}")]
    Ssh(#[from] russh::Error),
    #[error("SSH key error: {0}")]
    Key(String),
    #[error("server host key is unknown or does not match known_hosts")]
    HostKeyRejected,
    #[error("server rejected authentication")]
    AuthRejected,
    #[error("remote channel closed")]
    RemoteClosed,
}

/// Factory: creates the protocol-specific connection for a session spec.
pub struct ConnectionFactory;

impl ConnectionFactory {
    /// Build the transport matching `spec.protocol` (open/closed principle).
    pub fn create(spec: &SessionSpec) -> Result<Box<dyn Connection>, ConnError> {
        match spec.protocol {
            #[cfg(feature = "ssh")]
            Protocol::Ssh | Protocol::Sftp | Protocol::X11 => Ok(Box::new(ssh::SshConn::new(spec))),
            #[cfg(not(feature = "ssh"))]
            Protocol::Ssh | Protocol::Sftp | Protocol::X11 => Err(ConnError::Unsupported("ssh")),
            #[cfg(feature = "telnet")]
            Protocol::Telnet => Ok(Box::new(telnet::TelnetConn::new(spec))),
            #[cfg(not(feature = "telnet"))]
            Protocol::Telnet => Err(ConnError::Unsupported("telnet")),
            #[cfg(feature = "serial")]
            Protocol::Serial => Ok(Box::new(serial::SerialConn::new(spec))),
            #[cfg(not(feature = "serial"))]
            Protocol::Serial => Err(ConnError::Unsupported("serial")),
            // RDP/VNC/custom: spawn external clients (`freerdp`, VNC viewer,
            // arbitrary binaries) via `fork`/`exec` + PTY capture (#3, #4, #9).
            #[cfg(any(feature = "rdp", feature = "vnc"))]
            Protocol::Rdp | Protocol::Vnc | Protocol::Custom => {
                Ok(Box::new(spawn::SpawnConn::new(spec)))
            },
            #[cfg(not(any(feature = "rdp", feature = "vnc")))]
            Protocol::Rdp | Protocol::Vnc | Protocol::Custom => {
                Err(ConnError::Unsupported("spawn-based protocol"))
            },
            Protocol::Ftp => Err(ConnError::Unsupported("ftp")),
        }
    }
}

/// SSH transport (russh). Wire-up in prompt 0.5: auth strategy chain,
/// shell channel + PTY, forwarding, agent/X11 channels.
#[cfg(feature = "ssh")]
pub mod ssh;

/// Telnet transport (legacy devices; feature matrix #2, prompt 4.4).
#[cfg(feature = "telnet")]
pub mod telnet;

/// Serial transport (feature matrix #8; `serialport` backend, prompt 4.4).
#[cfg(feature = "serial")]
pub mod serial;

/// Spawn-based transport for external clients (RDP/VNC/custom commands).
#[cfg(any(feature = "rdp", feature = "vnc"))]
pub mod spawn {
    use super::*;

    #[derive(Debug)]
    pub struct SpawnConn {
        _spec: SessionSpec,
    }

    impl SpawnConn {
        pub fn new(spec: &SessionSpec) -> Self {
            Self {
                _spec: spec.clone(),
            }
        }
    }

    #[async_trait]
    impl Connection for SpawnConn {
        async fn start(
            &mut self,
            _auth: ConnectionAuth,
            _size: TerminalSize,
        ) -> Result<(), ConnError> {
            // Never pass secrets via argv (world-readable via /proc) — §6.3.
            Err(ConnError::NotYetImplemented)
        }
        async fn write(&mut self, _data: &[u8]) -> Result<(), ConnError> {
            Err(ConnError::NotYetImplemented)
        }
        async fn resize(&mut self, _size: TerminalSize) -> Result<(), ConnError> {
            Err(ConnError::NotYetImplemented)
        }
        async fn next_event(&mut self) -> Result<ConnectionEvent, ConnError> {
            Err(ConnError::NotYetImplemented)
        }
        async fn shutdown(&mut self) -> Result<(), ConnError> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbxt_core::AuthMethod;

    fn spec(protocol: Protocol) -> SessionSpec {
        SessionSpec {
            name: "test".into(),
            protocol,
            host: Some("example.invalid".into()),
            port: None,
            username: None,
            auth: AuthMethod::Password,
            tags: vec![],
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
        }
    }

    #[test]
    fn factory_respects_features() {
        // With default features (ssh, telnet), ftp is not built in.
        match ConnectionFactory::create(&spec(Protocol::Ftp)) {
            Err(ConnError::Unsupported(_)) => {},
            _ => panic!("expected Unsupported"),
        }
    }
}
