//! Bandwidth-test command construction.

use super::shell_quote;

pub fn command(mode: &str, host: &str, port: u16, seconds: u64) -> String {
    match mode.to_ascii_lowercase().as_str() {
        "receive" | "server" => format!("iperf3 -s -p {}", port),
        _ => format!(
            "iperf3 -c {} -p {} -t {}",
            shell_quote(host),
            port,
            seconds.max(1)
        ),
    }
}
