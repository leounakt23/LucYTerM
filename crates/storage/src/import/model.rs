//! Import data model: drafts, warnings, warning codes.
//!
//! [`ImportedSession`] deliberately has **no password field** —
//! dropping secrets is a compile-time property, not discipline.

use super::Source;

/// One importable session draft. The caller persists it (as
/// `SessionEntry` or a store row) after the user confirms the preview.
#[derive(Debug, Clone)]
pub struct ImportedSession {
    /// Proposed display name (unique within one import; collisions with
    /// the existing store are resolved by [`super::merge`] suggestions).
    pub proposed_name: String,
    /// Ready-to-store spec (passwords never survive parsing).
    pub spec: mbxt_core::SessionSpec,
    /// Where it came from (source tag + file basename, never a full
    /// path — paths may embed usernames).
    pub provenance: String,
    /// Disabled-by-default flags (`proxy-command`, `auto-command`):
    /// quarantined behaviors needing explicit per-host review.
    pub disabled_flags: Vec<String>,
}

impl ImportedSession {
    pub fn new(
        proposed_name: String,
        spec: mbxt_core::SessionSpec,
        source: Source,
        file_name: &str,
    ) -> Self {
        Self {
            proposed_name,
            spec,
            provenance: format!("Imported from {} ({})", source.tag(), file_name),
            disabled_flags: Vec::new(),
        }
    }
}

/// Machine-readable warning codes (each maps to a Fluent `import-*` key
/// at the UI layer; here they stay plain data for snapshots).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningCode {
    UnknownField,
    UnknownDirective,
    UnknownProtocol,
    UnsupportedProtocol,
    MissingHost,
    NoResolvableHost,
    UnnamedSessionRejected,
    PasswordDropped,
    ProxyCommandQuarantined,
    ProxyDisabledPendingReview,
    AutoCommandIgnored,
    AgentForwardingDisabled,
    SafePreferenceNoted,
    ExtraIdentityFilesNoted,
    ShadowedByPrecedence,
    IncludeOutsideRoot,
    IncludeNoMatch,
    IncludeDepthExceeded,
    MatchBlockSkipped,
    WildcardPatternKeptLiteral,
    AliasesCollapsed,
    NameCollisionSkipped,
    DuplicateNameInImport,
    TooManySessionsTruncated,
}

impl WarningCode {
    pub fn as_str(self) -> &'static str {
        match self {
            WarningCode::UnknownField => "unknown-field",
            WarningCode::UnknownDirective => "unknown-directive",
            WarningCode::UnknownProtocol => "unknown-protocol",
            WarningCode::UnsupportedProtocol => "unsupported-protocol",
            WarningCode::MissingHost => "missing-host",
            WarningCode::NoResolvableHost => "no-resolvable-host",
            WarningCode::UnnamedSessionRejected => "unnamed-session-rejected",
            WarningCode::PasswordDropped => "password-dropped",
            WarningCode::ProxyCommandQuarantined => "proxy-command-quarantined",
            WarningCode::ProxyDisabledPendingReview => "proxy-disabled-pending-review",
            WarningCode::AutoCommandIgnored => "auto-command-ignored",
            WarningCode::AgentForwardingDisabled => "agent-forwarding-disabled",
            WarningCode::SafePreferenceNoted => "safe-preference-noted",
            WarningCode::ExtraIdentityFilesNoted => "extra-identity-files-noted",
            WarningCode::ShadowedByPrecedence => "shadowed-by-precedence",
            WarningCode::IncludeOutsideRoot => "include-outside-root",
            WarningCode::IncludeNoMatch => "include-no-match",
            WarningCode::IncludeDepthExceeded => "include-depth-exceeded",
            WarningCode::MatchBlockSkipped => "match-block-skipped",
            WarningCode::WildcardPatternKeptLiteral => "wildcard-pattern-kept-literal",
            WarningCode::AliasesCollapsed => "aliases-collapsed",
            WarningCode::NameCollisionSkipped => "name-collision-skipped",
            WarningCode::DuplicateNameInImport => "duplicate-name-in-import",
            WarningCode::TooManySessionsTruncated => "too-many-sessions-truncated",
        }
    }
}

/// One reported oddity: code + source + optional line + human detail.
/// Values of dropped secrets never appear in `detail`.
#[derive(Debug, Clone)]
pub struct ImportWarning {
    pub code: WarningCode,
    pub source: Source,
    pub line: Option<usize>,
    pub detail: String,
}

impl ImportWarning {
    pub fn new(code: WarningCode, source: Source, detail: impl Into<String>) -> Self {
        Self {
            code,
            source,
            line: None,
            detail: detail.into(),
        }
    }

    pub fn at_line(
        code: WarningCode,
        source: Source,
        line: usize,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code,
            source,
            line: Some(line),
            detail: detail.into(),
        }
    }
}

/// A key is password-shaped when its name suggests a secret. Every
/// parser drops matching values with [`WarningCode::PasswordDropped`].
pub fn is_password_shaped(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("password") || lower.contains("passwd") || lower.contains("secret")
}
