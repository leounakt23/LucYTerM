//! Dynamic forwards (`-D`, Prompt 5.3): RFC 1928 SOCKS5 `CONNECT` routing.
//!
//! Supported subset: no-authentication method, `CONNECT` to IPv4 / domain /
//! IPv6 targets. Anything else fails with the protocol-correct reply code
//! instead of hanging the client: other commands → `0x07`, other address
//! types → `0x08`, other auth methods → `0xFF` (no acceptable methods).
//! Username/password auth (`0x02`) is explicitly beyond scope — dynamic
//! forwards bind loopback by default, which is the security boundary
//! (documented in the tunnels panel).

use std::future::Future;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::{proxy, ForwardError};

/// SOCKS5 reply codes we emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SocksReply {
    Succeeded = 0x00,
    GeneralFailure = 0x01,
    CommandNotSupported = 0x07,
    AddressNotSupported = 0x08,
}

/// CONNECT target after handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocksTarget {
    V4([u8; 4], u16),
    Domain(String, u16),
    V6([u8; 16], u16),
}

impl SocksTarget {
    /// `(host, port)` for the `direct-tcpip` open.
    pub fn host_port(&self) -> (String, u16) {
        match self {
            Self::V4(ip, port) => (format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]), *port),
            Self::Domain(name, port) => (name.clone(), *port),
            Self::V6(ip, port) => (
                format!(
                    "{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}",
                    ip[0], ip[1], ip[2], ip[3], ip[4], ip[5], ip[6], ip[7],
                    ip[8], ip[9], ip[10], ip[11], ip[12], ip[13], ip[14], ip[15]
                ),
                *port,
            ),
        }
    }
}

/// Serve one SOCKS5 client on `stream`: handshake, one CONNECT request,
/// then proxy to whatever `open` dials. Returns relayed byte totals.
pub async fn serve_socks5<S, O, OpenFut, Target>(
    stream: &mut S,
    open: O,
) -> Result<(u64, u64), SocksError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    O: FnOnce(String, u16) -> OpenFut,
    OpenFut: Future<Output = Result<Target, SocksError>>,
    Target: AsyncRead + AsyncWrite + Unpin,
{
    // Greeting: VER NMETHODS METHODS…
    let mut head = [0u8; 2];
    stream.read_exact(&mut head).await.map_err(SocksError::io)?;
    if head[0] != 0x05 {
        return Err(SocksError::protocol("not a SOCKS5 client"));
    }
    let mut methods = vec![0u8; head[1] as usize];
    if !methods.is_empty() {
        stream
            .read_exact(&mut methods)
            .await
            .map_err(SocksError::io)?;
    }
    if !methods.contains(&0x00) {
        stream
            .write_all(&[0x05, 0xFF])
            .await
            .map_err(SocksError::io)?;
        return Err(SocksError::protocol("client requires authentication"));
    }
    stream
        .write_all(&[0x05, 0x00])
        .await
        .map_err(SocksError::io)?;

    // Request: VER CMD RSV ATYP ADDR PORT.
    let mut request = [0u8; 4];
    stream
        .read_exact(&mut request)
        .await
        .map_err(SocksError::io)?;
    if request[0] != 0x05 {
        return Err(SocksError::protocol("not a SOCKS5 request"));
    }
    if request[1] != 0x01 {
        reply(stream, SocksReply::CommandNotSupported).await?;
        return Err(SocksError::protocol("only CONNECT is supported"));
    }
    let target = match request[3] {
        0x01 => {
            let mut addr = [0u8; 6];
            stream.read_exact(&mut addr).await.map_err(SocksError::io)?;
            SocksTarget::V4(
                [addr[0], addr[1], addr[2], addr[3]],
                u16::from_be_bytes([addr[4], addr[5]]),
            )
        },
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await.map_err(SocksError::io)?;
            let mut name = vec![0u8; len[0] as usize];
            stream.read_exact(&mut name).await.map_err(SocksError::io)?;
            let mut port = [0u8; 2];
            stream.read_exact(&mut port).await.map_err(SocksError::io)?;
            let name =
                String::from_utf8(name).map_err(|_| SocksError::protocol("bad domain encoding"))?;
            SocksTarget::Domain(name, u16::from_be_bytes(port))
        },
        0x04 => {
            let mut addr = [0u8; 18];
            stream.read_exact(&mut addr).await.map_err(SocksError::io)?;
            let mut ip = [0u8; 16];
            ip.copy_from_slice(&addr[..16]);
            SocksTarget::V6(ip, u16::from_be_bytes([addr[16], addr[17]]))
        },
        _ => {
            reply(stream, SocksReply::AddressNotSupported).await?;
            return Err(SocksError::protocol("address type not supported"));
        },
    };

    let (host, port) = target.host_port();
    let mut upstream = match open(host, port).await {
        Ok(upstream) => upstream,
        Err(err) => {
            reply(stream, SocksReply::GeneralFailure).await?;
            return Err(err);
        },
    };
    reply(stream, SocksReply::Succeeded).await?;
    proxy(stream, &mut upstream).await.map_err(SocksError::io)
}

