//! Port forwarding & tunneling (Prompt 5.3): local (`-L`), remote (`-R`),
//! and dynamic SOCKS (`-D`) tunnels over one multiplexed SSH connection.
//!
//! Layout (prompt-mandated):
//! - `local` — listener + `direct-tcpip` dial + proxy loop.
//! - `remote` — `forwarded-tcpip` accept loop + local dial + proxy loop.
//! - `dynamic` — RFC 1928 SOCKS5 server (CONNECT only) over `direct-tcpip`.
//! - `manager` — [`ForwardManager`]: definitions, runtime, stats, logs.
//!
//! Channel plumbing: `SessionManager` opens `direct-tcpip` channels and
//! routes server-opened `forwarded-tcpip` channels on the existing session
//! connection — no extra SSH connections per tunnel. All byte movement is
//! streaming (`copy_bidirectional`); memory stays flat at hundreds of
//! concurrent connections.

pub mod dynamic;
pub mod local;
pub mod manager;
pub mod remote;

pub use dynamic::{run_dynamic_forward, serve_socks5, SocksError, SocksTarget};
pub use local::{bind_with_fallback, run_local_forward, LogSink};
pub use manager::{ForwardManager, ForwardStatus, PortForward};
pub use remote::{route_matches, run_remote_forward};

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::task::JoinHandle;

/// Live byte/connection counters (atomics: updated from proxy tasks).
#[derive(Debug, Default)]
pub struct ForwardCounters {
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub active: std::sync::atomic::AtomicU32,
    pub total: AtomicU64,
}

impl ForwardCounters {
    /// Snapshot for the UI (`start_secs` filled by the caller).
    pub fn snapshot(&self, start_secs: Option<u64>) -> ForwardStats {
        ForwardStats {
            bytes_sent: self.bytes_sent.load(Ordering::SeqCst),
            bytes_received: self.bytes_received.load(Ordering::SeqCst),
            active_connections: self.active.load(Ordering::SeqCst),
            total_connections: self.total.load(Ordering::SeqCst),
            start_secs,
        }
    }
}

/// Statistics snapshot (spec shape; `start_secs` replaces `DateTime`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ForwardStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub active_connections: u32,
    pub total_connections: u64,
    pub start_secs: Option<u64>,
}

/// Forward failures (user-actionable messages, architecture §7).
#[derive(Debug, thiserror::Error)]
pub enum ForwardError {
    /// Local/remote I/O failure.
    #[error("forward I/O error: {0}")]
    Io(String),
    /// Remote protocol failure (refused listen, SOCKS denial, …).
    #[error("forward protocol error: {0}")]
    Protocol(String),
    /// Unusable definition (bad host, bad port, …).
    #[error("invalid forward: {0}")]
    Invalid(String),
    /// Underlying SSH failure.
    #[error("SSH error: {0}")]
    Ssh(String),
    /// No live session (reconnect first).
    #[error("session is not connected")]
    NotConnected,
}

impl ForwardError {
    /// Build an I/O error from any displayable failure.
    pub fn io(error: impl std::fmt::Display) -> Self {
        Self::Io(error.to_string())
    }

    /// User-facing message (notification path).
    pub fn user_message(&self) -> String {
        match self {
            Self::Io(detail) => format!("Tunnel failed: {detail}"),
            Self::Protocol(detail) => format!("Tunnel refused: {detail}"),
            Self::Invalid(detail) => format!("Invalid tunnel: {detail}"),
            Self::Ssh(_) => {
                "Connection to the remote host failed. Check host, port, and credentials."
                    .to_string()
            },
            Self::NotConnected => "Session is not connected. Reconnect first.".to_string(),
        }
    }
}

impl From<std::io::Error> for ForwardError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// Async byte stream (TCP sockets, SSH channel streams, test duplexes).
pub trait ForwardIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ForwardIo for T {}

/// Metered stream: counts bytes through shared atomics (live stats).
pub struct Metered<S> {
    inner: S,
    read_bytes: Arc<AtomicU64>,
    written_bytes: Arc<AtomicU64>,
}

