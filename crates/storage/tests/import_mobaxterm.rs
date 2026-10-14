use mbxt_core::Protocol;
use mbxt_storage::import::mobaxterm::parse_mobaxterm_xml;

const BASIC: &str = include_str!("../../../tests/fixtures/import/mobaxterm_basic.xml");
const PROTOCOLS: &str = include_str!("../../../tests/fixtures/import/mobaxterm_protocols.xml");
const EDGE: &str = include_str!("../../../tests/fixtures/import/mobaxterm_edge.xml");
const PASSWORD_DROP: &str =
    include_str!("../../../tests/fixtures/import/mobaxterm_password_drop.xml");

fn find<'a>(
    sessions: &'a [mbxt_storage::import::ImportedSession],
    name: &str,
) -> Option<&'a mbxt_storage::import::ImportedSession> {
    sessions.iter().find(|s| s.proposed_name == name)
}

fn codes(report: &mbxt_storage::import::ImportReport) -> Vec<&'static str> {
    report.warning_codes()
}

#[test]
fn import_basic_folders_ports_and_tags() {
    let (sessions, report) =
        parse_mobaxterm_xml(BASIC, "mobaxterm_basic.xml").expect("basic fixture parses");
    assert_eq!(sessions.len(), 4);
    assert_eq!(report.imported, 4);
    assert_eq!(report.rejected, 0);

    if let Some(web) = find(&sessions, "web-ssh") {
        assert_eq!(web.spec.protocol, Protocol::Ssh);
        if let Some(host) = web.spec.host.as_ref() {
            assert_eq!(host, "192.0.2.10");
        } else {
            panic!("web-ssh missing host");
        }
        assert_eq!(web.spec.port, Some(22));
        assert!(web.spec.tags.iter().any(|t| t == "Prod"));
    } else {
        panic!("web-ssh missing");
    }

    if let Some(telnet) = find(&sessions, "db-telnet") {
        assert_eq!(telnet.spec.protocol, Protocol::Telnet);
        assert_eq!(telnet.spec.port, Some(2323));
        assert!(telnet.spec.tags.iter().any(|t| t == "Prod"));
    } else {
        panic!("db-telnet missing");
    }

    if let Some(nested) = find(&sessions, "db-main") {
        assert!(nested.spec.tags.iter().any(|t| t == "Prod"));
        assert!(nested.spec.tags.iter().any(|t| t == "DC1"));
        if let Some(host) = nested.spec.host.as_ref() {
            assert_eq!(host, "db.example.com");
        } else {
            panic!("db-main missing host");
        }
        assert_eq!(nested.spec.port, Some(2222));
    } else {
        panic!("db-main missing");
    }

    if let Some(rdp) = find(&sessions, "top-rdp") {
        assert_eq!(rdp.spec.protocol, Protocol::Rdp);
        assert_eq!(rdp.spec.port, Some(3389));
        assert!(rdp.spec.tags.is_empty());
    } else {
        panic!("top-rdp missing");
    }
}

#[test]
fn import_protocol_map_serial_and_unknown_reject() {
    let (sessions, report) =
        parse_mobaxterm_xml(PROTOCOLS, "mobaxterm_protocols.xml").expect("protocols parse");
    assert_eq!(sessions.len(), 5);
    assert_eq!(report.rejected, 1);
    assert!(codes(&report).contains(&"unsupported-protocol"));

    if let Some(ssh) = find(&sessions, "ssh-ok") {
        assert_eq!(ssh.spec.protocol, Protocol::Ssh);
    } else {
        panic!("ssh-ok missing");
    }
    if let Some(telnet) = find(&sessions, "telnet-ok") {
        assert_eq!(telnet.spec.protocol, Protocol::Telnet);
    } else {
        panic!("telnet-ok missing");
    }
    if let Some(rdp) = find(&sessions, "rdp-ok") {
        assert_eq!(rdp.spec.protocol, Protocol::Rdp);
    } else {
        panic!("rdp-ok missing");
    }
    if let Some(vnc) = find(&sessions, "vnc-ok") {
        assert_eq!(vnc.spec.protocol, Protocol::Vnc);
        assert_eq!(vnc.spec.port, Some(5900));
    } else {
        panic!("vnc-ok missing");
    }
    if let Some(serial) = find(&sessions, "serial-ok") {
        assert_eq!(serial.spec.protocol, Protocol::Serial);
        if let Some(params) = serial.spec.serial.as_ref() {
            assert_eq!(params.device, "/dev/ttyUSB0");
            assert_eq!(params.baud_rate, 115_200);
        } else {
            panic!("serial-ok missing serial params");
        }
        assert!(serial.spec.host.is_none());
    } else {
        panic!("serial-ok missing");
    }
    assert!(find(&sessions, "weird-proto").is_none());
    // Unknown protocol must never be coerced to SSH.
    for s in sessions.iter() {
        if s.proposed_name == "weird-proto" {
            panic!("unknown protocol session must be rejected");
        }
    }
}

