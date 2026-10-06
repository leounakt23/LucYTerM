//! `X11Forwarder`: per-session X11 forwarding endpoint (Prompt 4.1).
//!
//! The forwarder binds a display string to the local X server and proxies
//! each incoming X11 stream (server-opened `x11` channels handed over as
//! `AsyncRead + AsyncWrite`, or loopback listener peers) to it:
//!
//! ```text
//! remote xeyes → sshd X proxy → x11 channel → forward_stream → local X socket
//! ```
//!
//! Prompt field mapping: `display` (display string), `socket_path` (loopback
//! listener path once spawned), `auth_cookie` (hex cookie, if any). The live
//! `ssh_channel` itself is owned by the transport (`SshConn` drains
//! server-opened channels into [`X11Forwarder::forward_stream`]); storing the
//! raw channel here would split ownership of one connection across two
//! tasks, so the forwarder takes handed-over streams instead.

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

use tokio::io::{AsyncRead, AsyncWrite};

use super::display::{local_endpoint, read_cookie, LocalEndpoint, XAuthCookie};
use super::proxy::{dial_local, proxy_bidirectional, ProxyStats};
use super::X11Error;

/// Runtime counters (status indicator).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ForwarderStats {
    /// Currently proxied connections.
    pub active: usize,
    /// Connections completed since `enable`.
    pub completed: usize,
    /// Bytes forwarded in both directions, total.
    pub bytes: u64,
}

/// One X11 forwarding endpoint for a session.
pub struct X11Forwarder {
    /// Display string (`:0`, `localhost:10.0`).
    display: String,
    /// Loopback listener path, once [`X11Forwarder::spawn_listener`] ran.
    socket_path: Mutex<Option<PathBuf>>,
    /// Hex cookie forwarded via `request_x11`, if any.
    auth_cookie: Option<String>,
    /// Local endpoint resolved at `enable` time.
    endpoint: LocalEndpoint,
    active: AtomicUsize,
    completed: AtomicUsize,
    bytes: AtomicUsize,
    enabled_at: Instant,
    listener: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl std::fmt::Debug for X11Forwarder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never render the cookie (security §6.2).
        f.debug_struct("X11Forwarder")
            .field("display", &self.display)
            .field("socket_path", &self.socket_path)
            .field("has_cookie", &self.auth_cookie.is_some())
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl X11Forwarder {
    /// Set up forwarding for `display` (validates the local endpoint and
    /// reads the auth cookie; fails while the local X server is unreachable
    /// so the UI can warn instead of black-holing remote apps).
    pub fn enable(display: &str) -> Result<Arc<Self>, X11Error> {
        let info = mbxt_core::x11::parse_display(display)
            .map_err(|err| X11Error::Display(err.to_string()))?;
        let endpoint = local_endpoint(display)?;
        let cookie: Option<XAuthCookie> = read_cookie(info.display_number)?;
        Ok(Arc::new(Self {
            display: display.to_string(),
            socket_path: Mutex::new(None),
            auth_cookie: cookie.map(|cookie| cookie.hex),
            endpoint,
            active: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            bytes: AtomicUsize::new(0),
            enabled_at: Instant::now(),
            listener: Mutex::new(None),
        }))
    }

    /// Accept a server-opened `x11` channel (already converted to a stream)
    /// and proxy it to the local X server. Returns per-connection stats;
    /// graceful remote disconnects resolve as `Ok` with partial counters.
    pub async fn forward_stream<S>(&self, mut stream: S) -> Result<ProxyStats, X11Error>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        self.active.fetch_add(1, Ordering::SeqCst);
        let result = self.proxy_one(&mut stream).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        let stats = result?;
        self.completed.fetch_add(1, Ordering::SeqCst);
        self.bytes
            .fetch_add((stats.a_to_b + stats.b_to_a) as usize, Ordering::SeqCst);
        Ok(stats)
    }

