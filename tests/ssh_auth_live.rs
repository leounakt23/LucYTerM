//! SSH auth live tests: password, key-file, and agent logins against a
//! disposable OpenSSH container (verification checklist: SSH connects
//! with password, private key, and agent).
//!
//! These tests are **ignored by default** — they need a reachable
//! OpenSSH server (e.g. the `openssh` service in CI):
//!
//! ```sh
//! docker run -d --name openssh-test -p 2223:22 \
//!   -e PUID=1000 -e PGID=1000 -e USER_NAME=test -e USER_PASSWORD=test \
//!   -e PASSWORD_ACCESS=true \
//!   -v $PWD/docker/sshd_config:/config/sshd/sshd_config \
//!   lscr.io/linuxserver/openssh-server:latest
//! MBXT_SSH_TEST_ADDR=127.0.0.1:2223 \
//! MBXT_SSH_TEST_USER=test MBXT_SSH_TEST_PASSWORD=test \
//! cargo test --test ssh_auth_live -- --ignored
//! ```
//!
//! Host keys are verified strictly: the key is learned into a throwaway
//! HOME via `ssh-keyscan` before dialing, never into the user's
//! `known_hosts`. Without the env vars the harness reports a single
//! passing `requires_live_server_env` test so plain `cargo test` stays
//! green. Needs `ssh-keygen`, `ssh-add`, and `ssh-agent` (openssh-client).

#![cfg(feature = "ssh")]

use std::env;
use std::time::Duration;

use mbxt_connections::{Connection, ConnectionAuth, ConnectionEvent, TerminalSize};
use mbxt_core::{AuthMethod, Protocol, SessionSpec};
use zeroize::Zeroizing;

fn live_config() -> Option<(String, String, String)> {
    let addr = env::var("MBXT_SSH_TEST_ADDR").ok()?;
    let user = env::var("MBXT_SSH_TEST_USER").ok()?;
    let password = env::var("MBXT_SSH_TEST_PASSWORD").ok()?;
    Some((addr, user, password))
}

#[test]
fn requires_live_server_env() {
    if live_config().is_none() {
        eprintln!("MBXT_SSH_TEST_ADDR not set; live SSH auth tests skipped");
    }
}

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(fut)
}

fn spec(addr: &str, user: &str, auth: AuthMethod) -> SessionSpec {
    let (host, port) = addr.rsplit_once(':').expect("HOST:PORT");
    SessionSpec {
        name: "live-auth".into(),
        protocol: Protocol::Ssh,
        host: Some(host.to_string()),
        port: Some(port.parse().expect("numeric port")),
        username: Some(user.to_string()),
        auth,
        tags: Vec::new(),
        notes: String::new(),
        x11_forwarding: false,
        serial: None,
        forwards: Vec::new(),
    }
}

/// Read shell output until `needle` appears (tolerates MOTD/prompt noise
/// from a real server) or the timeout expires.
async fn expect_output_contains(
    conn: &mut mbxt_connections::ssh::SshConn,
    needle: &[u8],
    timeout: Duration,
) {
    let mut buffer = Vec::new();
    let found = tokio::time::timeout(timeout, async {
        loop {
            let event = conn.next_event().await.expect("no transport error");
            if let ConnectionEvent::Output(bytes) = event {
                buffer.extend_from_slice(&bytes);
                if buffer.windows(needle.len()).any(|w| w == needle) {
                    return true;
                }
            } else {
                // Eof / ExitStatus end the wait.
                return false;
            }
        }
    })
    .await
    .expect("probe output in time");
    assert!(
        found,
        "probe bytes missing from shell output ({buffer:?})"
    );
}

fn start_and_probe(spec: &SessionSpec, auth: ConnectionAuth) {
    let started = std::time::Instant::now();
    block_on(async {
        let mut conn = mbxt_connections::ssh::SshConn::new(spec);
        tokio::time::timeout(
            Duration::from_secs(30),
            conn.start(auth, TerminalSize::default()),
        )
        .await
        .expect("connect in time")
        .expect("live login");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "live handshake took {:?}",
            started.elapsed()
        );
        conn.write(b"mbxt-live-probe").await.expect("write");
        expect_output_contains(&mut conn, b"mbxt-live-probe", Duration::from_secs(10)).await;
        conn.shutdown().await.expect("shutdown");
    });
}

#[test]
#[ignore = "needs a live OpenSSH server (see module docs)"]
fn password_login_against_openssh() {
    let Some((addr, user, password)) = live_config() else {
        return;
    };
    remote_app::test_utils::learn_live_host_key(&addr);
    start_and_probe(
        &spec(&addr, &user, AuthMethod::Password),
        ConnectionAuth::Password(Zeroizing::new(password)),
    );
}

