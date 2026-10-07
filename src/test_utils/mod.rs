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
