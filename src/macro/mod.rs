//! Macro recording & playback (Prompt 5.2): data model, recorder, player,
//! variable engine, and RON storage.
//!
//! Adaptations from the prompt spec (all documented at the item):
//! - Crate layout: the `macro` identifier is a Rust keyword, so the module
//!   is `macros` living in `src/macro/` (output paths preserved).
//! - Timestamps are unix seconds (`u64`), matching the rest of the codebase
//!   (`SessionEntry::created_secs`) instead of `chrono::DateTime` (no chrono
//!   dependency).
//! - `MacroStep::SendKey` carries an owned [`MacroKey`] (serializable name +
//!   modifiers) instead of iced's `KeyCode`/`Modifiers`, keeping the model
//!   UI-free; it encodes through `mbxt-terminal`'s key encoder.
//! - Storage is pretty-printed RON (like `config.ron`), one file per macro;
//!   packs are single-file RON bundles (no zip dependency).
//! - Editor reordering uses up/down buttons (iced has no widget drag-drop).

pub mod player;
pub mod recorder;
pub mod storage;
pub mod variables;

pub use player::{count_steps, dry_run, DryRunReport, MacroPlayer, PlayerError, StepEffect};
pub use recorder::{MacroRecorder, RecorderState};
pub use storage::MacroStore;
pub use variables::{sanitized_preview, Context};

use mbxt_core::{Protocol, SessionId};

/// One recorded key press (serializable; encodes to terminal bytes).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MacroKey {
    /// Key name: single printable char, `Enter`, `Backspace`, `Tab`,
    /// `Escape`, `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `Insert`,
    /// `Delete`, `PageUp`, `PageDown`, or `F1`–`F12`.
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl MacroKey {
    /// Encode to terminal input bytes via the shared key encoder.
    pub fn to_bytes(&self) -> Vec<u8> {
        use mbxt_terminal::input::{encode_key, Key};
        use mbxt_terminal::TerminalMode;
        let mode = TerminalMode::default();
        let key = match self.key.as_str() {
            "Enter" => Key::Enter,
            "Backspace" => Key::Backspace,
            "Tab" => Key::Tab,
            "Escape" => Key::Escape,
            "Up" => Key::Up,
            "Down" => Key::Down,
            "Left" => Key::Left,
            "Right" => Key::Right,
            "Home" => Key::Home,
            "End" => Key::End,
            "Insert" => Key::Insert,
            "Delete" => Key::Delete,
            "PageUp" => Key::PageUp,
            "PageDown" => Key::PageDown,
            name if name.starts_with('F') => match name[1..].parse::<u8>() {
                Ok(n) if (1..=12).contains(&n) => Key::Function(n),
                _ => return self.key.as_bytes().to_vec(),
            },
            name => match name.chars().collect::<Vec<_>>()[..] {
                [c] => Key::Char(c),
                _ => return self.key.as_bytes().to_vec(),
            },
        };
        encode_key(key, self.ctrl, self.alt, &mode)
    }
}

/// One recorded/playable macro (spec shape; timestamps are unix seconds).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Macro {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_secs: u64,
    pub updated_secs: u64,
    pub steps: Vec<MacroStep>,
    pub variables: Vec<MacroVariable>,
    pub target_protocol: Option<Protocol>,
    pub author: Option<String>,
    /// Hotkey binding (`F5`–`F8`) for one-press playback, if any.
    pub hotkey: Option<String>,
}

impl Macro {
    /// New macro shell (id + timestamps stamped now).
    pub fn new(name: String) -> Self {
        let now = now_secs();
        Self {
            id: uuid::Uuid::new_v4(),
            name,
            description: None,
            tags: Vec::new(),
            created_secs: now,
            updated_secs: now,
            steps: Vec::new(),
            variables: Vec::new(),
            target_protocol: None,
            author: None,
            hotkey: None,
        }
    }

    /// `true` when any `SendInput` step trips the destructive gate.
    pub fn has_destructive_steps(&self) -> bool {
        self.steps.iter().any(|step| match step {
            MacroStep::SendInput { data } => {
                crate::app::multi_exec::is_destructive(data.as_bytes())
            },
            _ => false,
        })
    }
}

/// One macro step (spec shape; see module docs for `SendKey` adaptation).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MacroStep {
    /// Literal text to send (supports `{{variables}}`).
    SendInput { data: String },
    /// Encoded key press.
    SendKey { key: MacroKey },
    /// Wait before the next step.
    Wait { duration_ms: u64 },
    /// Wait until output contains `pattern` (supports `{{variables}}`).
    WaitForPattern { pattern: String, timeout_ms: u64 },
    /// Set a variable without prompting.
    SetVariable { name: String, value: String },
    /// Ask the user for a variable at playback.
    PromptVariable {
        name: String,
        prompt: String,
        default: Option<String>,
        secret: bool,
    },
    /// Send a variable's value.
    SendVariable { name: String },
    /// Tiny expression language (`var == value`, `var != value`,
    /// `empty(var)`, `!empty(var)`).
    Conditional {
        expression: String,
        then_steps: Vec<MacroStep>,
        else_steps: Vec<MacroStep>,
    },
    /// Repeat nested steps.
    Loop { count: u32, steps: Vec<MacroStep> },
    /// Annotation (never executes).
    Comment { text: String },
}

