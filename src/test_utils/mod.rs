//! Small deterministic helpers shared by integration tests and examples.

use std::path::{Path, PathBuf};

/// Temporary configuration directory with cleanup on drop.
#[derive(Debug)]
pub struct TempConfig {
    path: PathBuf,
}

impl TempConfig {
    pub fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "remote-app-test-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Headless terminal fixture for grid/parser tests.
pub struct MockTerminal {
    pub grid: mbxt_terminal::Grid,
}

impl MockTerminal {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            grid: mbxt_terminal::Grid::new(cols, rows, 128),
        }
    }
    pub fn write_text(&mut self, text: &str) {
        for (index, ch) in text.chars().enumerate() {
            if let Some(cell) = self.grid.get_cell_mut(0, index as u16) {
                cell.ch = ch;
            }
        }
    }
}

/// Environment configuration for opt-in live SSH/SFTP fixtures.
#[derive(Debug, Clone)]
pub struct MockSshServer {
    pub address: String,
    pub username: String,
    pub password: String,
}

impl MockSshServer {
    pub fn from_env() -> Option<Self> {
        Some(Self {
            address: std::env::var("MBXT_SSH_TEST_ADDR").ok()?,
            username: std::env::var("MBXT_SSH_TEST_USER").ok()?,
            password: std::env::var("MBXT_SSH_TEST_PASSWORD").ok()?,
        })
    }
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default()
}

/// Learn a live server host key into a throwaway `HOME` so strict
/// `known_hosts` verification passes exactly as in production: no user
/// `known_hosts` pollution, works in CI and locally.
///
/// Process-scoped by necessity (process env is global): the temp HOME
/// lives for the harness lifetime and is never restored. Serialized
/// internally; call once per live test before dialing. Requires the
/// `ssh-keyscan` binary (openssh-client) and a reachable server.
pub fn learn_live_host_key(addr: &str) {
    static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = GUARD.lock().unwrap();
    let (host, port) = addr.rsplit_once(':').expect("live addr as HOST:PORT");
    let home = std::env::temp_dir().join(format!("mbxt-live-home-{}", std::process::id()));
    let ssh_dir = home.join(".ssh");
    std::fs::create_dir_all(&ssh_dir).expect("temp ssh dir");
    let output = std::process::Command::new("ssh-keyscan")
        .args(["-p", port, "-t", "rsa,ecdsa,ed25519", host])
        .output()
        .expect("ssh-keyscan runs (openssh-client required for live tests)");
    assert!(
        !output.stdout.is_empty(),
        "ssh-keyscan found no host keys at {addr}"
    );
    std::fs::write(ssh_dir.join("known_hosts"), &output.stdout).expect("write known_hosts");
    std::env::set_var("HOME", &home);
}

/// Process-scoped throwaway `HOME` with restoration on drop. Serializes
/// the global-env mutation internally; holders keep the lock for their
/// whole lifetime, so parallel tests never observe a half-moved `HOME`.
#[derive(Debug)]
pub struct ThrowawayHome {
    saved: Option<std::ffi::OsString>,
    _dir: TempConfig,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl ThrowawayHome {
    pub fn set() -> std::io::Result<Self> {
        static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let lock = GUARD.lock().unwrap();
        let dir = TempConfig::new()?;
        let saved = std::env::var_os("HOME");
        std::env::set_var("HOME", dir.path());
        Ok(Self {
            saved,
            _dir: dir,
            _lock: lock,
        })
    }
}

impl Drop for ThrowawayHome {
    fn drop(&mut self) {
        match self.saved.take() {
            Some(home) => std::env::set_var("HOME", home),
            None => std::env::remove_var("HOME"),
        }
    }
}

/// In-process SSH echo server for hermetic tests (no network beyond
/// loopback, no credentials on disk): accepts `test`/`test` over
/// password auth and echoes shell bytes. Pair with [`ThrowawayHome`]
/// plus [`learn_loopback_key`] so the client's fail-closed
/// `known_hosts` check runs exactly as in production.
#[cfg(feature = "ssh")]
pub struct EchoSshServer {
    pub port: u16,
    pub public: russh::keys::key::PublicKey,
    pub task: tokio::task::JoinHandle<()>,
}

#[cfg(feature = "ssh")]
struct EchoHandler;

#[cfg(feature = "ssh")]
impl russh::server::Server for EchoHandler {
    type Handler = EchoHandler;

    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self::Handler {
        EchoHandler
    }
}

#[cfg(feature = "ssh")]
#[async_trait::async_trait]
impl russh::server::Handler for EchoHandler {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<russh::server::Auth, Self::Error> {
        if user == "test" && password == "test" {
            Ok(russh::server::Auth::Accept)
        } else {
            Ok(russh::server::Auth::Reject {
                proceed_with_methods: None,
            })
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: russh::Channel<russh::server::Msg>,
        _session: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        _channel: russh::ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        _session: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn shell_request(
        &mut self,
        _channel: russh::ChannelId,
        _session: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn data(
        &mut self,
        channel: russh::ChannelId,
        data: &[u8],
        session: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, data.to_vec().into());
        Ok(())
    }
}

#[cfg(feature = "ssh")]
impl EchoSshServer {
    /// Bind an ephemeral loopback port and serve exactly one connection.
    pub async fn start() -> Self {
        let host_key = russh::keys::key::KeyPair::generate_ed25519().expect("ed25519 host key");
        let public = host_key.clone_public_key().expect("host pubkey");
        let config = std::sync::Arc::new(russh::server::Config {
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
            use russh::server::Server as _;
            let mut server = EchoHandler;
            let handler = server.new_client(socket.peer_addr().ok());
            if let Ok(session) = russh::server::run_stream(config, socket, handler).await {
                let _ = session.await;
            }
        });
        Self { port, public, task }
    }
}

/// Learn a loopback echo-server key into the current (throwaway) HOME.
#[cfg(feature = "ssh")]
pub fn learn_loopback_key(port: u16, public: &russh::keys::key::PublicKey) {
    russh::keys::learn_known_hosts("127.0.0.1", port, public).expect("learn loopback key");
}
