//! Privacy-preserving feedback preparation and explicit submission.
//!
//! Nothing in this module submits automatically. Callers first create and
//! display a [`FeedbackPreview`], then invoke a backend only after consent.

pub mod sentry;
pub mod telemetry;
pub mod updates;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// User-selected feedback category.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeedbackKind {
    #[default]
    BugReport,
    FeatureRequest,
    GeneralFeedback,
    CrashReport,
}

impl std::fmt::Display for FeedbackKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::BugReport => "Bug report",
            Self::FeatureRequest => "Feature request",
            Self::GeneralFeedback => "General feedback",
            Self::CrashReport => "Crash report",
        })
    }
}

impl FeedbackKind {
    #[cfg(feature = "feedback-network")]
    fn issue_label(self) -> &'static str {
        match self {
            Self::BugReport | Self::CrashReport => "bug,beta",
            Self::FeatureRequest => "feature-request,beta",
            Self::GeneralFeedback => "beta",
        }
    }
}

/// Explicit attachment choices. All optional data starts disabled.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedbackConsent {
    pub include_redacted_logs: bool,
    pub include_breadcrumbs: bool,
    pub include_screenshot: bool,
}

/// Allow-listed, non-content system information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemContext {
    pub app_version: String,
    pub release_channel: String,
    pub os: String,
    pub architecture: String,
    pub kernel: String,
    pub display_server: String,
}

impl SystemContext {
    pub fn collect(channel: crate::utils::config::ReleaseChannel) -> Self {
        Self {
            app_version: crate::build_info::long(),
            release_channel: channel.to_string(),
            os: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            kernel: kernel_version(),
            display_server: display_server(),
        }
    }
}

/// Complete data shown to the user before any submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackPreview {
    pub kind: FeedbackKind,
    pub title: String,
    pub description: String,
    pub system: SystemContext,
    pub breadcrumbs: Vec<String>,
    pub redacted_log: Option<String>,
    pub screenshot: Option<PathBuf>,
}

impl FeedbackPreview {
    /// Build a preview. Screenshot paths and log text are included only when
    /// the matching consent is true.
    pub fn prepare(
        kind: FeedbackKind,
        title: String,
        description: String,
        system: SystemContext,
        consent: &FeedbackConsent,
        logs_dir: &Path,
        screenshot: Option<PathBuf>,
    ) -> Result<Self, String> {
        let breadcrumbs = if consent.include_breadcrumbs {
            telemetry::recent_breadcrumbs(30)
        } else {
            Vec::new()
        };
        let redacted_log = if consent.include_redacted_logs {
            newest_log(logs_dir)
                .map(|path| {
                    std::fs::read_to_string(path)
                        .map(|text| redact_log(&text))
                        .map_err(|error| format!("cannot read log: {error}"))
                })
                .transpose()?
        } else {
            None
        };
        Ok(Self {
            kind,
            title,
            description,
            system,
            breadcrumbs,
            redacted_log,
            screenshot: consent.include_screenshot.then_some(screenshot).flatten(),
        })
    }

    /// Stable text representation used by both the UI preview and backends.
    pub fn render(&self) -> String {
        let mut output = format!(
            "# {}\n\n{}\n\n## Environment\n\n```text\nversion: {}\nchannel: {}\nos: {}\narchitecture: {}\nkernel: {}\ndisplay server: {}\n```\n",
            self.title,
            self.description,
            self.system.app_version,
            self.system.release_channel,
            self.system.os,
            self.system.architecture,
            self.system.kernel,
            self.system.display_server,
        );
        if !self.breadcrumbs.is_empty() {
            output.push_str("\n## Recent actions (content-free)\n\n");
            for action in &self.breadcrumbs {
                output.push_str("- ");
                output.push_str(action);
                output.push('\n');
            }
        }
        if let Some(log) = &self.redacted_log {
            output.push_str("\n## Redacted logs\n\n```text\n");
            output.push_str(log);
            output.push_str("\n```\n");
        }
        if self.screenshot.is_some() {
            output.push_str("\n## Screenshot\n\nA separately reviewed screenshot is attached as `screenshot`.\n");
        }
        output
    }
}

