//! MobaXterm XML import: session export → session drafts.
//!
//! Contracts: no DTD/entity expansion (XXE-safe hand parsing, no
//! network fetch); folder paths become tags; unknown protocols are
//! rejected with reason, never coerced to SSH; password-shaped
//! attributes dropped with a warning.

use super::model::{ImportWarning, ImportedSession, WarningCode};
use super::report::ImportReport;
use super::{check_size, ImportError, Source, MAX_LINE_BYTES, MAX_SESSIONS};
use mbxt_core::{AuthMethod, Protocol, SerialParams};

/// Parse MobaXterm XML export text.
pub fn parse_mobaxterm_xml(
    text: &str,
    file_name: &str,
) -> Result<(Vec<ImportedSession>, ImportReport), ImportError> {
    check_size(text)?;
    for (idx, line) in text.lines().enumerate() {
        if line.len() > MAX_LINE_BYTES {
            return Err(ImportError::Malformed(
                idx + 1,
                "line exceeds maximum length".to_string(),
            ));
        }
    }

    let base = basename(file_name);
    let mut report = ImportReport::default();
    let mut sessions: Vec<ImportedSession> = Vec::new();
    let mut seen_names: Vec<String> = Vec::new();
    let mut rejected: usize = 0;
    let mut session_elements: usize = 0;

    let mut stack: Vec<StackEntry> = Vec::new();
    let mut root_seen = false;
    let mut root_closed = false;

    let len = text.len();
    let bytes = text.as_bytes();
    let mut pos: usize = 0;

    while pos < len {
        // Find next '<'.
        let mut lt_opt: Option<usize> = None;
        let mut i = pos;
        while i < len {
            if let Some(b) = bytes.get(i) {
                if *b == b'<' {
                    lt_opt = Some(i);
                    break;
                }
                i += 1;
            } else {
                break;
            }
        }
        let lt = match lt_opt {
            Some(v) => v,
            None => break,
        };
        let rest = match text.get(lt..) {
            Some(r) => r,
            None => {
                return Err(ImportError::Malformed(
                    line_of(text, lt),
                    "invalid byte offset".to_string(),
                ));
            },
        };

        // Comments, CDATA, PIs, and declarations are handled before
        // generic tag scanning so embedded '>' bytes cannot confuse us.
        if rest.starts_with("<!--") {
            if let Some(end_rel) = rest.find("-->") {
                pos = lt + end_rel + 3;
                continue;
            }
            return Err(ImportError::Malformed(
                line_of(text, lt),
                "unclosed comment".to_string(),
            ));
        }
        if rest.starts_with("<![CDATA[") {
            if let Some(end_rel) = rest.find("]]>") {
                pos = lt + end_rel + 3;
                continue;
            }
            return Err(ImportError::Malformed(
                line_of(text, lt),
                "unclosed CDATA section".to_string(),
            ));
        }
        if rest.starts_with("<?") {
            if let Some(end_rel) = rest.find("?>") {
                pos = lt + end_rel + 2;
                continue;
            }
            return Err(ImportError::Malformed(
                line_of(text, lt),
                "unclosed processing instruction".to_string(),
            ));
        }
        if rest.starts_with("<!") {
            // No DTD / entity expansion, ever.
            let head: String = rest.chars().take(32).collect();
            let head_upper = head.to_ascii_uppercase();
            if head_upper.contains("DOCTYPE") {
                return Err(ImportError::Malformed(
                    line_of(text, lt),
                    "DOCTYPE declarations are not supported".to_string(),
                ));
            }
            if head_upper.contains("ENTITY") {
                return Err(ImportError::Malformed(
                    line_of(text, lt),
                    "ENTITY declarations are not supported".to_string(),
                ));
            }
            return Err(ImportError::Malformed(
                line_of(text, lt),
                "unsupported markup declaration".to_string(),
            ));
        }

        // Generic tag: find '>' honoring single/double quotes.
        let mut gt_opt: Option<usize> = None;
        let mut quote: Option<u8> = None;
        let mut k = lt + 1;
        while k < len {
            let b = match bytes.get(k) {
                Some(v) => *v,
                None => break,
            };
            if let Some(q) = quote {
                if b == q {
                    quote = None;
                }
            } else if b == b'"' || b == b'\'' {
                quote = Some(b);
            } else if b == b'>' {
                gt_opt = Some(k);
                break;
            }
            k += 1;
        }
        let gt = match gt_opt {
            Some(v) => v,
            None => {
                return Err(ImportError::Malformed(
                    line_of(text, lt),
                    "unclosed tag".to_string(),
                ));
            },
        };
        let inner = match text.get(lt + 1..gt) {
            Some(s) => s,
            None => {
                return Err(ImportError::Malformed(
                    line_of(text, lt),
                    "invalid tag bytes".to_string(),
                ));
            },
        };
        let trimmed = inner.trim();
        if trimmed.is_empty() {
            return Err(ImportError::Malformed(
                line_of(text, lt),
                "empty tag".to_string(),
            ));
        }
        if trimmed.contains('<') {
            return Err(ImportError::Malformed(
                line_of(text, lt),
                "unexpected '<' inside tag".to_string(),
            ));
        }

        if trimmed.starts_with('/') {
            // Closing tag.
            let name = trimmed.get(1..).map(str::trim).unwrap_or("");
            if name.is_empty() {
                return Err(ImportError::Malformed(
                    line_of(text, lt),
                    "empty closing tag".to_string(),
                ));
            }
            // Closing tag names must be plain (no attributes).
            if name.contains(char::is_whitespace) {
                return Err(ImportError::Malformed(
                    line_of(text, lt),
                    "malformed closing tag".to_string(),
                ));
            }
            let top = match stack.pop() {
                Some(t) => t,
                None => {
                    return Err(ImportError::Malformed(
                        line_of(text, lt),
                        "stray closing tag".to_string(),
                    ));
                },
            };
            if top.tag != name.to_ascii_lowercase() {
                return Err(ImportError::Malformed(
                    line_of(text, lt),
                    format!(
                        "mismatched closing tag: expected '{}', found '{}'",
                        top.tag, name
                    ),
                ));
            }
            if stack.is_empty() {
                root_closed = true;
            }
            pos = gt + 1;
            continue;
        }

        // Opening (possibly self-closing) tag.
        let self_closing = trimmed.ends_with('/');
        let body = if self_closing {
            match trimmed.get(..trimmed.len().saturating_sub(1)) {
                Some(s) => s.trim_end(),
                None => {
                    return Err(ImportError::Malformed(
                        line_of(text, lt),
                        "invalid tag".to_string(),
                    ));
                },
            }
        } else {
            trimmed
        };
        let (tag_name, attr_text) = split_tag_head(body);
        if tag_name.is_empty() {
            return Err(ImportError::Malformed(
                line_of(text, lt),
                "empty element name".to_string(),
            ));
        }
        if !is_valid_tag_name(tag_name) {
            return Err(ImportError::Malformed(
                line_of(text, lt),
                format!("invalid element name '{tag_name}'"),
            ));
        }
        let line_no = line_of(text, lt);
        let raw_attrs = parse_attrs(attr_text, line_no)?;
        let lower = tag_name.to_ascii_lowercase();

        if is_session_tag(&lower) {
            if stack.is_empty() && root_closed {
                return Err(ImportError::Malformed(
                    line_no,
                    "content after document root".to_string(),
                ));
            }
            // A top-level session doubles as the document root.
            if stack.is_empty() && !root_seen {
                root_seen = true;
                root_closed = self_closing;
            }
            let folder_tags: Vec<String> = stack.iter().filter_map(|e| e.folder.clone()).collect();
            process_session(
                &raw_attrs,
                &folder_tags,
                line_no,
                base,
                &mut sessions,
                &mut report,
                &mut seen_names,
                &mut rejected,
                &mut session_elements,
            )?;
            if !self_closing {
                stack.push(StackEntry {
                    tag: lower,
                    folder: None,
                });
                root_seen = true;
            }
        } else if lower == "folder" {
            if stack.is_empty() && root_closed {
                return Err(ImportError::Malformed(
                    line_no,
                    "content after document root".to_string(),
                ));
            }
            if self_closing {
                if stack.is_empty() && !root_seen {
                    root_seen = true;
                    root_closed = true;
                }
            } else {
                let folder_name = folder_attr(&raw_attrs, line_no)?;
                stack.push(StackEntry {
                    tag: lower,
                    folder: folder_name,
                });
                root_seen = true;
            }
        } else if is_transparent_tag(&lower) {
            if stack.is_empty() && root_closed {
                return Err(ImportError::Malformed(
                    line_no,
                    "content after document root".to_string(),
                ));
            }
            if self_closing {
                if stack.is_empty() && !root_seen {
                    root_seen = true;
                    root_closed = true;
                }
            } else {
                stack.push(StackEntry {
                    tag: lower,
                    folder: None,
                });
                root_seen = true;
            }
        } else {
            // Unknown element: report, keep parsing children.
            if stack.is_empty() && root_closed {
                return Err(ImportError::Malformed(
                    line_no,
                    "content after document root".to_string(),
                ));
            }
            report.warn(ImportWarning::at_line(
                WarningCode::UnknownField,
                Source::MobaXterm,
                line_no,
                format!("unknown element '{tag_name}' ignored"),
            ));
            if self_closing {
                if stack.is_empty() && !root_seen {
                    root_seen = true;
                    root_closed = true;
                }
            } else {
                stack.push(StackEntry {
                    tag: lower,
                    folder: None,
                });
                root_seen = true;
            }
        }
        pos = gt + 1;
    }

    if !stack.is_empty() {
        return Err(ImportError::Malformed(
            line_of(text, len.saturating_sub(1)),
            "unclosed element at end of input".to_string(),
        ));
    }
    if !root_seen {
        return Err(ImportError::Malformed(
            1,
            "empty document: no root element".to_string(),
        ));
    }

    report.imported = sessions.len();
    report.rejected = rejected;
    Ok((sessions, report))
}

