//! Macro player (Prompt 5.2 output path): message-driven step machine.
//!
//! [`MacroPlayer`] owns no I/O: `step()` returns an effect
//! ([`StepEffect`]) the driver (update loop) executes — send bytes, sleep,
//! await a pattern, or ask the user for a variable. Output observations flow
//! back through [`MacroPlayer::observe_output`]. Cancellation is immediate:
//! `stop()` drops the stack, and in-flight sleeps resolve into a stopped
//! player on the next advance. `step()` executes exactly one step per call
//! (single-stepping), which the editor's dry-run mode also uses.

use std::time::{Duration, Instant};

use super::variables::{substitute, Context};
use super::{Macro, MacroStep};

/// One driver effect per [`MacroPlayer::step`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepEffect {
    /// Transmit bytes to the session.
    Send(Vec<u8>),
    /// Sleep, then advance again.
    Sleep(u64),
    /// Feed output until the pattern matches or the timeout lapses.
    AwaitPattern,
    /// A variable value is needed first.
    NeedVariable {
        name: String,
        prompt: String,
        default: Option<String>,
        secret: bool,
    },
    /// Playback finished (or was stopped with an empty stack).
    Finished,
}

/// Player failures (graceful, user-actionable — never a panic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerError {
    Cancelled,
    Timeout(String),
    MissingVariable(String),
    Empty,
}

impl std::fmt::Display for PlayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "macro cancelled"),
            Self::Timeout(pattern) => write!(f, "timed out waiting for {pattern:?}"),
            Self::MissingVariable(name) => write!(f, "missing variable: {name}"),
            Self::Empty => write!(f, "macro has no steps"),
        }
    }
}

/// One stack frame (loop bodies and conditional branches push frames, so
/// nesting works to any depth).
#[derive(Debug, Clone)]
struct Frame {
    steps: Vec<MacroStep>,
    index: usize,
    loop_remaining: u32,
}

/// Message-driven macro player.
#[derive(Debug)]
pub struct MacroPlayer {
    macro_id: uuid::Uuid,
    macro_name: String,
    stack: Vec<Frame>,
    context: Context,
    awaiting: Option<AwaitingPattern>,
    waiting_deadline: Option<Instant>,
    completed_steps: usize,
    stopped: bool,
}

#[derive(Debug, Clone)]
struct AwaitingPattern {
    pattern: String,
    timeout_ms: u64,
    started: Instant,
}

impl MacroPlayer {
    /// New player over `macro_` with `context` (builtins + answers).
    pub fn new(macro_: &Macro, context: Context) -> Self {
        Self {
            macro_id: macro_.id,
            macro_name: macro_.name.clone(),
            stack: vec![Frame {
                steps: macro_.steps.clone(),
                index: 0,
                loop_remaining: 1,
            }],
            context,
            awaiting: None,
            waiting_deadline: None,
            completed_steps: 0,
            stopped: false,
        }
    }

    pub fn macro_id(&self) -> uuid::Uuid {
        self.macro_id
    }

    pub fn macro_name(&self) -> &str {
        &self.macro_name
    }

    /// `(completed steps, total static steps)`.
    pub fn progress(&self) -> (usize, usize) {
        (
            self.completed_steps,
            self.stack
                .first()
                .map(|frame| count_frame(&frame.steps))
                .unwrap_or(0),
        )
    }

