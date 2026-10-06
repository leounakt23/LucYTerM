//! Variable substitution engine (Prompt 5.2 output path).
//!
//! `{{name}}` placeholders expand from a [`Context`]; builtins
//! (`host`, `username`, `session_name`, `date`, `timestamp`) fill from the
//! session spec at play time. Secret values are tracked so logs and previews
//! mask them ([`sanitized_preview`]).

use std::collections::{HashMap, HashSet};

use mbxt_core::SessionSpec;

/// Live variable values + which names are secret.
#[derive(Debug, Clone, Default)]
pub struct Context {
    values: HashMap<String, String>,
    secrets: HashSet<String>,
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a value (`secret` marks it for masking in logs/previews).
    pub fn set(&mut self, name: &str, value: String, secret: bool) {
        if secret {
            self.secrets.insert(name.to_string());
        }
        self.values.insert(name.to_string(), value);
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// Names currently marked secret (redaction lists).
    pub fn secret_values(&self) -> Vec<String> {
        self.secrets
            .iter()
            .filter_map(|name| self.values.get(name).cloned())
            .filter(|value| !value.is_empty())
            .collect()
    }

    /// Fill builtins from the session (`{{host}}`, `{{username}}`,
    /// `{{session_name}}`, `{{date}}`, `{{timestamp}}`). Explicit values
    /// already present win over builtins.
    pub fn fill_builtins(&mut self, spec: &SessionSpec, now_secs: u64) {
        for (name, value) in [
            ("host", spec.host.clone().unwrap_or_default()),
            ("username", spec.username.clone().unwrap_or_default()),
            ("session_name", spec.name.clone()),
            ("date", format_date(now_secs)),
            ("timestamp", now_secs.to_string()),
        ] {
            self.values.entry(name.to_string()).or_insert(value);
        }
    }

    /// Missing required declarations (no value and no default).
    pub fn missing_required(&self, variables: &[super::MacroVariable]) -> Vec<String> {
        variables
            .iter()
            .filter(|var| {
                var.required && !self.values.contains_key(&var.name) && var.default_value.is_none()
            })
            .map(|var| var.name.clone())
            .collect()
    }

    /// Apply declared defaults for values not yet set.
    pub fn apply_defaults(&mut self, variables: &[super::MacroVariable]) {
        for var in variables {
            if let (None, Some(default)) = (self.values.get(&var.name), var.default_value.as_ref())
            {
                self.set(&var.name, default.clone(), var.secret);
            }
        }
    }
}

/// Expand `{{name}}` placeholders (unknown names pass through literally).
pub fn substitute(text: &str, context: &Context) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let name = after[..end].trim();
                out.push_str(context.get(name).unwrap_or(&format!("{{{{{name}}}}}")));
                rest = &after[end + 2..];
            },
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            },
        }
    }
    out.push_str(rest);
    out
}

/// Mask every secret value in `text` with `•••` (log/preview redaction).
pub fn sanitized_preview(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            out = out.replace(secret, "•••");
        }
    }
    out
}

/// `YYYY-MM-DD` from unix seconds (proleptic Gregorian, UTC).
fn format_date(secs: u64) -> String {
    const DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let days = secs / 86400;
    let (mut year, mut remaining) = (1970u32, days);
    loop {
        let leap = is_leap(year) as u64;
        if remaining < 365 + leap {
            break;
        }
        remaining -= 365 + leap;
        year += 1;
    }
    let mut month = 1u32;
    for (index, days_in_month) in DAYS.iter().enumerate() {
        let mut length = u64::from(*days_in_month);
        if index == 1 && is_leap(year) {
            length = 29;
        }
        if remaining < length {
            break;
        }
        remaining -= length;
        month += 1;
    }
    format!("{year:04}-{:02}-{:02}", month, remaining + 1)
}

fn is_leap(year: u32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SessionSpec {
        SessionSpec {
            name: "bastion".into(),
            protocol: mbxt_core::Protocol::Ssh,
            host: Some("10.0.0.1".into()),
            port: Some(2222),
            username: Some("ops".into()),
            auth: mbxt_core::AuthMethod::Password,
            tags: vec![],
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
        }
    }

    #[test]
    fn substitution_expands_and_passes_unknowns_through() {
        let mut context = Context::new();
        context.fill_builtins(&spec(), 1_700_000_000);
        assert_eq!(
            substitute("ssh {{username}}@{{host}} # {{session_name}}", &context),
            "ssh ops@10.0.0.1 # bastion"
        );
        assert_eq!(substitute("{{nope}}", &context), "{{nope}}");
        assert_eq!(substitute("dangling {{oops", &context), "dangling {{oops");
        assert_eq!(substitute("{{ timestamp }}", &context), "1700000000");
    }

    #[test]
    fn builtins_respect_explicit_values() {
        let mut context = Context::new();
        context.set("host", "override".into(), false);
        context.fill_builtins(&spec(), 0);
        assert_eq!(context.get("host"), Some("override"));
    }

    #[test]
    fn required_and_defaults() {
        use super::super::MacroVariable;
        let variables = vec![
            MacroVariable {
                name: "user".into(),
                default_value: None,
                description: None,
                secret: false,
                required: true,
            },
            MacroVariable {
                name: "port".into(),
                default_value: Some("22".into()),
                description: None,
                secret: false,
                required: true,
            },
        ];
        let mut context = Context::new();
        assert_eq!(context.missing_required(&variables), vec!["user"]);
        context.apply_defaults(&variables);
        assert_eq!(context.get("port"), Some("22"));
        assert_eq!(context.missing_required(&variables), vec!["user"]);
    }

    #[test]
    fn secrets_mask_everywhere() {
        let mut context = Context::new();
        context.set("pw", "hunter2".into(), true);
        context.set("user", "ops".into(), false);
        let masked = sanitized_preview("login hunter2 as ops:hunter2", &context.secret_values());
        assert_eq!(masked, "login ••• as ops:•••");
    }

    #[test]
    fn date_math_spots_leap_days() {
        // 2023-11-14 (matches 1700000000) and a leap day.
        assert_eq!(format_date(1_700_000_000), "2023-11-14");
        assert_eq!(format_date(1_582_934_400), "2020-02-29");
    }
}
