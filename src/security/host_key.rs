//! Host-key policy primitives for SSH connection prompts and auditing.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyDecision {
    Known,
    FirstUse,
    Mismatch,
    Rejected,
}

#[derive(Debug, Clone, Default)]
pub struct HostKeyPins {
    pins: HashMap<String, String>,
}

impl HostKeyPins {
    pub fn pin(&mut self, host: &str, fingerprint: &str) {
        self.pins.insert(host.to_owned(), fingerprint.to_owned());
    }
    pub fn remove(&mut self, host: &str) {
        self.pins.remove(host);
    }
    pub fn fingerprint(&self, host: &str) -> Option<&str> {
        self.pins.get(host).map(String::as_str)
    }

    pub fn decide(
        &self,
        host: &str,
        fingerprint: &str,
        known_hosts_match: bool,
    ) -> HostKeyDecision {
        match self.fingerprint(host) {
            Some(expected) if expected == fingerprint && known_hosts_match => {
                HostKeyDecision::Known
            },
            Some(_) => HostKeyDecision::Mismatch,
            None if known_hosts_match => HostKeyDecision::Known,
            None => HostKeyDecision::FirstUse,
        }
    }
}

/// Unknown and changed keys require explicit user confirmation.
pub fn allows_connection(decision: HostKeyDecision, user_confirmed: bool) -> bool {
    matches!(decision, HostKeyDecision::Known)
        || matches!(
            decision,
            HostKeyDecision::FirstUse | HostKeyDecision::Mismatch
        ) && user_confirmed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mismatches_are_not_implicitly_accepted() {
        let mut pins = HostKeyPins::default();
        pins.pin("host", "new");
        assert_eq!(pins.decide("host", "old", true), HostKeyDecision::Mismatch);
        assert!(!allows_connection(HostKeyDecision::Mismatch, false));
        assert!(allows_connection(HostKeyDecision::Mismatch, true));
    }
}