    /// Cancel any time (a pending wait resolves as cancelled on observe).
    pub fn stop(&mut self) {
        self.stopped = true;
        self.stack.clear();
        self.awaiting = None;
        self.waiting_deadline = None;
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    pub fn is_waiting(&self) -> bool {
        self.awaiting.is_some()
    }

    /// Execute exactly one step.
    pub fn step(&mut self) -> Result<StepEffect, PlayerError> {
        if self.stopped {
            return Err(PlayerError::Cancelled);
        }
        if self.awaiting.is_some() {
            return Ok(StepEffect::AwaitPattern);
        }
        loop {
            let Some(frame) = self.stack.last_mut() else {
                return Ok(StepEffect::Finished);
            };
            let Some(step) = frame.steps.get(frame.index).cloned() else {
                // Frame exhausted: loop again or pop.
                let remaining = frame.loop_remaining;
                if remaining > 1 {
                    frame.loop_remaining = remaining - 1;
                    frame.index = 0;
                    continue;
                }
                self.stack.pop();
                continue;
            };
            frame.index += 1;
            self.completed_steps += 1;
            match step {
                MacroStep::Comment { .. } => continue,
                MacroStep::SendInput { data } => {
                    return Ok(StepEffect::Send(
                        substitute(&data, &self.context).into_bytes(),
                    ));
                },
                MacroStep::SendKey { key } => return Ok(StepEffect::Send(key.to_bytes())),
                MacroStep::SendVariable { name } => {
                    let Some(value) = self.context.get(&name).map(str::to_string) else {
                        self.completed_steps -= 1;
                        rewind_one(self);
                        return Err(PlayerError::MissingVariable(name));
                    };
                    return Ok(StepEffect::Send(value.into_bytes()));
                },
                MacroStep::SetVariable { name, value } => {
                    let value = substitute(&value, &self.context);
                    self.context.set(&name, value, false);
                    continue;
                },
                MacroStep::PromptVariable {
                    name,
                    prompt,
                    default,
                    secret,
                } => {
                    if self.context.get(&name).is_none() {
                        self.completed_steps -= 1;
                        rewind_one(self);
                        return Ok(StepEffect::NeedVariable {
                            name,
                            prompt,
                            default,
                            secret,
                        });
                    }
                    continue;
                },
                MacroStep::Wait { duration_ms } => {
                    return Ok(StepEffect::Sleep(duration_ms));
                },
                MacroStep::WaitForPattern {
                    pattern,
                    timeout_ms,
                } => {
                    let pattern = substitute(&pattern, &self.context);
                    if timeout_ms == 0 {
                        return Err(PlayerError::Timeout(pattern));
                    }
                    self.awaiting = Some(AwaitingPattern {
                        pattern,
                        timeout_ms,
                        started: Instant::now(),
                    });
                    return Ok(StepEffect::AwaitPattern);
                },
                MacroStep::Conditional {
                    expression,
                    then_steps,
                    else_steps,
                } => {
                    let branch = if evaluate_condition(&expression, &self.context) {
                        then_steps
                    } else {
                        else_steps
                    };
                    self.stack.push(Frame {
                        steps: branch,
                        index: 0,
                        loop_remaining: 1,
                    });
                    continue;
                },
                MacroStep::Loop { count, steps } => {
                    if count == 0 {
                        continue;
                    }
                    self.stack.push(Frame {
                        steps,
                        index: 0,
                        loop_remaining: count,
                    });
                    continue;
                },
            }
        }
    }

    /// Answer a pending variable prompt, then continue.
    pub fn answer_variable(&mut self, name: &str, value: String, secret: bool) {
        self.context.set(name, value, secret);
    }

    /// Feed terminal output: completes a pending pattern wait when the
    /// buffer contains it, or fails on timeout/cancel.
    pub fn observe_output(&mut self, text: &str) -> Result<bool, PlayerError> {
        if self.stopped {
            return Err(PlayerError::Cancelled);
        }
        let Some(awaiting) = self.awaiting.clone() else {
            return Ok(false);
        };
        if text.contains(&awaiting.pattern) {
            self.awaiting = None;
            self.waiting_deadline = None;
            return Ok(true);
        }
        if awaiting.started.elapsed() > Duration::from_millis(awaiting.timeout_ms) {
            self.awaiting = None;
            self.waiting_deadline = None;
            return Err(PlayerError::Timeout(awaiting.pattern));
        }
        Ok(false)
    }

    /// Deadline for the pending wait, if any (drivers schedule a re-check).
    pub fn wait_deadline_ms(&self) -> Option<u64> {
        self.awaiting.as_ref().map(|awaiting| awaiting.timeout_ms)
    }

    /// Fail an expired wait without new output (timeout-guard path).
    pub fn check_timeout(&mut self) -> Result<(), PlayerError> {
        if self.stopped {
            return Err(PlayerError::Cancelled);
        }
        if let Some(awaiting) = self.awaiting.clone() {
            if awaiting.started.elapsed() > Duration::from_millis(awaiting.timeout_ms) {
                self.awaiting = None;
                self.waiting_deadline = None;
                return Err(PlayerError::Timeout(awaiting.pattern));
            }
        }
        Ok(())
    }

    /// Variable context (inspection, tests).
    pub fn context(&self) -> &Context {
        &self.context
    }
}

/// Rewind one step so answering a variable re-executes the asker.
fn rewind_one(player: &mut MacroPlayer) {
    if let Some(frame) = player.stack.last_mut() {
        frame.index = frame.index.saturating_sub(1);
    }
}

/// Count static steps recursively (progress totals).
pub fn count_steps(macro_: &Macro) -> usize {
    count_frame(&macro_.steps)
}

fn count_frame(steps: &[MacroStep]) -> usize {
    steps
        .iter()
        .map(|step| match step {
            MacroStep::Conditional {
                then_steps,
                else_steps,
                ..
            } => 1 + count_frame(then_steps) + count_frame(else_steps),
            MacroStep::Loop { steps, .. } => 1 + count_frame(steps),
            _ => 1,
        })
        .sum()
}

/// Tiny conditional language: `var == value`, `var != value`,
/// `empty(var)`, `!empty(var)`. Anything else is `false` (never crash on
/// user input).
pub fn evaluate_condition(expression: &str, context: &Context) -> bool {
    let expression = expression.trim();
    if let Some(inner) = expression
        .strip_prefix("!empty(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return context.get(inner.trim()).is_some_and(|v| !v.is_empty());
    }
    if let Some(inner) = expression
        .strip_prefix("empty(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return context.get(inner.trim()).map_or(true, |v| v.is_empty());
    }
    if let Some((left, right)) = expression.split_once("==") {
        let var = left.trim();
        let value = right.trim().trim_matches(['"', '\'']);
        return context.get(var) == Some(value);
    }
    if let Some((left, right)) = expression.split_once("!=") {
        let var = left.trim();
        let value = right.trim().trim_matches(['"', '\'']);
        return context.get(var).map(|v| v != value).unwrap_or(true);
    }
    false
}

// ---------------------------------------------------------------------------
// Dry run (editor test mode)
// ---------------------------------------------------------------------------

/// Editor test-mode report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunReport {
    pub completed: bool,
    pub steps_executed: usize,
    pub bytes_sent: usize,
    pub error: Option<String>,
    /// Final screen text (visible rows joined).
    pub screen: Vec<String>,
}

