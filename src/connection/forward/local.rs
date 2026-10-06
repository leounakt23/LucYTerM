//! Local forwards (`-L`, Prompt 5.3): listen locally, dial remotely.
//!
//! Each accepted connection opens a `direct-tcpip` channel through the
//! provided opener and proxies both directions with live byte metering.
//! Dial failures kill that connection only — the listener survives for the
//! next client (one bad backend must not take down the tunnel).

use std::future::Future;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::net::{TcpListener, TcpStream};

use super::{proxy, ChildTracker, ForwardCounters, ForwardError};
use crate::connection::sftp::CancelToken;

/// Bind `host:port`, falling through `port+1..` on `AddrInUse` (up to
/// `MAX_PORT_TRIES` successors). Returns the listener and the actual port —
/// the UI offers this port when the requested one was taken.
pub const MAX_PORT_TRIES: u16 = 10;

pub async fn bind_with_fallback(host: &str, port: u16) -> Result<(TcpListener, u16), ForwardError> {
    // `port == 0` means "any free port": report the OS-picked one.
    if port == 0 {
        let listener = TcpListener::bind((host, 0))
            .await
            .map_err(ForwardError::io)?;
        let actual = listener.local_addr().map_err(ForwardError::io)?.port();
        return Ok((listener, actual));
    }
    let mut last_error = String::new();
    for candidate in port..port.saturating_add(MAX_PORT_TRIES) {
        match TcpListener::bind((host, candidate)).await {
            Ok(listener) => return Ok((listener, candidate)),
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                last_error = err.to_string();
                continue;
            },
            Err(err) => return Err(ForwardError::io(err)),
        }
    }
    Err(ForwardError::Invalid(format!(
        "port {port} is in use (tried {MAX_PORT_TRIES} successors): {last_error}"
    )))
}

/// Accept loop: each peer gets a channeled proxy task. Runs until `cancel`
/// fires or the listener errors; children are tracked for abort-on-stop so
/// neither sockets nor tasks leak.
pub async fn run_local_forward<O, OpenFut, Target>(
    listener: TcpListener,
    open: O,
    counters: Arc<ForwardCounters>,
    log: LogSink,
    cancel: CancelToken,
    children: ChildTracker,
) -> Result<(), ForwardError>
where
    O: Fn() -> OpenFut + Send + Sync + 'static,
    OpenFut: Future<Output = Result<Target, ForwardError>> + Send,
    Target: super::ForwardIo + 'static,
{
    let open = Arc::new(open);
    loop {
        if cancel.is_cancelled() {
            break;
        }
        let accept = tokio::time::timeout(std::time::Duration::from_millis(200), listener.accept());
        let (socket, peer) = match accept.await {
            Ok(Ok((socket, peer))) => (socket, peer),
            Ok(Err(_)) => break, // listener closed
            Err(_) => continue,  // tick: re-check cancellation
        };
        counters.total.fetch_add(1, Ordering::SeqCst);
        log.push(format!("{peer} connected"));
        let open = Arc::clone(&open);
        let counters = Arc::clone(&counters);
        let log = log.clone();
        children.push(tokio::spawn(async move {
            counters.active.fetch_add(1, Ordering::SeqCst);
            let outcome =
                proxy_one(socket, peer.to_string(), move || (*open)(), &counters, &log).await;
            counters.active.fetch_sub(1, Ordering::SeqCst);
            if let Err(reason) = outcome {
                log.push(format!("{peer} failed: {reason}"));
            }
        }));
    }
    Ok(())
}

