//! No-overwrite merge: incoming drafts never replace stored sessions.
//!
//! By design there is no `overwrite` flag, env var, or `--force` —
//! collisions skip with a rename suggestion the user confirms explicitly.

use super::model::{ImportWarning, ImportedSession, WarningCode};
use super::report::ImportReport;
use super::Source;

/// One skipped draft (collision or duplicate).
#[derive(Debug, Clone)]
pub struct SkippedSession {
    pub proposed_name: String,
    pub reason: WarningCode,
    /// Suggested rename (`"<name> (imported)"`, `"<name> (imported 2)"`, …).
    pub suggested_rename: String,
}

/// Merge result: drafts to create + skipped + report rows.
#[derive(Debug, Clone, Default)]
pub struct MergeOutcome {
    pub to_create: Vec<ImportedSession>,
    pub skipped: Vec<SkippedSession>,
    pub report: ImportReport,
}

fn suggest_rename(base: &str, taken: &[String]) -> String {
    let first = format!("{base} (imported)");
    if !taken.contains(&first) {
        return first;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base} (imported {n})");
        if !taken.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Merge `imported` against `existing_names` (no overwrite, ever).
pub fn apply(
    existing_names: &[String],
    imported: Vec<ImportedSession>,
    source: Source,
) -> MergeOutcome {
    let mut outcome = MergeOutcome::default();
    let mut taken: Vec<String> = existing_names.to_vec();
    let mut seen: Vec<String> = Vec::new();
    for draft in imported {
        if seen.contains(&draft.proposed_name) {
            let suggested_rename = suggest_rename(&draft.proposed_name, &taken);
            outcome.report.warn(ImportWarning::new(
                WarningCode::DuplicateNameInImport,
                source,
                format!("duplicate '{}' in import; first kept", draft.proposed_name),
            ));
            outcome.skipped.push(SkippedSession {
                proposed_name: draft.proposed_name,
                reason: WarningCode::DuplicateNameInImport,
                suggested_rename,
            });
            continue;
        }
        seen.push(draft.proposed_name.clone());
        if taken.contains(&draft.proposed_name) {
            let suggested_rename = suggest_rename(&draft.proposed_name, &taken);
            outcome.report.warn(ImportWarning::new(
                WarningCode::NameCollisionSkipped,
                source,
                format!(
                    "'{}' exists; skipped without overwriting",
                    draft.proposed_name
                ),
            ));
            outcome.skipped.push(SkippedSession {
                proposed_name: draft.proposed_name,
                reason: WarningCode::NameCollisionSkipped,
                suggested_rename,
            });
            continue;
        }
        taken.push(draft.proposed_name.clone());
        outcome.to_create.push(draft);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbxt_core::{AuthMethod, Protocol, SessionSpec};

    fn draft(name: &str) -> ImportedSession {
        ImportedSession {
            proposed_name: name.to_string(),
            spec: SessionSpec {
                name: name.to_string(),
                protocol: Protocol::Ssh,
                host: Some("192.0.2.10".to_string()),
                port: Some(22),
                username: None,
                auth: AuthMethod::Password,
                tags: vec![],
                notes: String::new(),
                x11_forwarding: false,
                serial: None,
                forwards: vec![],
            },
            provenance: "test".to_string(),
            disabled_flags: vec![],
        }
    }

    #[test]
    fn merge_never_overwrites_and_suggests_renames() {
        let existing = vec!["web".to_string()];
        let outcome = apply(
            &existing,
            vec![draft("web"), draft("db")],
            Source::SshConfig,
        );
        assert_eq!(outcome.to_create.len(), 1);
        assert_eq!(outcome.to_create[0].proposed_name, "db");
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].suggested_rename, "web (imported)");
        assert!(outcome
            .report
            .warning_codes()
            .contains(&"name-collision-skipped"));
    }

    #[test]
    fn merge_numbered_rename_when_first_taken() {
        let existing = vec!["web".to_string(), "web (imported)".to_string()];
        let outcome = apply(&existing, vec![draft("web")], Source::Putty);
        assert_eq!(outcome.skipped[0].suggested_rename, "web (imported 2)");
    }

    #[test]
    fn merge_drops_second_duplicate_within_import() {
        let outcome = apply(&[], vec![draft("a"), draft("a")], Source::MobaXterm);
        assert_eq!(outcome.to_create.len(), 1);
        assert_eq!(
            outcome.skipped[0].reason,
            WarningCode::DuplicateNameInImport
        );
    }
}
