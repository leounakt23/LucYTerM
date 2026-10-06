//! HTTP command construction for local or remote execution.

use super::shell_quote;

pub fn command(method: &str, url: &str, headers: &str, body: &str) -> String {
    let mut command = format!(
        "curl -L --fail --silent --show-error -X {}",
        shell_quote(method)
    );
    for header in headers
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        command.push_str(&format!(" -H {}", shell_quote(header)));
    }
    if !body.is_empty() {
        command.push_str(&format!(" --data {}", shell_quote(body)));
    }
    command.push_str(&format!(" {}", shell_quote(url)));
    command
}
