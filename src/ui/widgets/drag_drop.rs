//! Drag-and-drop handlers (Prompt 3.3, platform-specific).
//!
//! Linux DND reality: raw X11 (`Xdnd`) / Wayland (`wl_data_device`) drops
//! require toolkit-level negotiation that iced 0.13 does not expose to
//! widgets, so drops from/to the desktop file manager go through the
//! portal/data-device when available and degrade gracefully otherwise:
//!
//! - **Export** (browser → file manager): the widget offers per-selection
//!   download plus a `text/uri-list` payload builder for clipboard/DND handoff.
//! - **Import** (file manager → browser): drops arrive as `text/uri-list`
//!   payloads; [`parse_uri_list`] turns them into local paths that feed the
//!   transfer manager as uploads.
//! - **Fallback** (no DND backend): explicit local-path input + buttons in
//!   the browser — every operation stays reachable without DND.
//!
//! All parsing/detection here is pure and unit-tested; the iced view only
//! calls [`plan_drop`] and the `text/uri-list` codecs.

use std::path::{Path, PathBuf};

/// Available drop backend, probed from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DndBackend {
    /// `xdg-desktop-portal` file chooser / DND (sandboxed + Wayland).
    Portal,
    /// Native Wayland data-device (compositor sidesteps portals).
    Wayland,
    /// Classic X11 Xdnd selection transfers.
    X11,
    /// No DND backend — manual path entry + buttons only.
    None,
}

impl DndBackend {
    /// Short UI label for the status bar / browser footer.
    pub fn label(self) -> &'static str {
        match self {
            Self::Portal => "portal drag-and-drop",
            Self::Wayland => "Wayland drag-and-drop",
            Self::X11 => "X11 drag-and-drop",
            Self::None => "manual path entry (no DND backend)",
        }
    }
}

/// Probe the backend from environment slices (pure; [`detect_backend`] reads
/// the real environment).
pub fn detect_backend_from_env(
    display: Option<&str>,
    wayland_display: Option<&str>,
    portal_hint: bool,
) -> DndBackend {
    if portal_hint {
        DndBackend::Portal
    } else if wayland_display.is_some_and(|v| !v.is_empty()) {
        DndBackend::Wayland
    } else if display.is_some_and(|v| !v.is_empty()) {
        DndBackend::X11
    } else {
        DndBackend::None
    }
}

/// Probe the session backend (`WAYLAND_DISPLAY` / `DISPLAY` / portal hint).
pub fn detect_backend() -> DndBackend {
    let portal_hint = std::env::var_os("XDG_CURRENT_DESKTOP").is_some()
        && std::env::var_os("XDG_SESSION_TYPE").is_some_and(|t| t == "wayland");
    detect_backend_from_env(
        std::env::var("DISPLAY").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
        portal_hint,
    )
}

/// Percent-encode a path for `file://` URIs (unreserved + `/` pass through).
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~/:@".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Percent-decode a `file://` URI path (malformed sequences pass through).
fn percent_decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(((h << 4) | l) as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Build a `text/uri-list` payload for local `paths` (export direction).
pub fn encode_uri_list(paths: &[&Path]) -> String {
    paths
        .iter()
        .map(|p| format!("file://{}", percent_encode(&p.to_string_lossy())))
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// Parse a `text/uri-list` payload into local paths (import direction).
///
/// Skips `#` comments, blank lines, and non-`file://` URIs; `localhost`
/// authorities are stripped.
pub fn parse_uri_list(payload: &str) -> Vec<PathBuf> {
    payload
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.strip_prefix("file://").map(str::trim))
        .filter(|rest| !rest.is_empty())
        .map(|rest| {
            let without_host = rest.strip_prefix("localhost").unwrap_or(rest);
            PathBuf::from(percent_decode(without_host))
        })
        .collect()
}

/// Validated drop onto the browser: local sources + remote target dir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropAction {
    pub session: mbxt_core::SessionId,
    pub remote_dir: String,
    pub local_paths: Vec<PathBuf>,
}

/// Validate an import drop (empty drops and missing dirs are user errors,
///
/// never panics — the widget surfaces the `Err` as a notification).
pub fn plan_drop(
    session: mbxt_core::SessionId,
    remote_dir: &str,
    local_paths: Vec<PathBuf>,
) -> Result<DropAction, String> {
    if local_paths.is_empty() {
        return Err("nothing to upload: drop files onto the browser first".to_string());
    }
    if remote_dir.is_empty() {
        return Err("no remote target directory".to_string());
    }
    Ok(DropAction {
        session,
        remote_dir: remote_dir.to_string(),
        local_paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_detection_prefers_portal_then_wayland_then_x11() {
        assert_eq!(
            detect_backend_from_env(None, None, true),
            DndBackend::Portal
        );
        assert_eq!(
            detect_backend_from_env(Some(":0"), Some("wayland-0"), false),
            DndBackend::Wayland
        );
        assert_eq!(
            detect_backend_from_env(Some(":0"), None, false),
            DndBackend::X11
        );
        assert_eq!(detect_backend_from_env(None, None, false), DndBackend::None);
        assert_eq!(
            detect_backend_from_env(Some(""), Some(""), false),
            DndBackend::None
        );
    }

    #[test]
    fn backend_probe_runs_without_panicking() {
        let _ = detect_backend();
    }

    #[test]
    fn uri_list_round_trip_with_spaces() {
        let paths = [Path::new("/home/ops/my file.txt"), Path::new("/tmp/a")];
        let payload = encode_uri_list(&paths);
        assert!(payload.contains("%20"));
        let parsed = parse_uri_list(&payload);
        assert_eq!(
            parsed,
            vec![
                PathBuf::from("/home/ops/my file.txt"),
                PathBuf::from("/tmp/a")
            ]
        );
    }

    #[test]
    fn uri_list_skips_comments_and_remote_uris() {
        let payload = "# comment\n\nfile:///a/b\nhttps://example.invalid/x\nfile://localhost/c\n";
        assert_eq!(
            parse_uri_list(payload),
            vec![PathBuf::from("/a/b"), PathBuf::from("/c")]
        );
    }

    #[test]
    fn malformed_escapes_pass_through() {
        assert_eq!(
            parse_uri_list("file:///a%2Fb\n"),
            vec![PathBuf::from("/a/b")]
        );
        assert_eq!(
            parse_uri_list("file:///a%zz\n"),
            vec![PathBuf::from("/a%zz")]
        );
    }

    #[test]
    fn drop_planning_rejects_empty_targets() {
        assert!(plan_drop(1, "/remote", vec![]).is_err());
        assert!(plan_drop(1, "", vec![PathBuf::from("/a")]).is_err());
        let action = plan_drop(1, "/remote", vec![PathBuf::from("/a")]).unwrap();
        assert_eq!(action.local_paths.len(), 1);
    }
}
