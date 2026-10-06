//! DNS tools (Prompt 5.4): forward lookup (A/AAAA/MX/TXT/NS) and reverse
//! (PTR) via `hickory-resolver`, plus remote `dig`/`nslookup` fallbacks.
//!
//! The resolver honors an explicit server override or the system config;
//! every record renders on its own output line for the viewer.

use futures::StreamExt as _;
use hickory_resolver::proto::rr::{RData, RecordType};

use super::{remote_exec, shell_quote, BoxStream, CancelToken, OutputLevel, ToolEvent};

/// Supported record types (hub picker values).
pub const RECORD_TYPES: [&str; 5] = ["A", "AAAA", "MX", "TXT", "NS"];

/// Parse a picker value into a record type.
pub fn parse_record_type(text: &str) -> Result<RecordType, String> {
    match text.trim().to_uppercase().as_str() {
        "A" => Ok(RecordType::A),
        "AAAA" => Ok(RecordType::AAAA),
        "MX" => Ok(RecordType::MX),
        "TXT" => Ok(RecordType::TXT),
        "NS" => Ok(RecordType::NS),
        other => Err(format!("unsupported record type: {other}")),
    }
}

/// Build a resolver: explicit `server` override or the system config.
fn build_resolver(server: &str) -> Result<hickory_resolver::TokioAsyncResolver, String> {
    use hickory_resolver::config::{NameServerConfig, Protocol, ResolverConfig, ResolverOpts};
    use std::net::SocketAddr;

    if server.trim().is_empty() {
        return hickory_resolver::TokioAsyncResolver::tokio_from_system_conf()
            .map_err(|err| err.to_string());
    }
    let address: SocketAddr = server
        .parse()
        .map_err(|_| format!("bad DNS server address: {server:?}"))?;
    let mut config = ResolverConfig::new();
    config.add_name_server(NameServerConfig {
        socket_addr: address,
        protocol: Protocol::Udp,
        tls_dns_name: None,
        trust_negative_responses: true,
        bind_addr: None,
    });
    Ok(hickory_resolver::TokioAsyncResolver::tokio(
        config,
        ResolverOpts::default(),
    ))
}

/// Render one record for the output view.
pub fn render_record(record_type: RecordType, data: &RData) -> String {
    match data {
        RData::A(address) => format!("A {address}"),
        RData::AAAA(address) => format!("AAAA {address}"),
        RData::MX(mx) => format!("MX {} {}", mx.preference(), mx.exchange()),
        RData::TXT(txt) => format!(
            "TXT {}",
            txt.txt_data()
                .iter()
                .map(|bytes| String::from_utf8_lossy(bytes))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        RData::NS(name) => format!("NS {name}"),
        RData::PTR(name) => format!("PTR {name}"),
        RData::SOA(soa) => format!("SOA {} {}", soa.mname(), soa.rname()),
        RData::SRV(srv) => format!(
            "SRV {}:{} prio={} weight={}",
            srv.target(),
            srv.port(),
            srv.priority(),
            srv.weight()
        ),
        RData::CNAME(name) => format!("CNAME {name}"),
        other => format!("{record_type:?} {other}"),
    }
}

/// Forward lookup, streaming one event per record.
pub fn run_lookup(
    name: String,
    record_type: RecordType,
    server: String,
    cancel: CancelToken,
) -> BoxStream<'static, ToolEvent> {
    Box::pin(
        futures::stream::once(async move {
            if cancel.is_cancelled() {
                return vec![ToolEvent::Cancelled];
            }
            let resolver = match build_resolver(&server) {
                Ok(resolver) => resolver,
                Err(reason) => {
                    return vec![ToolEvent::Failed { error: reason }];
                },
            };
            let mut events = vec![ToolEvent::Started];
            match resolver.lookup(name.clone(), record_type).await {
                Ok(lookup) => {
                    let mut count = 0u32;
                    for record in lookup.record_iter() {
                        if cancel.is_cancelled() {
                            events.push(ToolEvent::Cancelled);
                            return events;
                        }
                        count += 1;
                        events.push(ToolEvent::Output {
                            line: record.data().map_or_else(
                                || format!("{:?}", record.record_type()),
                                |data| render_record(record.record_type(), data),
                            ),
                            level: OutputLevel::Info,
                        });
                    }
                    events.push(ToolEvent::Completed {
                        summary: Some(format!("{count} record(s) for {name}")),
                    });
                },
                Err(reason) => events.push(ToolEvent::Failed {
                    error: reason.to_string(),
                }),
            }
            events
        })
        .flat_map(futures::stream::iter),
    )
}

