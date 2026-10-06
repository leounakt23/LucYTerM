//! Multi-execution engine (Prompt 5.1): broadcast planning, safety gates,
//! templates, filters, and the combined output log.
//!
//! Pure data + algorithms — no I/O, no locks — so every rule is
//! headless-testable. Threading rationale (quality bar): the fan-out itself
//! runs on the UI thread through `SessionManager`'s non-blocking control
//! queues (`try_send` per target, never blocking), so commands cannot be
//! lost to a slow peer and a dropped session fails one target, not the
//! broadcast. Disconnect races resolve by pruning against the authoritative
//! `session_states` mirror at broadcast time.

use std::collections::VecDeque;

use mbxt_core::{Protocol, SessionId, SessionSpec};

/// Combined-output log cap (lines; old lines drop off the front).
pub const MAX_LOG_LINES: usize = 500;
/// Per-run command history cap (completed lines sent while broadcasting).
pub const MAX_HISTORY: usize = 100;
/// Stagger ceiling (ms between targets; thundering-herd guard stays sane).
pub const MAX_STAGGER_MS: u64 = 5000;

/// Substrings that mark a broadcast as destructive (lowercase match).
/// Conservative on purpose: describing the command (`echo rm -rf /`) still
/// holds the broadcast for confirmation — safe side for a foot-gun gate.
const DESTRUCTIVE_PATTERNS: [&str; 9] = [
    "rm -rf",
    "mkfs",
    "dd if=",
    ":(){:|:&};",
    "shutdown",
    "reboot",
    "halt",
    "poweroff",
    "drop database",
];

/// `true` when broadcasting `input` needs explicit confirmation.
pub fn is_destructive(input: &[u8]) -> bool {
    let text = String::from_utf8_lossy(input).to_lowercase();
    DESTRUCTIVE_PATTERNS
        .iter()
        .any(|pattern| text.contains(pattern))
}

/// Broadcast targets: the leader first (it always receives its own
/// keystrokes), then configured targets in order, deduplicated.
pub fn broadcast_targets(leader: SessionId, targets: &[SessionId]) -> Vec<SessionId> {
    let mut out = Vec::with_capacity(targets.len() + 1);
    out.push(leader);
    for id in targets {
        if !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

/// Stagger schedule `(session, delay_ms)`: target `i` waits `i * stagger_ms`
/// (clamped to [`MAX_STAGGER_MS`] per step so a huge setting degrades to a
/// fixed ceiling instead of hanging the tail).
pub fn stagger_schedule(targets: &[SessionId], stagger_ms: u64) -> Vec<(SessionId, u64)> {
    let step = stagger_ms.min(MAX_STAGGER_MS);
    targets
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index as u64 * step))
        .collect()
}

/// Host filter for conditional execution: empty matches everything;
/// `*`/`?` wildcards (case-insensitive), otherwise substring.
/// Sessions without a host never match a non-empty filter.
pub fn matches_host_filter(host: Option<&str>, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let Some(host) = host else {
        return false;
    };
    if !filter.contains(['*', '?']) {
        return host.to_lowercase().contains(&filter.to_lowercase());
    }
    glob_match(&host.to_lowercase(), &filter.to_lowercase())
}

fn glob_match(text: &str, pattern: &str) -> bool {
    let (mut ti, mut pi) = (0, 0);
    let (text, pattern) = (text.as_bytes(), pattern.as_bytes());
    let (mut star, mut match_idx) = (None, 0);
    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == b'?' || pattern[pi] == text[ti]) {
            ti += 1;
            pi += 1;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            star = Some(pi);
            match_idx = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }
    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }
    pi == pattern.len()
}

