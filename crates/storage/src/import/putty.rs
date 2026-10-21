//! PuTTY/KiTTY `.reg` import: exported registry text → session drafts.
//!
//! Contracts: parses exported `.reg` *file text* only — never a live
//! registry; `dword` hex ports; password-shaped values dropped with a
//! warning; KiTTY auto-commands never imported as auto-run; host-key
//! caches out of scope (clean reject).

use super::model::{is_password_shaped, ImportWarning, ImportedSession, WarningCode};
use super::report::ImportReport;
use super::{check_size, ImportError, Source, MAX_LINE_BYTES, MAX_SESSIONS};
use mbxt_core::{AuthMethod, Protocol, SessionSpec};

/// Registry key markers (ASCII; matched case-insensitively) whose trailing
/// component is a session name. Both SimonTatham PuTTY and 9bis.com KiTTY
/// export roots are accepted.
const SESSION_MARKERS: [&str; 2] = [
    "software\\simontatham\\putty\\sessions\\",
    "software\\9bis.com\\kitty\\sessions\\",
];

/// One parsed `.reg` right-hand side.
enum RegValue {
    Str(String),
    Dword(u32),
    /// `hex:...` blobs and anything else opaque (never mapped, only warned).
    Other,
}

/// Parse exported `.reg` text (CRLF or LF).
pub fn parse_putty_reg(
    text: &str,
    file_name: &str,
) -> Result<(Vec<ImportedSession>, ImportReport), ImportError> {
    check_size(text)?;
    let mut report = ImportReport::default();
    let mut sessions = Vec::new();
    let mut current: Option<SessionBuilder> = None;
    let mut blocks: usize = 0;
    let mut rejected: usize = 0;

    for (idx, raw) in text.split('\n').enumerate() {
        let line_no = idx + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.len() > MAX_LINE_BYTES {
            report.warn(ImportWarning::at_line(
                WarningCode::UnknownField,
                Source::Putty,
                line_no,
                "line too long; skipped",
            ));
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            continue;
        }
        if let Some(header) = trimmed.strip_prefix('[') {
            if let Some(open) = current.take() {
                flush_builder(open, file_name, &mut sessions, &mut report, &mut rejected);
            }
            match header.strip_suffix(']') {
                // Non-session keys (host-key caches, config roots): out of
                // scope, cleanly ignored.
                Some(inner) => {
                    if let Some(name) = session_name_from_key(inner) {
                        blocks += 1;
                        if blocks > MAX_SESSIONS {
                            return Err(ImportError::TooManySessions(MAX_SESSIONS));
                        }
                        if name.is_empty() {
                            report.warn(ImportWarning::at_line(
                                WarningCode::UnnamedSessionRejected,
                                Source::Putty,
                                line_no,
                                "session block without a name; rejected",
                            ));
                            rejected += 1;
                        } else {
                            current = Some(SessionBuilder::new(name, line_no));
                        }
                    }
                },
                None => {
                    report.warn(ImportWarning::at_line(
                        WarningCode::UnknownField,
                        Source::Putty,
                        line_no,
                        "malformed registry key header; skipped",
                    ));
                },
            }
            continue;
        }
        // Lines outside any session block (REGEDIT banner, blank space):
        // ignored. Lines inside a block are parsed as values.
        if let Some(builder) = current.as_mut() {
            builder.apply(trimmed, line_no, &mut report);
        }
    }
    if let Some(open) = current.take() {
        flush_builder(open, file_name, &mut sessions, &mut report, &mut rejected);
    }
    report.imported = sessions.len();
    report.rejected = rejected;
    Ok((sessions, report))
}

/// Extract the session name from a registry key, or `None` when the key is
/// not a PuTTY/KiTTY session block. The name is the last key component with
/// `%20` unescaped.
fn session_name_from_key(key: &str) -> Option<String> {
    let lower = key.to_ascii_lowercase();
    for marker in SESSION_MARKERS {
        if let Some(pos) = lower.find(marker) {
            let tail = key.get(pos + marker.len()..)?;
            let last = match tail.rsplit('\\').next() {
                Some(component) => component,
                None => tail,
            };
            return Some(unescape_session_name(last));
        }
    }
    None
}

/// Unescape PuTTY session-name munging (`%20` → space, either case).
/// Other sequences pass through untouched.
fn unescape_session_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while !rest.is_empty() {
        if rest.len() >= 3 {
            if let Some(prefix) = rest.get(..3) {
                if prefix.eq_ignore_ascii_case("%20") {
                    out.push(' ');
                    rest = rest.get(3..).unwrap_or("");
                    continue;
                }
            }
        }
        match rest.chars().next() {
            Some(c) => {
                out.push(c);
                rest = rest.get(c.len_utf8()..).unwrap_or("");
            },
            None => break,
        }
    }
    out
}