/// Submit to GitHub after the preview has been accepted by the user.
#[cfg(feature = "feedback-network")]
pub async fn submit_github(
    repository: &str,
    token: &str,
    preview: &FeedbackPreview,
) -> Result<String, String> {
    #[derive(Deserialize)]
    struct CreatedIssue {
        html_url: String,
    }

    let response = reqwest::Client::new()
        .post(format!("https://api.github.com/repos/{repository}/issues"))
        .header(reqwest::header::USER_AGENT, "remote-app-feedback")
        .bearer_auth(token)
        .json(&serde_json::json!({
            "title": preview.title,
            "body": preview.render(),
            "labels": preview.kind.issue_label().split(',').collect::<Vec<_>>(),
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<CreatedIssue>()
        .await
        .map_err(|error| error.to_string())?;
    Ok(response.html_url)
}

/// Transparent fallback that opens no connection and can be copied into mail.
pub fn email_fallback(address: &str, preview: &FeedbackPreview) -> String {
    format!(
        "To: {address}\nSubject: [Remote App beta] {}\n\n{}",
        preview.title,
        preview.render()
    )
}

/// Conservative scrubber: removes likely secrets and all content-bearing log
/// classes rather than attempting to infer whether terminal data is safe.
pub fn redact_log(input: &str) -> String {
    input
        .lines()
        .rev()
        .take(200)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .filter_map(redact_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_line(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    const SAFE_EVENTS: [(&str, &str); 9] = [
        ("starting remote-app", "application started"),
        ("configuration loaded", "configuration loaded"),
        ("user requested quit", "application exit requested"),
        ("screen locked", "screen lock observed"),
        ("network status changed", "network status changed"),
        ("connected", "connection state changed: connected"),
        ("disconnected", "connection state changed: disconnected"),
        ("task completed", "background task completed"),
        ("task failed", "background task failed"),
    ];
    SAFE_EVENTS
        .iter()
        .find(|(pattern, _)| lower.contains(pattern))
        .map(|(_, event)| (*event).to_string())
}

fn newest_log(logs_dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(logs_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.metadata().map(|meta| meta.is_file()).unwrap_or(false))
        .max_by_key(|entry| entry.metadata().and_then(|meta| meta.modified()).ok())
        .map(|entry| entry.path())
}

fn kernel_version() -> String {
    #[cfg(target_family = "unix")]
    {
        std::process::Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|output| !output.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
    }
    #[cfg(not(target_family = "unix"))]
    "unknown".to_string()
}

fn display_server() -> String {
    match std::env::var("XDG_SESSION_TYPE").ok().as_deref() {
        Some("wayland") => "Wayland".to_string(),
        Some("x11") => "X11".to_string(),
        _ if std::env::var_os("WAYLAND_DISPLAY").is_some() => "Wayland".to_string(),
        _ if std::env::var_os("DISPLAY").is_some() => "X11".to_string(),
        _ => "unknown/headless".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_removes_secrets_and_content() {
        let redacted = redact_log(
            "connected host=private.example user=alice\npassword=hunter2\nterminal output: private\npath=C:/Users/alice/private",
        );
        assert!(redacted.contains("connected"));
        assert!(!redacted.contains("private.example"));
        assert!(!redacted.contains("alice"));
        assert!(!redacted.contains("hunter2"));
        assert!(!redacted.contains("private"));
    }

    #[test]
    fn consent_defaults_to_no_attachments() {
        assert_eq!(
            FeedbackConsent::default(),
            FeedbackConsent {
                include_redacted_logs: false,
                include_breadcrumbs: false,
                include_screenshot: false,
            }
        );
    }
}
