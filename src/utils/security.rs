//! Process-level security initialization.

/// Disable core dumps as early as possible. Linux additionally becomes
/// non-dumpable to reduce credential exposure through local debuggers.
pub fn initialize() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: the pointer refers to a local value and libc does not retain it.
        if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: PR_SET_DUMPABLE receives scalar arguments only.
        if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0 {
            tracing::warn!(error = ?std::io::Error::last_os_error(), "could not disable ptrace dumps");
        }
    }
    Ok(())
}

/// Best-effort memory locking for sensitive buffers. Platforms may reject it.
pub fn lock_memory(bytes: &mut [u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    // SAFETY: pointer and length are borrowed from a live slice for this call.
    unsafe { memsec::mlock(bytes.as_mut_ptr(), bytes.len()) }
}

pub fn unlock_memory(bytes: &mut [u8]) {
    if !bytes.is_empty() {
        // SAFETY: pointer and length are borrowed from a live slice for this call.
        unsafe {
            memsec::munlock(bytes.as_mut_ptr(), bytes.len());
        }
    }
}
