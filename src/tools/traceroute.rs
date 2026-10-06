//! Traceroute tool (Prompt 5.4): hop-by-hop path discovery.
//!
//! Local path shells out to `traceroute` (falling back to `tracepath`),
//! streaming one event per hop; remote targets run the same command through
//! the session shell. Raw-socket TTL crafting would need privileges, so the
//! subprocess route is the portable default (documented).

use futures::StreamExt as _;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::{remote_exec, shell_quote, BoxStream, CancelToken, OutputLevel, ToolEvent};

/// One parsed hop: number, address (if resolved), and RTT samples.
#[derive(Debug, Clone, PartialEq)]
pub struct Hop {
    pub number: u8,
    pub address: Option<String>,
    pub rtts_ms: Vec<f64>,
}

/// Build the shell command (`traceroute` preferred, `tracepath` fallback
/// handled by the runner probing both binaries).
pub fn traceroute_command(host: &str, max_hops: u8, timeout_secs: u64) -> String {
    format!(
        "traceroute -m {} -w {} {}",
        max_hops.clamp(1, 64),
        timeout_secs.max(1),
        shell_quote(host),
    )
}

/// Parse one `traceroute` hop line (` 3  10.0.0.1  1.234 ms  1.111 ms ...`).
/// Returns `None` for headers/blank lines; `* * *` hops yield no address.
pub fn parse_hop(line: &str) -> Option<Hop> {
    let mut tokens = line.split_whitespace();
    let number: u8 = tokens.next()?.parse().ok()?;
    let mut address = None;
    let mut rtts_ms = Vec::new();
    let mut tokens = tokens.peekable();
    while let Some(token) = tokens.next() {
        if token == "*" {
            continue;
        }
        // Address tokens look like IPs or hostnames followed by RTTs; the
        // token before the first `ms` that is not `ms` itself is the address.
        if address.is_none() && !token.ends_with("ms") && token != "ms" {
            // Peek: an address is followed by a number or `*`.
            if let Some(next) = tokens.peek() {
                if next.parse::<f64>().is_ok() || *next == "*" {
                    address = Some(token.trim_matches(['(', ')']).to_string());
                    continue;
                }
            }
        }
        if let Ok(rtt) = token.parse::<f64>() {
            rtts_ms.push(rtt);
        }
    }
    Some(Hop {
        number,
        address,
        rtts_ms,
    })
}

/// Run traceroute locally, streaming one event per hop.
pub fn run_local(
    host: String,
    max_hops: u8,
    timeout_secs: u64,
    cancel: CancelToken,
) -> BoxStream<'static, ToolEvent> {
    enum State {
        Init,
        Reading {
            child: Box<tokio::process::Child>,
            lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
            hops: u32,
        },
        Done,
    }

    Box::pin(
        futures::stream::unfold(State::Init, move |state| {
            let (host, max_hops, timeout_secs, cancel) =
                (host.clone(), max_hops, timeout_secs, cancel.clone());
            async move {
                match state {
                    State::Init => {
                        let binary = if which_binary("traceroute") {
                            "traceroute"
                        } else if which_binary("tracepath") {
                            // `tracepath` takes no hop flag; max hops still
                            // bound the parse below by hop number.
                            "tracepath"
                        } else {
                            return Some((
                                ToolEvent::Failed {
                                    error: "neither traceroute nor tracepath is installed"
                                        .to_string(),
                                },
                                State::Done,
                            ));
                        };
                        let command = if binary == "tracepath" {
                            format!("tracepath {}", shell_quote(&host))
                        } else {
                            traceroute_command(&host, max_hops, timeout_secs)
                        };
                        match Command::new("sh")
                            .arg("-c")
                            .arg(&command)
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .spawn()
                        {
                            Ok(mut child) => {
                                let stdout = child.stdout.take().expect("piped above");
                                let lines = BufReader::new(stdout).lines();
                                Some((
                                    ToolEvent::Started,
                                    State::Reading {
                                        child: Box::new(child),
                                        lines,
                                        hops: 0,
                                    },
                                ))
                            },
                            Err(reason) => Some((
                                ToolEvent::Failed {
                                    error: format!("cannot start {binary}: {reason}"),
                                },
                                State::Done,
                            )),
                        }
                    },
                    State::Reading {
                        mut child,
                        mut lines,
                        mut hops,
                    } => loop {
                        if cancel.is_cancelled() {
                            let _ = child.kill().await;
                            return Some((ToolEvent::Cancelled, State::Done));
                        }
                        match tokio::time::timeout(
                            std::time::Duration::from_millis(200),
                            lines.next_line(),
                        )
                        .await
                        {
                            Ok(Ok(Some(line))) => {
                                if line.trim().is_empty()
                                    || line.starts_with("traceroute to")
                                    || line.starts_with("tracepath")
                                {
                                    continue;
                                }
                                if let Some(hop) = parse_hop(&line) {
                                    if hop.number > max_hops {
                                        let _ = child.kill().await;
                                        return Some((
                                            ToolEvent::Completed {
                                                summary: Some(format!("{hops} hops traced")),
                                            },
                                            State::Done,
                                        ));
                                    }
                                    hops += 1;
                                    let rendered = render_hop(&hop);
                                    return Some((
                                        ToolEvent::Output {
                                            line: rendered,
                                            level: OutputLevel::Info,
                                        },
                                        State::Reading { child, lines, hops },
                                    ));
                                }
                            },
                            Ok(Ok(None)) => {
                                let _ = child.wait().await;
                                return Some((
                                    ToolEvent::Completed {
                                        summary: Some(format!("{hops} hops traced")),
                                    },
                                    State::Done,
                                ));
                            },
                            Ok(Err(reason)) => {
                                return Some((
                                    ToolEvent::Failed {
                                        error: reason.to_string(),
                                    },
                                    State::Done,
                                ));
                            },
                            Err(_) => {},
                        }
                    },
                    State::Done => None,
                }
            }
        })
        .boxed(),
    )
}

