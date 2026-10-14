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
        // MBXT_DEBUGGABLE=1 keeps the process ptrace-able (smaps readable)
        // for profiling sessions; the default stays non-dumpable.
        if std::env::var_os("MBXT_DEBUGGABLE").is_none()
            && unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0
        {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that mutate the process-wide `MBXT_DEBUGGABLE` flag
    /// (environment mutation is process-global, so parallel tests would race).
    static ENV_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn core_limits() -> Option<(u64, u64)> {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: refers to a live local; libc only writes through it.
        if unsafe { libc::getrlimit(libc::RLIMIT_CORE, &mut limit) } != 0 {
            return None;
        }
        Some((limit.rlim_cur as u64, limit.rlim_max as u64))
    }

    fn dumpable_flag() -> Option<i64> {
        // SAFETY: scalar prctl query with no pointer arguments.
        let rc = unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) };
        if rc < 0 {
            None
        } else {
            Some(rc as i64)
        }
    }

    #[test]
    fn initialize_zeroes_core_rlimit() {
        if initialize().is_err() {
            panic!("security::initialize failed");
        }
        match core_limits() {
            Some((0, 0)) => {},
            Some((cur, max)) => {
                panic!("RLIMIT_CORE not zeroed: cur={cur} max={max}");
            },
            None => panic!("getrlimit(RLIMIT_CORE) failed"),
        }
    }

    #[test]
    fn default_process_is_non_dumpable() {
        let _serial = match ENV_SERIAL.lock() {
            Ok(guard) => guard,
            Err(_) => panic!("env serial mutex poisoned"),
        };
        let saved = std::env::var_os("MBXT_DEBUGGABLE");
        std::env::remove_var("MBXT_DEBUGGABLE");
        let init = initialize();
        match saved {
            Some(value) => std::env::set_var("MBXT_DEBUGGABLE", value),
            None => {},
        }
        if init.is_err() {
            panic!("security::initialize failed without MBXT_DEBUGGABLE");
        }
        match dumpable_flag() {
            Some(0) => {},
            Some(flag) => panic!("process still dumpable by default: flag={flag}"),
            None => panic!("prctl(PR_GET_DUMPABLE) failed"),
        }
    }

    #[test]
    fn debuggable_flag_keeps_process_dumpable() {
        let _serial = match ENV_SERIAL.lock() {
            Ok(guard) => guard,
            Err(_) => panic!("env serial mutex poisoned"),
        };
        // Establish the fresh-process precondition explicitly: test execution
        // order is undefined and a sibling test may have already disabled
        // dumping for this process.
        // SAFETY: scalar prctl with no pointer arguments.
        if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 1, 0, 0, 0) } < 0 {
            panic!("could not precondition dumpable=1");
        }
        let saved = std::env::var_os("MBXT_DEBUGGABLE");
        std::env::set_var("MBXT_DEBUGGABLE", "1");
        let init = initialize();
        // Observe the flag while the opt-out is still active, then restore
        // the default hardening so later tests see a clean process.
        let flag = dumpable_flag();
        match saved {
            Some(value) => std::env::set_var("MBXT_DEBUGGABLE", value),
            None => std::env::remove_var("MBXT_DEBUGGABLE"),
        }
        if init.is_err() {
            panic!("security::initialize failed with MBXT_DEBUGGABLE=1");
        }
        match flag {
            Some(1) => {},
            Some(other) => {
                panic!("MBXT_DEBUGGABLE=1 did not keep the process dumpable: {other}");
            },
            None => panic!("prctl(PR_GET_DUMPABLE) failed"),
        }
        if initialize().is_err() {
            panic!("security::initialize failed while restoring defaults");
        }
    }
}
