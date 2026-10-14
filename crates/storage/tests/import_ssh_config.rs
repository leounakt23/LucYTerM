//! OpenSSH `ssh_config` importer integration tests (plain asserts).
//!
//! Fixtures live in the workspace `tests/fixtures/import/` directory and
//! use TEST-NET-1 documentation addresses only (`example.com`,
//! `192.0.2.x`, `testuser`).

use mbxt_core::{AuthMethod, ForwardType};
use mbxt_storage::import::ssh_config::parse_ssh_config;
use mbxt_storage::import::{ImportError, ImportedSession, MAX_SESSIONS};

fn fixture(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/import")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

fn no_include(_: &str) -> Option<String> {
    None
}

fn by_name<'a>(sessions: &'a [ImportedSession], name: &str) -> &'a ImportedSession {
    sessions
        .iter()
        .find(|s| s.proposed_name == name)
        .unwrap_or_else(|| panic!("session '{name}' missing"))
}

#[test]
fn basic_imports_two_hosts() {
    let text = fixture("ssh_basic.conf");
    let (sessions, report) =
        parse_ssh_config(&text, "ssh_basic.conf", &no_include).expect("basic import parses");
    assert_eq!(sessions.len(), 2);
    assert_eq!(report.imported, 2);
    assert_eq!(report.rejected, 0);
    assert!(
        report.warnings.is_empty(),
        "unexpected: {:?}",
        report.warnings
    );

    let web = by_name(&sessions, "web");
    assert_eq!(web.spec.host.as_deref(), Some("192.0.2.10"));
    assert_eq!(web.spec.username.as_deref(), Some("testuser"));
    assert_eq!(web.spec.port, Some(2222));
    assert_eq!(
        web.spec.auth,
        AuthMethod::KeyFile {
            path: "~/.ssh/id_ed25519_example".to_string()
        }
    );
    assert!(web.provenance.contains("ssh-config"));
    assert!(web.provenance.contains("ssh_basic.conf"));

    let db = by_name(&sessions, "db");
    assert_eq!(db.spec.host.as_deref(), Some("example.com"));
    assert_eq!(db.spec.username.as_deref(), Some("testuser"));
    assert_eq!(db.spec.port, Some(22));
    assert_eq!(db.spec.auth, AuthMethod::Agent { forward: false });
}

#[test]
fn precedence_first_wins_across_include() {
    let text = fixture("ssh_precedence_include.conf");
    let extra = fixture("ssh_precedence_include.d/extra.conf");
    let include = |pattern: &str| -> Option<String> {
        if pattern == "ssh_precedence_include.d/extra.conf" {
            Some(extra.clone())
        } else {
            None
        }
    };
    let (sessions, report) = parse_ssh_config(&text, "ssh_precedence_include.conf", &include)
        .expect("precedence import parses");
    assert_eq!(sessions.len(), 1);
    let app = by_name(&sessions, "app");
    // First-obtained values win over both the include and the later block.
    assert_eq!(app.spec.host.as_deref(), Some("192.0.2.10"));
    assert_eq!(app.spec.username.as_deref(), Some("testuser"));
    assert_eq!(app.spec.port, Some(2222));

    let codes = report.warning_codes();
    assert!(codes.contains(&"shadowed-by-precedence"), "{codes:?}");
    assert!(codes.contains(&"safe-preference-noted"), "{codes:?}");
    assert!(codes.contains(&"include-no-match"), "{codes:?}");
    assert!(codes.contains(&"include-outside-root"), "{codes:?}");
}

#[test]
fn forwards_and_proxy_jump_mapping() {
    let text = fixture("ssh_forward_jump.conf");
    let (sessions, report) = parse_ssh_config(&text, "ssh_forward_jump.conf", &no_include)
        .expect("forward import parses");
    assert_eq!(sessions.len(), 1);
    let tunnel = by_name(&sessions, "tunnel");
    assert_eq!(tunnel.spec.forwards.len(), 3);
    assert_eq!(
        tunnel.spec.forwards[0].forward_type,
        ForwardType::Local {
            local_host: "127.0.0.1".to_string(),
            local_port: 8080,
            remote_host: "192.0.2.40".to_string(),
            remote_port: 80,
        }
    );
    assert_eq!(
        tunnel.spec.forwards[1].forward_type,
        ForwardType::Remote {
            remote_host: "localhost".to_string(),
            remote_port: 9090,
            local_host: "example.com".to_string(),
            local_port: 90,
        }
    );
    assert_eq!(
        tunnel.spec.forwards[2].forward_type,
        ForwardType::Dynamic {
            local_host: "127.0.0.1".to_string(),
            local_port: 1080,
        }
    );
    assert!(
        tunnel.spec.notes.contains("proxy-jump")
            && tunnel.spec.notes.contains("testuser@192.0.2.1"),
        "notes: {:?}",
        tunnel.spec.notes
    );
    assert!(report
        .warning_codes()
        .contains(&"proxy-disabled-pending-review"));
}