/// Render one hop for the output view.
pub fn render_hop(hop: &Hop) -> String {
    let address = hop.address.as_deref().unwrap_or("*");
    if hop.rtts_ms.is_empty() {
        format!("{}  {address}  * * *", hop.number)
    } else {
        let rtts = hop
            .rtts_ms
            .iter()
            .map(|rtt| format!("{rtt:.2} ms"))
            .collect::<Vec<_>>()
            .join("  ");
        format!("{}  {address}  {rtts}", hop.number)
    }
}

/// Synchronous `PATH` probe (avoids spawning a doomed child).
fn which_binary(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths)
            .any(|dir| dir.join(name).is_file() || dir.join(format!("{name}.exe")).is_file())
    })
}

/// Run traceroute on a remote session (same command, exec channel).
pub fn run_remote(
    session: mbxt_core::SessionId,
    host: String,
    max_hops: u8,
    timeout_secs: u64,
    cancel: CancelToken,
) -> BoxStream<'static, ToolEvent> {
    Box::pin(
        futures::stream::once(async move {
            if cancel.is_cancelled() {
                return vec![ToolEvent::Cancelled];
            }
            let command = traceroute_command(&host, max_hops, timeout_secs);
            let output = match remote_exec(session, &command).await {
                Ok(output) => output,
                Err(reason) => {
                    return vec![ToolEvent::Failed { error: reason }];
                },
            };
            let mut events = vec![ToolEvent::Started];
            for line in output.lines() {
                if cancel.is_cancelled() {
                    events.push(ToolEvent::Cancelled);
                    return events;
                }
                if let Some(hop) = parse_hop(line) {
                    events.push(ToolEvent::Output {
                        line: render_hop(&hop),
                        level: OutputLevel::Info,
                    });
                }
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
    fn command_builds_bounded_flags() {
        assert_eq!(
            traceroute_command("example.com", 30, 5),
            "traceroute -m 30 -w 5 example.com"
        );
        assert!(traceroute_command("h", 200, 0).contains("-m 64"));
    }

    #[test]
    fn hops_parse_classic_and_timeout_lines() {
        let hop = parse_hop(" 3  10.0.0.1 (10.0.0.1)  1.234 ms  1.111 ms  0.987 ms").unwrap();
        assert_eq!(hop.number, 3);
        assert_eq!(hop.address.as_deref(), Some("10.0.0.1"));
        assert_eq!(hop.rtts_ms.len(), 3);
        let timeout = parse_hop(" 4  * * *").unwrap();
        assert_eq!(timeout.address, None);
        assert!(timeout.rtts_ms.is_empty());
        assert!(parse_hop("traceroute to example.com").is_none());
    }

    #[test]
    fn render_covers_timeouts() {
        let hop = Hop {
            number: 4,
            address: None,
            rtts_ms: vec![],
        };
        assert!(render_hop(&hop).contains("* * *"));
    }
}