/// Play a macro against a headless terminal emulator: `SendInput` bytes go
/// straight into the grid, `WaitForPattern` matches visible text, `Wait`
/// steps are recorded without sleeping, prompts resolve from defaults
/// (required variables without defaults fail fast).
pub fn dry_run(macro_: &Macro, context: &mut Context) -> DryRunReport {
    let mut player = MacroPlayer::new(macro_, context.clone());
    let mut terminal = mbxt_terminal::Terminal::new(80, 24, 100);
    let mut steps_executed = 0usize;
    let mut bytes_sent = 0usize;
    loop {
        let effect = match player.step() {
            Ok(effect) => effect,
            Err(err) => {
                return DryRunReport {
                    completed: false,
                    steps_executed,
                    bytes_sent,
                    error: Some(err.to_string()),
                    screen: terminal.grid.visible_text(),
                };
            },
        };
        match effect {
            StepEffect::Send(bytes) => {
                steps_executed += 1;
                bytes_sent += bytes.len();
                terminal.write_bytes(&bytes);
            },
            StepEffect::Sleep(_) => {
                steps_executed += 1;
            },
            StepEffect::AwaitPattern => {
                steps_executed += 1;
                let text = terminal.grid.visible_text().join("\n");
                match player.observe_output(&text) {
                    Ok(true) => {},
                    Ok(false) => {
                        return DryRunReport {
                            completed: false,
                            steps_executed,
                            bytes_sent,
                            error: Some("pattern not present in dry-run screen".into()),
                            screen: terminal.grid.visible_text(),
                        };
                    },
                    Err(err) => {
                        return DryRunReport {
                            completed: false,
                            steps_executed,
                            bytes_sent,
                            error: Some(err.to_string()),
                            screen: terminal.grid.visible_text(),
                        };
                    },
                }
            },
            StepEffect::NeedVariable { name, default, .. } => {
                if let Some(value) = default {
                    player.answer_variable(&name, value, false);
                } else {
                    return DryRunReport {
                        completed: false,
                        steps_executed,
                        bytes_sent,
                        error: Some(format!("missing variable: {name}")),
                        screen: terminal.grid.visible_text(),
                    };
                }
            },
            StepEffect::Finished => {
                return DryRunReport {
                    completed: true,
                    steps_executed,
                    bytes_sent,
                    error: None,
                    screen: terminal.grid.visible_text(),
                };
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context_with(pairs: &[(&str, &str)]) -> Context {
        let mut context = Context::new();
        for (name, value) in pairs {
            context.set(name, (*value).to_string(), false);
        }
        context
    }

    fn play_all(player: &mut MacroPlayer) -> Vec<StepEffect> {
        let mut effects = Vec::new();
        for _ in 0..64 {
            match player.step().expect("step") {
                StepEffect::Finished => break,
                StepEffect::AwaitPattern => panic!("unexpected wait in linear play"),
                effect => effects.push(effect),
            }
        }
        effects
    }

    #[test]
    fn linear_play_sends_substituted_bytes() {
        let macro_ = Macro {
            steps: vec![
                MacroStep::Comment { text: "hi".into() },
                MacroStep::SendInput {
                    data: "ssh {{host}}\n".into(),
                },
                MacroStep::Wait { duration_ms: 250 },
            ],
            ..Macro::new("demo".into())
        };
        let mut player = MacroPlayer::new(&macro_, context_with(&[("host", "h")]));
        assert_eq!(
            play_all(&mut player),
            vec![
                StepEffect::Send(b"ssh h\n".to_vec()),
                StepEffect::Sleep(250),
            ]
        );
        assert_eq!(player.progress().0, 3);
    }

    #[test]
    fn wait_for_pattern_matches_and_times_out() {
        let macro_ = Macro {
            steps: vec![MacroStep::WaitForPattern {
                pattern: "$ ".into(),
                timeout_ms: 50,
            }],
            ..Macro::new("demo".into())
        };
        let mut player = MacroPlayer::new(&macro_, Context::new());
        assert_eq!(player.step().unwrap(), StepEffect::AwaitPattern);
        assert!(player.is_waiting());
        assert!(player.observe_output("user@h:~$ ").unwrap());
        assert_eq!(player.step().unwrap(), StepEffect::Finished);

        let mut player = MacroPlayer::new(&macro_, Context::new());
        assert_eq!(player.step().unwrap(), StepEffect::AwaitPattern);
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert!(matches!(
            player.observe_output("nothing here"),
            Err(PlayerError::Timeout(_))
        ));
    }

    #[test]
    fn variables_set_prompt_and_send() {
        let macro_ = Macro {
            steps: vec![
                MacroStep::SetVariable {
                    name: "a".into(),
                    value: "x{{b}}".into(),
                },
                MacroStep::PromptVariable {
                    name: "pw".into(),
                    prompt: "Password".into(),
                    default: None,
                    secret: true,
                },
                MacroStep::SendVariable { name: "pw".into() },
            ],
            ..Macro::new("demo".into())
        };
        let mut player = MacroPlayer::new(&macro_, context_with(&[("b", "y")]));
        // SetVariable applies, then the prompt surfaces (value substituted).
        match player.step().unwrap() {
            StepEffect::NeedVariable { name, .. } => {
                assert_eq!(name, "pw");
                assert_eq!(player.context().get("a"), Some("xy"));
            },
            other => panic!("expected NeedVariable, got {other:?}"),
        }
        player.answer_variable("pw", "s3krit".into(), true);
        assert_eq!(player.step().unwrap(), StepEffect::Send(b"s3krit".to_vec()));
        assert_eq!(player.step().unwrap(), StepEffect::Finished);
    }

    #[test]
    fn loops_and_conditionals_nest() {
        let macro_ = Macro {
            steps: vec![MacroStep::Loop {
                count: 2,
                steps: vec![MacroStep::Conditional {
                    expression: "mode == fast".into(),
                    then_steps: vec![MacroStep::SendInput { data: "f".into() }],
                    else_steps: vec![MacroStep::SendInput { data: "s".into() }],
                }],
            }],
            ..Macro::new("demo".into())
        };
        let mut player = MacroPlayer::new(&macro_, context_with(&[("mode", "fast")]));
        let effects = play_all(&mut player);
        assert_eq!(
            effects,
            vec![
                StepEffect::Send(b"f".to_vec()),
                StepEffect::Send(b"f".to_vec()),
            ]
        );
    }

    #[test]
    fn condition_language() {
        let context = context_with(&[("a", "1"), ("e", "")]);
        assert!(evaluate_condition("a == 1", &context));
        assert!(!evaluate_condition("a == 2", &context));
        assert!(evaluate_condition("a != 2", &context));
        assert!(evaluate_condition("empty(e)", &context));
        assert!(evaluate_condition("!empty(a)", &context));
        assert!(!evaluate_condition("empty(a)", &context));
        assert!(!evaluate_condition("nonsense(((", &context));
    }

    #[test]
    fn dry_run_executes_against_headless_terminal() {
        let macro_ = Macro {
            steps: vec![
                MacroStep::SendInput {
                    data: "echo hi\n".into(),
                },
                MacroStep::SetVariable {
                    name: "x".into(),
                    value: "1".into(),
                },
            ],
            ..Macro::new("demo".into())
        };
        let mut context = Context::new();
        let report = dry_run(&macro_, &mut context);
        assert!(report.completed, "report: {report:?}");
        assert!(report.bytes_sent > 0);
        assert!(report.screen.join("\n").contains("echo hi"));
    }
}