impl<S> Metered<S> {
    pub fn new(inner: S, read_bytes: Arc<AtomicU64>, written_bytes: Arc<AtomicU64>) -> Self {
        Self {
            inner,
            read_bytes,
            written_bytes,
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Metered<S> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let this = self.get_mut();
        match std::pin::Pin::new(&mut this.inner).poll_read(cx, buf) {
            ok @ std::task::Poll::Ready(Ok(())) => {
                this.read_bytes
                    .fetch_add((buf.filled().len() - before) as u64, Ordering::SeqCst);
                ok
            },
            other => other,
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Metered<S> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        match std::pin::Pin::new(&mut this.inner).poll_write(cx, buf) {
            std::task::Poll::Ready(Ok(count)) => {
                this.written_bytes.fetch_add(count as u64, Ordering::SeqCst);
                std::task::Poll::Ready(Ok(count))
            },
            other => other,
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

/// Proxy two streams to EOF, returning `(a_to_b, b_to_a)` byte totals.
/// EOF on either side ends the connection; both handles drop on scope exit.
pub async fn proxy<A, B>(a: &mut A, b: &mut B) -> std::io::Result<(u64, u64)>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    tokio::io::copy_bidirectional(a, b).await
}

/// Parsed `-L`/`-R` spec: `[bind:]port:host:port` (prompt CLI shape
/// `8080:localhost:80` binds loopback by default).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardSpec {
    pub bind: String,
    pub port: u16,
    pub host: String,
    pub host_port: u16,
}

/// Parse `[bind:]port:host:port` (3 or 4 colon parts; IPv6 hosts must use
/// the 4-part form with a bracketed bind or are rejected as ambiguous).
pub fn parse_forward_spec(text: &str) -> Result<ForwardSpec, String> {
    let parts: Vec<&str> = text.split(':').collect();
    let (bind, port, host, host_port) = match parts[..] {
        [port, host, host_port] => ("127.0.0.1", port, host, host_port),
        [bind, port, host, host_port] => (bind, port, host, host_port),
        _ => {
            return Err(format!("expected [bind:]port:host:port, got {text:?}"));
        },
    };
    if host.is_empty() || host.contains(':') {
        return Err(format!("bad remote host in {text:?}"));
    }
    Ok(ForwardSpec {
        bind: bind.to_string(),
        port: parse_port(port)?,
        host: host.to_string(),
        host_port: parse_port(host_port)?,
    })
}

/// Parse a `-D` spec: `port` or `bind:port`.
pub fn parse_dynamic_spec(text: &str) -> Result<(String, u16), String> {
    match text.split(':').collect::<Vec<_>>()[..] {
        [port] => Ok(("127.0.0.1".to_string(), parse_port(port)?)),
        [bind, port] => Ok((bind.to_string(), parse_port(port)?)),
        _ => Err(format!("expected [bind:]port, got {text:?}")),
    }
}

fn parse_port(text: &str) -> Result<u16, String> {
    text.parse::<u16>()
        .map_err(|_| format!("bad port {text:?} (1–65535)"))
}

/// Direct SSH dial for headless/tunnel use (password or key file; agent and
/// keyboard-interactive are rejected with guidance, mirroring the tunnel
/// scope). Returns the shared handle plus the forwarded-channel receiver
/// for `-R` loops.
pub struct TunnelDial {
    pub handle: russh::client::Handle<TunnelHandler>,
    pub forwarded_rx: tokio::sync::mpsc::UnboundedReceiver<mbxt_connections::ForwardedTcpIp>,
}

/// `russh` handler for tunnel dials (known-hosts checked, channels routed
/// with their listen endpoints for multi-`-R` dispatch).
#[derive(Debug)]
pub struct TunnelHandler {
    host: String,
    port: u16,
    forwarded_tx: Option<tokio::sync::mpsc::UnboundedSender<mbxt_connections::ForwardedTcpIp>>,
}

#[async_trait::async_trait]
impl russh::client::Handler for TunnelHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        match russh::keys::check_known_hosts(&self.host, self.port, key) {
            Ok(accepted) => Ok(accepted),
            Err(error) => {
                tracing::warn!(%error, "tunnel known_hosts check failed");
                Ok(false)
            },
        }
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<russh::client::Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        _session: &mut russh::client::Session,
    ) -> Result<(), Self::Error> {
        if let Some(tx) = self.forwarded_tx.as_ref() {
            let _ = tx.send(mbxt_connections::ForwardedTcpIp {
                stream: channel.into_stream(),
                connected_host: connected_address.to_string(),
                connected_port,
            });
        }
        Ok(())
    }
}

/// Establish one SSH connection for tunneling (CLI + API embedding).
pub async fn dial_tunnel(
    host: &str,
    port: u16,
    username: &str,
    auth: super::ConnectionAuth,
) -> Result<TunnelDial, ForwardError> {
    use std::sync::Arc;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let config = Arc::new(russh::client::Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(30)),
        keepalive_interval: Some(std::time::Duration::from_secs(15)),
        keepalive_max: 3,
        ..Default::default()
    });
    let mut handle = russh::client::connect(
        config,
        (host, port),
        TunnelHandler {
            host: host.to_string(),
            port,
            forwarded_tx: Some(tx),
        },
    )
    .await
    .map_err(|err| ForwardError::Ssh(err.to_string()))?;

    let authenticated = match auth {
        super::ConnectionAuth::Password(password) => handle
            .authenticate_password(username, password.as_str())
            .await
            .map_err(|err| ForwardError::Ssh(err.to_string()))?,
        super::ConnectionAuth::KeyFile { path, passphrase } => {
            let key = russh::keys::load_secret_key(path, passphrase.as_deref().map(String::as_str))
                .map_err(|err| ForwardError::Ssh(err.to_string()))?;
            handle
                .authenticate_publickey(username, Arc::new(key))
                .await
                .map_err(|err| ForwardError::Ssh(err.to_string()))?
        },
        _ => {
            return Err(ForwardError::Ssh(
                "tunnel dial needs password or key-file auth".into(),
            ));
        },
    };
    if !authenticated {
        return Err(ForwardError::Ssh("server rejected authentication".into()));
    }
    Ok(TunnelDial {
        handle,
        forwarded_rx: rx,
    })
}

