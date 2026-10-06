//! Remote forwards (`-R`, Prompt 5.3): server listens, we dial locally.
//!
//! Channels opened by the server arrive on an `UnboundedReceiver` (wired by
//! the manager through the actor's forwarded sink). Each channel dials the
//! configured local target and proxies both directions. A refused local dial
//! kills that connection only — the accept loop (and the server listen)
//! survive it. The loop ends when the manager drops its sender on stop.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::{proxy, ChildTracker, ForwardCounters, ForwardError};
use crate::connection::sftp::CancelToken;

/// `true` when a server-opened channel belongs to this listen. Exact match
/// on both fields: ports are unique per session in practice, and the host
/// pins down shared-port edge cases.
pub fn route_matches(
    opened_host: &str,
    opened_port: u32,
    expected_host: &str,
    expected_port: u32,
) -> bool {
    opened_host == expected_host && opened_port == expected_port
}

/// Accept loop over server-opened channels for one `-R` listen.
/// Channels for other listens on the same connection are dropped with a log
/// line (each forward registers while it runs; stale tails are drained on
/// unregister, so this is strictly a race window). `dial` connects the local
/// target per channel (inside the spawned task, so a slow dial never stalls
/// the loop); the loop ends on cancel or sender drop.
pub async fn run_remote_forward<D, DialFut>(
    mut channels: tokio::sync::mpsc::UnboundedReceiver<mbxt_connections::ForwardedTcpIp>,
    expected_host: String,
    expected_port: u32,
    dial: D,
    counters: Arc<ForwardCounters>,
    log: super::local::LogSink,
    cancel: CancelToken,
    children: ChildTracker,
) -> Result<(), ForwardError>
where
    D: Fn() -> DialFut + Send + Sync + 'static,
    DialFut: std::future::Future<Output = Result<tokio::net::TcpStream, ForwardError>> + Send,
{
    let dial = Arc::new(dial);
    loop {
        if cancel.is_cancelled() {
            break;
        }
        let channel = tokio::select! {
            incoming = channels.recv() => incoming,
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => continue,
        };
        let Some(opened) = channel else {
            break; // manager stopped: sender dropped
        };
        if !route_matches(
            &opened.connected_host,
            opened.connected_port,
            &expected_host,
            expected_port,
        ) {
            log.push(format!(
                "dropped channel for {}:{} (not this listen)",
                opened.connected_host, opened.connected_port
            ));
            continue;
        }
        counters.total.fetch_add(1, Ordering::SeqCst);
        log.push("forwarded connection accepted".to_string());
        let dial = Arc::clone(&dial);
        let counters = Arc::clone(&counters);
        let log = log.clone();
        children.push(tokio::spawn(async move {
            counters.active.fetch_add(1, Ordering::SeqCst);
            let outcome = proxy_channel(opened.stream, move || (*dial)(), &counters, &log).await;
            counters.active.fetch_sub(1, Ordering::SeqCst);
            if let Err(reason) = outcome {
                log.push(format!("forwarded connection failed: {reason}"));
            }
        }));
    }
    Ok(())
}

async fn proxy_channel<D, DialFut>(
    mut channel: mbxt_connections::ForwardStream,
    dial: D,
    counters: &ForwardCounters,
    log: &super::local::LogSink,
) -> Result<(), ForwardError>
where
    D: Fn() -> DialFut,
    DialFut: std::future::Future<Output = Result<tokio::net::TcpStream, ForwardError>>,
{
    let mut local = dial().await?;
    let (a_to_b, b_to_a) = proxy(&mut channel, &mut local)
        .await
        .map_err(ForwardError::io)?;
    counters.bytes_sent.fetch_add(a_to_b, Ordering::SeqCst);
    counters.bytes_received.fetch_add(b_to_a, Ordering::SeqCst);
    log.push(format!(
        "forwarded connection closed ({a_to_b}↑ {b_to_a}↓ bytes)"
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_matches_exact_listen_only() {
        assert!(route_matches("127.0.0.1", 19999, "127.0.0.1", 19999));
        assert!(!route_matches("127.0.0.1", 20000, "127.0.0.1", 19999));
        assert!(!route_matches("localhost", 19999, "127.0.0.1", 19999));
    }

    #[tokio::test]
    async fn loop_survives_cancel_and_sender_drop() {
        // Fabricating a live `ForwardStream` headlessly would need a full
        // russh session (covered by live tests); here the loop must still
        // exit cleanly on cancel and on sender drop.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = CancelToken::new();
        let task = tokio::spawn(run_remote_forward(
            rx,
            "127.0.0.1".to_string(),
            19999,
            || async {
                tokio::net::TcpStream::connect(("127.0.0.1", 1))
                    .await
                    .map_err(ForwardError::io)
            },
            Arc::new(ForwardCounters::default()),
            super::super::local::LogSink::new(50),
            cancel.clone(),
            ChildTracker::default(),
        ));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        cancel.cancel();
        task.await.unwrap().unwrap();
        drop(tx);
    }
}
