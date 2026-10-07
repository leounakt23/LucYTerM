//! Hermetic SSH connect test: an in-process russh server on loopback
//! accepts one password login and echoes shell input. No network beyond
//! 127.0.0.1, no credentials on disk, no live server needed.
//!
//! The server host key is learned into a throwaway `HOME` (guarded: the
//! agent tests mutate process env the same way), so the client's
//! fail-closed known_hosts check runs exactly as in production.

#![cfg(feature = "ssh")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use mbxt_connections::{Connection, ConnectionAuth, ConnectionEvent, TerminalSize};
use mbxt_core::{AuthMethod, Protocol, SessionSpec};
use russh::server::{Auth, Handler, Server, Session};
use russh::{Channel, ChannelId};
use zeroize::Zeroizing;

struct EchoServer;

struct EchoHandler;

impl Server for EchoServer {
    type Handler = EchoHandler;

    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self::Handler {
        EchoHandler
    }
}

#[async_trait::async_trait]
impl Handler for EchoHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == "test" && password == "test" {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::Reject {
                proceed_with_methods: None,
            })
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<russh::server::Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        _channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn shell_request(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, data.to_vec().into());
        Ok(())
    }
}

/// Serializes throwaway-`HOME` mutation (mirrors the agent provider
/// tests; the loopback server key must be learned for the client's
/// known_hosts check to pass exactly as in production).
static HOME_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn with_throwaway_home(run: impl FnOnce(&std::path::Path)) {
    let _guard = HOME_GUARD.lock().unwrap();
    let dir = std::env::temp_dir().join(format!("mbxt-ssh-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp home");
    let saved = std::env::var_os("HOME");
    std::env::set_var("HOME", &dir);
    run(&dir);
    match saved {
        Some(home) => std::env::set_var("HOME", home),
        None => std::env::remove_var("HOME"),
    }
    std::fs::remove_dir_all(&dir).ok();
}

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

/// Start the echo server on an ephemeral loopback port. Returns the
/// port; the server task serves exactly one connection, then ends.
async fn start_echo_server() -> (
    u16,
    tokio::task::JoinHandle<()>,
    russh::keys::key::PublicKey,
) {
    let host_key = russh::keys::key::KeyPair::generate_ed25519().expect("ed25519 host key");
    let public = host_key.clone_public_key().expect("host pubkey");
    let config = Arc::new(russh::server::Config {
        keys: vec![host_key],
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("loopback bind");
    let port = listener.local_addr().expect("addr").port();
    let task = tokio::spawn(async move {
        let Ok((socket, _)) = listener.accept().await else {
            return;
        };
        let mut server = EchoServer;
        let handler = server.new_client(socket.peer_addr().ok());
        if let Ok(session) = russh::server::run_stream(config, socket, handler).await {
            let _ = session.await;
        }
    });
    (port, task, public)
}

#[test]
fn password_login_connects_and_echoes_on_loopback() {
    with_throwaway_home(|_| {
        block_on(async {
            let (port, server, public) = start_echo_server().await;
            russh::keys::learn_known_hosts("127.0.0.1", port, &public).expect("learn loopback key");

            let started = Instant::now();
            let mut conn = mbxt_connections::ssh::SshConn::new(&spec(port));
            conn.start(
                ConnectionAuth::Password(Zeroizing::new("test".into())),
                TerminalSize::default(),
            )
            .await
            .expect("loopback login");
            let handshake = started.elapsed();
            assert!(
                handshake < Duration::from_secs(3),
                "loopback handshake took {handshake:?}"
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
            server.abort();
        });
    });
}

#[test]
fn unknown_host_key_fails_closed() {
    with_throwaway_home(|_| {
        block_on(async {
            // Key NOT learned: the client must refuse, never connect.
            let (port, server, _public) = start_echo_server().await;
            let mut conn = mbxt_connections::ssh::SshConn::new(&spec(port));
            let result = conn
                .start(
                    ConnectionAuth::Password(Zeroizing::new("test".into())),
                    TerminalSize::default(),
                )
                .await;
            assert!(result.is_err(), "unknown host key must fail closed");
            server.abort();
        });
    });
}