struct StackEntry {
    tag: String,
    folder: Option<String>,
}

fn basename(path: &str) -> &str {
    let bytes = path.as_bytes();
    let mut last: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        if let Some(b) = bytes.get(i) {
            if *b == b'/' || *b == b'\\' {
                last = Some(i);
            }
        }
        i += 1;
    }
    if let Some(idx) = last {
        if let Some(s) = path.get(idx + 1..) {
            if !s.is_empty() {
                return s;
            }
        }
        return path;
    }
    path
}

fn line_of(text: &str, offset: usize) -> usize {
    let len = text.len();
    let end = if offset < len { offset } else { len };
    let mut count = 1usize;
    if let Some(prefix) = text.as_bytes().get(..end) {
        for b in prefix {
            if *b == b'\n' {
                count += 1;
            }
        }
    }
    count
}

fn split_tag_head(body: &str) -> (&str, &str) {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(b) = bytes.get(i) {
            if b.is_ascii_whitespace() {
                break;
            }
        } else {
            break;
        }
        i += 1;
    }
    let head = body.get(..i).unwrap_or("");
    let rest = body.get(i..).unwrap_or("").trim_start();
    (head, rest)
}

fn is_valid_tag_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    for c in name.chars() {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '.' || c == '-') {
            return false;
        }
    }
    true
}

