//! OS signal handling: SIGHUP/SIGINT/SIGTERM → graceful shutdown event
//! (architecture §4). Consumed by the UI bridge as `Message::Quit`.

/// Resolve when a termination signal (or Ctrl+C) arrives.
///
/// On unix: SIGHUP, SIGINT, SIGTERM via `tokio::signal::unix`.
/// Elsewhere: Ctrl+C.
pub async fn wait_for_shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate())
            .unwrap_or_else(|e| panic!("cannot install SIGTERM handler: {e}"));
        let mut sigint = signal(SignalKind::interrupt())
            .unwrap_or_else(|e| panic!("cannot install SIGINT handler: {e}"));
        let mut sighup = signal(SignalKind::hangup())
            .unwrap_or_else(|e| panic!("cannot install SIGHUP handler: {e}"));
        tokio::select! {
            _ = sigterm.recv() => {},
            _ = sigint.recv() => {},
            _ = sighup.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
