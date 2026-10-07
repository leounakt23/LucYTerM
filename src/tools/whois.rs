//! WHOIS command construction. The actual network operation is deliberately
//! delegated to the local/remote `whois` client so referral behavior follows
//! the host's maintained implementation.

use super::{remote_exec, shell_quote, BoxStream, CancelToken, OutputLevel, ToolEvent};

pub fn command(target: &str, server: &str) -> String {
    if server.trim().is_empty() {
        format!("whois {}", shell_quote(target))
    } else {
        format!("whois -h {} {}", shell_quote(server), shell_quote(target))
    }
}

pub fn run_remote(
    session: mbxt_core::SessionId,
    target: String,
    server: String,
    cancel: CancelToken,
) -> BoxStream<'static, ToolEvent> {
    use futures::StreamExt as _;
    Box::pin(
        futures::stream::once(async move {
            if cancel.is_cancelled() {
                return vec![ToolEvent::Cancelled];
            }
            match remote_exec(session, &command(&target, &server)).await {
                Ok(output) => output
                    .lines()
                    .map(|line| ToolEvent::Output {
                        line: line.to_string(),
                        level: OutputLevel::Info,
                    })
                    .chain([ToolEvent::Completed { summary: None }])
                    .collect(),
                Err(error) => vec![ToolEvent::Failed { error }],
            }
        })
        .flat_map(futures::stream::iter),
    )
}

/// Default WHOIS port (TCP/43) and referral-chase limits.
pub const DEFAULT_PORT: u16 = 43;
const MAX_REFERRALS: usize = 5;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// Extract a referral target from WHOIS output. Understands
/// `ReferralServer: whois://host[:port]` (ARIN-style) and
/// `Whois Server: host` (TLD-style, port 43). Returns `None` for
/// `rwhois://` targets, which speak a different protocol.
pub fn extract_referral(text: &str) -> Option<(String, u16)> {
    for line in text.lines() {
        let (key, value) = line.split_once(':')?;
        let value = value.trim();
        if key.eq_ignore_ascii_case("referralserver") {
            let rest = value.strip_prefix("whois://")?;
            let (host, port) = match rest.split_once(':') {
                Some((host, port)) => (host, port.parse().ok()?),
                None => (rest, DEFAULT_PORT),
            };
            if host.is_empty() {
                continue;
            }
            return Some((host.to_string(), port));
        }
        if key.eq_ignore_ascii_case("whois server") {
            let host = value.split_whitespace().next()?;
            if !host.is_empty() {
                return Some((host.to_string(), DEFAULT_PORT));
            }
        }
    }
    None
}

/// One WHOIS query: TCP connect, send `query\\r\\n`, read to EOF with a
/// per-operation timeout. Responses are capped at 1 MiB.
pub async fn query_once(
    server: &str,
    port: u16,
    query: &str,
    timeout_ms: u64,
) -> Result<String, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let address = format!("{server}:{port}");
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms.max(1)),
        tokio::net::TcpStream::connect(&address),
    )
    .await
    .map_err(|_| format!("whois connect to {address} timed out"))?
    .map_err(|error| format!("whois connect to {address} failed: {error}"))?;
    let request = format!("{query}\r\n");
    tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms.max(1)),
        stream.write_all(request.as_bytes()),
    )
    .await
    .map_err(|_| format!("whois write to {address} timed out"))?
    .map_err(|error| format!("whois write to {address} failed: {error}"))?;
    let mut raw = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        if raw.len() >= MAX_RESPONSE_BYTES {
            break;
        }
        match tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms.max(1)),
            stream.read(&mut chunk),
        )
        .await
        {
            Err(_) => return Err(format!("whois read from {address} timed out")),
            Ok(Err(error)) => return Err(format!("whois read from {address} failed: {error}")),
            Ok(Ok(0)) => break,
            Ok(Ok(count)) => raw.extend_from_slice(&chunk[..count]),
        }
    }
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

/// Follow referrals from `(server, port)` until a response carries none
/// (or the hop cap / a loop is hit). Returns the answering server plus
/// its text. Split from [`query_with_referral`] so tests can chase
/// across loopback ports; production starts at TCP/43.
pub async fn query_with_referral_from(
    server: &str,
    port: u16,
    query: &str,
    timeout_ms: u64,
    cancel: &CancelToken,
) -> Result<(String, String), String> {
    let mut server = (server.trim().to_string(), port);
    let mut visited = Vec::new();
    for _ in 0..=MAX_REFERRALS {
        if cancel.is_cancelled() {
            return Err("whois cancelled".to_string());
        }
        if visited.contains(&server) {
            return Err(format!("whois referral loop at {}", server.0));
        }
        visited.push(server.clone());
        let text = query_once(&server.0, server.1, query, timeout_ms).await?;
        match extract_referral(&text) {
            Some(next) if next != server => {
                server = next;
                continue;
            },
            _ => return Ok((server.0, text)),
        }
    }
    Err(format!(
        "whois exceeded {MAX_REFERRALS} referrals starting at {}",
        visited
            .first()
            .map(|(host, _)| host.as_str())
            .unwrap_or("?")
    ))
}

/// Follow referrals from `start_server` until a response carries none
/// (or the hop cap / a loop is hit). Returns the answering server plus
/// its text. An empty `server` starts at `whois.iana.org`. Referred
/// ports are honored; the default is TCP/43.
pub async fn query_with_referral(
    start_server: &str,
    query: &str,
    timeout_ms: u64,
    cancel: &CancelToken,
) -> Result<(String, String), String> {
    let start = if start_server.trim().is_empty() {
        "whois.iana.org"
    } else {
        start_server.trim()
    };
    query_with_referral_from(start, DEFAULT_PORT, query, timeout_ms, cancel).await
}