/// Tracked child tasks: abort them all on stop so no socket or task leaks.
/// Completed handles are pruned opportunistically to bound memory.
#[derive(Debug, Default, Clone)]
pub struct ChildTracker {
    handles: Arc<std::sync::Mutex<Vec<JoinHandle<()>>>>,
}

impl ChildTracker {
    pub fn push(&self, handle: JoinHandle<()>) {
        if let Ok(mut handles) = self.handles.lock() {
            handles.retain(|handle| !handle.is_finished());
            handles.push(handle);
        }
    }

    pub fn abort_all(&self) {
        if let Ok(handles) = self.handles.lock() {
            for handle in handles.iter() {
                handle.abort();
            }
        }
    }

    pub fn live_count(&self) -> usize {
        self.handles
            .lock()
            .map(|handles| handles.iter().filter(|h| !h.is_finished()).count())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_specs_parse_three_and_four_parts() {
        assert_eq!(
            parse_forward_spec("8080:localhost:80").unwrap(),
            ForwardSpec {
                bind: "127.0.0.1".into(),
                port: 8080,
                host: "localhost".into(),
                host_port: 80,
            }
        );
        assert_eq!(
            parse_forward_spec("0.0.0.0:8080:db.internal:5432").unwrap(),
            ForwardSpec {
                bind: "0.0.0.0".into(),
                port: 8080,
                host: "db.internal".into(),
                host_port: 5432,
            }
        );
        assert!(parse_forward_spec("8080").is_err());
        assert!(parse_forward_spec("a:b:c:d:e").is_err());
        assert!(parse_forward_spec("0:notaport:h:80").is_err());
        assert!(parse_forward_spec("8080::80").is_err());
    }

    #[test]
    fn dynamic_specs_parse_bare_and_bound() {
        assert_eq!(
            parse_dynamic_spec("1080").unwrap(),
            ("127.0.0.1".into(), 1080)
        );
        assert_eq!(
            parse_dynamic_spec("0.0.0.0:1080").unwrap(),
            ("0.0.0.0".into(), 1080)
        );
        assert!(parse_dynamic_spec("a:b:c").is_err());
    }

    #[tokio::test]
    async fn metered_counts_both_directions() {
        let (a1, mut a2) = tokio::io::duplex(65536);
        let sent = Arc::new(AtomicU64::new(0));
        let received = Arc::new(AtomicU64::new(0));
        let mut metered = Metered::new(a1, Arc::clone(&received), Arc::clone(&sent));
        tokio::io::AsyncWriteExt::write_all(&mut metered, b"hello")
            .await
            .unwrap();
        let mut buf = [0u8; 5];
        tokio::io::AsyncReadExt::read_exact(&mut a2, &mut buf)
            .await
            .unwrap();
        tokio::io::AsyncWriteExt::write_all(&mut a2, b"world!")
            .await
            .unwrap();
        let mut back = [0u8; 6];
        tokio::io::AsyncReadExt::read_exact(&mut metered, &mut back)
            .await
            .unwrap();
        assert_eq!(sent.load(Ordering::SeqCst), 5);
        assert_eq!(received.load(Ordering::SeqCst), 6);
    }

    #[test]
    fn tracker_prunes_finished_children() {
        let tracker = ChildTracker::default();
        assert_eq!(tracker.live_count(), 0);
        tracker.abort_all(); // no-op, never panics
    }
}
