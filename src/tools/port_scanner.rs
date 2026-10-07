//! Bounded TCP connect scanner.

use super::{BoxStream, CancelToken, OutputLevel, ToolEvent};
use std::net::SocketAddr;
use std::time::Duration;

pub fn parse_ports(spec: &str) -> Result<Vec<u16>, String> {
    let mut ports = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if let Some((start, end)) = part.split_once('-') {
            let start: u16 = start.parse().map_err(|_| "invalid port")?;
            let end: u16 = end.parse().map_err(|_| "invalid port")?;
            if start > end {
                return Err("port range is reversed".into());
            }
            ports.extend(start..=end);
        } else {
            ports.push(part.parse().map_err(|_| "invalid port")?);
        }
    }
    ports.sort_unstable();
    ports.dedup();
    Ok(ports)
}

pub fn scan(
    host: String,
    ports: Vec<u16>,
    timeout_ms: u64,
    concurrency: u32,
    banner: bool,
    cancel: CancelToken,
) -> BoxStream<'static, ToolEvent> {
    use futures::StreamExt as _;
    Box::pin(
        futures::stream::once(async move {
            let mut events = vec![ToolEvent::Started];
            if cancel.is_cancelled() {
                events.push(ToolEvent::Cancelled);
                return events;
            }
            let limit = concurrency.max(1) as usize;
            let mut probes = futures::stream::iter(ports.into_iter().map(|port| {
                let host = host.clone();
                let cancel = cancel.clone();
                async move { probe_one(&host, port, timeout_ms, banner, &cancel).await }
            }))
            .buffer_unordered(limit)
            .collect::<Vec<_>>()
            .await;
            // Deterministic port order regardless of completion order.
            probes.sort_by_key(|(port, _)| *port);
            for (port, found) in probes {
                if cancel.is_cancelled() {
                    events.push(ToolEvent::Cancelled);
                    break;
                }
                if let Some(line) = found {
                    events.push(ToolEvent::Output {
                        line: format!("{port}/tcp open{line}"),
                        level: OutputLevel::Success,
                    });
                }
            }
            if !matches!(events.last(), Some(ToolEvent::Cancelled)) {
                events.push(ToolEvent::Completed { summary: None });
            }
            events
        })
        .flat_map(futures::stream::iter),
    )
}

/// One probe: connect, optionally grab a banner. Returns the port plus
/// an optional ` <banner>` suffix when the port is open.
async fn probe_one(
    host: &str,
    port: u16,
    timeout_ms: u64,
    banner: bool,
    cancel: &CancelToken,
) -> (u16, Option<String>) {
    use tokio::io::AsyncReadExt;
    if cancel.is_cancelled() {
        return (port, None);
    }
    let address = format!("{host}:{port}");
    let socket = address
        .parse::<SocketAddr>()
        .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], port)));
    let stream = match tokio::time::timeout(
        Duration::from_millis(timeout_ms.max(1)),
        tokio::net::TcpStream::connect(socket),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        _ => return (port, None),
    };
    if !banner {
        return (port, Some(String::new()));
    }
    let mut stream = stream;
    let mut buf = [0u8; 256];
    match tokio::time::timeout(
        Duration::from_millis(timeout_ms.clamp(1, 2000)),
        stream.read(&mut buf),
    )
    .await
    {
        Ok(Ok(count)) if count > 0 => {
            let text: String = String::from_utf8_lossy(&buf[..count])
                .chars()
                .filter(|c| !c.is_control() || *c == ' ')
                .take(120)
                .collect();
            let text = text.trim().to_string();
            if text.is_empty() {
                (port, Some(String::new()))
            } else {
                (port, Some(format!(" {text}")))
            }
        },
        _ => (port, Some(String::new())),
    }
}
