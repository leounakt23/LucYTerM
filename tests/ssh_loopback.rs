//! Hermetic SSH connect test: an in-process russh server on loopback
//! accepts one password login and echoes shell input. No network beyond
//! 127.0.0.1, no credentials on disk, no live server needed.
//!
//! The server host key is learned into a throwaway `HOME`, so the
//! client's fail-closed known_hosts check runs exactly as in production.

#![cfg(feature = "ssh")]

use std::time::Duration;

use mbxt_connections::{Connection, ConnectionAuth, ConnectionEvent, TerminalSize};
use mbxt_core::{AuthMethod, Protocol, SessionSpec};
use remote_app::test_utils as tu;
use zeroize::Zeroizing;

fn spec(port: u16) -> SessionSpec {
    SessionSpec {
        name: "loopback".into(),
        protocol: Protocol::Ssh,
        host: Some("127.0.0.1".into()),
        port: Some(port),
        username: Some("test".into()),
        auth: AuthMethod::Password,
        tags: Vec::new(),
        notes: String::new(),
        x11_forwarding: false,
        serial: None,
        forwards: Vec::new(),
    }
}

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(fut)
}

#[test]
fn password_login_connects_and_echoes_on_loopback() {
    let _home = tu::ThrowawayHome::set().expect("temp home");
    block_on(async {
        let server = tu::EchoSshServer::start().await;
        tu::learn_loopback_key(server.port, &server.public);

        let started = std::time::Instant::now();
        let mut conn = mbxt_connections::ssh::SshConn::new(&spec(server.port));
        conn.start(
            ConnectionAuth::Password(Zeroizing::new("test".into())),
            TerminalSize::default(),
        )
        .await
        .expect("loopback login");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "loopback handshake took {:?}",
            started.elapsed()
        );

        conn.write(b"hello").await.expect("write");
        let event = tokio::time::timeout(Duration::from_secs(5), conn.next_event())
            .await
            .expect("event in time")
            .expect("no transport error");
        match event {
            ConnectionEvent::Output(bytes) => {
                assert_eq!(bytes, b"hello", "echo mismatch: {bytes:?}");
            },
            other => panic!("expected echo output, got {other:?}"),
        }
        conn.shutdown().await.expect("shutdown");
        server.task.abort();
    });
}

#[test]
fn unknown_host_key_fails_closed() {
    let _home = tu::ThrowawayHome::set().expect("temp home");
    block_on(async {
        // Key NOT learned: the client must refuse, never connect.
        let server = tu::EchoSshServer::start().await;
        let mut conn = mbxt_connections::ssh::SshConn::new(&spec(server.port));
        let result = conn
            .start(
                ConnectionAuth::Password(Zeroizing::new("test".into())),
                TerminalSize::default(),
            )
            .await;
        assert!(result.is_err(), "unknown host key must fail closed");
        server.task.abort();
    });
}