/// Fixed 10-byte success reply (bound address zeroed — the client only
/// needs the status).
async fn reply<S: AsyncWrite + Unpin>(stream: &mut S, code: SocksReply) -> Result<(), SocksError> {
    stream
        .write_all(&[0x05, code as u8, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(SocksError::io)
}

/// Accept loop: SOCKS handshake per peer, upstream opened through `open`
/// (a `direct-tcpip` channel in production, echo duplexes in tests).
/// Children are tracked for abort-on-stop; the loop ends on cancel.
pub async fn run_dynamic_forward<O, OpenFut, Target>(
    listener: tokio::net::TcpListener,
    open: O,
    counters: std::sync::Arc<super::ForwardCounters>,
    log: super::local::LogSink,
    cancel: crate::connection::sftp::CancelToken,
    children: super::ChildTracker,
) -> Result<(), super::ForwardError>
where
    O: Fn(String, u16) -> OpenFut + Send + Sync + 'static,
    OpenFut: std::future::Future<Output = Result<Target, SocksError>> + Send,
    Target: super::ForwardIo + 'static,
{
    use std::sync::atomic::Ordering;
    let open = std::sync::Arc::new(open);
    loop {
        if cancel.is_cancelled() {
            break;
        }
        let accept =
            tokio::time::timeout(std::time::Duration::from_millis(200), listener.accept()).await;
        let (socket, peer) = match accept {
            Ok(Ok((socket, peer))) => (socket, peer),
            Ok(Err(_)) => break,
            Err(_) => continue,
        };
        counters.total.fetch_add(1, Ordering::SeqCst);
        log.push(format!("SOCKS client {peer}"));
        let open = std::sync::Arc::clone(&open);
        let counters = std::sync::Arc::clone(&counters);
        let log = log.clone();
        children.push(tokio::spawn(async move {
            counters.active.fetch_add(1, Ordering::SeqCst);
            let outcome = serve_one(socket, open, &counters, &log).await;
            counters.active.fetch_sub(1, Ordering::SeqCst);
            if let Err(reason) = outcome {
                log.push(format!("{peer} failed: {reason}"));
            }
        }));
    }
    Ok(())
}

async fn serve_one<O, OpenFut, Target>(
    mut socket: tokio::net::TcpStream,
    open: std::sync::Arc<O>,
    counters: &super::ForwardCounters,
    log: &super::local::LogSink,
) -> Result<(), super::ForwardError>
where
    O: Fn(String, u16) -> OpenFut,
    OpenFut: std::future::Future<Output = Result<Target, SocksError>>,
    Target: super::ForwardIo,
{
    use std::sync::atomic::Ordering;
    let (a_to_b, b_to_a) = serve_socks5(&mut socket, move |host, port| (*open)(host, port)).await?;
    counters.bytes_sent.fetch_add(a_to_b, Ordering::SeqCst);
    counters.bytes_received.fetch_add(b_to_a, Ordering::SeqCst);
    log.push(format!("SOCKS relay closed ({a_to_b}↑ {b_to_a}↓ bytes)"));
    Ok(())
}

/// SOCKS-layer failures (reply already sent where the protocol demands it).
#[derive(Debug, thiserror::Error)]
pub enum SocksError {
    /// Transport failure.
    #[error("SOCKS I/O error: {0}")]
    Io(String),
    /// Malformed handshake/request.
    #[error("SOCKS protocol error: {0}")]
    Protocol(String),
}

impl SocksError {
    /// Build an I/O error from any displayable failure.
    pub fn io(error: impl std::fmt::Display) -> Self {
        Self::Io(error.to_string())
    }

    /// Build a protocol error from any displayable failure.
    pub fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol(message.into())
    }
}

impl From<SocksError> for ForwardError {
    fn from(error: SocksError) -> Self {
        match error {
            SocksError::Io(detail) => Self::Io(detail),
            SocksError::Protocol(detail) => Self::Protocol(detail),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Echo upstream: mirrors everything back (stands in for the SSH side).
    async fn echo_target() -> Result<tokio::io::DuplexStream, SocksError> {
        let (a, b) = tokio::io::duplex(65536);
        tokio::spawn(async move {
            let (mut reader, mut writer) = tokio::io::split(a);
            let _ = tokio::io::copy(&mut reader, &mut writer).await;
        });
        Ok(b)
    }

    /// Greeting + optional request exchange. Empty `request` ends after the
    /// method reply (auth-rejection path never sends one).
    async fn connect_exchange(methods: &[u8], request: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let (mut client, mut server) = tokio::io::duplex(65536);
        let server_task =
            tokio::spawn(async move { serve_socks5(&mut server, |_, _| echo_target()).await });
        let mut greeting = vec![0x05, methods.len() as u8];
        greeting.extend_from_slice(methods);
        client.write_all(&greeting).await.unwrap();
        let mut method_reply = [0u8; 2];
        client.read_exact(&mut method_reply).await.unwrap();
        let mut reply = Vec::new();
        if !request.is_empty() {
            client.write_all(request).await.unwrap();
            let mut ten = [0u8; 10];
            client.read_exact(&mut ten).await.unwrap();
            reply = ten.to_vec();
        }
        drop(client);
        let _ = server_task.await;
        (method_reply.to_vec(), reply)
    }

    #[tokio::test]
    async fn ipv4_connect_proxies() {
        let (mut client, mut server) = tokio::io::duplex(65536);
        let server_task = tokio::spawn(async move {
            serve_socks5(&mut server, |host, port| async move {
                assert_eq!((host.as_str(), port), ("93.184.216.34", 80));
                echo_target().await
            })
            .await
        });
        client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut method_reply = [0u8; 2];
        client.read_exact(&mut method_reply).await.unwrap();
        assert_eq!(method_reply, [0x05, 0x00]);
        client
            .write_all(&[0x05, 0x01, 0x00, 0x01, 93, 184, 216, 34, 0, 80])
            .await
            .unwrap();
        let mut reply = [0u8; 10];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[1], 0x00, "success reply");
        client.write_all(b"hello").await.unwrap();
        let mut back = [0u8; 5];
        client.read_exact(&mut back).await.unwrap();
        assert_eq!(&back, b"hello");
        drop(client);
        let (a_to_b, b_to_a) = server_task.await.unwrap().unwrap();
        assert_eq!((a_to_b, b_to_a), (5, 5));
    }

    #[tokio::test]
    async fn domain_and_rejections() {
        // Domain CONNECT succeeds.
        let (methods, reply) = connect_exchange(
            &[0x00],
            &[
                0x05, 0x01, 0x00, 0x03, 11, b'e', b'x', b'a', b'm', b'p', b'l', b'e', b'.', b'c',
                b'o', b'm', 1, 187,
            ],
        )
        .await;
        assert_eq!(methods, vec![0x05, 0x00]);
        assert_eq!(reply[1], 0x00);

        // Auth-required client gets 0xFF with no request phase.
        let (methods, _) = connect_exchange(&[0x02], &[]).await;
        assert_eq!(methods, vec![0x05, 0xFF]);

        // BIND command gets 0x07.
        let (_, reply) =
            connect_exchange(&[0x00], &[0x05, 0x02, 0x00, 0x01, 1, 2, 3, 4, 0, 80]).await;
        assert_eq!(reply[1], 0x07);
    }

    #[test]
    fn target_host_port_formats() {
        assert_eq!(
            SocksTarget::V4([93, 184, 216, 34], 80).host_port(),
            ("93.184.216.34".to_string(), 80)
        );
        assert_eq!(
            SocksTarget::Domain("example.com".into(), 443).host_port(),
            ("example.com".to_string(), 443)
        );
        assert!(SocksTarget::V6([0; 16], 80).host_port().0.contains(':'));
    }
}
