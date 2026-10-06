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
    cancel: CancelToken,
) -> BoxStream<'static, ToolEvent> {
    use futures::StreamExt as _;
    Box::pin(
        futures::stream::once(async move {
            let mut events = vec![ToolEvent::Started];
            for port in ports {
                if cancel.is_cancelled() {
                    events.push(ToolEvent::Cancelled);
                    break;
                }
                let address = format!("{host}:{port}");
                let result = tokio::time::timeout(
                    Duration::from_millis(timeout_ms.max(1)),
                    tokio::net::TcpStream::connect(
                        address
                            .parse::<SocketAddr>()
                            .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], port))),
                    ),
                )
                .await;
                if matches!(result, Ok(Ok(_))) {
                    events.push(ToolEvent::Output {
                        line: format!("{port}/tcp open"),
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