/// Render a command template with per-session substitution:
/// `$SESSION_HOST`, `$SESSION_NAME`, `$SESSION_USER`, `$SESSION_PORT`.
/// `$$` escapes to `$`; unknown variables pass through literally.
/// One substitutable template variable.
type TemplateVar = (&'static str, fn(&SessionSpec) -> String);

pub fn render_template(template: &str, spec: &SessionSpec) -> String {
    const VARS: [TemplateVar; 4] = [
        ("SESSION_HOST", |spec| spec.host.clone().unwrap_or_default()),
        ("SESSION_NAME", |spec| spec.name.clone()),
        ("SESSION_USER", |spec| {
            spec.username.clone().unwrap_or_default()
        }),
        ("SESSION_PORT", |spec| {
            spec.port.map(|p| p.to_string()).unwrap_or_default()
        }),
    ];
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while !rest.is_empty() {
        if rest.starts_with("$$") {
            out.push('$');
            rest = &rest[2..];
            continue;
        }
        if let Some(after_dollar) = rest.strip_prefix('$') {
            let mut matched = false;
            for (name, resolve) in VARS {
                if after_dollar.starts_with(name)
                    && !after_dollar[name.len()..]
                        .starts_with(|c: char| c.is_alphanumeric() || c == '_')
                {
                    out.push_str(&resolve(spec));
                    rest = &after_dollar[name.len()..];
                    matched = true;
                    break;
                }
            }
            if !matched {
                out.push('$');
                rest = after_dollar;
            }
            continue;
        }
        let c = rest.chars().next().expect("non-empty str has a char");
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Completed (newline-terminated) lines in a chunk, for broadcast history.
/// `\r` stripped; blank lines skipped; unterminated tails are not history.
pub fn completed_lines(chunk: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(chunk);
    let mut parts: Vec<&str> = text.split('\n').collect();
    if !text.ends_with('\n') {
        parts.pop(); // unterminated tail is not a completed line
    }
    parts
        .into_iter()
        .map(|line| line.trim_end_matches('\r').trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Push one combined-output line, evicting the oldest past the cap.
pub fn push_log_line(log: &mut VecDeque<(SessionId, String)>, session: SessionId, line: String) {
    if log.len() >= MAX_LOG_LINES {
        log.pop_front();
    }
    log.push_back((session, line));
}

/// Split targets into `(kept, dropped)` by a connectivity predicate.
pub fn prune_disconnected(
    targets: &[SessionId],
    connected: &dyn Fn(SessionId) -> bool,
) -> (Vec<SessionId>, Vec<SessionId>) {
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    for id in targets {
        if connected(*id) {
            kept.push(*id);
        } else {
            dropped.push(*id);
        }
    }
    (kept, dropped)
}

/// Minimal session summary for quick-select (no actor access needed).
pub struct SessionSummary {
    pub id: SessionId,
    pub protocol: Protocol,
    pub connected: bool,
    pub tags: Vec<String>,
    pub in_tabs: bool,
}

fn ssh_family(protocol: Protocol) -> bool {
    matches!(protocol, Protocol::Ssh | Protocol::Sftp | Protocol::X11)
}

/// Connected sessions, any protocol.
pub fn select_all_connected(sessions: &[SessionSummary]) -> Vec<SessionId> {
    sessions
        .iter()
        .filter(|s| s.connected)
        .map(|s| s.id)
        .collect()
}

/// Connected SSH-family sessions.
pub fn select_all_ssh(sessions: &[SessionSummary]) -> Vec<SessionId> {
    sessions
        .iter()
        .filter(|s| s.connected && ssh_family(s.protocol))
        .map(|s| s.id)
        .collect()
}

/// Connected sessions carrying `tag`.
pub fn select_by_tag(sessions: &[SessionSummary], tag: &str) -> Vec<SessionId> {
    sessions
        .iter()
        .filter(|s| s.connected && s.tags.iter().any(|t| t == tag))
        .map(|s| s.id)
        .collect()
}

/// Connected sessions with open tabs.
pub fn select_open_tabs(sessions: &[SessionSummary]) -> Vec<SessionId> {
    sessions
        .iter()
        .filter(|s| s.connected && s.in_tabs)
        .map(|s| s.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbxt_core::AuthMethod;

    fn test_spec(name: &str, host: Option<&str>) -> SessionSpec {
        SessionSpec {
            name: name.into(),
            protocol: Protocol::Ssh,
            host: host.map(str::to_string),
            port: Some(22),
            username: Some("ops".into()),
            auth: AuthMethod::Password,
            tags: vec![],
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
        }
    }

    #[test]
    fn destructive_gate_flags_foot_guns_only() {
        assert!(is_destructive(b"rm -rf /var/tmp\n"));
        assert!(is_destructive(b"sudo RM -RF /"));
        assert!(is_destructive(b"mkfs.ext4 /dev/sda1"));
        assert!(is_destructive(b"dd if=/dev/zero of=/dev/sda"));
        assert!(is_destructive(b":(){:|:&};:"));
        assert!(is_destructive(b"systemctl reboot"));
        assert!(is_destructive(b"DROP DATABASE prod;"));
        assert!(!is_destructive(b"rm old.log\n"));
        assert!(!is_destructive(b"ls -la\n"));
        assert!(!is_destructive(b"echo hello\n"));
    }

    #[test]
    fn broadcast_targets_keep_leader_first_and_deduped() {
        assert_eq!(broadcast_targets(1, &[2, 3, 2]), vec![1, 2, 3]);
        assert_eq!(broadcast_targets(2, &[2, 3]), vec![2, 3]);
        assert_eq!(broadcast_targets(9, &[]), vec![9]);
    }

    #[test]
    fn stagger_spreads_with_ceiling() {
        assert_eq!(
            stagger_schedule(&[1, 2, 3], 100),
            vec![(1, 0), (2, 100), (3, 200)]
        );
        assert_eq!(stagger_schedule(&[1], 100), vec![(1, 0)]);
        let over = stagger_schedule(&[1, 2], u64::MAX);
        assert_eq!(over[1].1, MAX_STAGGER_MS);
    }

    #[test]
    fn host_filter_matches_substrings_and_wildcards() {
        assert!(matches_host_filter(Some("web-01"), ""));
        assert!(matches_host_filter(Some("Web-01"), "web"));
        assert!(matches_host_filter(Some("web-01"), "web-*"));
        assert!(!matches_host_filter(Some("db-01"), "web-*"));
        assert!(!matches_host_filter(None, "web"));
        assert!(matches_host_filter(None, ""));
    }

    #[test]
    fn templates_substitute_per_session() {
        let spec = test_spec("bastion", Some("10.0.0.1"));
        assert_eq!(
            render_template(
                "ssh $SESSION_USER@$SESSION_HOST -p $SESSION_PORT #$SESSION_NAME",
                &spec
            ),
            "ssh ops@10.0.0.1 -p 22 #bastion"
        );
        assert_eq!(
            render_template("cost: $$5 on $SESSION_HOST", &spec),
            "cost: $5 on 10.0.0.1"
        );
        // Unknown vars and prefix-collisions pass through.
        assert_eq!(
            render_template("$SESSION_HOSTNAME $NOPE", &spec),
            "$SESSION_HOSTNAME $NOPE"
        );
        let no_host = test_spec("x", None);
        assert_eq!(render_template("h=$SESSION_HOST;", &no_host), "h=;");
    }

    #[test]
    fn completed_lines_skip_partials_and_blanks() {
        assert_eq!(completed_lines(b"ls\npwd\n"), vec!["ls", "pwd"]);
        assert_eq!(completed_lines(b"top\r\n"), vec!["top"]);
        assert!(completed_lines(b"partial").is_empty());
        assert!(completed_lines(b"\n\n").is_empty());
    }

    #[test]
    fn log_caps_at_max_lines() {
        let mut log = VecDeque::new();
        for i in 0..MAX_LOG_LINES + 10 {
            push_log_line(&mut log, 1, format!("line {i}"));
        }
        assert_eq!(log.len(), MAX_LOG_LINES);
        assert_eq!(log.front().unwrap().1, "line 10");
    }

    #[test]
    fn prune_splits_kept_and_dropped() {
        let (kept, dropped) = prune_disconnected(&[1, 2, 3], &|id| id != 2);
        assert_eq!(kept, vec![1, 3]);
        assert_eq!(dropped, vec![2]);
    }

    #[test]
    fn quick_select_respects_connectivity() {
        let sessions = vec![
            SessionSummary {
                id: 1,
                protocol: Protocol::Ssh,
                connected: true,
                tags: vec!["prod".into()],
                in_tabs: true,
            },
            SessionSummary {
                id: 2,
                protocol: Protocol::Telnet,
                connected: true,
                tags: vec!["prod".into()],
                in_tabs: false,
            },
            SessionSummary {
                id: 3,
                protocol: Protocol::Ssh,
                connected: false,
                tags: vec![],
                in_tabs: true,
            },
        ];
        assert_eq!(select_all_connected(&sessions), vec![1, 2]);
        assert_eq!(select_all_ssh(&sessions), vec![1]);
        assert_eq!(select_by_tag(&sessions, "prod"), vec![1, 2]);
        assert!(select_by_tag(&sessions, "nope").is_empty());
        assert_eq!(select_open_tabs(&sessions), vec![1]);
    }
}