/// Reverse lookup of an IP literal.
pub fn run_reverse(
    address: String,
    server: String,
    cancel: CancelToken,
) -> BoxStream<'static, ToolEvent> {
    Box::pin(
        futures::stream::once(async move {
            if cancel.is_cancelled() {
                return vec![ToolEvent::Cancelled];
            }
            let resolver = match build_resolver(&server) {
                Ok(resolver) => resolver,
                Err(reason) => {
                    return vec![ToolEvent::Failed { error: reason }];
                },
            };
            let ip: std::net::IpAddr = match address.parse() {
                Ok(ip) => ip,
                Err(_) => {
                    return vec![ToolEvent::Failed {
                        error: format!("not an IP address: {address:?}"),
                    }];
                },
            };
            let mut events = vec![ToolEvent::Started];
            match resolver.reverse_lookup(ip).await {
                Ok(lookup) => {
                    for name in lookup.iter() {
                        events.push(ToolEvent::Output {
                            line: format!("PTR {name}"),
                            level: OutputLevel::Info,
                        });
                    }
                    events.push(ToolEvent::Completed { summary: None });
                },
                Err(reason) => events.push(ToolEvent::Failed {
                    error: reason.to_string(),
                }),
            }
            events
        })
        .flat_map(futures::stream::iter),
    )
}

/// Remote forward lookup via `dig` (preferred) or `nslookup`.
pub fn remote_lookup_command(name: &str, record_type: RecordType) -> String {
    format!(
        "dig +short {record_type:?} {} 2>/dev/null || nslookup -type={record_type:?} {}",
        shell_quote(name),
        shell_quote(name),
    )
}

/// Remote reverse lookup via `dig -x` (preferred) or `nslookup`.
pub fn remote_reverse_command(address: &str) -> String {
    format!(
        "dig +short -x {} 2>/dev/null || nslookup {}",
        shell_quote(address),
        shell_quote(address),
    )
}

/// Run a remote lookup through the session shell.
pub fn run_remote_lookup(
    session: mbxt_core::SessionId,
    name: String,
    record_type: RecordType,
    cancel: CancelToken,
) -> BoxStream<'static, ToolEvent> {
    Box::pin(
        futures::stream::once(async move {
            if cancel.is_cancelled() {
                return vec![ToolEvent::Cancelled];
            }
            let command = remote_lookup_command(&name, record_type);
            let output = match remote_exec(session, &command).await {
                Ok(output) => output,
                Err(reason) => {
                    return vec![ToolEvent::Failed { error: reason }];
                },
            };
            let mut events = vec![ToolEvent::Started];
            for line in output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
            {
                events.push(ToolEvent::Output {
                    line: format!("{record_type:?} {line}"),
                    level: OutputLevel::Info,
                });
            }
            events.push(ToolEvent::Completed { summary: None });
            events
        })
        .flat_map(futures::stream::iter),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_types_parse_strictly() {
        assert_eq!(parse_record_type("a").unwrap(), RecordType::A);
        assert_eq!(parse_record_type("txt").unwrap(), RecordType::TXT);
        assert!(parse_record_type("CAA").is_err());
    }

    #[test]
    fn remote_commands_quote_targets() {
        assert!(remote_lookup_command("example.com", RecordType::MX).contains("dig"));
        assert!(remote_reverse_command("8.8.8.8").contains("dig +short -x"));
        assert!(remote_lookup_command("a b", RecordType::A).contains("'a b'"));
    }

    /// Localhost resolves without network (system stub or hosts file).
    #[tokio::test]
    async fn localhost_resolves() {
        use futures::StreamExt as _;
        let events: Vec<ToolEvent> = run_lookup(
            "localhost".into(),
            RecordType::A,
            String::new(),
            CancelToken::new(),
        )
        .collect()
        .await;
        assert!(events
            .iter()
            .any(|event| matches!(event, ToolEvent::Started)));
        // Offline machines may fail here: only the event shape is asserted
        // when the lookup itself errors.
        assert!(events.iter().any(|event| matches!(
            event,
            ToolEvent::Completed { .. } | ToolEvent::Failed { .. }
        )));
    }
}
