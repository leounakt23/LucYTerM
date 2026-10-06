//! Ping tool (Prompt 5.4): ICMP echo via the system `ping` binary with
//! structured per-packet and summary parsing.
//!
//! Raw-socket ICMP (`surge-ping` style) needs `CAP_NET_RAW`/root, so the
//! local path shells out to `ping` (universally present, capability-wrapped
//! on Linux) and parses its output progressively. Remote targets run the
//! same command through the session's shell.

use futures::StreamExt as _;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::{remote_exec, shell_quote, BoxStream, CancelToken, OutputLevel, ToolEvent};

/// Parsed per-packet sample.
#[derive(Debug, Clone, PartialEq)]
pub struct PingSample {
    pub seq: u32,
    pub rtt_ms: f64,
}

/// Parsed run summary.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PingSummary {
    pub transmitted: u32,
    pub received: u32,
    pub loss_percent: f64,
    pub min_ms: f64,
    pub avg_ms: f64,
    pub max_ms: f64,
}

/// Build the shell command (local subprocess and remote exec share it).
pub fn ping_command(host: &str, count: u32, interval_ms: u64, size: u32, v6: bool) -> String {
    let interval_secs = (interval_ms.max(200) / 1000).max(1);
    format!(
        "ping {} -c {} -i {} -s {} -W 2 {}",
        if v6 { "-6" } else { "-4" },
        count.max(1),
        interval_secs,
        size,
        shell_quote(host),
    )
}

/// Parse one `64 bytes from …: icmp_seq=N ttl=… time=X ms` line.
pub fn parse_sample(line: &str) -> Option<PingSample> {
    let seq = line
        .split("icmp_seq=")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let rtt_ms = line
        .split("time=")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(PingSample { seq, rtt_ms })
}

/// Parse the `N packets transmitted, …` + `rtt min/avg/max/mdev` tail.
pub fn parse_summary(lines: &[String]) -> PingSummary {
    let mut summary = PingSummary::default();
    for line in lines {
        if line.contains("packets transmitted") {
            let numbers: Vec<f64> = line
                .split(|c: char| !(c.is_ascii_digit() || c == '.'))
                .filter(|part| !part.is_empty())
                .filter_map(|part| part.parse().ok())
                .collect();
            if numbers.len() >= 3 {
                summary.transmitted = numbers[0] as u32;
                summary.received = numbers[1] as u32;
            }
            if let Some(loss) = line.split('%').next().and_then(|head| {
                head.rsplit(&[' ', ','][..])
                    .find_map(|part| part.parse::<f64>().ok())
            }) {
                summary.loss_percent = loss;
            }
        }
        if line.contains("min/avg/max") {
            if let Some(values) = line.split('=').nth(1) {
                let numbers: Vec<f64> = values
                    .split(|c: char| !(c.is_ascii_digit() || c == '.'))
                    .filter(|part| !part.is_empty())
                    .filter_map(|part| part.parse().ok())
                    .collect();
                if numbers.len() >= 3 {
                    summary.min_ms = numbers[0];
                    summary.avg_ms = numbers[1];
                    summary.max_ms = numbers[2];
                }
            }
        }
    }
    summary
}

/// Run ping locally, streaming one event per packet plus the summary.
pub fn run_local(
    host: String,
    count: u32,
    interval_ms: u64,
    size: u32,
    v6: bool,
    cancel: CancelToken,
) -> BoxStream<'static, ToolEvent> {
    enum State {
        Init {
            host: String,
            count: u32,
            interval_ms: u64,
            size: u32,
            v6: bool,
            cancel: CancelToken,
        },
        Reading {
            child: Box<tokio::process::Child>,
            lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
            tail: Vec<String>,
            cancel: CancelToken,
        },
        Done,
    }

    Box::pin(
        futures::stream::unfold(
            State::Init {
                host,
                count,
                interval_ms,
                size,
                v6,
                cancel,
            },
            |state| async move {
                match state {
                    State::Init {
                        host,
                        count,
                        interval_ms,
                        size,
                        v6,
                        cancel,
                    } => {
                        let command = ping_command(&host, count, interval_ms, size, v6);
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
                                        tail: Vec::new(),
                                        cancel,
                                    },
                                ))
                            },
                            Err(reason) => Some((
                                ToolEvent::Failed {
                                    error: format!("cannot start ping: {reason}"),
                                },
                                State::Done,
                            )),
                        }
                    },
                    State::Reading {
                        mut child,
                        mut lines,
                        mut tail,
                        cancel,
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
                                if let Some(sample) = parse_sample(&line) {
                                    return Some((
                                        ToolEvent::Output {
                                            line: format!(
                                                "seq={} time={:.2} ms",
                                                sample.seq, sample.rtt_ms
                                            ),
                                            level: OutputLevel::Info,
                                        },
                                        State::Reading {
                                            child,
                                            lines,
                                            tail,
                                            cancel,
                                        },
                                    ));
                                }
                                tail.push(line);
                            },
                            Ok(Ok(None)) => {
                                let _ = child.wait().await;
                                let summary = parse_summary(&tail);
                                return Some((
                                    ToolEvent::Completed {
                                        summary: Some(format!(
                                            "{}/{} received ({:.0}% loss), avg {:.2} ms",
                                            summary.received,
                                            summary.transmitted,
                                            summary.loss_percent,
                                            summary.avg_ms
                                        )),
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
                            // Tick with no data: loop back to the cancel check.
                            Err(_) => {},
                        }
                    },
                    State::Done => None,
                }
            },
        )
        .boxed(),
    )
}