/// Install `pubkey` into the container user's `authorized_keys` over a
/// password session (strict host key, throwaway HOME).
fn install_authorized_key(addr: &str, user: &str, password: &str, pubkey: &str) {
    use russh::client;
    use std::sync::Arc;

    #[derive(Debug)]
    struct StrictHostKey {
        host: String,
        port: u16,
    }
    #[async_trait::async_trait]
    impl client::Handler for StrictHostKey {
        type Error = russh::Error;
        async fn check_server_key(
            &mut self,
            key: &russh::keys::key::PublicKey,
        ) -> Result<bool, Self::Error> {
            match russh::keys::check_known_hosts(&self.host, self.port, key) {
                Ok(accepted) => Ok(accepted),
                Err(error) => {
                    tracing::warn!(%error, host = %self.host, "known_hosts check failed");
                    Ok(false)
                },
            }
        }
    }

    let (host, port) = addr.rsplit_once(':').expect("HOST:PORT");
    block_on(async {
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        });
        let mut handle = client::connect(
            config,
            addr,
            StrictHostKey {
                host: host.to_string(),
                port: port.parse().expect("numeric port"),
            },
        )
        .await
        .expect("ssh connect");
        assert!(handle
            .authenticate_password(user, password)
            .await
            .expect("auth call"));
        let mut channel = handle.channel_open_session().await.expect("channel");
        channel
            .exec(true, "mkdir -p ~/.ssh && chmod 700 ~/.ssh")
            .await
            .expect("mkdir");
        channel.wait().await.expect("exec done");
        let mut channel = handle.channel_open_session().await.expect("channel");
        channel
            .exec(
                true,
                format!("printf '%s\\n' '{pubkey}' >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys"),
            )
            .await
            .expect("install");
        channel.wait().await.expect("exec done");
        handle
            .disconnect(russh::Disconnect::ByApplication, "", "")
            .await
            .expect("bye");
    });
}

#[test]
#[ignore = "needs a live OpenSSH server (see module docs)"]
fn key_login_against_openssh() {
    let Some((addr, user, password)) = live_config() else {
        return;
    };
    remote_app::test_utils::learn_live_host_key(&addr);

    let dir = tempfile::tempdir().expect("tempdir");
    let key_path = dir.path().join("mbxt-live-ed25519");
    let status = std::process::Command::new("ssh-keygen")
        .args(["-t", "ed25519", "-N", "", "-f"])
        .arg(&key_path)
        .arg("-q")
        .status()
        .expect("ssh-keygen runs (openssh-client required for live tests)");
    assert!(status.success(), "ssh-keygen failed");
    let pubkey = std::fs::read_to_string(key_path.with_extension("pub")).expect("pubkey");
    install_authorized_key(&addr, &user, &password, pubkey.trim());

    start_and_probe(
        &spec(
            &addr,
            &user,
            AuthMethod::KeyFile {
                path: key_path.to_string_lossy().into_owned(),
            },
        ),
        ConnectionAuth::KeyFile {
            path: key_path,
            passphrase: None,
        },
    );
}

/// A spawned `ssh-agent` holding the throwaway key. Process-global
/// `SSH_AUTH_SOCK` by necessity; only this test touches it, and the
/// agent is killed on drop.
struct TestAgent {
    sock: String,
    pid: String,
}

impl TestAgent {
    fn spawn_with_key(key_path: &std::path::Path) -> Self {
        let output = std::process::Command::new("ssh-agent")
            .arg("-s")
            .output()
            .expect("ssh-agent runs (openssh-client required for live tests)");
        assert!(output.status.success(), "ssh-agent failed to start");
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let var = |name: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(&format!("{name}=")))
                .and_then(|rest| rest.split(';').next())
                .unwrap_or_else(|| panic!("ssh-agent printed no {name}"))
                .to_string()
        };
        let agent = Self {
            sock: var("SSH_AUTH_SOCK"),
            pid: var("SSH_AGENT_PID"),
        };
        // The agent owns its environment; point only this process at it.
        env::set_var("SSH_AUTH_SOCK", &agent.sock);
        let status = std::process::Command::new("ssh-add")
            .arg(key_path)
            .env("SSH_AUTH_SOCK", &agent.sock)
            .status()
            .expect("ssh-add runs");
        assert!(status.success(), "ssh-add failed");
        agent
    }
}

impl Drop for TestAgent {
    fn drop(&mut self) {
        let _ = std::process::Command::new("ssh-agent")
            .arg("-k")
            .env("SSH_AUTH_SOCK", &self.sock)
            .env("SSH_AGENT_PID", &self.pid)
            .status();
        env::remove_var("SSH_AUTH_SOCK");
    }
}

#[test]
#[ignore = "needs a live OpenSSH server (see module docs)"]
fn agent_login_against_openssh() {
    let Some((addr, user, _)) = live_config() else {
        return;
    };
    remote_app::test_utils::learn_live_host_key(&addr);

    let dir = tempfile::tempdir().expect("tempdir");
    let key_path = dir.path().join("mbxt-live-agent-ed25519");
    let status = std::process::Command::new("ssh-keygen")
        .args(["-t", "ed25519", "-N", "", "-f"])
        .arg(&key_path)
        .arg("-q")
        .status()
        .expect("ssh-keygen runs (openssh-client required for live tests)");
    assert!(status.success(), "ssh-keygen failed");
    let pubkey = std::fs::read_to_string(key_path.with_extension("pub")).expect("pubkey");
    // Install via password (strict host key); the login itself uses the agent.
    let password = env::var("MBXT_SSH_TEST_PASSWORD").expect("password env");
    install_authorized_key(&addr, &user, &password, pubkey.trim());

    let _agent = TestAgent::spawn_with_key(&key_path);
    start_and_probe(
        &spec(&addr, &user, AuthMethod::Agent { forward: false }),
        ConnectionAuth::Agent,
    );
}