/// Basename only — provenance must never carry a full path (paths may embed
/// usernames).
fn file_basename(file_name: &str) -> &str {
    let mut base = file_name;
    if let Some(pos) = base.rfind('/') {
        if let Some(rest) = base.get(pos + 1..) {
            base = rest;
        }
    }
    if let Some(pos) = base.rfind('\\') {
        if let Some(rest) = base.get(pos + 1..) {
            base = rest;
        }
    }
    if base.is_empty() {
        "import.reg"
    } else {
        base
    }
}

/// Split `"Name"=rest` into name + raw rest. `None` when malformed.
fn split_name_value(line: &str) -> Option<(&str, &str)> {
    let body = line.strip_prefix('"')?;
    let end = body.find('"')?;
    let name = body.get(..end)?;
    let after = body.get(end + 1..)?.trim_start();
    let rest = after.strip_prefix('=')?.trim_start();
    Some((name, rest))
}

/// Decode the right-hand side of a value line. `None` means unparseable
/// (caller warns and skips); callers handle `"-"` deletion markers first.
fn parse_rest(rest: &str) -> Option<RegValue> {
    let trimmed = rest.trim_end();
    if let Some(quoted) = trimmed.strip_prefix('"') {
        return unquote(quoted).map(RegValue::Str);
    }
    if trimmed.len() >= 6 {
        if let Some(prefix) = trimmed.get(..6) {
            if prefix.eq_ignore_ascii_case("dword:") {
                let payload = trimmed.get(6..).unwrap_or("").trim();
                return u32::from_str_radix(payload, 16).ok().map(RegValue::Dword);
            }
        }
    }
    if trimmed.len() >= 3 {
        if let Some(prefix) = trimmed.get(..3) {
            if prefix.eq_ignore_ascii_case("hex") {
                return Some(RegValue::Other);
            }
        }
    }
    None
}

/// Decode a `.reg` quoted string (`\"` → `"`, `\\` → `\`; any other
/// backslash sequence is kept literally). `None` when unterminated.
fn unquote(quoted: &str) -> Option<String> {
    let body = quoted.strip_suffix('"')?;
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                },
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// Accumulates the values of one session block until `flush_builder`.
struct SessionBuilder {
    name: String,
    start_line: usize,
    host: Option<String>,
    port: Option<u16>,
    protocol: Option<String>,
    protocol_line: usize,
    username: Option<String>,
    agent_fwd: bool,
    agent_line: usize,
    auto_command: bool,
    seen: Vec<String>,
}

impl SessionBuilder {
    fn new(name: String, start_line: usize) -> Self {
        Self {
            name,
            start_line,
            host: None,
            port: None,
            protocol: None,
            protocol_line: start_line,
            username: None,
            agent_fwd: false,
            agent_line: start_line,
            auto_command: false,
            seen: Vec::new(),
        }
    }

    fn unknown(&self, key: &str, line_no: usize, report: &mut ImportReport, why: &str) {
        report.warn(ImportWarning::at_line(
            WarningCode::UnknownField,
            Source::Putty,
            line_no,
            format!("value '{key}' in session '{}' {why}; skipped", self.name),
        ));
    }

    fn apply(&mut self, line: &str, line_no: usize, report: &mut ImportReport) {
        let (name, rest) = match split_name_value(line) {
            Some(parsed) => parsed,
            None => {
                self.unknown_line(line_no, report);
                return;
            },
        };
        // Deletion marker: the value is absent.
        if rest.trim_end() == "-" {
            return;
        }
        // Secrets are dropped before any other dispatch, and their values
        // never reach a warning detail.
        if is_password_shaped(name) {
            report.warn(ImportWarning::at_line(
                WarningCode::PasswordDropped,
                Source::Putty,
                line_no,
                format!(
                    "dropped password-shaped value '{name}' in session '{}'",
                    self.name
                ),
            ));
            return;
        }
        let key = name.to_ascii_lowercase();
        if self.seen.iter().any(|seen| seen == &key) {
            report.warn(ImportWarning::at_line(
                WarningCode::UnknownField,
                Source::Putty,
                line_no,
                format!(
                    "duplicate value '{name}' in session '{}'; last wins",
                    self.name
                ),
            ));
        } else {
            self.seen.push(key.clone());
        }
        // KiTTY auto-commands are quarantined: warned here, hinted in notes
        // at flush, never stored or auto-run. The command text is not parsed.
        if key.contains("autocommand") {
            report.warn(ImportWarning::at_line(
                WarningCode::AutoCommandIgnored,
                Source::Putty,
                line_no,
                format!(
                    "KiTTY auto-command '{name}' in session '{}' ignored; never auto-run",
                    self.name
                ),
            ));
            self.auto_command = true;
            return;
        }
        let value = match parse_rest(rest) {
            Some(parsed) => parsed,
            None => {
                self.unknown(name, line_no, report, "is unparseable");
                return;
            },
        };
        match key.as_str() {
            "hostname" => match value {
                RegValue::Str(host) => {
                    self.host = Some(host);
                },
                _ => self.unknown(name, line_no, report, "is not a string"),
            },
            "portnumber" => match value {
                RegValue::Dword(raw) => {
                    if raw <= u16::MAX as u32 {
                        self.port = Some(raw as u16);
                    } else {
                        self.unknown(
                            name,
                            line_no,
                            report,
                            "is out of range; using protocol default",
                        );
                    }
                },
                RegValue::Str(text) => match text.trim().parse::<u16>() {
                    Ok(port) => {
                        self.port = Some(port);
                    },
                    Err(_) => {
                        self.unknown(
                            name,
                            line_no,
                            report,
                            "is not a port; using protocol default",
                        );
                    },
                },
                RegValue::Other => {
                    self.unknown(
                        name,
                        line_no,
                        report,
                        "is not a port; using protocol default",
                    );
                },
            },
            "protocol" => match value {
                RegValue::Str(proto) => {
                    self.protocol = Some(proto.to_ascii_lowercase());
                    self.protocol_line = line_no;
                },
                _ => self.unknown(name, line_no, report, "is not a string"),
            },
            "username" => match value {
                RegValue::Str(user) => {
                    self.username = Some(user);
                },
                _ => self.unknown(name, line_no, report, "is not a string"),
            },
            "agentfwd" => {
                let on = match value {
                    RegValue::Dword(raw) => raw != 0,
                    RegValue::Str(text) => match text.trim() {
                        "1" => true,
                        "0" => false,
                        _ => {
                            self.unknown(name, line_no, report, "is not 0/1");
                            false
                        },
                    },
                    RegValue::Other => {
                        self.unknown(name, line_no, report, "is not 0/1");
                        false
                    },
                };
                if on {
                    self.agent_fwd = true;
                    self.agent_line = line_no;
                }
            },
            _ => self.unknown(name, line_no, report, "is not imported"),
        }
    }

