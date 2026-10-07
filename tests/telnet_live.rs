//! Telnet integration test against a disposable server (Prompt 4.4).
//!
//! Ignored by default — needs `busybox` with the `telnetd` applet (present
//! on most dev hosts and in CI via `apt install busybox`):
//!
//! ```sh
//! cargo test --test telnet_live --features telnet -- --ignored
//! ```
//!
//! The server is `busybox telnetd -l /bin/cat` on loopback: no login, no
//! credentials, bytes round-trip through the real Telnet negotiation
//! path. Without `busybox` the harness reports a passing skip so plain
//! `cargo test` stays green.

#![cfg(feature = "telnet")]

use std::time::Duration;

use mbxt_connections::{Connection, ConnectionAuth, ConnectionEvent, TerminalSize};
use mbxt_core::{AuthMethod, Protocol, SessionSpec};
use zeroize::Zeroizing;

fn spec(port: u16) -> SessionSpec {
    SessionSpec {
        name: "telnet-live".into(),
        protocol: Protocol::Telnet,
        host: Some("127.0.0.1".into()),
        port: Some(port),
        username: None,
        auth: AuthMethod::Password,
        tags: Vec::new(),
        notes: String::new(),
        x11_forwarding: false,
        serial: None,
        forwards: Vec::new(),
    }
}

fn busybox_present() -> bool {
    std::process::Command::new("busybox")
        .arg("--list")
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.trim() == "telnetd")
        })
        .unwrap_or(false)
}

/// Start `busybox telnetd -l /bin/cat` on the first free candidate
/// port. The caller kills the child when done.
fn start_telnetd() -> Option<(std::process::Child, u16)> {
    for port in [23232u16, 23233, 23234] {
        let child = std::process::Command::new("busybox")
            .args(["telnetd", "-F", "-p", &port.to_string(), "-l", "/bin/cat"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        // telnetd binds synchronously at startup; one short wait, then
        // probe. A dead child means the port was taken — try the next.
        std::thread::sleep(Duration::from_millis(300));
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Some((child, port));
        }
        let mut child = child;
        let _ = child.kill();
        let _ = child.wait();
    }
    None
}

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(fut)
}

#[test]
#[ignore = "needs busybox telnetd (see module docs)"]
fn telnet_round_trip_through_disposable_server() {
    if !busybox_present() {
        eprintln!("busybox telnetd absent; live telnet test skipped");
        return;
    }
    let Some((mut server, port)) = start_telnetd() else {
        panic!("could not start busybox telnetd on candidate ports");
    };
    block_on(async {
        let mut conn = mbxt_connections::telnet::TelnetConn::new(&spec(port));
        conn.start(
            ConnectionAuth::Password(Zeroizing::new(String::new())),
            TerminalSize::default(),
        )
        .await
        .expect("telnet connect");
        conn.write(b"hello-telnet").await.expect("write");
        let mut echoed = Vec::new();
        let deadline = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                event = conn.next_event() => {
                    match event.expect("no transport error") {
                        ConnectionEvent::Output(bytes) => {
                            echoed.extend_from_slice(&bytes);
                            if echoed.windows(12).any(|w| w == b"hello-telnet") {
                                break;
                            }
                        },
                        other => panic!("expected output, got {other:?}"),
                    }
                },
                _ = &mut deadline => panic!("echo not seen; got {echoed:?}"),
            }
        }
        conn.shutdown().await.expect("shutdown");
    });
    server.kill().expect("kill telnetd");
    let _ = server.wait();
}