async fn proxy_one<O, OpenFut, Target>(
    socket: TcpStream,
    peer: String,
    open: O,
    counters: &ForwardCounters,
    log: &LogSink,
) -> Result<(), ForwardError>
where
    O: Fn() -> OpenFut,
    OpenFut: Future<Output = Result<Target, ForwardError>>,
    Target: super::ForwardIo,
{
    let mut target = open().await?;
    let mut socket = socket;
    // Totals land on the shared counters at close (live per-poll metering
    // is available via `Metered` when a view needs sub-connection updates).
    let (a_to_b, b_to_a) = proxy(&mut socket, &mut target)
        .await
        .map_err(ForwardError::io)?;
    counters.bytes_sent.fetch_add(a_to_b, Ordering::SeqCst);
    counters.bytes_received.fetch_add(b_to_a, Ordering::SeqCst);
    log.push(format!("{peer} closed ({a_to_b}↑ {b_to_a}↓ bytes)"));
    Ok(())
}

/// Bounded traffic log shared with the UI (`LogSink::push` drops the oldest
/// past the cap so a busy tunnel cannot grow memory).
#[derive(Debug, Clone, Default)]
pub struct LogSink {
    lines: Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
    capacity: usize,
}

impl LogSink {
    pub fn new(capacity: usize) -> Self {
        Self {
            lines: Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
            capacity,
        }
    }

    pub fn push(&self, line: String) {
        if let Ok(mut lines) = self.lines.lock() {
            while lines.len() >= self.capacity.max(1) {
                lines.pop_front();
            }
            lines.push_back(line);
        }
    }

    pub fn tail(&self, count: usize) -> Vec<String> {
        self.lines
            .lock()
            .map(|lines| {
                lines
                    .iter()
                    .rev()
                    .take(count)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fallback_rolls_forward_on_conflict() {
        let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = first.local_addr().unwrap().port();
        let (_listener, actual) = bind_with_fallback("127.0.0.1", port).await.unwrap();
        // Unix refuses the double bind (rolls forward); Windows
        // SO_REUSEADDR semantics may rebind the same port. Either way the
        // reported port must really listen (parallel tests also bind here,
        // so no exact-successor assertion).
        assert!(actual >= port);
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            TcpStream::connect(("127.0.0.1", actual)),
        )
        .await
        .expect("connect does not hang")
        .expect("reported port listens");
        drop(first);
    }

    #[tokio::test]
    async fn wildcard_port_reports_the_picked_one() {
        let (_listener, actual) = bind_with_fallback("127.0.0.1", 0).await.unwrap();
        assert_ne!(actual, 0, "OS-picked port is reported");
    }

    #[tokio::test]
    async fn local_forward_proxies_through_opener() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (listener, port) = bind_with_fallback("127.0.0.1", 0).await.unwrap();
        let counters = Arc::new(ForwardCounters::default());
        let log = LogSink::new(50);
        let cancel = CancelToken::new();
        let children = ChildTracker::default();

        // Opener = loopback echo duplex (stands in for the SSH channel).
        let task = tokio::spawn(run_local_forward(
            listener,
            || async {
                let (a, b) = tokio::io::duplex(65536);
                tokio::spawn(async move {
                    let (mut reader, mut writer) = tokio::io::split(a);
                    let _ = tokio::io::copy(&mut reader, &mut writer).await;
                });
                Ok::<_, ForwardError>(b)
            },
            Arc::clone(&counters),
            log.clone(),
            cancel.clone(),
            children.clone(),
        ));

        let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        client.write_all(b"ping").await.unwrap();
        let mut reply = [0u8; 4];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"ping");
        drop(client);
        cancel.cancel();
        task.await.unwrap().unwrap();
        // The detached proxy task drains EOF concurrently; wait for it.
        for _ in 0..100 {
            if counters.active.load(Ordering::SeqCst) == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(counters.total.load(Ordering::SeqCst), 1);
        assert_eq!(counters.active.load(Ordering::SeqCst), 0);
        assert!(log.tail(10).iter().any(|line| line.contains("closed")));
    }

    #[test]
    fn log_sink_caps_memory() {
        let log = LogSink::new(3);
        for i in 0..10 {
            log.push(format!("line {i}"));
        }
        assert_eq!(log.tail(10), vec!["line 7", "line 8", "line 9"]);
    }
}
