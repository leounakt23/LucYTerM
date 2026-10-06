//! Low-overhead profiling hooks.
//!
//! Normal builds only create tracing spans when the caller asks for them;
//! enabling the `profiling` feature additionally exposes puffin scopes for
//! interactive capture. Flame output is configured by the profiling command
//! documented in `docs/performance.md`, keeping production startup unchanged.

use std::time::Instant;

/// Begin a measured operation. The returned guard logs elapsed time at DEBUG.
#[must_use]
pub fn operation(name: &'static str) -> Operation {
    Operation {
        name,
        started: Instant::now(),
    }
}

/// RAII timer used around hot operations without allocating a formatted label.
pub struct Operation {
    name: &'static str,
    started: Instant,
}

impl Drop for Operation {
    fn drop(&mut self) {
        tracing::debug!(
            operation = self.name,
            elapsed_us = self.started.elapsed().as_micros() as u64,
            "profiled operation"
        );
    }
}

/// Mark a puffin scope when the optional real-time profiler is enabled.
#[cfg(feature = "profiling")]
pub fn realtime_scope(name: &'static str) -> Option<puffin::ProfilerScope> {
    puffin::profile_scope_custom!(name)
}

/// No-op equivalent for normal builds.
#[cfg(not(feature = "profiling"))]
pub fn realtime_scope(_name: &'static str) {}

#[cfg(test)]
mod tests {
    #[test]
    fn operation_guard_is_constructible() {
        let _guard = super::operation("test");
    }
}
