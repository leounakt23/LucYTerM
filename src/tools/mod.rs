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

impl Default for ToolParams {
    fn default() -> Self {
        Self::Ping {
            count: 4,
            interval_ms: 1000,
            size: 56,
            v6: false,
        }
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
