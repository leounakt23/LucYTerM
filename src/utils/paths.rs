//! Application directory layout (XDG) with secure permissions.

use std::path::PathBuf;

/// Resolved application directories, created on `init` with mode `0700`
/// on unix (security architecture §6.3).
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// `~/.config/mbxt` — config files (`config.ron`, `sessions.enc`).
    pub config_dir: PathBuf,
    /// `~/.config/mbxt/logs` — rotating log files (prompt 1.3).
    pub logs_dir: PathBuf,
    /// `~/.local/share/mbxt` — `sessions.db`, key material references.
    pub data_dir: PathBuf,
    /// `$XDG_RUNTIME_DIR/mbxt` or fallback temp dir — lock files, sockets.
    pub runtime_dir: PathBuf,
}

impl AppPaths {
    /// Resolve and create the directory layout. Fails only on I/O errors.
    pub fn init() -> std::io::Result<Self> {
        let proj = directories::ProjectDirs::from("org", "LucYTerM", "remote-app")
            .ok_or_else(|| std::io::Error::other("application directories unavailable"))?;

        let config_dir = proj.config_dir().to_path_buf();
        let logs_dir = config_dir.join("logs");
        let data_dir = proj.data_dir().to_path_buf();
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("mbxt");

        for dir in [&config_dir, &logs_dir, &data_dir, &runtime_dir] {
            std::fs::create_dir_all(dir)?;
            restrict_permissions(dir)?;
            verify_owner(dir)?;
        }

        Ok(Self {
            config_dir,
            logs_dir,
            data_dir,
            runtime_dir,
        })
    }

    /// Default single-instance lock file location.
    pub fn lock_file(&self) -> PathBuf {
        self.runtime_dir.join("remote-app.lock")
    }
}

/// Enforce owner-only permissions (unix only; a no-op on other platforms).
fn restrict_permissions(dir: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(dir)?.permissions();
        perms.set_mode(0o700);
        std::fs::set_permissions(dir, perms)?;
    }
    let _ = dir;
    Ok(())
}

fn verify_owner(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(path)?;
        // SAFETY: geteuid has no preconditions and returns the current process
        // effective user id.
        let uid = unsafe { libc::geteuid() } as u32;
        if metadata.uid() != uid {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "secure path is not owned by current user: {}",
                    path.display()
                ),
            ));
        }
    }
    let _ = path;
    Ok(())
}