#[test]
fn import_edge_empty_host_bad_port_dups_unknown_attrs_utf8() {
    let (sessions, report) =
        parse_mobaxterm_xml(EDGE, "mobaxterm_edge.xml").expect("edge fixture parses");
    let c = codes(&report);
    assert!(c.contains(&"missing-host"));
    assert!(c.contains(&"duplicate-name-in-import"));
    assert!(c.contains(&"unknown-field"));
    assert!(c.contains(&"safe-preference-noted"));

    // Empty host rejected.
    assert!(find(&sessions, "empty-host").is_none());

    // Bad port falls back to the SSH default.
    if let Some(bad) = find(&sessions, "bad-port") {
        assert_eq!(bad.spec.port, Some(22));
        if let Some(host) = bad.spec.host.as_ref() {
            assert_eq!(host, "192.0.2.12");
        } else {
            panic!("bad-port missing host");
        }
    } else {
        panic!("bad-port missing (should fall back to default port)");
    }

    // Duplicate names: first kept, later dropped.
    let dup_count = sessions.iter().filter(|s| s.proposed_name == "dup").count();
    assert_eq!(dup_count, 1);
    if let Some(dup) = find(&sessions, "dup") {
        if let Some(host) = dup.spec.host.as_ref() {
            assert_eq!(host, "192.0.2.13");
        } else {
            panic!("dup missing host");
        }
    } else {
        panic!("dup missing");
    }

    // UTF-8 name preserved with folder tag.
    if let Some(fancy) = find(&sessions, "fancy-üñîcode-日本語") {
        assert!(fancy.spec.tags.iter().any(|t| t == "Edge"));
    } else {
        panic!("UTF-8 session missing");
    }
}

#[test]
fn import_password_shaped_attributes_dropped_never_stored() {
    let (sessions, report) = parse_mobaxterm_xml(PASSWORD_DROP, "mobaxterm_password_drop.xml")
        .expect("password fixture parses");
    assert_eq!(sessions.len(), 1);
    assert!(codes(&report).contains(&"password-dropped"));

    if let Some(s) = find(&sessions, "secret-ssh") {
        let rendered = format!(
            "{}|{}|{}|{}|{}|{}",
            s.proposed_name,
            s.spec.name,
            s.spec.host.as_deref().unwrap_or(""),
            s.spec.username.as_deref().unwrap_or(""),
            s.spec.notes,
            s.provenance,
        );
        assert!(!rendered.contains("hunter2-super-secret"));
        assert!(!rendered.contains("shhh-dont-store"));
        let tags_joined = s.spec.tags.join("|");
        assert!(!tags_joined.contains("hunter2"));
        assert!(!tags_joined.contains("shhh"));
        // Disabled flags / notes must not smuggle the secret either.
        for flag in s.disabled_flags.iter() {
            assert!(!flag.contains("hunter2"));
            assert!(!flag.contains("shhh"));
        }
    } else {
        panic!("secret-ssh missing");
    }

    for w in report.warnings.iter() {
        assert!(
            !w.detail.contains("hunter2-super-secret"),
            "secret echoed in warning: {}",
            w.detail
        );
        assert!(
            !w.detail.contains("shhh-dont-store"),
            "secret echoed in warning: {}",
            w.detail
        );
    }
}

#[test]
fn import_malformed_xml_and_doctype_rejected() {
    let bad = "<MobaXterm><Session name=\"a\" host=\"192.0.2.10\">";
    let err = match parse_mobaxterm_xml(bad, "bad.xml") {
        Ok(_) => panic!("expected malformed error"),
        Err(e) => e,
    };
    match err {
        mbxt_storage::import::ImportError::Malformed(_, _) => {},
        other => panic!("expected Malformed, got {other:?}"),
    }

    let dtd = "<?xml version=\"1.0\"?><!DOCTYPE foo><MobaXterm></MobaXterm>";
    let err2 = match parse_mobaxterm_xml(dtd, "dtd.xml") {
        Ok(_) => panic!("expected DOCTYPE rejection"),
        Err(e) => e,
    };
    match err2 {
        mbxt_storage::import::ImportError::Malformed(_, _) => {},
        other => panic!("expected Malformed for DOCTYPE, got {other:?}"),
    }

    let entity = "<MobaXterm><Session name=\"a\" protocol=\"ssh\" host=\"192.0.2.10\" password=\"&foo;\"/></MobaXterm>";
    let err3 = match parse_mobaxterm_xml(entity, "entity.xml") {
        Ok(_) => panic!("expected entity rejection"),
        Err(e) => e,
    };
    match err3 {
        mbxt_storage::import::ImportError::Malformed(_, _) => {},
        other => panic!("expected Malformed for entity, got {other:?}"),
    }
}
