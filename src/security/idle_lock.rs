//! Idle-lock state machine. UI input handlers call `activity`; a timer calls
//! `should_lock` and transitions the application into the locked state.

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct IdleLock {
    timeout: Duration,
    last_activity: Instant,
    locked: bool,
}

impl IdleLock {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            last_activity: Instant::now(),
            locked: false,
        }
    }
    pub fn activity(&mut self) {
        self.last_activity = Instant::now();
        self.locked = false;
    }
    pub fn should_lock(&self) -> bool {
        !self.locked && self.last_activity.elapsed() >= self.timeout
    }
    pub fn lock(&mut self) {
        self.locked = true;
    }
    pub fn is_locked(&self) -> bool {
        self.locked
    }
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lock_state_can_be_reset_by_activity() {
        let mut state = IdleLock::new(Duration::from_secs(60));
        assert!(!state.should_lock());
        state.lock();
        assert!(state.is_locked());
        state.activity();
        assert!(!state.is_locked());
    }
}