fn is_session_tag(lower: &str) -> bool {
    matches!(lower, "session" | "bookmark" | "connection" | "entry")
}

fn is_transparent_tag(lower: &str) -> bool {
    matches!(
        lower,
        "mobaxterm"
            | "sessions"
            | "bookmarks"
            | "connections"
            | "root"
            | "folders"
            | "folderlist"
            | "sessionlist"
    )
}

/// Parse `name="value"` pairs; values stay entity-encoded here and are
/// decoded per-attribute so one bad entity reports its session line.
fn parse_attrs(s: &str, line: usize) -> Result<Vec<(String, String)>, ImportError> {
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    let mut out: Vec<(String, String)> = Vec::new();
    while i < n {
        while i < n {
            if let Some(b) = bytes.get(i) {
                if !b.is_ascii_whitespace() {
                    break;
                }
            } else {
                break;
            }
            i += 1;
        }
        if i >= n {
            break;
        }
        let start = i;
        while i < n {
            if let Some(b) = bytes.get(i) {
                if b.is_ascii_whitespace() || *b == b'=' {
                    break;
                }
            } else {
                break;
            }
            i += 1;
        }
        let name = match s.get(start..i) {
            Some(v) => v,
            None => {
                return Err(ImportError::Malformed(
                    line,
                    "invalid attribute name".to_string(),
                ));
            },
        };
        if name.is_empty() {
            return Err(ImportError::Malformed(
                line,
                "empty attribute name".to_string(),
            ));
        }
        if !is_valid_attr_name(name) {
            return Err(ImportError::Malformed(
                line,
                format!("invalid attribute name '{name}'"),
            ));
        }
        while i < n {
            if let Some(b) = bytes.get(i) {
                if !b.is_ascii_whitespace() {
                    break;
                }
            } else {
                break;
            }
            i += 1;
        }
        if i >= n {
            return Err(ImportError::Malformed(
                line,
                format!("attribute '{name}' missing '='"),
            ));
        }
        if bytes.get(i) != Some(&b'=') {
            return Err(ImportError::Malformed(
                line,
                format!("attribute '{name}' missing '='"),
            ));
        }
        i += 1;
        while i < n {
            if let Some(b) = bytes.get(i) {
                if !b.is_ascii_whitespace() {
                    break;
                }
            } else {
                break;
            }
            i += 1;
        }
        if i >= n {
            return Err(ImportError::Malformed(
                line,
                format!("attribute '{name}' missing value"),
            ));
        }
        let q = match bytes.get(i) {
            Some(v) => *v,
            None => {
                return Err(ImportError::Malformed(
                    line,
                    format!("attribute '{name}' missing value"),
                ));
            },
        };
        if q != b'"' && q != b'\'' {
            return Err(ImportError::Malformed(
                line,
                format!("attribute '{name}' value must be quoted"),
            ));
        }
        i += 1;
        let vstart = i;
        let mut vend: Option<usize> = None;
        let mut k = i;
        while k < n {
            if let Some(b) = bytes.get(k) {
                if *b == q {
                    vend = Some(k);
                    break;
                }
            } else {
                break;
            }
            k += 1;
        }
        let end = match vend {
            Some(e) => e,
            None => {
                return Err(ImportError::Malformed(
                    line,
                    format!("attribute '{name}' value is unclosed"),
                ));
            },
        };
        let raw = match s.get(vstart..end) {
            Some(v) => v,
            None => {
                return Err(ImportError::Malformed(
                    line,
                    format!("attribute '{name}' has invalid bytes"),
                ));
            },
        };
        if raw.contains('<') {
            return Err(ImportError::Malformed(
                line,
                format!("attribute '{name}' contains '<'"),
            ));
        }
        // Duplicate attribute (case-insensitive) is malformed XML.
        let norm_new = normalize_key(name);
        let mut dup = false;
        for (existing, _) in out.iter() {
            if normalize_key(existing) == norm_new {
                dup = true;
                break;
            }
        }
        if dup {
            return Err(ImportError::Malformed(
                line,
                format!("duplicate attribute '{name}'"),
            ));
        }
        out.push((name.to_string(), raw.to_string()));
        i = end + 1;
    }
    Ok(out)
}

