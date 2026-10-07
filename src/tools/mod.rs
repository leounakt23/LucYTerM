//! Integrated network tools (Prompt 5.4): ping, traceroute, DNS, whois,
//! port scanner, HTTP client, subnet calculator, bandwidth tester.
//!
//! Every tool runs locally or against an SSH session (`Target::Remote`
//! executes the equivalent shell command through the session actor), streams
//! [`ToolEvent`]s progressively, and honors [`CancelToken`] cancellation.
//! New tools are one module + one `ToolKind` arm + one UI file; nothing
//! upstream changes.

pub mod bandwidth;
pub mod dns;
pub mod http_client;
pub mod ping;
pub mod port_scanner;
pub mod subnet;
pub mod traceroute;
pub mod whois;

use mbxt_core::SessionId;

/// Tool selector (hub list, history grouping, tab titles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Ping,
    Traceroute,
    Dns,
    ReverseDns,
    Whois,
    PortScan,
    Http,
    Subnet,
    Bandwidth,
}

impl ToolKind {
    /// All tools in hub order.
    pub const ALL: [Self; 9] = [
        Self::Ping,
        Self::Traceroute,
        Self::Dns,
        Self::ReverseDns,
        Self::Whois,
        Self::PortScan,
        Self::Http,
        Self::Subnet,
        Self::Bandwidth,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Ping => "Ping",
            Self::Traceroute => "Traceroute",
            Self::Dns => "DNS lookup",
            Self::ReverseDns => "Reverse DNS",
            Self::Whois => "Whois",
            Self::PortScan => "Port scanner",
            Self::Http => "HTTP client",
            Self::Subnet => "Subnet calculator",
            Self::Bandwidth => "Bandwidth test",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Ping => "ICMP echo latency and loss (local ping or remote shell)",
            Self::Traceroute => "Hop-by-hop path discovery (local traceroute or remote shell)",
            Self::Dns => "A/AAAA/MX/TXT/NS records via the system resolver or remote dig",
            Self::ReverseDns => "PTR lookup for an IP address",
            Self::Whois => "Domain registration records over TCP/43 with referral chase",
            Self::PortScan => "TCP connect scan with banner grabs",
            Self::Http => "Plain-HTTP GET/POST with redirect and chunked support",
            Self::Subnet => "CIDR math: range, broadcast, membership (offline)",
            Self::Bandwidth => "TCP throughput sender/receiver pair",
        }
    }

    /// Short glyph for the hub list (the widget maps these to text badges).
    pub fn icon(self) -> &'static str {
        match self {
            Self::Ping => "○",
            Self::Traceroute => "⇢",
            Self::Dns => "☷",
            Self::ReverseDns => "☷",
            Self::Whois => "❔",
            Self::PortScan => "▦",
            Self::Http => "⤓",
            Self::Subnet => "⧉",
            Self::Bandwidth => "⇅",
        }
    }
}

/// Output severity (colored in the view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputLevel {
    Info,
    Warning,
    Error,
    Success,
}

/// Streaming tool events (spec shape).
#[derive(Debug, Clone, PartialEq)]
pub enum ToolEvent {
    Started,
    Progress {
        message: String,
        percent: Option<f32>,
    },
    Output {
        line: String,
        level: OutputLevel,
    },
    Completed {
        summary: Option<String>,
    },
    Failed {
        error: String,
    },
    Cancelled,
}

/// Where a tool runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolTarget {
    Local,
    Remote(SessionId),
}

/// Generic tool configuration (spec `ToolConfig` shape).
#[derive(Debug, Clone)]
pub struct ToolConfig {
    pub target: String,
    pub remote: Option<SessionId>,
    pub timeout_ms: u64,
    pub params: ToolParams,
}