    fn unknown_line(&self, line_no: usize, report: &mut ImportReport) {
        report.warn(ImportWarning::at_line(
            WarningCode::UnknownField,
            Source::Putty,
            line_no,
            format!("unparseable line in session '{}'; skipped", self.name),
        ));
    }
}

/// Default ports for protocols the importer maps.
fn default_port(protocol: Protocol) -> Option<u16> {
    match protocol {
        Protocol::Ssh => Some(22),
        Protocol::Telnet => Some(23),
        Protocol::Rdp => Some(3389),
        _ => None,
    }
}

fn flush_builder(
    builder: SessionBuilder,
    file_name: &str,
    sessions: &mut Vec<ImportedSession>,
    report: &mut ImportReport,
    rejected: &mut usize,
) {
    let name = builder.name.clone();
    // Duplicate session names: first block kept, later ones dropped.
    if sessions.iter().any(|s| s.proposed_name == name) {
        report.warn(ImportWarning::at_line(
            WarningCode::DuplicateNameInImport,
            Source::Putty,
            builder.start_line,
            format!("duplicate session '{name}' in import; first kept"),
        ));
        *rejected += 1;
        return;
    }
    let host = match builder.host {
        Some(raw) => {
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        },
        None => None,
    };
    let host = match host {
        Some(host) => host,
        None => {
            report.warn(ImportWarning::at_line(
                WarningCode::MissingHost,
                Source::Putty,
                builder.start_line,
                format!("session '{name}' has no host; rejected"),
            ));
            *rejected += 1;
            return;
        },
    };
    let proto = builder.protocol.as_deref().unwrap_or("ssh");
    let protocol = match proto {
        "ssh" => Protocol::Ssh,
        "telnet" => Protocol::Telnet,
        "rdp" => Protocol::Rdp,
        "serial" => Protocol::Serial,
        _ => {
            report.warn(ImportWarning::at_line(
                WarningCode::UnsupportedProtocol,
                Source::Putty,
                builder.protocol_line,
                format!("session '{name}' uses unsupported protocol '{proto}'; rejected"),
            ));
            *rejected += 1;
            return;
        },
    };
    let auth = if builder.agent_fwd {
        report.warn(ImportWarning::at_line(
            WarningCode::AgentForwardingDisabled,
            Source::Putty,
            builder.agent_line,
            format!("agent forwarding in session '{name}' disabled on import"),
        ));
        AuthMethod::Agent { forward: false }
    } else {
        AuthMethod::Password
    };
    let username = match builder.username {
        Some(raw) => {
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        },
        None => None,
    };
    let mut notes = String::new();
    if builder.auto_command {
        notes.push_str("KiTTY auto-command ignored (never auto-run).");
    }
    let spec = SessionSpec {
        name: name.clone(),
        protocol,
        host: Some(host),
        port: if protocol == Protocol::Serial {
            None
        } else {
            builder.port.or_else(|| default_port(protocol))
        },
        username,
        auth,
        tags: Vec::new(),
        notes,
        x11_forwarding: false,
        serial: None,
        forwards: Vec::new(),
    };
    let mut imported = ImportedSession::new(name, spec, Source::Putty, file_basename(file_name));
    if builder.auto_command {
        imported.disabled_flags.push("auto-command".to_string());
    }
    sessions.push(imported);
}