fn is_valid_attr_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    for c in name.chars() {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '.' || c == '-') {
            return false;
        }
    }
    true
}

/// Lowercase and strip `-`/`_` so `keepAlive`, `keep-alive`, and
/// `keep_alive` compare equal.
fn normalize_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for c in key.chars() {
        if c == '-' || c == '_' {
            continue;
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Decode the five predefined XML entities; anything else is rejected
/// (no DTD / external / general-entity expansion).
fn decode_entities(s: &str) -> Result<String, ()> {
    if !s.contains('&') {
        return Ok(s.to_string());
    }
    let mut out = String::with_capacity(s.len());
    let mut idx = 0;
    while idx < s.len() {
        let slice = match s.get(idx..) {
            Some(t) => t,
            None => return Err(()),
        };
        let rel = match slice.find('&') {
            Some(r) => r,
            None => {
                if let Some(rest) = s.get(idx..) {
                    out.push_str(rest);
                } else {
                    return Err(());
                }
                break;
            },
        };
        let abs_amp = idx + rel;
        if let Some(pre) = s.get(idx..abs_amp) {
            out.push_str(pre);
        } else {
            return Err(());
        }
        let after = match s.get(abs_amp..) {
            Some(t) => t,
            None => return Err(()),
        };
        let semi = match after.find(';') {
            Some(r) => r,
            None => return Err(()),
        };
        let entity = match s.get(abs_amp..abs_amp + semi + 1) {
            Some(e) => e,
            None => return Err(()),
        };
        match entity {
            "&amp;" => out.push('&'),
            "&lt;" => out.push('<'),
            "&gt;" => out.push('>'),
            "&quot;" => out.push('"'),
            "&apos;" => out.push('\''),
            _ => return Err(()),
        }
        idx = abs_amp + semi + 1;
    }
    Ok(out)
}

fn folder_attr(raw_attrs: &[(String, String)], line: usize) -> Result<Option<String>, ImportError> {
    for (k, v) in raw_attrs.iter() {
        if normalize_key(k) == "name" {
            let decoded = decode_entities(v).map_err(|()| {
                ImportError::Malformed(line, "unsupported entity in folder name".to_string())
            })?;
            let trimmed = decoded.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            return Ok(Some(trimmed.to_string()));
        }
    }
    Ok(None)
}

fn default_port(protocol: Protocol) -> Option<u16> {
    match protocol {
        Protocol::Ssh => Some(22),
        Protocol::Telnet => Some(23),
        Protocol::Ftp => Some(21),
        Protocol::Sftp => Some(22),
        Protocol::Rdp => Some(3389),
        Protocol::Vnc => Some(5900),
        Protocol::X11 | Protocol::Serial | Protocol::Custom => None,
    }
}

fn map_protocol(raw: &str) -> Option<Protocol> {
    match raw {
        "ssh" => Some(Protocol::Ssh),
        "telnet" => Some(Protocol::Telnet),
        "ftp" => Some(Protocol::Ftp),
        "sftp" => Some(Protocol::Sftp),
        "rdp" => Some(Protocol::Rdp),
        "vnc" => Some(Protocol::Vnc),
        "x11" => Some(Protocol::X11),
        "serial" | "com" | "tty" | "uart" => Some(Protocol::Serial),
        "custom" => Some(Protocol::Custom),
        _ => None,
    }
}

fn is_name_key(norm: &str) -> bool {
    matches!(norm, "name" | "sessionname" | "title")
}

fn is_proto_key(norm: &str) -> bool {
    matches!(norm, "protocol" | "type" | "sessiontype" | "proto")
}

fn is_host_key(norm: &str) -> bool {
    matches!(norm, "host" | "hostname" | "address" | "ip" | "remotehost")
}

fn is_port_key(norm: &str) -> bool {
    norm == "port"
}

fn is_user_key(norm: &str) -> bool {
    matches!(norm, "username" | "user" | "login" | "logname")
}

fn is_device_key(norm: &str) -> bool {
    matches!(
        norm,
        "device" | "serialport" | "comport" | "tty" | "devicename" | "portname"
    )
}

fn is_baud_key(norm: &str) -> bool {
    matches!(norm, "baud" | "baudrate" | "speed")
}

fn is_mapped_key(norm: &str) -> bool {
    is_name_key(norm)
        || is_proto_key(norm)
        || is_host_key(norm)
        || is_port_key(norm)
        || is_user_key(norm)
        || is_device_key(norm)
        || is_baud_key(norm)
}

fn is_safe_pref(norm: &str) -> bool {
    matches!(
        norm,
        "color"
            | "colours"
            | "colors"
            | "font"
            | "fontsize"
            | "fontname"
            | "background"
            | "foreground"
            | "cursor"
            | "charset"
            | "encoding"
            | "terminal"
            | "term"
            | "keepalive"
            | "reconnect"
            | "timeout"
            | "retry"
            | "icon"
            | "image"
            | "display"
            | "compression"
            | "persistence"
            | "macro"
            | "macros"
            | "startup"
            | "description"
            | "comment"
            | "comments"
            | "notes"
            | "note"
            | "folder"
            | "path"
            | "group"
    )
}

fn first_mapped(raw_attrs: &[(String, String)], pred: fn(&str) -> bool) -> Option<String> {
    for (k, v) in raw_attrs.iter() {
        if pred(normalize_key(k).as_str()) {
            return Some(v.clone());
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn process_session(
    raw_attrs: &[(String, String)],
    folder_tags: &[String],
    line: usize,
    file_base: &str,
    sessions: &mut Vec<ImportedSession>,
    report: &mut ImportReport,
    seen_names: &mut Vec<String>,
    rejected: &mut usize,
    session_elements: &mut usize,
) -> Result<(), ImportError> {
    *session_elements += 1;
    if *session_elements > MAX_SESSIONS {
        return Err(ImportError::TooManySessions(MAX_SESSIONS));
    }

    // Decode values (entity errors are malformed input, not warnings).
    let mut attrs: Vec<(String, String)> = Vec::with_capacity(raw_attrs.len());
    for (k, v) in raw_attrs.iter() {
        let decoded = decode_entities(v).map_err(|()| {
            ImportError::Malformed(line, "unsupported entity reference".to_string())
        })?;
        attrs.push((k.clone(), decoded));
    }

    // Proposed name (needed for warning context).
    let raw_name = first_mapped(&attrs, is_name_key);
    let name = raw_name.map(|v| v.trim().to_string()).unwrap_or_default();
    if name.is_empty() {
        *rejected += 1;
        report.warn(ImportWarning::at_line(
            WarningCode::UnnamedSessionRejected,
            Source::MobaXterm,
            line,
            "session without a name rejected".to_string(),
        ));
        return Ok(());
    }

    // Password-shaped attributes are dropped before anything else; the
    // value never reaches warnings, notes, or the draft.
    let mut saw_password = false;
    let mut password_keys: Vec<String> = Vec::new();
    for (k, _) in attrs.iter() {
        if super::model::is_password_shaped(k) {
            saw_password = true;
            password_keys.push(k.clone());
        }
    }
    // Duplicate names within one file: first kept, later rejected.
    if seen_names.iter().any(|n| *n == name) {
        *rejected += 1;
        report.warn(ImportWarning::at_line(
            WarningCode::DuplicateNameInImport,
            Source::MobaXterm,
            line,
            format!("duplicate '{name}' in import; first kept"),
        ));
        // Still report the password drop so the user knows a secret was
        // present on the discarded duplicate.
        if saw_password {
            report.warn(ImportWarning::at_line(
                WarningCode::PasswordDropped,
                Source::MobaXterm,
                line,
                format!("dropped password-shaped attribute(s) on session '{name}'"),
            ));
        }
        return Ok(());
    }
    seen_names.push(name.clone());

    if saw_password {
        report.warn(ImportWarning::at_line(
            WarningCode::PasswordDropped,
            Source::MobaXterm,
            line,
            format!("dropped password-shaped attribute(s) on session '{name}'"),
        ));
    }

    // Protocol (missing/empty defaults to SSH; unknown is rejected, never
    // coerced).
    let raw_proto = first_mapped(&attrs, is_proto_key);
    let proto_text = raw_proto
        .map(|v| v.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let proto_text = if proto_text.is_empty() {
        "ssh".to_string()
    } else {
        proto_text
    };
    let protocol = match map_protocol(proto_text.as_str()) {
        Some(p) => p,
        None => {
            *rejected += 1;
            let short = truncate_for_detail(&proto_text, 48);
            report.warn(ImportWarning::at_line(
                WarningCode::UnsupportedProtocol,
                Source::MobaXterm,
                line,
                format!("unsupported protocol '{short}' on session '{name}' rejected"),
            ));
            return Ok(());
        },
    };

    // Unknown / safe-preference reporting (password-shaped already handled,
    // mapped keys are silent).
    for (k, _) in attrs.iter() {
        if super::model::is_password_shaped(k) {
            continue;
        }
        let norm = normalize_key(k);
        if is_mapped_key(norm.as_str()) {
            continue;
        }
        if is_safe_pref(norm.as_str()) {
            report.warn(ImportWarning::at_line(
                WarningCode::SafePreferenceNoted,
                Source::MobaXterm,
                line,
                format!("safe preference '{k}' on session '{name}' noted (not imported)"),
            ));
        } else {
            report.warn(ImportWarning::at_line(
                WarningCode::UnknownField,
                Source::MobaXterm,
                line,
                format!("unknown attribute '{k}' on session '{name}' ignored"),
            ));
        }
    }

    let host_raw = first_mapped(&attrs, is_host_key)
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    let port_raw = first_mapped(&attrs, is_port_key)
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    let user_raw = first_mapped(&attrs, is_user_key)
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    let device_raw = first_mapped(&attrs, is_device_key)
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    let baud_raw = first_mapped(&attrs, is_baud_key)
        .map(|v| v.trim().to_string())
        .unwrap_or_default();

    let username = if user_raw.is_empty() {
        None
    } else {
        Some(user_raw)
    };

    // Port: missing/empty uses the protocol default silently; present but
    // invalid falls back to the default with a warning.
    let mut port = default_port(protocol);
    if !port_raw.is_empty() {
        match port_raw.parse::<u32>() {
            Ok(v) if (1..=65_535).contains(&v) => {
                port = Some(v as u16);
            },
            _ => {
                port = default_port(protocol);
                let short = truncate_for_detail(&port_raw, 48);
                let fallback = match port {
                    Some(p) => p.to_string(),
                    None => "none".to_string(),
                };
                report.warn(ImportWarning::at_line(
                    WarningCode::UnknownField,
                    Source::MobaXterm,
                    line,
                    format!("invalid port '{short}' on session '{name}'; using default {fallback}"),
                ));
            },
        }
    }

    let (host, serial) = if protocol == Protocol::Serial {
        let mut device = device_raw;
        if device.is_empty() {
            device = host_raw;
        }
        if device.trim().is_empty() {
            *rejected += 1;
            report.warn(ImportWarning::at_line(
                WarningCode::MissingHost,
                Source::MobaXterm,
                line,
                format!("serial session '{name}' missing device rejected"),
            ));
            return Ok(());
        }
        let baud_rate = if baud_raw.is_empty() {
            115_200
        } else {
            match baud_raw.parse::<u32>() {
                Ok(v) if (50..=4_000_000).contains(&v) => v,
                _ => {
                    let short = truncate_for_detail(&baud_raw, 48);
                    report.warn(ImportWarning::at_line(
                        WarningCode::UnknownField,
                        Source::MobaXterm,
                        line,
                        format!("invalid baud '{short}' on session '{name}'; using default 115200"),
                    ));
                    115_200
                },
            }
        };
        let params = SerialParams {
            device: device.trim().to_string(),
            baud_rate,
            ..SerialParams::default()
        };
        (None, Some(params))
    } else {
        if host_raw.is_empty() {
            *rejected += 1;
            report.warn(ImportWarning::at_line(
                WarningCode::MissingHost,
                Source::MobaXterm,
                line,
                format!("session '{name}' missing host rejected"),
            ));
            return Ok(());
        }
        (Some(host_raw), None)
    };

    let spec = mbxt_core::SessionSpec {
        name: name.clone(),
        protocol,
        host,
        port,
        username,
        auth: AuthMethod::Password,
        tags: folder_tags.to_vec(),
        notes: String::new(),
        x11_forwarding: false,
        serial,
        forwards: Vec::new(),
    };
    sessions.push(ImportedSession::new(
        name,
        spec,
        Source::MobaXterm,
        file_base,
    ));
    let _ = password_keys;
    Ok(())
}

fn truncate_for_detail(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && s.get(..end).is_none() {
        end -= 1;
    }
    let mut out = s.get(..end).unwrap_or("").to_string();
    out.push('…');
    out
}
