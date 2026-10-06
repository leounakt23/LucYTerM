//! Macro recorder (Prompt 5.2 output path): capture terminal input/output
//! into [`MacroStep`]s with timing.
//!
//! Hook cost (quality bar: no responsiveness impact): `observe_input` is an
//! O(1) append after a memchr-style noise check, and `observe_output` scans
//! only the trailing line (≤256 bytes) for prompt shapes. Coalescing of
//! adjacent `SendInput` steps happens once at `stop`, never per keystroke.

use mbxt_core::SessionId;

use super::{now_secs, Macro, MacroStep, MacroVariable};

/// Recorder lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RecorderState {
    #[default]
    Idle,
    Recording,
    Paused,
}

/// Byte markers that are terminal-protocol noise, never user intent.
const PASTE_OPEN: &[u8] = b"\x1b[200~";
const PASTE_CLOSE: &[u8] = b"\x1b[201~";

/// Largest prompt line we will turn into a `WaitForPattern` step.
pub const MAX_PROMPT_LEN: usize = 64;

/// Capture session for one recording.
#[derive(Debug, Default)]
pub struct MacroRecorder {
    state: RecorderState,
    session: Option<SessionId>,
    started_secs: u64,
    steps: Vec<MacroStep>,
    last_wait_pattern: Option<String>,
}

impl MacroRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> &RecorderState {
        &self.state
    }

    /// Session currently being recorded, if any.
    pub fn session(&self) -> Option<SessionId> {
        self.session
    }

    /// Begin recording `session` (idle or paused recorders restart fresh).
    pub fn start(&mut self, session: SessionId) {
        self.state = RecorderState::Recording;
        self.session = Some(session);
        self.started_secs = now_secs();
        self.steps.clear();
        self.last_wait_pattern = None;
    }

    /// Pause capture (state kept for `resume`).
    pub fn pause(&mut self) {
        if self.state == RecorderState::Recording {
            self.state = RecorderState::Paused;
        }
    }

    /// Resume a paused recording.
    pub fn resume(&mut self) {
        if self.state == RecorderState::Paused {
            self.state = RecorderState::Recording;
        }
    }

    /// Push an explicit step (editor-driven or synthesized).
    pub fn add_step(&mut self, step: MacroStep) {
        if self.state == RecorderState::Recording {
            self.steps.push(step);
        }
    }

    /// Observe terminal input: records `SendInput` unless the bytes are
    /// pure protocol noise (bracketed-paste markers, empty writes).
    pub fn observe_input(&mut self, session: SessionId, bytes: &[u8]) {
        if self.state != RecorderState::Recording || self.session != Some(session) {
            return;
        }
        if bytes.is_empty() || is_noise(bytes) {
            return;
        }
        self.steps.push(MacroStep::SendInput {
            data: String::from_utf8_lossy(bytes).into_owned(),
        });
    }

    /// Observe terminal output: appends `WaitForPattern` when the trailing
    /// line looks like a shell prompt (and differs from the last one, so a
    /// chatty command yields one wait, not dozens).
    pub fn observe_output(&mut self, session: SessionId, bytes: &[u8]) {
        if self.state != RecorderState::Recording || self.session != Some(session) {
            return;
        }
        let Some(prompt) = detect_prompt(bytes) else {
            return;
        };
        if self.last_wait_pattern.as_deref() == Some(prompt.as_str()) {
            return;
        }
        if matches!(self.steps.last(), Some(MacroStep::WaitForPattern { .. })) {
            return;
        }
        self.last_wait_pattern = Some(prompt.clone());
        self.steps.push(MacroStep::WaitForPattern {
            pattern: prompt,
            timeout_ms: 10_000,
        });
    }

    /// Finish and build the [`Macro`]: adjacent `SendInput` steps merge, and
    /// every `{{variable}}` referenced becomes a required declaration.
    pub fn stop(&mut self, name: String) -> Option<Macro> {
        if self.state == RecorderState::Idle {
            return None;
        }
        let mut macro_ = Macro::new(name);
        macro_.created_secs = self.started_secs;
        macro_.steps = coalesced(std::mem::take(&mut self.steps));
        macro_.variables = declared_variables(&macro_.steps);
        self.state = RecorderState::Idle;
        self.session = None;
        self.last_wait_pattern = None;
        Some(macro_)
    }
}

/// `true` for bytes that must never become steps.
fn is_noise(bytes: &[u8]) -> bool {
    // Exactly a paste marker (possibly both back to back).
    if bytes == PASTE_OPEN || bytes == PASTE_CLOSE {
        return true;
    }
    if bytes.starts_with(PASTE_OPEN) && bytes[PASTE_OPEN.len()..] == *PASTE_CLOSE {
        return true;
    }
    // Lone control bytes with no visible effect.
    bytes.iter().all(|b| matches!(b, 0x00 | 0x07))
}