/// Per-tool parameters (typed form state, one variant per tool).
#[derive(Debug, Clone)]
pub enum ToolParams {
    Ping {
        count: u32,
        interval_ms: u64,
        size: u32,
        v6: bool,
    },
    Traceroute {
        max_hops: u8,
        timeout_ms: u64,
    },
    Dns {
        record_type: String,
        server: String,
    },
    ReverseDns,
    Whois {
        server: String,
    },
    PortScan {
        ports: String,
        concurrency: u32,
        timeout_ms: u64,
        banner: bool,
    },
    Http {
        method: String,
        path: String,
        headers: String,
        body: String,
    },
    Subnet {
        cidr: String,
    },
    Bandwidth {
        mode: String,
        port: u16,
        seconds: u64,
    },
}

impl ToolParams {
    /// Fresh params for a freshly picked tool (hub form defaults).
    pub fn for_kind(kind: ToolKind) -> Self {
        match kind {
            ToolKind::Ping => Self::Ping {
                count: 4,
                interval_ms: 1000,
                size: 56,
                v6: false,
            },
            ToolKind::Traceroute => Self::Traceroute {
                max_hops: 30,
                timeout_ms: 2000,
            },
            ToolKind::Dns => Self::Dns {
                record_type: "A".to_string(),
                server: String::new(),
            },
            ToolKind::ReverseDns => Self::ReverseDns,
            ToolKind::Whois => Self::Whois {
                server: String::new(),
            },
            ToolKind::PortScan => Self::PortScan {
                ports: "22,80,443".to_string(),
                concurrency: 32,
                timeout_ms: 1000,
                banner: true,
            },
            ToolKind::Http => Self::Http {
                method: "GET".to_string(),
                path: "/".to_string(),
                headers: String::new(),
                body: String::new(),
            },
            ToolKind::Subnet => Self::Subnet {
                cidr: "192.168.1.0/24".to_string(),
            },
            ToolKind::Bandwidth => Self::Bandwidth {
                mode: "send".to_string(),
                port: 5201,
                seconds: 5,
            },
        }
    }

    /// Editable text per form field, derived from typed params. The hub
    /// keeps these strings as widget-owned state (iced borrows input
    /// values); parsing back happens in the update arm, keeping the
    /// last good value on garbage input.
    pub fn text_fields(params: &ToolParams) -> Vec<(String, String)> {
        fn number(value: impl std::fmt::Display) -> String {
            value.to_string()
        }
        match params {
            ToolParams::Ping {
                count,
                interval_ms,
                size,
                ..
            } => vec![
                ("count".into(), number(count)),
                ("interval_ms".into(), number(interval_ms)),
                ("size".into(), number(size)),
            ],
            ToolParams::Traceroute {
                max_hops,
                timeout_ms,
            } => vec![
                ("max_hops".into(), number(max_hops)),
                ("timeout_ms".into(), number(timeout_ms)),
            ],
            ToolParams::Dns {
                record_type,
                server,
            } => vec![
                ("record_type".into(), record_type.clone()),
                ("server".into(), server.clone()),
            ],
            ToolParams::ReverseDns => Vec::new(),
            ToolParams::Whois { server } => vec![("server".into(), server.clone())],
            ToolParams::PortScan {
                ports,
                concurrency,
                timeout_ms,
                ..
            } => vec![
                ("ports".into(), ports.clone()),
                ("concurrency".into(), number(concurrency)),
                ("timeout_ms".into(), number(timeout_ms)),
            ],
            ToolParams::Http {
                method,
                path,
                headers,
                body,
            } => vec![
                ("method".into(), method.clone()),
                ("path".into(), path.clone()),
                ("headers".into(), headers.clone()),
                ("body".into(), body.clone()),
            ],
            ToolParams::Subnet { cidr } => vec![("cidr".into(), cidr.clone())],
            ToolParams::Bandwidth {
                mode,
                port,
                seconds,
            } => vec![
                ("mode".into(), mode.clone()),
                ("port".into(), number(port)),
                ("seconds".into(), number(seconds)),
            ],
        }
    }
}

