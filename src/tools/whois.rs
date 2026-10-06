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
