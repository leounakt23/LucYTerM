//! Byte proxy between X11 peers (Prompt 4.1).
//!
//! The core ([`proxy_bidirectional`]) is generic over any
//! `AsyncRead + AsyncWrite` pair, so SSH channel streams, unix sockets, and
//! in-memory duplexes share one path (unit-tested headless). [`dial_local`]
//! connects to the local X server; [`spawn_unix_listener`] serves the
//! forwarder's optional loopback socket.

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

use super::display::LocalEndpoint;
use super::X11Error;

/// Bytes moved in each direction (disconnect diagnostics).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProxyStats {
    pub a_to_b: u64,
    pub b_to_a: u64,
}

/// Proxy until both directions hit EOF (graceful X11 disconnects).
///
/// Half-closes propagate: when one side ends, its direction stops while the
/// other drains, then both handles drop on scope exit — no leaked sockets.
pub async fn proxy_bidirectional<A, B>(a: &mut A, b: &mut B) -> Result<ProxyStats, X11Error>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let (a_to_b, b_to_a) = tokio::io::copy_bidirectional(a, b)
        .await
        .map_err(X11Error::io)?;
    Ok(ProxyStats { a_to_b, b_to_a })
}

/// Connected local X peer (unix socket or TCP).
#[derive(Debug)]
pub enum LocalStream {
    /// Local socket (unix only — X11/XWayland have no Windows path).
    #[cfg(unix)]
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl AsyncRead for LocalStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            #[cfg(unix)]
            Self::Unix(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
            Self::Tcp(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for LocalStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            #[cfg(unix)]
            Self::Unix(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
            Self::Tcp(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            #[cfg(unix)]
            Self::Unix(stream) => std::pin::Pin::new(stream).poll_flush(cx),
            Self::Tcp(stream) => std::pin::Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            #[cfg(unix)]
            Self::Unix(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
            Self::Tcp(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
        }
    }
}

/// Connect to the local X server.
pub async fn dial_local(endpoint: &LocalEndpoint) -> Result<LocalStream, X11Error> {
    match endpoint {
        #[cfg(unix)]
        LocalEndpoint::Unix(path) => Ok(LocalStream::Unix(
            UnixStream::connect(path).await.map_err(X11Error::io)?,
        )),
        #[cfg(not(unix))]
        LocalEndpoint::Unix(path) => Err(X11Error::io(format!(
            "unix sockets are unsupported on this platform: {}",
            path.display()
        ))),
        LocalEndpoint::Tcp(host, port) => Ok(LocalStream::Tcp(
            TcpStream::connect((host.as_str(), *port))
                .await
                .map_err(X11Error::io)?,
        )),
    }
}

/// Accept loop over a bound unix socket; each peer is handed to `on_peer`.
/// Returns the join handle (abort to stop; the socket file is removed by
/// the forwarder's `disable`). Unix only — X11 has no Windows path.
#[cfg(unix)]
pub fn spawn_unix_listener<F, Fut>(
    listener: UnixListener,
    on_peer: F,
) -> tokio::task::JoinHandle<()>
where
    F: Fn(UnixStream) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let on_peer = std::sync::Arc::new(on_peer);
    tokio::spawn(async move {
        loop {
            let peer = on_peer.clone();
            match listener.accept().await {
                Ok((stream, _)) => {
                    tokio::spawn(async move { peer(stream).await });
                },
                Err(_) => break, // listener closed/disabled
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bidirectional_proxy_moves_bytes_both_ways() {
        let (mut a1, mut a2) = tokio::io::duplex(65536);
        let (mut b1, mut b2) = tokio::io::duplex(65536);

        let proxy = tokio::spawn(async move { proxy_bidirectional(&mut a1, &mut b1).await });
        tokio::io::AsyncWriteExt::write_all(&mut a2, b"hello-x11")
            .await
            .unwrap();
        let mut buf = [0u8; 9];
        tokio::io::AsyncReadExt::read_exact(&mut b2, &mut buf)
            .await
            .unwrap();
        assert_eq!(&buf, b"hello-x11");

        tokio::io::AsyncWriteExt::write_all(&mut b2, b"reply!")
            .await
            .unwrap();
        let mut back = [0u8; 6];
        tokio::io::AsyncReadExt::read_exact(&mut a2, &mut back)
            .await
            .unwrap();
        assert_eq!(&back, b"reply!");

        drop(a2);
        drop(b2);
        let stats = proxy.await.unwrap().unwrap();
        assert_eq!(stats.a_to_b, 9);
        assert_eq!(stats.b_to_a, 6);
    }

    #[tokio::test]
    async fn dial_missing_socket_fails_cleanly() {
        let err = dial_local(&LocalEndpoint::Unix("/nonexistent-mbxt-x11".into()))
            .await
            .unwrap_err();
        assert!(matches!(err, X11Error::Io(_)));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_listener_serves_one_peer_then_closes() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x11-test.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let served = std::sync::Arc::new(AtomicUsize::new(0));
        let served_clone = served.clone();
        let handle = spawn_unix_listener(listener, move |mut stream| {
            let served = served_clone.clone();
            async move {
                served.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 4];
                let _ = tokio::io::AsyncReadExt::read_exact(&mut stream, &mut buf).await;
            }
        });

        let mut client = UnixStream::connect(&path).await.unwrap();
        tokio::io::AsyncWriteExt::write_all(&mut client, b"ping")
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(served.load(Ordering::SeqCst), 1);
        handle.abort();
    }
}