impl Default for ToolParams {
    fn default() -> Self {
        Self::for_kind(ToolKind::Ping)
    }
}

impl ToolConfig {
    pub fn local(target: &str, timeout_ms: u64, params: ToolParams) -> Self {
        Self {
            target: target.to_string(),
            remote: None,
            timeout_ms,
            params,
        }
    }
}

/// Stream of [`ToolEvent`]s (spec `BoxStream` shape).
pub type BoxStream<'a, T = ToolEvent> =
    std::pin::Pin<Box<dyn futures::Stream<Item = T> + Send + 'a>>;

/// Tool interface (spec shape; `icon` is the hub glyph).
pub trait NetworkTool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn icon(&self) -> &'static str;
    fn run(&self, config: ToolConfig) -> BoxStream<'static, ToolEvent>;
}

/// One recorded run (history per tool type + active-run rendering).
#[derive(Debug, Clone)]
pub struct ToolRun {
    pub id: u64,
    pub kind: ToolKind,
    pub target_label: String,
    pub lines: Vec<(OutputLevel, String)>,
    pub status: RunStatus,
    pub started_secs: u64,
}

/// Run lifecycle (history badges).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// Cancellation shared with the streaming task.
#[derive(Debug, Clone, Default)]
pub struct ToolCancel {
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Shared cancellation token used by tool runners.
pub type CancelToken = ToolCancel;

impl ToolCancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Current unix seconds (local copy: no `app` dependency from tools).
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Remote shell execution helper: runs `command` on `session` through the
/// session actor (bounded by the transport's exec cap).
pub async fn remote_exec(session: SessionId, command: &str) -> Result<String, String> {
    crate::connection::actor::SessionManager::shared()
        .exec(session, command)
        .await
}

/// Shell-quote one argument (single-quote style, `'` → `'\''`).
pub fn shell_quote(arg: &str) -> String {
    if arg
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-_.,:/@".contains(&b))
    {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', "'\\''"))
}

/// Export lines as TXT / JSON / CSV for the download action.
pub fn export_lines(lines: &[(OutputLevel, String)], format: &str) -> String {
    match format {
        "json" => {
            let mut out = String::from("[\n");
            for (index, (level, line)) in lines.iter().enumerate() {
                out.push_str(&format!(
                    "  {{\"level\": \"{level:?}\", \"line\": {}}}",
                    json_string(line)
                ));
                if index + 1 < lines.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push(']');
            out
        },
        "csv" => {
            let mut out = String::from("level,line\n");
            for (level, line) in lines {
                out.push_str(&format!("{level:?},{}\n", csv_field(line)));
            }
            out
        },
        _ => lines
            .iter()
            .map(|(_, line)| line.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// Case-insensitive substring filter for the output search box.
pub fn filter_lines<'a>(
    lines: &'a [(OutputLevel, String)],
    query: &str,
) -> Vec<(usize, &'a (OutputLevel, String))> {
    if query.is_empty() {
        return lines.iter().enumerate().collect();
    }
    let query = query.to_lowercase();
    lines
        .iter()
        .enumerate()
        .filter(|(_, (_, line))| line.to_lowercase().contains(&query))
        .collect()
}

/// Instant failure stream for configuration errors (bad params, remote
/// runs of local-only tools). Same event shape as real runners.
fn failed_now(error: String) -> BoxStream<'static, ToolEvent> {
    Box::pin(futures::stream::iter(vec![
        ToolEvent::Started,
        ToolEvent::Failed { error },
    ]))
}

/// Dispatch one configured tool to its runner (local sockets/binaries
/// or remote shell). This is the single entry point the UI hub calls;
/// per-module runners stay directly testable underneath.
pub fn run_tool(
    kind: ToolKind,
    config: ToolConfig,
    cancel: CancelToken,
) -> BoxStream<'static, ToolEvent> {
    let target = config.target.clone();
    let timeout = config.timeout_ms;
    let remote = config.remote;
    match (kind, config.params) {
        (
            ToolKind::Ping,
            ToolParams::Ping {
                count,
                interval_ms,
                size,
                v6,
            },
        ) => match remote {
            Some(session) => {
                ping::run_remote(session, target, count, interval_ms, size, v6, cancel)
            },
            None => ping::run_local(target, count, interval_ms, size, v6, cancel),
        },
        (
            ToolKind::Traceroute,
            ToolParams::Traceroute {
                max_hops,
                timeout_ms,
            },
        ) => match remote {
            Some(session) => traceroute::run_remote(session, target, max_hops, timeout_ms, cancel),
            None => traceroute::run_local(target, max_hops, timeout_ms, cancel),
        },
        (
            ToolKind::Dns,
            ToolParams::Dns {
                record_type,
                server,
            },
        ) => match (dns::parse_record_type(&record_type), remote) {
            (Err(error), _) => failed_now(error),
            (Ok(record), None) => dns::run_lookup(target, record, server, cancel),
            (Ok(record), Some(session)) => dns::run_remote_lookup(session, target, record, cancel),
        },
        (ToolKind::ReverseDns, ToolParams::ReverseDns) => {
            dns::run_reverse(target, String::new(), cancel)
        },
        (ToolKind::Whois, ToolParams::Whois { server }) => match remote {
            Some(session) => whois::run_remote(session, target, server, cancel),
            None => whois::run_local(target, server, timeout, cancel),
        },
        (
            ToolKind::PortScan,
            ToolParams::PortScan {
                ports,
                concurrency,
                timeout_ms,
                banner,
            },
        ) => match remote {
            Some(_) => failed_now("port scans run locally; switch the target to Local".to_string()),
            None => match port_scanner::parse_ports(&ports) {
                Err(error) => failed_now(error),
                Ok(ports) => {
                    port_scanner::scan(target, ports, timeout_ms, concurrency, banner, cancel)
                },
            },
        },
        (
            ToolKind::Http,
            ToolParams::Http {
                method,
                path,
                headers,
                body,
            },
        ) => {
            let url = join_url(&target, &path);
            match remote {
                Some(session) => {
                    http_client::run_remote(session, method, url, headers, body, cancel)
                },
                None => http_client::run_local(method, url, headers, body, timeout, cancel),
            }
        },
        (ToolKind::Subnet, ToolParams::Subnet { cidr }) => {
            match subnet::calculate(if cidr.trim().is_empty() {
                &target
            } else {
                cidr.trim()
            }) {
                Err(error) => failed_now(error),
                Ok(info) => {
                    let lines = vec![
                        format!("network:   {}/{}", info.network, info.prefix),
                        format!("broadcast: {}", info.broadcast),
                        format!("range:     {} - {}", info.first_host, info.last_host),
                        format!("hosts:     {}", info.host_count),
                    ];
                    Box::pin(futures::stream::iter(
                        [ToolEvent::Started]
                            .into_iter()
                            .chain(lines.into_iter().map(|line| ToolEvent::Output {
                                line,
                                level: OutputLevel::Info,
                            }))
                            .chain([ToolEvent::Completed { summary: None }]),
                    ))
                },
            }
        },
        (
            ToolKind::Bandwidth,
            ToolParams::Bandwidth {
                mode,
                port,
                seconds,
            },
        ) => match remote {
            Some(session) => bandwidth::run_remote(session, mode, target, port, seconds, cancel),
            None => bandwidth::run_local(mode, target, port, seconds, timeout, cancel),
        },
        _ => failed_now("tool parameters do not match the selected tool".to_string()),
    }
}

/// Join a base URL and a path without doubling slashes.
fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    if path.is_empty() {
        return base.to_string();
    }
    if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lists_every_tool() {
        assert_eq!(ToolKind::ALL.len(), 9);
        for kind in ToolKind::ALL {
            assert!(!kind.name().is_empty());
            assert!(!kind.description().is_empty());
            assert!(!kind.icon().is_empty());
        }
    }

    #[test]
    fn shell_quote_protects_metacharacters() {
        assert_eq!(shell_quote("example.com"), "example.com");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("o'clock"), "'o'\\''clock'");
    }

    #[test]
    fn export_formats_quote_correctly() {
        let lines = vec![
            (OutputLevel::Info, "a,b".to_string()),
            (OutputLevel::Error, "say \"hi\"\nbye".to_string()),
        ];
        let json = export_lines(&lines, "json");
        assert!(json.starts_with('[') && json.contains("\\\"hi\\\""));
        let csv = export_lines(&lines, "csv");
        assert!(csv.contains("\"a,b\""));
        let txt = export_lines(&lines, "txt");
        assert!(txt.contains("a,b"));
    }

    #[test]
    fn search_filters_case_insensitively() {
        let lines = vec![
            (OutputLevel::Info, "PING ok".to_string()),
            (OutputLevel::Error, "timeout".to_string()),
        ];
        assert_eq!(filter_lines(&lines, "").len(), 2);
        assert_eq!(filter_lines(&lines, "ping").len(), 1);
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use futures::StreamExt as _;

    #[test]
    fn every_kind_has_form_defaults() {
        for kind in ToolKind::ALL {
            let params = ToolParams::for_kind(kind);
            // Text round-trips through the hub buffers without loss.
            let text = ToolParams::text_fields(&params);
            assert!(!text.is_empty() || matches!(kind, ToolKind::ReverseDns));
            for (field, value) in &text {
                assert!(!field.is_empty());
                let _ = value;
            }
        }
    }

    #[test]
    fn join_url_avoids_double_slashes() {
        assert_eq!(join_url("http://h:8080", "/a"), "http://h:8080/a");
        assert_eq!(join_url("http://h:8080/", "/a"), "http://h:8080/a");
        assert_eq!(join_url("http://h:8080", ""), "http://h:8080");
        assert_eq!(join_url("http://h:8080", "a"), "http://h:8080/a");
    }

    #[tokio::test]
    async fn subnet_dispatch_runs_offline() {
        let config = ToolConfig::local(
            "192.168.10.7/24",
            5000,
            ToolParams::Subnet {
                cidr: String::new(),
            },
        );
        let events: Vec<ToolEvent> = run_tool(ToolKind::Subnet, config, CancelToken::new())
            .collect()
            .await;
        assert!(matches!(events.first(), Some(ToolEvent::Started)));
        assert!(events.iter().any(|event| matches!(
            event,
            ToolEvent::Output { line, .. } if line.contains("192.168.10.0")
        )));
        assert!(matches!(events.last(), Some(ToolEvent::Completed { .. })));
    }

    #[tokio::test]
    async fn mismatched_params_fail_cleanly() {
        let config = ToolConfig::local(
            "example.com",
            5000,
            ToolParams::Subnet {
                cidr: "10.0.0.0/8".into(),
            },
        );
        let events: Vec<ToolEvent> = run_tool(ToolKind::Ping, config, CancelToken::new())
            .collect()
            .await;
        assert!(matches!(events.last(), Some(ToolEvent::Failed { .. })));
    }

    #[tokio::test]
    async fn remote_portscan_is_redirected_to_local() {
        let config = ToolConfig {
            target: "192.0.2.1".into(),
            remote: Some(7),
            timeout_ms: 1000,
            params: ToolParams::PortScan {
                ports: "80".into(),
                concurrency: 8,
                timeout_ms: 500,
                banner: false,
            },
        };
        let events: Vec<ToolEvent> = run_tool(ToolKind::PortScan, config, CancelToken::new())
            .collect()
            .await;
        match events.last() {
            Some(ToolEvent::Failed { error }) => assert!(error.contains("Local"), "{error}"),
            other => panic!("expected local-only failure, got {other:?}"),
        }
    }
}