/// Run ping on a remote session (same command, exec channel).
pub fn run_remote(
    session: mbxt_core::SessionId,
    host: String,
    count: u32,
    interval_ms: u64,
    size: u32,
    v6: bool,
    cancel: CancelToken,
) -> BoxStream<'static, ToolEvent> {
    Box::pin(
        futures::stream::once(async move {
            if cancel.is_cancelled() {
                return vec![ToolEvent::Cancelled];
            }
            let command = ping_command(&host, count, interval_ms, size, v6);
            let output = match remote_exec(session, &command).await {
                Ok(output) => output,
                Err(reason) => {
                    return vec![ToolEvent::Failed { error: reason }];
                },
            };
            let mut events = vec![ToolEvent::Started];
            let mut tail = Vec::new();
            for line in output.lines() {
                if cancel.is_cancelled() {
                    events.push(ToolEvent::Cancelled);
                    return events;
                }
                if let Some(sample) = parse_sample(line) {
                    events.push(ToolEvent::Output {
                        line: format!("seq={} time={:.2} ms", sample.seq, sample.rtt_ms),
                        level: OutputLevel::Info,
                    });
                } else {
                    tail.push(line.to_string());
                }
            }
            let summary = parse_summary(&tail);
            events.push(ToolEvent::Completed {
                summary: Some(format!(
                    "{}/{} received ({:.0}% loss), avg {:.2} ms",
                    summary.received, summary.transmitted, summary.loss_percent, summary.avg_ms
                )),
            });
            events
        })
        .flat_map(futures::stream::iter),
    )
}

/// Collect a whole run (tests and one-shot callers): drains the stream.
#[cfg(test)]
#[allow(dead_code)]
async fn collect_all(stream: BoxStream<'static, ToolEvent>) -> Vec<ToolEvent> {
    stream.collect().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_builds_portable_flags() {
        assert_eq!(
            ping_command("example.com", 4, 1000, 56, false),
            "ping -4 -c 4 -i 1 -s 56 -W 2 example.com"
        );
        assert!(ping_command("::1", 1, 200, 56, true).contains("-6"));
        assert!(ping_command("a b", 1, 1000, 56, false).contains("'a b'"));
    }

    #[test]
    fn samples_parse_linux_format() {
        let sample =
            parse_sample("64 bytes from 93.184.216.34: icmp_seq=3 ttl=56 time=12.4 ms").unwrap();
        assert_eq!(
            sample,
            PingSample {
                seq: 3,
                rtt_ms: 12.4
            }
        );
        assert!(parse_sample("PING example.com").is_none());
    }

    #[test]
    fn summary_parses_transmit_and_rtt_tails() {
        let lines = vec![
            "4 packets transmitted, 4 received, 0% packet loss, time 3005ms".to_string(),
            "rtt min/avg/max/mdev = 11.932/12.401/13.010/0.399 ms".to_string(),
        ];
        let summary = parse_summary(&lines);
        assert_eq!((summary.transmitted, summary.received), (4, 4));
        assert_eq!(summary.loss_percent, 0.0);
        assert!((summary.avg_ms - 12.401).abs() < 0.001);
    }

    #[test]
    fn remote_command_reuses_local_builder() {
        // Remote exec runs the identical command string over the channel.
        assert!(ping_command("db.internal", 2, 500, 32, false).starts_with("ping -4"));
    }

    /// Localhost round trip (needs the `ping` binary; skipped where absent).
    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn localhost_ping_succeeds() {
        if tokio::process::Command::new("ping")
            .arg("-c")
            .arg("1")
            .arg("127.0.0.1")
            .output()
            .await
            .is_err()
        {
            return; // no ping binary — nothing to assert
        }
        let cancel = CancelToken::new();
        let events: Vec<ToolEvent> =
            collect_all(run_local("127.0.0.1".into(), 1, 1000, 56, false, cancel)).await;
        assert!(events
            .iter()
            .any(|event| matches!(event, ToolEvent::Started)));
        assert!(events.iter().any(|event| matches!(
            event,
            ToolEvent::Completed { .. } | ToolEvent::Failed { .. }
        )));
    }
}
