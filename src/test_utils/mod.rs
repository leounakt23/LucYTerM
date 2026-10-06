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
