//! Application configuration: human-readable RON files with round-trip
//! support (tech_stack §5).

use std::path::{Path, PathBuf};

/// Application configuration (serialized as `config.ron`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Config {
    /// Default to the dark theme (portal watching refines this at runtime).
    pub theme_dark: bool,
    /// Terminal scrollback cap (feature matrix #21).
    pub scrollback_lines: usize,
    /// SFTP request pipeline depth (tech_stack R5 tuning knob).
    pub sftp_pipeline_depth: u32,
    /// Master password unlock required at startup (#48).
    pub require_master_password: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme_dark: true,
            scrollback_lines: 10_000,
            sftp_pipeline_depth: 20,
            require_master_password: false,
        }
    }
}

/// Loads/saves `config.ron` inside the app config directory.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    /// Store rooted at `config_dir` (created by `AppPaths::init`).
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("config.ron"),
        }
    }

    /// Filesystem location of the config file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load the config, falling back to defaults for a missing file.
    pub fn load_or_default(&self) -> Result<Config, crate::StorageError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => {
                ron::from_str(&text).map_err(|e| crate::StorageError::Serialize(e.to_string()))
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(err) => Err(err.into()),
        }
    }

    /// Persist the config (pretty-printed, round-trip safe).
    pub fn save(&self, config: &Config) -> Result<(), crate::StorageError> {
        let text = ron::ser::to_string_pretty(config, ron::ser::PrettyConfig::default())
            .map_err(|e| crate::StorageError::Serialize(e.to_string()))?;
        std::fs::write(&self.path, text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = std::env::temp_dir().join(format!("mbxt-cfg-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ConfigStore::new(&dir);
        let cfg = Config {
            scrollback_lines: 42,
            ..Default::default()
        };
        store.save(&cfg).unwrap();
        let loaded = store.load_or_default().unwrap();
        assert_eq!(loaded.scrollback_lines, 42);
        std::fs::remove_dir_all(&dir).ok();
    }
}