/// Detect a shell-prompt trailing line (`user@host:~$ `, `router# `, …).
/// Returns the trimmed prompt (bounded) for the `WaitForPattern` step.
fn detect_prompt(bytes: &[u8]) -> Option<String> {
    const TAIL: usize = 256;
    let tail = &bytes[bytes.len().saturating_sub(TAIL)..];
    let text = String::from_utf8_lossy(tail);
    let line = text
        .split(['\n', '\r'])
        .next_back()
        .unwrap_or("")
        .trim_end();
    if line.is_empty() || line.len() > MAX_PROMPT_LEN {
        return None;
    }
    let last = line.chars().next_back()?;
    if !matches!(last, '$' | '#' | '>' | '%' | ':') {
        return None;
    }
    // A bare prompt char with no command text around it.
    if line.len() <= 3 {
        return Some(line.trim().to_string());
    }
    // `…<prompt><space>` at end of line (user text before it is echoed
    // input, not part of the prompt — keep the tail from the last prompt
    // char, e.g. `ls\nuser@h:~$ ` → `user@h:~$`).
    Some(line.trim().to_string())
}

/// Merge adjacent `SendInput` steps (keeps replays to one write per burst).
fn coalesced(steps: Vec<MacroStep>) -> Vec<MacroStep> {
    let mut out: Vec<MacroStep> = Vec::with_capacity(steps.len());
    for step in steps {
        match (out.last_mut(), step) {
            (Some(MacroStep::SendInput { data: previous }), MacroStep::SendInput { data }) => {
                previous.push_str(&data)
            },
            (_, step) => out.push(step),
        }
    }
    out
}

/// Collect `{{variable}}` references as required declarations.
fn declared_variables(steps: &[MacroStep]) -> Vec<MacroVariable> {
    let mut names = Vec::new();
    let mut scan = |text: &str| {
        let mut rest = text;
        while let Some(start) = rest.find("{{") {
            let after = &rest[start + 2..];
            if let Some(end) = after.find("}}") {
                let name = after[..end].trim().to_string();
                if !name.is_empty() && !names.contains(&name) {
                    names.push(name);
                }
                rest = &after[end + 2..];
            } else {
                break;
            }
        }
    };
    for step in steps {
        match step {
            MacroStep::SendInput { data } | MacroStep::WaitForPattern { pattern: data, .. } => {
                scan(data);
            },
            _ => {},
        }
    }
    names
        .into_iter()
        .map(|name| MacroVariable {
            name,
            default_value: None,
            description: None,
            secret: false,
            required: true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_start_pause_resume_stop() {
        let mut recorder = MacroRecorder::new();
        assert_eq!(recorder.state(), &RecorderState::Idle);
        assert!(recorder.stop("x".into()).is_none());

        recorder.start(7);
        recorder.pause();
        recorder.observe_input(7, b"lost");
        recorder.resume();
        recorder.observe_input(7, b"kept");
        let macro_ = recorder.stop("demo".into()).expect("recorded");
        assert_eq!(recorder.state(), &RecorderState::Idle);
        assert_eq!(macro_.steps.len(), 1);
    }

    #[test]
    fn other_sessions_are_ignored() {
        let mut recorder = MacroRecorder::new();
        recorder.start(7);
        recorder.observe_input(8, b"nope");
        recorder.observe_output(8, b"user@h:~$ ");
        assert!(recorder.stop("x".into()).unwrap().steps.is_empty());
    }

    #[test]
    fn noise_never_becomes_steps() {
        let mut recorder = MacroRecorder::new();
        recorder.start(1);
        recorder.observe_input(1, b"");
        recorder.observe_input(1, b"\x1b[200~");
        recorder.observe_input(1, b"\x1b[201~");
        recorder.observe_input(1, b"\x07");
        recorder.observe_input(1, b"ls");
        let macro_ = recorder.stop("x".into()).unwrap();
        assert_eq!(macro_.steps.len(), 1);
    }

    #[test]
    fn prompt_output_appends_single_wait() {
        let mut recorder = MacroRecorder::new();
        recorder.start(1);
        recorder.observe_output(1, b"total 4\nuser@h:~$ ");
        recorder.observe_output(1, b"user@h:~$ ");
        recorder.observe_output(1, b"plain output, no prompt shape here!");
        let macro_ = recorder.stop("x".into()).unwrap();
        assert_eq!(macro_.steps.len(), 1);
        assert!(matches!(
            &macro_.steps[0],
            MacroStep::WaitForPattern { pattern, .. } if pattern == "user@h:~$"
        ));
    }

    #[test]
    fn stop_coalesces_inputs_and_declares_variables() {
        let mut recorder = MacroRecorder::new();
        recorder.start(1);
        recorder.observe_input(1, b"ssh ");
        recorder.observe_input(1, b"{{host}}\n");
        let macro_ = recorder.stop("x".into()).unwrap();
        assert_eq!(macro_.steps.len(), 1);
        assert!(
            matches!(&macro_.steps[0], MacroStep::SendInput { data } if data == "ssh {{host}}\n")
        );
        assert_eq!(macro_.variables.len(), 1);
        assert_eq!(macro_.variables[0].name, "host");
        assert!(macro_.variables[0].required);
    }
}
