//! OpenSSH `ssh_config` import: `Host` blocks → SSH session drafts.
//!
//! Contracts: first-obtained value wins (OpenSSH precedence); `Include`
//! resolves relative to the imported file via the caller-supplied
//! resolver only; `ProxyCommand`/`LocalCommand`/`Match exec` parse as
//! opaque quarantined text and are **never executed**; `%h/%p/%r`
//! tokens are never expanded at import.
//!
//! Import semantics (deliberate simplifications, all warning-annotated):
//! - directives before the first `Host` line (plus a lone `Host *`
//!   block) are global defaults: block-specific values take precedence,
//!   globals fill gaps;
//! - one `Host` line with several patterns collapses to a single draft
//!   named by the first usable pattern ([`WarningCode::AliasesCollapsed`]);
//! - wildcard patterns are kept literal, never expanded
//!   ([`WarningCode::WildcardPatternKeptLiteral`]);
//! - `Match` blocks are skipped wholesale, never materialized;
//! - passwords and password-shaped values are dropped, never stored
//!   (there is no password field anywhere in this path).

use std::collections::{HashMap, HashSet};

use super::model::{ImportWarning, ImportedSession, WarningCode};
use super::report::ImportReport;
use super::{check_size, ImportError, Source, MAX_LINE_BYTES, MAX_SESSIONS};
use mbxt_core::{AuthMethod, ForwardDef, ForwardType, Protocol, SessionSpec};

/// Cap on nested `Include` expansion (depth 0 is the top-level file).
const MAX_INCLUDE_DEPTH: usize = 8;
/// Quarantined command text kept in session notes is cut at this width.
const MAX_QUARANTINE_CHARS: usize = 500;
/// `Match` criteria echoed in warnings are cut at this width.
const MAX_CRITERIA_CHARS: usize = 120;

/// Recognized-but-unmapped safe preferences: noted, never applied.
const SAFE_PREFS: &[&str] = &[
    "serveraliveinterval",
    "serveralivecountmax",
    "compression",
    "tcpkeepalive",
    "connecttimeout",
    "connectionattempts",
    "loglevel",
    "stricthostkeychecking",
    "identitiesonly",
    "batchmode",
    "canonicalizehostname",
    "canonicalizefallback",
    "canonicalizemaxdots",
    "canonicalizepermittedcnames",
    "permitlocalcommand",
    "clearallforwardings",
    "exitonforwardfailure",
    "sessiontype",
    "requesttty",
    "sendenv",
    "setenv",
    "remotecommand",
    "rekeylimit",
    "ipqos",
    "addressfamily",
    "tunnel",
    "tundevice",
];

/// Parse `ssh_config` text. `include` resolves an `Include` pattern to
/// file text (sandboxed by the caller — tests use a fixture map, prod
/// resolves relative to the imported file's directory only).
pub fn parse_ssh_config(
    text: &str,
    file_name: &str,
    include: &dyn Fn(&str) -> Option<String>,
) -> Result<(Vec<ImportedSession>, ImportReport), ImportError> {
    check_size(text)?;
    let mut report = ImportReport::default();
    let mut state = State::default();
    process_text(text, file_name, include, 0, &mut state, &mut report)?;
    let sessions = materialize(state, file_name, &mut report)?;
    Ok((sessions, report))
}

/// Opaque quarantined command text: stored, never executed, never expanded.
#[derive(Debug, Default, Clone)]
struct Quarantined {
    text: String,
    line: usize,
}

/// First-wins option set (one per `Host` alias, plus the global defaults).
#[derive(Debug, Default)]
struct BlockData {
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_files: Vec<String>,
    forward_agent: Option<bool>,
    proxy_command: Option<Quarantined>,
    local_command: Option<Quarantined>,
    proxy_jump: Option<Quarantined>,
    forwards: Vec<ForwardDef>,
    x11: Option<bool>,
}

