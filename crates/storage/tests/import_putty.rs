//! PuTTY/KiTTY `.reg` importer integration tests.
//!
//! Fixtures under `tests/fixtures/import/` are synthetic exports using
//! TEST-NET-1 documentation addresses (192.0.2.x) and `testuser` only.

use mbxt_core::{AuthMethod, Protocol};
use mbxt_storage::import::putty::parse_putty_reg;
use mbxt_storage::import::{ImportReport, ImportedSession};

const BASIC: &str = include_str!("../../../tests/fixtures/import/putty_basic.reg");
const KITTY: &str = include_str!("../../../tests/fixtures/import/putty_kitty_ext.reg");
const EDGE: &str = include_str!("../../../tests/fixtures/import/putty_edge.reg");
const SECRET_DROP: &str = include_str!("../../../tests/fixtures/import/putty_secret_drop.reg");

fn parse_ok(text: &str, file: &str) -> (Vec<ImportedSession>, ImportReport) {
    match parse_putty_reg(text, file) {
        Ok(out) => out,
        Err(err) => panic!("parse of {file} failed: {err:?}"),
    }
}

fn find<'a>(sessions: &'a [ImportedSession], name: &str) -> Option<&'a ImportedSession> {
    sessions.iter().find(|s| s.proposed_name == name)
}

fn codes(report: &ImportReport) -> Vec<&'static str> {
    report.warning_codes()
}