impl MacroStep {
    /// One-line preview for the editor (secrets masked via `secrets`).
    pub fn preview(&self, secrets: &[String]) -> String {
        let mask = |text: &str| sanitized_preview(text, secrets);
        match self {
            Self::SendInput { data } => format!("send {:?}", mask(data)),
            Self::SendKey { key } => {
                let mut mods = String::new();
                if key.ctrl {
                    mods.push_str("Ctrl+");
                }
                if key.alt {
                    mods.push_str("Alt+");
                }
                if key.shift {
                    mods.push_str("Shift+");
                }
                format!("key {mods}{}", key.key)
            },
            Self::Wait { duration_ms } => format!("wait {duration_ms}ms"),
            Self::WaitForPattern {
                pattern,
                timeout_ms,
            } => {
                format!("wait for {:?} ({}ms)", mask(pattern), timeout_ms)
            },
            Self::SetVariable { name, value } => format!("set {name} = {:?}", mask(value)),
            Self::PromptVariable {
                name,
                prompt,
                default,
                secret,
            } => {
                let star = if *secret { " (secret)" } else { "" };
                format!(
                    "ask {name}: {prompt}{star}{}",
                    default
                        .as_deref()
                        .map(|d| format!(" [{d}]"))
                        .unwrap_or_default()
                )
            },
            Self::SendVariable { name } => format!("send ${{{name}}}"),
            Self::Conditional {
                expression,
                then_steps,
                else_steps,
            } => {
                format!(
                    "if {expression} ({} / {} steps)",
                    then_steps.len(),
                    else_steps.len()
                )
            },
            Self::Loop { count, steps } => format!("repeat {count}× ({} steps)", steps.len()),
            Self::Comment { text } => format!("# {text}"),
        }
    }
}

/// A macro variable declaration.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MacroVariable {
    pub name: String,
    pub default_value: Option<String>,
    pub description: Option<String>,
    pub secret: bool,
    pub required: bool,
}

/// A running playback (player + target session).
#[derive(Debug)]
pub struct RunningMacro {
    pub player: MacroPlayer,
    pub session: SessionId,
}

/// Pending variable prompt (modal form state).
#[derive(Debug, Clone)]
pub struct MacroPrompt {
    pub var_name: String,
    pub prompt: String,
    pub default: Option<String>,
    pub secret: bool,
    pub session: SessionId,
}

/// Pending destructive play awaiting confirmation.
#[derive(Debug, Clone)]
pub struct PendingPlay {
    pub macro_id: uuid::Uuid,
    /// `None` = multi-exec targets, `Some` = single session.
    pub session: Option<SessionId>,
    /// Accumulated variable answers (prompts fill this in).
    pub vars: Context,
}

/// Editor draft state (owned here so `AppState` stays a thin holder).
#[derive(Debug, Clone)]
pub struct MacroEditorState {
    pub draft: Macro,
    pub selected: Option<usize>,
    pub new_kind: MacroStepKind,
    pub text1: String,
    pub text2: String,
    pub number: u64,
    pub report: Option<String>,
}

impl MacroEditorState {
    pub fn new(draft: Macro) -> Self {
        Self {
            draft,
            selected: None,
            new_kind: MacroStepKind::Input,
            text1: String::new(),
            text2: String::new(),
            number: 1000,
            report: None,
        }
    }