/// Local WHOIS run as a [`ToolEvent`] stream (referral chase included).
pub fn run_local(
    target: String,
    server: String,
    timeout_ms: u64,
    cancel: CancelToken,
) -> BoxStream<'static, ToolEvent> {
    use futures::StreamExt as _;
    Box::pin(
        futures::stream::once(async move {
            let mut events = vec![ToolEvent::Started];
            match query_with_referral(&server, &target, timeout_ms, &cancel).await {
                Ok((answering, text)) => {
                    events.push(ToolEvent::Progress {
                        message: format!("answered by {answering}"),
                        percent: None,
                    });
                    events.extend(text.lines().map(|line| ToolEvent::Output {
                        line: line.to_string(),
                        level: OutputLevel::Info,
                    }));
                    events.push(ToolEvent::Completed { summary: None });
                },
                Err(error) => events.push(ToolEvent::Failed { error }),
            }
            events
        })
        .flat_map(futures::stream::iter),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referral_forms_parse() {
        assert_eq!(
            extract_referral("ReferralServer: whois://whois.ripe.net:43"),
            Some(("whois.ripe.net".to_string(), 43))
        );
        assert_eq!(
            extract_referral("referralserver: whois://whois.ripe.net"),
            Some(("whois.ripe.net".to_string(), 43))
        );
        assert_eq!(
            extract_referral("Whois Server: whois.example.com"),
            Some(("whois.example.com".to_string(), 43))
        );
        // rwhois speaks another protocol: no referral.
        assert_eq!(
            extract_referral("ReferralServer: rwhois://rwhois.example:4321"),
            None
        );
        assert_eq!(extract_referral("no referral here"), None);
    }

    /// Canned WHOIS server on loopback: reads one query line, writes
    /// `reply`, closes. Returns the bound port.
    async fn canned_server(reply: &'static str) -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("loopback bind");
        let port = listener.local_addr().expect("addr").port();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut line = Vec::new();
                let mut byte = [0u8; 1];
                while byte != [b'\n'] {
                    if socket.read_exact(&mut byte).await.is_err() {
                        return;
                    }
                    line.push(byte[0]);
                    if line.len() > 256 {
                        return;
                    }
                }
                let _ = socket.write_all(reply.as_bytes()).await;
            }
        });
        port
    }

    #[tokio::test]
    async fn query_once_round_trip() {
        let port = canned_server("domain: EXAMPLE\n").await;
        let text = query_once("127.0.0.1", port, "example", 2000)
            .await
            .expect("query");
        assert!(text.contains("domain: EXAMPLE"), "got: {text}");
    }

    #[tokio::test]
    async fn referral_chain_is_followed() {
        let final_port = canned_server("domain: FINAL\n").await;
        let first_reply: &'static str =
            Box::leak(format!("ReferralServer: whois://127.0.0.1:{final_port}\n").into_boxed_str());
        // Direct canned-port probing: the first hop must point at the
        // final server, and the final hop must carry no referral.
        assert_eq!(
            extract_referral(first_reply),
            Some(("127.0.0.1".to_string(), final_port))
        );
        let first_port = canned_server(first_reply).await;
        // Simulate the chase over the (host, port) pairs the parser
        // produced, without touching port 43.
        let text = query_once("127.0.0.1", first_port, "example", 2000)
            .await
            .expect("first hop");
        let (host, port) = extract_referral(&text).expect("referral");
        assert_eq!(host, "127.0.0.1");
        let final_text = query_once(&host, port, "example", 2000)
            .await
            .expect("final hop");
        assert!(final_text.contains("domain: FINAL"), "got: {final_text}");
        assert_eq!(extract_referral(&final_text), None);
    }

    #[tokio::test]
    async fn referral_loop_is_rejected() {
        let cancel = CancelToken::new();
        // Closed port fails fast on connect-refused instead of hanging:
        // the error must name the target.
        let err = query_with_referral("127.0.0.1", "example", 500, &cancel)
            .await
            .expect_err("port 43 is closed on loopback");
        assert!(err.contains("127.0.0.1"), "got: {err}");
    }

    #[tokio::test]
    async fn back_and_forth_referrals_are_a_loop() {
        // Two canned servers referring to each other: the chase must
        // stop with a loop error, not bounce for all MAX_REFERRALS hops
        // and not hang. Both bind first so each knows the other's port.
        let listener_a = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind A");
        let port_a = listener_a.local_addr().expect("addr").port();
        let listener_b = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind B");
        let port_b = listener_b.local_addr().expect("addr").port();
        let serve = |listener: tokio::net::TcpListener, peer: u16| {
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                while let Ok((mut socket, _)) = listener.accept().await {
                    let mut byte = [0u8; 1];
                    while byte != [b'\n'] {
                        if socket.read_exact(&mut byte).await.is_err() {
                            break;
                        }
                    }
                    let reply = format!("ReferralServer: whois://127.0.0.1:{peer}\n");
                    if socket.write_all(reply.as_bytes()).await.is_err() {
                        break;
                    }
                }
            })
        };
        let _task_a = serve(listener_a, port_b);
        let _task_b = serve(listener_b, port_a);
        let cancel = CancelToken::new();
        let err = query_with_referral_from("127.0.0.1", port_a, "example", 2000, &cancel)
            .await
            .expect_err("A<->B must be a loop");
        assert!(err.contains("loop"), "got: {err}");
    }
}
