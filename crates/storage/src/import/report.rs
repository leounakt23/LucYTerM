//! Import report: counts + enumerated warnings (capped) for preview UI
//! and snapshot tests. Never carries file bytes or secret values.

use super::model::ImportWarning;
use super::MAX_WARNINGS_ENUMERATED;

/// Outcome summary of one parse (before merge).
#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    /// Sessions successfully drafted.
    pub imported: usize,
    /// Sessions rejected with reason (kept as warning rows).
    pub rejected: usize,
    /// Enumerated warnings (capped at [`MAX_WARNINGS_ENUMERATED`]).
    pub warnings: Vec<ImportWarning>,
    /// Warnings beyond the cap (counted, not stored).
    pub warnings_overflow: usize,
}

impl ImportReport {
    pub fn warn(&mut self, warning: ImportWarning) {
        if self.warnings.len() < MAX_WARNINGS_ENUMERATED {
            self.warnings.push(warning);
        } else {
            self.warnings_overflow += 1;
        }
    }

    pub fn warning_codes(&self) -> Vec<&'static str> {
        self.warnings.iter().map(|w| w.code.as_str()).collect()
    }
}