    /// Build a step from the new-step form (numbers parsed, keys validated).
    pub fn build_step(&self) -> Result<MacroStep, String> {
        match self.new_kind {
            MacroStepKind::Input => Ok(MacroStep::SendInput {
                data: self.text1.clone(),
            }),
            MacroStepKind::Key => {
                if self.text1.trim().is_empty() {
                    return Err("key name is empty".to_string());
                }
                Ok(MacroStep::SendKey {
                    key: MacroKey {
                        key: self.text1.trim().to_string(),
                        ctrl: false,
                        alt: false,
                        shift: false,
                    },
                })
            },
            MacroStepKind::Wait => Ok(MacroStep::Wait {
                duration_ms: self.number,
            }),
            MacroStepKind::WaitPattern => {
                if self.text1.is_empty() {
                    return Err("pattern is empty".to_string());
                }
                Ok(MacroStep::WaitForPattern {
                    pattern: self.text1.clone(),
                    timeout_ms: self.number.max(1),
                })
            },
            MacroStepKind::SetVar => {
                if self.text1.trim().is_empty() {
                    return Err("variable name is empty".to_string());
                }
                Ok(MacroStep::SetVariable {
                    name: self.text1.trim().to_string(),
                    value: self.text2.clone(),
                })
            },
            MacroStepKind::PromptVar => {
                if self.text1.trim().is_empty() {
                    return Err("variable name is empty".to_string());
                }
                Ok(MacroStep::PromptVariable {
                    name: self.text1.trim().to_string(),
                    prompt: if self.text2.is_empty() {
                        format!("Value for {}", self.text1.trim())
                    } else {
                        self.text2.clone()
                    },
                    default: None,
                    secret: false,
                })
            },
            MacroStepKind::SendVar => {
                if self.text1.trim().is_empty() {
                    return Err("variable name is empty".to_string());
                }
                Ok(MacroStep::SendVariable {
                    name: self.text1.trim().to_string(),
                })
            },
            MacroStepKind::Conditional => Ok(MacroStep::Conditional {
                expression: if self.text1.is_empty() {
                    "mode == fast".to_string()
                } else {
                    self.text1.clone()
                },
                then_steps: Vec::new(),
                else_steps: Vec::new(),
            }),
            MacroStepKind::Loop => Ok(MacroStep::Loop {
                count: self.number.max(1) as u32,
                steps: Vec::new(),
            }),
            MacroStepKind::Comment => Ok(MacroStep::Comment {
                text: self.text1.clone(),
            }),
        }
    }
}

/// New-step kinds offered by the editor form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MacroStepKind {
    #[default]
    Input,
    Key,
    Wait,
    WaitPattern,
    SetVar,
    PromptVar,
    SendVar,
    Conditional,
    Loop,
    Comment,
}

impl MacroStepKind {
    pub const ALL: [Self; 10] = [
        Self::Input,
        Self::Key,
        Self::Wait,
        Self::WaitPattern,
        Self::SetVar,
        Self::PromptVar,
        Self::SendVar,
        Self::Conditional,
        Self::Loop,
        Self::Comment,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Input => "send text",
            Self::Key => "key press",
            Self::Wait => "wait",
            Self::WaitPattern => "wait for prompt",
            Self::SetVar => "set variable",
            Self::PromptVar => "ask variable",
            Self::SendVar => "send variable",
            Self::Conditional => "if/else",
            Self::Loop => "repeat",
            Self::Comment => "comment",
        }
    }
}

/// Unix seconds (local copy: this module must not depend on `app`).
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_shell_stamps_id_and_time() {
        let a = Macro::new("demo".into());
        let b = Macro::new("demo".into());
        assert_ne!(a.id, b.id, "v4 ids are unique");
        assert!(a.created_secs > 0);
    }

    #[test]
    fn destructive_scan_covers_steps() {
        let mut macro_ = Macro::new("ops".into());
        macro_.steps.push(MacroStep::SendInput {
            data: "uptime".into(),
        });
        assert!(!macro_.has_destructive_steps());
        macro_.steps.push(MacroStep::SendInput {
            data: "rm -rf /tmp/x".into(),
        });
        assert!(macro_.has_destructive_steps());
    }

    #[test]
    fn key_encoding_covers_named_and_text() {
        let enter = MacroKey {
            key: "Enter".into(),
            ctrl: false,
            alt: false,
            shift: false,
        };
        assert_eq!(enter.to_bytes(), b"\r");
        let text = MacroKey {
            key: "ls".into(),
            ctrl: false,
            alt: false,
            shift: false,
        };
        assert_eq!(text.to_bytes(), b"ls");
        let f5 = MacroKey {
            key: "F5".into(),
            ctrl: false,
            alt: false,
            shift: false,
        };
        assert!(!f5.to_bytes().is_empty());
    }

    #[test]
    fn macro_round_trips_through_ron() {
        let mut macro_ = Macro::new("sysinfo".into());
        macro_.description = Some("gather facts".into());
        macro_.tags = vec!["linux".into()];
        macro_.steps = vec![
            MacroStep::SendInput {
                data: "uname -a\n".into(),
            },
            MacroStep::WaitForPattern {
                pattern: "$ ".into(),
                timeout_ms: 5000,
            },
        ];
        macro_.variables = vec![MacroVariable {
            name: "host".into(),
            default_value: None,
            description: None,
            secret: false,
            required: true,
        }];
        let text = ron::ser::to_string_pretty(&macro_, ron::ser::PrettyConfig::default()).unwrap();
        let back: Macro = ron::from_str(&text).unwrap();
        assert_eq!(back, macro_);
    }
}
