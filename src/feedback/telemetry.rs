//! Anonymous, allow-listed telemetry. Collection and sending are both opt-in.

use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Default)]
struct LocalMetrics {
    launches: u64,
    feature_usage: BTreeMap<String, u64>,
    performance_ms: BTreeMap<String, u64>,
    breadcrumbs: VecDeque<String>,
}

fn metrics() -> &'static Mutex<LocalMetrics> {
    static METRICS: OnceLock<Mutex<LocalMetrics>> = OnceLock::new();
    METRICS.get_or_init(|| Mutex::new(LocalMetrics::default()))
}

/// The complete telemetry schema. It intentionally has no user, host,
/// session, command, path, or content fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TelemetryPayload {
    pub schema: u8,
    pub app_version: String,
    pub release_channel: String,
    pub os: String,
    pub architecture: String,
    pub launches: u64,
    pub feature_usage: BTreeMap<String, u64>,
    pub performance_ms: BTreeMap<String, u64>,
}

pub fn record_launch() {
    if let Ok(mut state) = metrics().lock() {
        state.launches = state.launches.saturating_add(1);
    }
}

pub fn record_feature(feature: &'static str) {
    if let Ok(mut state) = metrics().lock() {
        let value = state.feature_usage.entry(feature.to_string()).or_default();
        *value = value.saturating_add(1);
    }
}

pub fn record_performance(metric: &'static str, milliseconds: u64) {
    if let Ok(mut state) = metrics().lock() {
        state
            .performance_ms
            .insert(metric.to_string(), milliseconds);
    }
}

/// Record only an allow-listed action name, never its parameters or content.
pub fn breadcrumb(action: &'static str) {
    if let Ok(mut state) = metrics().lock() {
        if state.breadcrumbs.len() == 100 {
            state.breadcrumbs.pop_front();
        }
        state.breadcrumbs.push_back(action.to_string());
    }
}

pub fn recent_breadcrumbs(limit: usize) -> Vec<String> {
    metrics()
        .lock()
        .map(|state| {
            state
                .breadcrumbs
                .iter()
                .rev()
                .take(limit)
                .rev()
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

pub fn snapshot(channel: crate::utils::config::ReleaseChannel) -> TelemetryPayload {
    let state = metrics().lock().unwrap_or_else(|error| error.into_inner());
    TelemetryPayload {
        schema: 1,
        app_version: crate::build_info::VERSION.to_string(),
        release_channel: channel.to_string(),
        os: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        launches: state.launches,
        feature_usage: state.feature_usage.clone(),
        performance_ms: state.performance_ms.clone(),
    }
}

/// Rate-limit background delivery to one aggregate payload per five minutes.
pub fn should_flush(now_secs: u64) -> bool {
    static LAST_FLUSH: AtomicU64 = AtomicU64::new(0);
    let previous = LAST_FLUSH.load(Ordering::Relaxed);
    if now_secs.saturating_sub(previous) < 300 {
        return false;
    }
    LAST_FLUSH
        .compare_exchange(previous, now_secs, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
}

pub fn inspect_json(channel: crate::utils::config::ReleaseChannel) -> String {
    serde_json::to_string_pretty(&snapshot(channel)).unwrap_or_else(|_| "{}".to_string())
}

/// Send exactly the inspectable payload to a self-hosted Matomo/Plausible
/// gateway. Callers must check the persisted opt-in before invoking this.
#[cfg(feature = "feedback-network")]
pub async fn send(endpoint: &str, payload: &TelemetryPayload) -> Result<(), String> {
    reqwest::Client::new()
        .post(endpoint)
        .header(reqwest::header::USER_AGENT, "remote-app-telemetry")
        .json(payload)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_contains_only_allow_list() {
        let value = serde_json::to_value(snapshot(Default::default())).unwrap();
        let fields = value.as_object().unwrap();
        let actual = fields
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let allowed = [
            "schema",
            "app_version",
            "release_channel",
            "os",
            "architecture",
            "launches",
            "feature_usage",
            "performance_ms",
        ]
        .into_iter()
        .collect();
        assert_eq!(actual, allowed);
    }
}