    /// Handle one incoming connection on the loopback listener.
    pub async fn handle_connection<S>(&self, stream: S) -> Result<ProxyStats, X11Error>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        self.forward_stream(stream).await
    }

    /// Listen on a private unix socket; each peer is proxied to the local X
    /// server (loopback/nested-`Xephyr` use; the primary server→client path
    /// needs no listener — sshd opens `x11` channels on demand).
    /// Unix only — X11 has no Windows path.
    #[cfg(unix)]
    pub fn spawn_listener(self: &Arc<Self>, socket_path: PathBuf) -> Result<(), X11Error> {
        let listener = tokio::net::UnixListener::bind(&socket_path).map_err(X11Error::io)?;
        let forwarder = Arc::clone(self);
        let handle = super::proxy::spawn_unix_listener(listener, move |stream| {
            let forwarder = Arc::clone(&forwarder);
            async move {
                let _ = forwarder.handle_connection(stream).await;
            }
        });
        *self.listener.lock().expect("forwarder lock poisoned") = Some(handle);
        *self.socket_path.lock().expect("forwarder lock poisoned") = Some(socket_path);
        Ok(())
    }

    /// Stop the listener (socket file removed), drop in-flight accounting.
    /// Already-proxied streams finish on their own EOF; no handles leak —
    /// the abort only stops `accept`, and per-connection tasks are detached
    /// [`tokio`] tasks that end with their streams.
    pub fn disable(&self) {
        if let Ok(mut listener) = self.listener.lock() {
            if let Some(handle) = listener.take() {
                handle.abort();
            }
        }
        if let Ok(mut path) = self.socket_path.lock() {
            if let Some(socket) = path.take() {
                let _ = std::fs::remove_file(socket);
            }
        }
    }

    /// Display string under forwarding.
    pub fn display(&self) -> &str {
        &self.display
    }

    /// Hex cookie, if one was found (used for `request_x11`).
    pub fn cookie_hex(&self) -> Option<&str> {
        self.auth_cookie.as_deref()
    }

    /// Loopback socket path, if spawned.
    pub fn socket_path(&self) -> Option<PathBuf> {
        self.socket_path
            .lock()
            .expect("forwarder lock poisoned")
            .clone()
    }

    /// Current counters (status indicator).
    pub fn stats(&self) -> ForwarderStats {
        ForwarderStats {
            active: self.active.load(Ordering::SeqCst),
            completed: self.completed.load(Ordering::SeqCst),
            bytes: self.bytes.load(Ordering::SeqCst) as u64,
        }
    }

    /// Seconds since `enable` (diagnostics).
    pub fn uptime_secs(&self) -> u64 {
        self.enabled_at.elapsed().as_secs()
    }

    async fn proxy_one<S>(&self, stream: &mut S) -> Result<ProxyStats, X11Error>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let mut local = dial_local(&self.endpoint).await?;
        proxy_bidirectional(stream, &mut local).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_rejects_unreachable_display() {
        // :99 never exists; enable must fail so the UI can warn.
        let err = X11Forwarder::enable(":99").unwrap_err();
        assert!(matches!(err, X11Error::NoLocalServer(_)));
        assert!(err.user_message().contains("not accessible"));
    }

    #[test]
    fn debug_never_leaks_cookie() {
        let forwarder = X11Forwarder {
            display: ":0".to_string(),
            socket_path: Mutex::new(None),
            auth_cookie: Some("deadbeef".to_string()),
            endpoint: LocalEndpoint::Unix("/tmp/.X11-unix/X0".into()),
            active: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            bytes: AtomicUsize::new(0),
            enabled_at: Instant::now(),
            listener: Mutex::new(None),
        };
        assert!(!format!("{forwarder:?}").contains("deadbeef"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn forward_stream_proxies_to_local_unix_socket() {
        // Fake local X server: echoes one message, then EOF.
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("X0");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        tokio::spawn(async move {
            let (mut peer, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 5];
            tokio::io::AsyncReadExt::read_exact(&mut peer, &mut buf)
                .await
                .unwrap();
            tokio::io::AsyncWriteExt::write_all(&mut peer, b"world")
                .await
                .unwrap();
        });

        let forwarder = X11Forwarder {
            display: ":0".to_string(),
            socket_path: Mutex::new(None),
            auth_cookie: None,
            endpoint: LocalEndpoint::Unix(socket),
            active: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            bytes: AtomicUsize::new(0),
            enabled_at: Instant::now(),
            listener: Mutex::new(None),
        };
        let (mut client, server) = tokio::io::duplex(65536);
        let forwarded = tokio::spawn(async move { forwarder.forward_stream(server).await });
        tokio::io::AsyncWriteExt::write_all(&mut client, b"hello")
            .await
            .unwrap();
        let mut reply = [0u8; 5];
        tokio::io::AsyncReadExt::read_exact(&mut client, &mut reply)
            .await
            .unwrap();
        assert_eq!(&reply, b"world");
        drop(client);
        let stats = forwarded.await.unwrap().unwrap();
        assert_eq!(stats.a_to_b, 5);
        assert_eq!(stats.b_to_a, 5);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn listener_and_disable_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let forwarder = X11Forwarder {
            display: ":0".to_string(),
            socket_path: Mutex::new(None),
            auth_cookie: None,
            endpoint: LocalEndpoint::Unix(dir.path().join("nowhere")),
            active: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            bytes: AtomicUsize::new(0),
            enabled_at: Instant::now(),
            listener: Mutex::new(None),
        };
        let forwarder = Arc::new(forwarder);
        let socket = dir.path().join("loop.sock");
        forwarder.spawn_listener(socket.clone()).unwrap();
        assert_eq!(forwarder.socket_path(), Some(socket.clone()));
        assert!(socket.exists());
        forwarder.disable();
        assert!(!socket.exists(), "socket file removed on disable");
        assert_eq!(forwarder.socket_path(), None);
        // Idempotent.
        forwarder.disable();
    }
}
