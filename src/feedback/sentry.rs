//! Opt-in Sentry crash reporting and minidump upload helpers.

use crate::utils::config::{GeneralSettings, ReleaseChannel};
#[cfg(feature = "crash-reporting")]
use std::path::Path;
#[cfg(feature = "crash-reporting")]
use std::sync::Arc;

/// Keeps the Sentry client alive. `None` means reporting is disabled or not
/// configured; this is the default behavior.
#[cfg(feature = "crash-reporting")]
pub struct CrashReporter {
    _guard: ::sentry::ClientInitGuard,
}

#[cfg(not(feature = "crash-reporting"))]
pub struct CrashReporter;

/// Initialize only after explicit opt-in. The DSN is deployment configuration,
/// not embedded user data. Sentry's default PII collection remains disabled.
#[cfg(feature = "crash-reporting")]
pub fn init(settings: &GeneralSettings, dsn: Option<&str>) -> Option<CrashReporter> {
    if !settings.crash_reporting_enabled {
        return None;
    }
    let dsn = dsn?;
    let guard = ::sentry::init((
        dsn,
        ::sentry::ClientOptions {
            release: Some(crate::build_info::VERSION.into()),
            environment: Some(settings.release_channel.to_string().into()),
            send_default_pii: false,
            attach_stacktrace: true,
            max_breadcrumbs: 0,
            before_send: Some(Arc::new(|mut event| {
                event.message = event.message.map(|_| "panic".to_string());
                event.logentry = None;
                event.culprit = None;
                event.transaction = None;
                event.user = None;
                event.request = None;
                event.server_name = None;
                event.breadcrumbs.values.clear();
                event.extra.clear();
                for exception in &mut event.exception.values {
                    exception.value = None;
                }
                Some(event)
            })),
            ..Default::default()
        },
    ));
    ::sentry::configure_scope(|scope| {
        scope.set_tag("channel", settings.release_channel.to_string());
        scope.set_tag("os", std::env::consts::OS);
        scope.set_tag("arch", std::env::consts::ARCH);
    });
    Some(CrashReporter { _guard: guard })
}

#[cfg(not(feature = "crash-reporting"))]
pub fn init(_settings: &GeneralSettings, _dsn: Option<&str>) -> Option<CrashReporter> {
    None
}

/// Upload a locally generated minidump only after the crash-reporting opt-in.
/// The self-hosted receiver performs symbolication with uploaded `symbolic`
/// debug bundles. Session and file content are never attached.
#[cfg(feature = "crash-reporting")]
pub async fn upload_minidump(
    settings: &GeneralSettings,
    dump_consent: bool,
    endpoint: &str,
    dump: &Path,
) -> Result<(), String> {
    if !settings.crash_reporting_enabled || !dump_consent {
        return Err("minidump upload requires explicit per-dump consent".to_string());
    }
    let bytes = tokio::fs::read(dump)
        .await
        .map_err(|error| error.to_string())?;
    let part = reqwest::multipart::Part::bytes(bytes).file_name("remote-app.dmp");
    let form = reqwest::multipart::Form::new()
        .text("version", crate::build_info::VERSION.to_string())
        .text("channel", settings.release_channel.to_string())
        .part("upload_file_minidump", part);
    reqwest::Client::new()
        .post(endpoint)
        .multipart(form)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Types used by the companion crash monitor. Keeping these re-exports here
/// ensures dump capture and server-side symbolication stay feature-gated.
#[cfg(feature = "crash-reporting")]
pub mod crash_tools {
    pub use minidumper::{Client, Server, ServerHandler};
    pub use symbolic;
}

pub fn channel_name(channel: ReleaseChannel) -> &'static str {
    match channel {
        ReleaseChannel::Stable => "stable",
        ReleaseChannel::Beta => "beta",
        ReleaseChannel::Nightly => "nightly",
    }
}
