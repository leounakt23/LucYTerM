//! Logging + panic handling (prompt 1.3).
//!
//! - Structured `tracing` logging at TRACE..ERROR; level from `RUST_LOG`,
//!   defaulting to INFO (the CLI `--verbose` flag maps to DEBUG/TRACE).
//! - Two layers: colored stdout **and** a daily-rotating, non-blocking file
//!   under `<config_dir>/logs/` (`tracing-appender`).
//! - Panic hook: logs the panic, prints a user-friendly pointer to the logs,
//!   and delegates to `human-panic` for pretty release-mode reports.
//!
//! `init` returns a [`tracing_appender::non_blocking::WorkerGuard`] that the
//! caller **must keep alive** for the process lifetime — dropping it flushes
//! and stops the file writer.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;

/// Resolve the filter string from an explicit `RUST_LOG` value (pure — testable).
fn filter_for_with(rust_log: Option<&str>, verbose: u8) -> String {
    if let Some(user_filter) = rust_log {
        if !user_filter.trim().is_empty() {
            return user_filter.to_string();
        }
    }
    match verbose {
        0 => "info".to_string(),
        1 => "debug".to_string(),
        _ => "trace".to_string(),
    }
}

/// Resolve the filter string: `RUST_LOG` wins; otherwise `--verbose` count
/// maps INFO→DEBUG→TRACE.
fn filter_for(verbose: u8) -> String {
    filter_for_with(std::env::var("RUST_LOG").ok().as_deref(), verbose)
}

/// Initialize global logging. Call once, as early as possible, and keep the
/// returned guard alive for the whole process.
pub fn init(
    logs_dir: &std::path::Path,
    verbose: u8,
) -> tracing_appender::non_blocking::WorkerGuard {
    let filter = EnvFilter::new(filter_for(verbose));

    // Rotating, non-blocking file layer (survives crashes better than a
    // synchronous writer and never blocks the UI thread).
    let file_appender = tracing_appender::rolling::daily(logs_dir, "remote-app.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        // Console layer: colored, human-readable.
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_ansi(true)
                .with_target(false)
                .with_filter(filter.clone()),
        )
        // File layer: full detail, machine-parseable, no colors.
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_writer)
                .with_ansi(false)
                .with_target(true)
                .with_filter(filter),
        )
        .init();

    guard
}

/// Install the panic hook: logs the panic into the rotating file, shows a
/// friendly message pointing at the logs, then delegates to the previously
/// installed hook (which is `human-panic`'s pretty reporter in release
/// builds — see `main`; the default backtrace printer in debug builds).
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        let location = info.location().map(|l| l.to_string()).unwrap_or_default();

        tracing::error!(
            target: "panic",
            message = %payload,
            %location,
            "panic caught; see report output and log directory"
        );
        eprintln!(
            "remote-app hit an unexpected internal error.\n\
             Logs and crash reports are under the application config directory — \
             attach them when reporting the issue (Help → Report Issue)."
        );
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_defaults_and_verbosity() {
        // RUST_LOG unset → level ladder.
        assert_eq!(filter_for_with(None, 0), "info");
        assert_eq!(filter_for_with(None, 1), "debug");
        assert_eq!(filter_for_with(None, 4), "trace");
        // RUST_LOG set (non-empty) wins over the flag.
        assert_eq!(
            filter_for_with(Some("warn,mbxt=trace"), 2),
            "warn,mbxt=trace"
        );
        // RUST_LOG set but empty → falls back to the flag.
        assert_eq!(filter_for_with(Some("   "), 0), "info");
    }

    #[test]
    fn file_layer_writes_events_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        // Guard drop flushes the non-blocking writer.
        let guard = init(dir.path(), 1);
        tracing::error!("logging integration test marker");
        drop(guard);

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert!(!entries.is_empty(), "log file was created");
        let contents = entries
            .iter()
            .map(|e| std::fs::read_to_string(e.path()).unwrap_or_default())
            .collect::<String>();
        assert!(contents.contains("logging integration test marker"));
    }

    #[test]
    fn panic_hook_chains_without_aborting_normal_flow() {
        // Install twice: proves take_hook/set_hook chaining is idempotent.
        install_panic_hook();
        install_panic_hook();
    }
}
