//! Single-instance guard via an exclusive-create lock file in the runtime
//! directory. (Named-pipe variant is unnecessary for v1; file lock is
//! portable and dependency-free.)

use std::path::{Path, PathBuf};

/// Held for the lifetime of the app; removing the lock file on drop.
#[derive(Debug)]
pub struct InstanceGuard {
    path: PathBuf,
}

/// Reasons single-instance acquisition can fail.
#[derive(Debug, thiserror::Error)]
pub enum SingleInstanceError {
    #[error("another instance is already running (lock: {0}) — remove the stale lock file if no instance is running")]
    AlreadyRunning(PathBuf),
    #[error("cannot create lock file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Acquire the default lock (`AppPaths::runtime_dir/remote-app.lock`).
pub fn acquire_default() -> Result<InstanceGuard, SingleInstanceError> {
    let paths = crate::utils::paths::AppPaths::init().map_err(|e| SingleInstanceError::Io {
        path: PathBuf::from("<runtime_dir>"),
        source: e,
    })?;
    acquire(&paths.runtime_dir.join("remote-app.lock"))
}

/// Acquire the single-instance lock at `path`.
///
/// Uses `create_new` (O_EXCL semantics) so the kernel guarantees exclusivity.
/// Stale locks from a crashed process are detected and removed if the
/// recorded PID is no longer alive.
pub fn acquire(path: &Path) -> Result<InstanceGuard, SingleInstanceError> {
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            use std::io::Write;
            let _ = writeln!(file, "{}", std::process::id());
            Ok(InstanceGuard {
                path: path.to_path_buf(),
            })
        },
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            if !process_alive(read_pid(path)) {
                // Stale lock from a crashed instance — reclaim it.
                let _ = std::fs::remove_file(path);
                return retry_create(path);
            }
            Err(SingleInstanceError::AlreadyRunning(path.to_path_buf()))
        },
        Err(err) => Err(SingleInstanceError::Io {
            path: path.to_path_buf(),
            source: err,
        }),
    }
}

fn retry_create(path: &Path) -> Result<InstanceGuard, SingleInstanceError> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(|_| InstanceGuard {
            path: path.to_path_buf(),
        })
        .map_err(|e| SingleInstanceError::Io {
            path: path.to_path_buf(),
            source: e,
        })
}

fn read_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn process_alive(pid: Option<u32>) -> bool {
    match pid {
        None => false, // unreadable/corrupt lock: treat as stale
        Some(pid) if pid == std::process::id() => true,
        #[cfg(unix)]
        Some(pid) => {
            // signal 0 = existence probe
            let ret = unsafe { libc::kill(pid as i32, 0) };
            ret == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
        },
        #[cfg(not(unix))]
        Some(_) => true, // cannot check on this platform; assume alive
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