fn details(report: &ImportReport) -> String {
    report
        .warnings
        .iter()
        .map(|w| w.detail.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn import_basic_two_sessions_with_dword_ports() {
    let (sessions, report) = parse_ok(BASIC, "putty_basic.reg");
    assert_eq!(sessions.len(), 2);
    assert_eq!(report.imported, 2);
    assert_eq!(report.rejected, 0);

    if let Some(web) = find(&sessions, "web front") {
        assert_eq!(web.spec.protocol, Protocol::Ssh);
        assert_eq!(web.spec.host.as_deref(), Some("192.0.2.10"));
        assert_eq!(web.spec.port, Some(22));
        assert_eq!(web.spec.username.as_deref(), Some("testuser"));
    } else {
        panic!("web front missing (%20 unescape)");
    }

    if let Some(db) = find(&sessions, "db replica") {
        assert_eq!(db.spec.protocol, Protocol::Telnet);
        assert_eq!(db.spec.host.as_deref(), Some("192.0.2.11"));
        assert_eq!(db.spec.port, Some(23));
        assert_eq!(db.spec.username.as_deref(), Some("testuser"));
    } else {
        panic!("db replica missing");
    }
}

#[test]
fn import_kitty_extensions_ignored_with_warnings() {
    let (sessions, report) = parse_ok(KITTY, "putty_kitty_ext.reg");
    assert_eq!(sessions.len(), 1);
    assert_eq!(report.rejected, 0);
    let c = codes(&report);
    assert!(c.contains(&"auto-command-ignored"));
    assert!(c.contains(&"unknown-field"));

    if let Some(s) = find(&sessions, "kitty box") {
        assert_eq!(s.spec.protocol, Protocol::Ssh);
        assert_eq!(s.spec.host.as_deref(), Some("192.0.2.12"));
        assert_eq!(s.spec.port, Some(22));
        assert_eq!(s.spec.auth, AuthMethod::Password);
        assert!(s.spec.forwards.is_empty());
        // Quarantined: at most a notes hint, never the command itself.
        assert!(s.spec.notes.contains("auto-command"));
        assert!(!s.spec.notes.contains("uptime"));
        assert!(s.disabled_flags.iter().any(|f| f == "auto-command"));
        let visible = format!(
            "{}|{}|{}|{:?}|{}",
            s.proposed_name,
            s.provenance,
            s.spec.notes,
            s.spec,
            details(&report)
        );
        assert!(
            !visible.contains("uptime"),
            "auto-command leaked: {visible}"
        );
    } else {
        panic!("kitty box missing (KiTTY key path)");
    }
}

#[test]
fn import_edge_crlf_bad_dword_unknown_dup() {
    // The fixture is stored CRLF; the re-encoded variant proves LF works too
    // (and stays correct even if a checkout normalizes line endings).
    let crlf = EDGE.replace('\n', "\r\n");
    for text in [EDGE, crlf.as_str()] {
        let (sessions, report) = parse_ok(text, "putty_edge.reg");
        assert_eq!(sessions.len(), 1);
        assert_eq!(report.rejected, 1);
        let c = codes(&report);
        assert!(c.contains(&"unknown-field"));
        assert!(c.contains(&"unsupported-protocol"));

        if let Some(edge) = find(&sessions, "edge box") {
            // Duplicate HostName: last wins.
            assert_eq!(edge.spec.host.as_deref(), Some("192.0.2.21"));
            // Bad dword port falls back to the protocol default.
            assert_eq!(edge.spec.port, Some(22));
            assert_eq!(edge.spec.username.as_deref(), Some("testuser"));
        } else {
            panic!("edge box missing");
        }
        // Unsupported Protocol value must reject, never coerce to SSH.
        assert!(find(&sessions, "raw box").is_none());
    }
}

#[test]
fn import_secret_drop_proxy_password() {
    const SECRET: &str = "hunter2-proxy-s3cr3t";
    assert!(SECRET_DROP.contains(SECRET));
    let (sessions, report) = parse_ok(SECRET_DROP, "putty_secret_drop.reg");
    assert_eq!(sessions.len(), 1);
    assert!(codes(&report).contains(&"password-dropped"));

    if let Some(s) = find(&sessions, "proxy box") {
        assert_eq!(s.spec.host.as_deref(), Some("192.0.2.13"));
        assert_eq!(s.spec.port, Some(22));
        let visible = format!(
            "{}|{}|{}|{:?}|{}",
            s.proposed_name,
            s.provenance,
            s.spec.notes,
            s.spec,
            details(&report)
        );
        assert!(!visible.contains(SECRET), "secret leaked: {visible}");
        for flag in s.disabled_flags.iter() {
            assert!(!flag.contains(SECRET), "secret in flag: {flag}");
        }
    } else {
        panic!("proxy box missing");
    }
    for w in report.warnings.iter() {
        assert!(
            !w.detail.contains(SECRET),
            "secret echoed in warning: {}",
            w.detail
        );
    }
}

#[test]
fn import_agent_forwarding_disabled_with_warning() {
    let text = "[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\agent%20box]\n\
        \"HostName\"=\"192.0.2.14\"\n\
        \"Protocol\"=\"ssh\"\n\
        \"AgentFwd\"=dword:00000001\n";
    let (sessions, report) = parse_ok(text, "agent.reg");
    assert_eq!(sessions.len(), 1);
    assert!(codes(&report).contains(&"agent-forwarding-disabled"));
    if let Some(s) = find(&sessions, "agent box") {
        assert_eq!(s.spec.auth, AuthMethod::Agent { forward: false });
    } else {
        panic!("agent box missing");
    }
}

#[test]
fn import_empty_host_and_unnamed_rejected() {
    let text = "[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\lonely]\n\
        \"Protocol\"=\"ssh\"\n\
        \"UserName\"=\"testuser\"\n\
        [HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\]\n\
        \"HostName\"=\"192.0.2.15\"\n";
    let (sessions, report) = parse_ok(text, "rejects.reg");
    assert!(sessions.is_empty());
    assert_eq!(report.rejected, 2);
    let c = codes(&report);
    assert!(c.contains(&"missing-host"));
    assert!(c.contains(&"unnamed-session-rejected"));
}

#[test]
fn import_duplicate_session_names_first_kept() {
    let text = "[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\dup]\n\
        \"HostName\"=\"192.0.2.16\"\n\
        [HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\dup]\n\
        \"HostName\"=\"192.0.2.17\"\n";
    let (sessions, report) = parse_ok(text, "dups.reg");
    assert_eq!(sessions.len(), 1);
    assert!(codes(&report).contains(&"duplicate-name-in-import"));
    if let Some(dup) = find(&sessions, "dup") {
        assert_eq!(dup.spec.host.as_deref(), Some("192.0.2.16"));
    } else {
        panic!("dup missing");
    }
}

#[test]
fn import_too_many_sessions_rejected() {
    let mut text = String::from("Windows Registry Editor Version 5.00\n\n");
    for i in 0..=mbxt_storage::import::MAX_SESSIONS {
        text.push_str(&format!(
            "[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\bulk{i}]\n\
            \"HostName\"=\"192.0.2.1\"\n"
        ));
    }
    match parse_putty_reg(&text, "bulk.reg") {
        Err(mbxt_storage::import::ImportError::TooManySessions(cap)) => {
            assert_eq!(cap, mbxt_storage::import::MAX_SESSIONS);
        },
        Err(other) => panic!("expected TooManySessions, got {other:?}"),
        Ok(_) => panic!("expected TooManySessions error"),
    }
}