#[test]
fn proxy_command_quarantined_and_never_executed() {
    let marker = std::path::PathBuf::from("/tmp/mbxt-import-must-not-run");
    let _ = std::fs::remove_file(&marker);

    let text = fixture("ssh_proxycommand_quarantine.conf");
    let (sessions, report) =
        parse_ssh_config(&text, "ssh_proxycommand_quarantine.conf", &no_include)
            .expect("quarantine import parses");

    assert!(!marker.exists(), "import must never execute ProxyCommand");
    let _ = std::fs::remove_file(&marker);

    let sneaky = by_name(&sessions, "sneaky");
    assert_eq!(sneaky.disabled_flags, vec!["proxy-command".to_string()]);
    assert!(
        sneaky
            .spec
            .notes
            .contains("touch /tmp/mbxt-import-must-not-run"),
        "notes: {:?}",
        sneaky.spec.notes
    );

    // `%` tokens are never expanded at import.
    let percent = by_name(&sessions, "percent");
    assert!(
        percent.spec.notes.contains("%h:%p"),
        "notes: {:?}",
        percent.spec.notes
    );

    let runner = by_name(&sessions, "runner");
    assert_eq!(runner.disabled_flags, vec!["auto-command".to_string()]);

    let codes = report.warning_codes();
    assert!(codes.contains(&"proxy-command-quarantined"), "{codes:?}");
    assert!(codes.contains(&"auto-command-ignored"), "{codes:?}");
}

#[test]
fn edge_cases_unknown_port_match_password_wildcard() {
    let text = fixture("ssh_edge.conf");
    let (sessions, report) =
        parse_ssh_config(&text, "ssh_edge.conf", &no_include).expect("edge import parses");

    // Two drafts (edge1 + wildcard literal); bare Host and negated-only
    // blocks are rejected.
    assert_eq!(sessions.len(), 2);
    assert_eq!(report.imported, 2);
    assert_eq!(report.rejected, 2);

    let edge = by_name(&sessions, "edge1");
    assert_eq!(edge.spec.host.as_deref(), Some("192.0.2.60"));
    assert_eq!(edge.spec.port, Some(22), "bad port falls back to 22");
    assert_eq!(
        edge.spec.auth,
        AuthMethod::KeyFile {
            path: "~/.ssh/id_ed25519_example".to_string()
        }
    );

    let wild = by_name(&sessions, "*.example.com");
    assert_eq!(wild.spec.host.as_deref(), Some("192.0.2.70"));

    // Match contents, bare-Host contents, and negated-only contents must
    // never materialize.
    for session in &sessions {
        assert_ne!(
            session.spec.host.as_deref(),
            Some("192.0.2.99"),
            "unmaterialized block leaked into '{}'",
            session.proposed_name
        );
    }

    let codes = report.warning_codes();
    for expected in [
        "unknown-directive",
        "safe-preference-noted",
        "match-block-skipped",
        "password-dropped",
        "agent-forwarding-disabled",
        "extra-identity-files-noted",
        "wildcard-pattern-kept-literal",
        "unnamed-session-rejected",
        "no-resolvable-host",
    ] {
        assert!(codes.contains(&expected), "missing {expected}: {codes:?}");
    }

    // The password-shaped value must appear nowhere: not in notes,
    // provenance, or warning details.
    for session in &sessions {
        assert!(!session.spec.notes.contains("hunter2-canary-xyz"));
        assert!(!session.provenance.contains("hunter2-canary-xyz"));
    }
    for warning in &report.warnings {
        assert!(
            !warning.detail.contains("hunter2-canary-xyz"),
            "secret leaked into warning: {:?}",
            warning.detail
        );
    }
}

#[test]
fn too_many_sessions_rejected() {
    let mut text = String::new();
    for i in 0..(MAX_SESSIONS + 5) {
        text.push_str(&format!("Host gen-{i}\n    HostName 192.0.2.1\n"));
    }
    let result = parse_ssh_config(&text, "gen.conf", &no_include);
    assert_eq!(
        result.unwrap_err(),
        ImportError::TooManySessions(MAX_SESSIONS)
    );
}

#[test]
fn oversize_input_rejected() {
    let big = "x".repeat(mbxt_storage::import::MAX_INPUT_BYTES + 1);
    let result = parse_ssh_config(&big, "big.conf", &no_include);
    assert!(matches!(result.unwrap_err(), ImportError::TooLarge(..)));
}