#[derive(Debug, Default)]
struct State {
    global: BlockData,
    /// First-alias names in first-seen order (materialization order).
    order: Vec<String>,
    blocks: HashMap<String, BlockData>,
    current: Option<String>,
    seen_host: bool,
    /// A lone `Host *` block writes to global defaults instead of a draft.
    sticky_global: bool,
    in_match: bool,
    rejected: usize,
    visited_includes: HashSet<String>,
}

impl State {
    /// Human label for warning details: current alias or global scope.
    fn ctx(&self) -> &str {
        self.current.as_deref().unwrap_or("global")
    }

    /// Mutable option set for the current position, or `None` when the
    /// line must be dropped (inside `Match`, or under a rejected block).
    fn target(&mut self) -> Option<&mut BlockData> {
        if self.in_match {
            return None;
        }
        if let Some(key) = self.current.clone() {
            return self.blocks.get_mut(&key);
        }
        if !self.seen_host || self.sticky_global {
            return Some(&mut self.global);
        }
        None
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

fn has_wildcard(s: &str) -> bool {
    s.chars().any(|c| c == '*' || c == '?' || c == '[')
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "yes" | "true" | "1" | "on" => Some(true),
        "no" | "false" | "0" | "off" => Some(false),
        _ => None,
    }
}

fn parse_port(value: &str) -> Option<u16> {
    value.parse::<u16>().ok().filter(|p| *p > 0)
}

/// Split `host:port` on the last colon (tolerates `::1:22`-style hosts).
fn split_host_port(s: &str) -> Option<(String, u16)> {
    let (host, port) = s.rsplit_once(':')?;
    if host.is_empty() {
        return None;
    }
    let port = parse_port(port)?;
    Some((host.to_string(), port))
}

/// Parse `[bind:]port`; bare ports default to `default_bind`.
fn parse_bind_port(s: &str, default_bind: &str) -> Option<(String, u16)> {
    if let Some((host, port)) = split_host_port(s) {
        return Some((host, port));
    }
    let port = parse_port(s)?;
    Some((default_bind.to_string(), port))
}

/// Split one config line into `(keyword, value)`.
///
/// Accepts both `Keyword value` and `Keyword=value` (optional spaces
/// around `=`). Only full-line `#` comments are stripped — a `#`
/// mid-line is data (e.g. inside `ProxyCommand`), per ssh_config(5).
fn split_directive(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let mut end = trimmed.len();
    for (i, c) in trimmed.char_indices() {
        if c.is_whitespace() || c == '=' {
            end = i;
            break;
        }
    }
    let keyword = trimmed[..end].to_string();
    if keyword.is_empty() {
        return None;
    }
    let mut rest = trimmed[end..].trim_start();
    if let Some(stripped) = rest.strip_prefix('=') {
        rest = stripped.trim_start();
    }
    Some((keyword, rest.to_string()))
}

fn warn_at(code: WarningCode, line: usize, detail: String) -> ImportWarning {
    ImportWarning::at_line(code, Source::SshConfig, line, detail)
}

fn process_text(
    text: &str,
    file_name: &str,
    include: &dyn Fn(&str) -> Option<String>,
    depth: usize,
    state: &mut State,
    report: &mut ImportReport,
) -> Result<(), ImportError> {
    check_size(text)?;
    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        if raw.len() > MAX_LINE_BYTES {
            report.warn(warn_at(
                WarningCode::SafePreferenceNoted,
                line_no,
                format!(
                    "line {line_no} of '{file_name}' is {} bytes (cap is {MAX_LINE_BYTES}); processed anyway",
                    raw.len()
                ),
            ));
        }
        let Some((keyword, value)) = split_directive(raw) else {
            continue;
        };
        handle_line(
            state, report, &keyword, &value, line_no, file_name, include, depth,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_line(
    state: &mut State,
    report: &mut ImportReport,
    keyword: &str,
    value: &str,
    line: usize,
    file: &str,
    include: &dyn Fn(&str) -> Option<String>,
    depth: usize,
) -> Result<(), ImportError> {
    let lower = keyword.to_ascii_lowercase();
    match lower.as_str() {
        "host" => handle_host(state, report, value, line, file),
        "match" => {
            state.current = None;
            state.sticky_global = false;
            state.in_match = true;
            let criteria = truncate_chars(value, MAX_CRITERIA_CHARS);
            report.warn(warn_at(
                WarningCode::MatchBlockSkipped,
                line,
                format!("match block '{criteria}' in '{file}' skipped; contents not imported"),
            ));
            Ok(())
        },
        "include" => handle_include(state, report, value, line, file, include, depth),
        "hostname" => {
            if value.is_empty() {
                return noted_empty(state, report, keyword, line);
            }
            let ctx = state.ctx().to_string();
            let mut wild = false;
            let shadowed = match state.target() {
                None => return Ok(()),
                Some(t) => {
                    if t.hostname.is_some() {
                        true
                    } else {
                        wild = has_wildcard(value);
                        t.hostname = Some(value.to_string());
                        false
                    }
                },
            };
            if shadowed {
                report.warn(warn_at(
                    WarningCode::ShadowedByPrecedence,
                    line,
                    format!("ignoring later 'HostName' for '{ctx}'; first value wins"),
                ));
            } else if wild {
                report.warn(warn_at(
                    WarningCode::WildcardPatternKeptLiteral,
                    line,
                    format!("host name for '{ctx}' contains wildcards; kept literal"),
                ));
            }
            Ok(())
        },
        "user" => {
            if value.is_empty() {
                return noted_empty(state, report, keyword, line);
            }
            let first = value.split_whitespace().next().unwrap_or(value);
            let ctx = state.ctx().to_string();
            let shadowed = match state.target() {
                None => return Ok(()),
                Some(t) => {
                    if t.user.is_some() {
                        true
                    } else {
                        t.user = Some(first.to_string());
                        false
                    }
                },
            };
            if shadowed {
                report.warn(warn_at(
                    WarningCode::ShadowedByPrecedence,
                    line,
                    format!("ignoring later 'User' for '{ctx}'; first value wins"),
                ));
            }
            Ok(())
        },
        "port" => {
            let first = value.split_whitespace().next().unwrap_or("").to_string();
            let parsed = parse_port(&first);
            let ctx = state.ctx().to_string();
            let Some(slot) = state.target() else {
                return Ok(());
            };
            match parsed {
                None => {
                    report.warn(warn_at(
                        WarningCode::SafePreferenceNoted,
                        line,
                        format!("ignoring invalid Port '{first}' for '{ctx}'; using 22"),
                    ));
                },
                Some(port) => {
                    if slot.port.is_some() {
                        report.warn(warn_at(
                            WarningCode::ShadowedByPrecedence,
                            line,
                            format!("ignoring later 'Port' for '{ctx}'; first value wins"),
                        ));
                    } else {
                        slot.port = Some(port);
                    }
                },
            }
            Ok(())
        },
        "identityfile" => {
            let paths: Vec<&str> = value.split_whitespace().collect();
            if paths.is_empty() {
                return noted_empty(state, report, keyword, line);
            }
            if let Some(t) = state.target() {
                t.identity_files.extend(paths.iter().map(|p| p.to_string()));
            }
            Ok(())
        },
        "forwardagent" => {
            let first = value.split_whitespace().next().unwrap_or("").to_string();
            let parsed = parse_bool(&first);
            let ctx = state.ctx().to_string();
            let Some(slot) = state.target() else {
                return Ok(());
            };
            match parsed {
                None => {
                    report.warn(warn_at(
                        WarningCode::SafePreferenceNoted,
                        line,
                        format!("ignoring invalid ForwardAgent '{first}' for '{ctx}'"),
                    ));
                },
                Some(flag) => {
                    if slot.forward_agent.is_some() {
                        report.warn(warn_at(
                            WarningCode::ShadowedByPrecedence,
                            line,
                            format!("ignoring later 'ForwardAgent' for '{ctx}'; first value wins"),
                        ));
                    } else {
                        slot.forward_agent = Some(flag);
                    }
                },
            }
            Ok(())
        },
        "proxycommand" => {
            if value.is_empty() {
                return noted_empty(state, report, keyword, line);
            }
            quarantine(state, report, keyword, value, line, true)
        },
        "localcommand" => {
            if value.is_empty() {
                return noted_empty(state, report, keyword, line);
            }
            quarantine(state, report, keyword, value, line, false)
        },
        "proxyjump" => {
            if value.is_empty() || value.eq_ignore_ascii_case("none") {
                return Ok(());
            }
            let ctx = state.ctx().to_string();
            let shadowed = match state.target() {
                None => return Ok(()),
                Some(t) => {
                    if t.proxy_jump.is_some() {
                        true
                    } else {
                        t.proxy_jump = Some(Quarantined {
                            text: value.to_string(),
                            line,
                        });
                        false
                    }
                },
            };
            if shadowed {
                report.warn(warn_at(
                    WarningCode::ShadowedByPrecedence,
                    line,
                    format!("ignoring later 'ProxyJump' for '{ctx}'; first value wins"),
                ));
            }
            Ok(())
        },
        "localforward" | "remoteforward" | "dynamicforward" => {
            handle_forward(state, report, keyword, &lower, value, line)
        },
        "forwardx11" => {
            let first = value.split_whitespace().next().unwrap_or("").to_string();
            let parsed = parse_bool(&first);
            let ctx = state.ctx().to_string();
            let Some(slot) = state.target() else {
                return Ok(());
            };
            match parsed {
                None => {
                    report.warn(warn_at(
                        WarningCode::SafePreferenceNoted,
                        line,
                        format!("ignoring invalid ForwardX11 '{first}' for '{ctx}'"),
                    ));
                },
                Some(flag) => {
                    if slot.x11.is_some() {
                        report.warn(warn_at(
                            WarningCode::ShadowedByPrecedence,
                            line,
                            format!("ignoring later 'ForwardX11' for '{ctx}'; first value wins"),
                        ));
                    } else {
                        slot.x11 = Some(flag);
                    }
                },
            }
            Ok(())
        },
        "preferredauthentications" => {
            if state.target().is_none() {
                return Ok(());
            }
            let ctx = state.ctx().to_string();
            if value.to_ascii_lowercase().contains("password") {
                report.warn(warn_at(
                    WarningCode::PasswordDropped,
                    line,
                    format!("password authentication method for '{ctx}' dropped; never stored"),
                ));
            } else {
                report.warn(warn_at(
                    WarningCode::SafePreferenceNoted,
                    line,
                    format!("noted 'PreferredAuthentications' for '{ctx}'; not applied at import"),
                ));
            }
            Ok(())
        },
        _ => {
            if state.target().is_none() {
                return Ok(());
            }
            let ctx = state.ctx().to_string();
            if super::model::is_password_shaped(keyword) {
                // Value deliberately excluded from the detail: secrets
                // never enter warnings, notes, or logs.
                report.warn(warn_at(
                    WarningCode::PasswordDropped,
                    line,
                    format!("password-shaped '{keyword}' for '{ctx}' dropped; never stored"),
                ));
            } else if SAFE_PREFS.contains(&lower.as_str()) {
                report.warn(warn_at(
                    WarningCode::SafePreferenceNoted,
                    line,
                    format!("noted '{keyword}' for '{ctx}'; not applied at import"),
                ));
            } else {
                report.warn(warn_at(
                    WarningCode::UnknownDirective,
                    line,
                    format!("unknown directive '{keyword}' for '{ctx}'; ignored"),
                ));
            }
            Ok(())
        },
    }
}

/// Store `ProxyCommand`/`LocalCommand` as opaque quarantined text.
/// Never executed, never `%`-expanded: the raw string is kept verbatim.
fn quarantine(
    state: &mut State,
    report: &mut ImportReport,
    keyword: &str,
    value: &str,
    line: usize,
    is_proxy: bool,
) -> Result<(), ImportError> {
    let ctx = state.ctx().to_string();
    let shadowed = match state.target() {
        None => return Ok(()),
        Some(t) => {
            let slot = if is_proxy {
                &mut t.proxy_command
            } else {
                &mut t.local_command
            };
            if slot.is_some() {
                true
            } else {
                *slot = Some(Quarantined {
                    text: value.to_string(),
                    line,
                });
                false
            }
        },
    };
    if shadowed {
        report.warn(warn_at(
            WarningCode::ShadowedByPrecedence,
            line,
            format!("ignoring later '{keyword}' for '{ctx}'; first value wins"),
        ));
    }
    Ok(())
}

fn noted_empty(
    state: &mut State,
    report: &mut ImportReport,
    keyword: &str,
    line: usize,
) -> Result<(), ImportError> {
    if state.target().is_none() {
        return Ok(());
    }
    let ctx = state.ctx().to_string();
    report.warn(warn_at(
        WarningCode::SafePreferenceNoted,
        line,
        format!("ignoring empty '{keyword}' for '{ctx}'"),
    ));
    Ok(())
}

fn handle_host(
    state: &mut State,
    report: &mut ImportReport,
    value: &str,
    line: usize,
    file: &str,
) -> Result<(), ImportError> {
    state.seen_host = true;
    state.in_match = false;
    state.sticky_global = false;
    state.current = None;
    let patterns: Vec<&str> = value.split_whitespace().collect();
    if patterns.is_empty() {
        state.rejected += 1;
        report.warn(warn_at(
            WarningCode::UnnamedSessionRejected,
            line,
            format!("'Host' without a pattern in '{file}'; block rejected"),
        ));
        return Ok(());
    }
    let kept: Vec<&str> = patterns
        .iter()
        .copied()
        .filter(|p| !p.starts_with('!'))
        .collect();
    if kept.is_empty() {
        state.rejected += 1;
        report.warn(warn_at(
            WarningCode::NoResolvableHost,
            line,
            format!("host block in '{file}' only negates patterns; nothing to import"),
        ));
        return Ok(());
    }
    if kept.len() == 1 && kept[0] == "*" {
        // Lone `Host *`: defaults for every draft, not a draft itself.
        state.sticky_global = true;
        return Ok(());
    }
    let mut key: Option<String> = None;
    for pattern in &kept {
        if *pattern != "*" {
            key = Some((*pattern).to_string());
            break;
        }
    }
    if key.is_none() {
        key = kept.first().map(|p| (*p).to_string());
    }
    let Some(name) = key else {
        state.rejected += 1;
        report.warn(warn_at(
            WarningCode::NoResolvableHost,
            line,
            format!("host block in '{file}' has no usable pattern; rejected"),
        ));
        return Ok(());
    };
    if !state.blocks.contains_key(&name) {
        state.blocks.insert(name.clone(), BlockData::default());
        state.order.push(name.clone());
    }
    state.current = Some(name.clone());
    if kept.len() > 1 {
        report.warn(warn_at(
            WarningCode::AliasesCollapsed,
            line,
            format!("aliases {} collapsed into '{name}'", kept.join(", ")),
        ));
    }
    if kept.iter().any(|p| has_wildcard(p)) {
        report.warn(warn_at(
            WarningCode::WildcardPatternKeptLiteral,
            line,
            format!("pattern for '{name}' contains wildcards; kept literal"),
        ));
    }
    Ok(())
}

fn handle_include(
    state: &mut State,
    report: &mut ImportReport,
    value: &str,
    line: usize,
    file: &str,
    include: &dyn Fn(&str) -> Option<String>,
    depth: usize,
) -> Result<(), ImportError> {
    if state.in_match {
        return Ok(());
    }
    let patterns: Vec<&str> = value.split_whitespace().collect();
    if patterns.is_empty() {
        let ctx = state.ctx().to_string();
        // An empty Include is only worth noting when it could apply.
        if state.target().is_some() {
            report.warn(warn_at(
                WarningCode::SafePreferenceNoted,
                line,
                format!("ignoring empty 'Include' for '{ctx}'"),
            ));
        }
        return Ok(());
    }
    for pattern in patterns {
        if pattern.starts_with('/') || pattern.starts_with('~') || pattern.contains("..") {
            report.warn(warn_at(
                WarningCode::IncludeOutsideRoot,
                line,
                format!("include '{pattern}' in '{file}' is absolute or outside the import root; skipped"),
            ));
            continue;
        }
        if state.visited_includes.contains(pattern) {
            report.warn(warn_at(
                WarningCode::IncludeDepthExceeded,
                line,
                format!("include '{pattern}' in '{file}' already expanded; skipped (cycle-safe)"),
            ));
            continue;
        }
        if depth >= MAX_INCLUDE_DEPTH {
            return Err(ImportError::IncludeDepthExceeded);
        }
        match include(pattern) {
            None => {
                report.warn(warn_at(
                    WarningCode::IncludeNoMatch,
                    line,
                    format!("include '{pattern}' in '{file}' matched nothing; skipped"),
                ));
            },
            Some(text) => {
                state.visited_includes.insert(pattern.to_string());
                let label = basename(pattern).to_string();
                process_text(&text, &label, include, depth + 1, state, report)?;
            },
        }
    }
    Ok(())
}

fn handle_forward(
    state: &mut State,
    report: &mut ImportReport,
    keyword: &str,
    lower: &str,
    value: &str,
    line: usize,
) -> Result<(), ImportError> {
    let ctx = state.ctx().to_string();
    let has_target = state.target().is_some();
    if !has_target {
        return Ok(());
    }
    let tokens: Vec<&str> = value.split_whitespace().collect();
    let parsed: Option<ForwardType> = match lower {
        "localforward" => {
            if tokens.len() < 2 {
                None
            } else {
                let local = parse_bind_port(tokens[0], "127.0.0.1");
                let remote = split_host_port(tokens[1]);
                match (local, remote) {
                    (Some((local_host, local_port)), Some((remote_host, remote_port))) => {
                        Some(ForwardType::Local {
                            local_host,
                            local_port,
                            remote_host,
                            remote_port,
                        })
                    },
                    _ => None,
                }
            }
        },
        "remoteforward" => {
            if tokens.len() < 2 {
                None
            } else {
                let remote = parse_bind_port(tokens[0], "localhost");
                let local = split_host_port(tokens[1]);
                match (remote, local) {
                    (Some((remote_host, remote_port)), Some((local_host, local_port))) => {
                        Some(ForwardType::Remote {
                            remote_host,
                            remote_port,
                            local_host,
                            local_port,
                        })
                    },
                    _ => None,
                }
            }
        },
        _ => {
            // dynamicforward
            if tokens.is_empty() {
                None
            } else {
                parse_bind_port(tokens[0], "127.0.0.1").map(|(local_host, local_port)| {
                    ForwardType::Dynamic {
                        local_host,
                        local_port,
                    }
                })
            }
        },
    };
    let Some(forward_type) = parsed else {
        report.warn(warn_at(
            WarningCode::SafePreferenceNoted,
            line,
            format!("ignoring malformed '{keyword}' for '{ctx}'"),
        ));
        return Ok(());
    };
    let extra = match lower {
        "dynamicforward" => tokens.len() > 1,
        _ => tokens.len() > 2,
    };
    // Re-borrow after the parse (parse is pure over `value`).
    if let Some(t) = state.target() {
        t.forwards.push(ForwardDef::new(None, forward_type, false));
    }
    if extra {
        report.warn(warn_at(
            WarningCode::SafePreferenceNoted,
            line,
            format!("ignoring extra arguments in '{keyword}' for '{ctx}'"),
        ));
    }
    Ok(())
}

fn materialize(
    state: State,
    file_name: &str,
    report: &mut ImportReport,
) -> Result<Vec<ImportedSession>, ImportError> {
    if state.order.len() > MAX_SESSIONS {
        return Err(ImportError::TooManySessions(MAX_SESSIONS));
    }
    let short_file = basename(file_name).to_string();
    let mut sessions = Vec::with_capacity(state.order.len());
    for name in &state.order {
        let Some(block) = state.blocks.get(name) else {
            continue;
        };
        let host = block
            .hostname
            .clone()
            .or_else(|| state.global.hostname.clone())
            .unwrap_or_else(|| name.clone());
        let port = block.port.or(state.global.port).unwrap_or(22);
        let username = block.user.clone().or_else(|| state.global.user.clone());

        let identities: Vec<String> = if block.identity_files.is_empty() {
            state.global.identity_files.clone()
        } else {
            block.identity_files.clone()
        };
        let forward_agent = block.forward_agent.or(state.global.forward_agent);
        let auth = match identities.first() {
            Some(path) => AuthMethod::KeyFile { path: path.clone() },
            None => AuthMethod::Agent {
                forward: forward_agent == Some(true),
            },
        };

        let proxy_command = block
            .proxy_command
            .clone()
            .or_else(|| state.global.proxy_command.clone());
        let local_command = block
            .local_command
            .clone()
            .or_else(|| state.global.local_command.clone());
        let proxy_jump = block
            .proxy_jump
            .clone()
            .or_else(|| state.global.proxy_jump.clone());

        let mut notes_lines: Vec<String> = Vec::new();
        let mut disabled_flags: Vec<String> = Vec::new();
        if let Some(q) = &proxy_command {
            notes_lines.push(format!(
                "quarantined proxy-command (line {}): {}",
                q.line,
                truncate_chars(&q.text, MAX_QUARANTINE_CHARS)
            ));
            disabled_flags.push("proxy-command".to_string());
        }
        if let Some(q) = &local_command {
            notes_lines.push(format!(
                "quarantined local-command (line {}): {}",
                q.line,
                truncate_chars(&q.text, MAX_QUARANTINE_CHARS)
            ));
            disabled_flags.push("auto-command".to_string());
        }
        if let Some(q) = &proxy_jump {
            notes_lines.push(format!(
                "proxy-jump (line {}): {}",
                q.line,
                truncate_chars(&q.text, MAX_QUARANTINE_CHARS)
            ));
        }

        let mut forwards = state.global.forwards.clone();
        forwards.extend(block.forwards.clone());

        let spec = SessionSpec {
            name: name.clone(),
            protocol: Protocol::Ssh,
            host: Some(host),
            port: Some(port),
            username,
            auth,
            tags: Vec::new(),
            notes: notes_lines.join("\n"),
            x11_forwarding: block.x11.or(state.global.x11).unwrap_or(false),
            serial: None,
            forwards,
        };
        let mut draft = ImportedSession::new(name.clone(), spec, Source::SshConfig, &short_file);
        draft.disabled_flags = disabled_flags;
        sessions.push(draft);

        if proxy_command.is_some() {
            report.warn(ImportWarning::new(
                WarningCode::ProxyCommandQuarantined,
                Source::SshConfig,
                format!("proxy-command for '{name}' quarantined; disabled pending review"),
            ));
        }
        if local_command.is_some() {
            report.warn(ImportWarning::new(
                WarningCode::AutoCommandIgnored,
                Source::SshConfig,
                format!("local-command for '{name}' quarantined as auto-command; disabled pending review"),
            ));
        }
        if proxy_jump.is_some() {
            report.warn(ImportWarning::new(
                WarningCode::ProxyDisabledPendingReview,
                Source::SshConfig,
                format!("proxy-jump for '{name}' recorded as text; disabled pending review"),
            ));
        }
        if identities.len() > 1 {
            report.warn(ImportWarning::new(
                WarningCode::ExtraIdentityFilesNoted,
                Source::SshConfig,
                format!(
                    "{} identity files for '{name}'; using the first",
                    identities.len()
                ),
            ));
        }
        if forward_agent == Some(false) {
            report.warn(ImportWarning::new(
                WarningCode::AgentForwardingDisabled,
                Source::SshConfig,
                format!("agent forwarding disabled for '{name}'"),
            ));
        }
    }
    report.imported = sessions.len();
    report.rejected = state.rejected;
    Ok(sessions)
}
