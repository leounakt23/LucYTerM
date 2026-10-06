//! Clipboard integration backed by `copypasta` (X11 / Wayland / Windows / OSX).
//!
//! Terminal-initiated clipboard writes arrive via OSC 52 and are forwarded
//! here; reads stay user-initiated only (security architecture §6.2).

use copypasta::ClipboardProvider;

/// Real clipboard context. Construction is infallible at type level;
/// backend availability is reported per operation.
#[derive(Debug, Default)]
pub struct Clipboard {
    _private: (),
}

impl Clipboard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store text on the system clipboard.
    pub fn set_text(&mut self, text: &str) -> Result<(), crate::SystemError> {
        let mut ctx = copypasta::ClipboardContext::new()
            .map_err(|_| crate::SystemError::ClipboardUnavailable)?;
        ctx.set_contents(text.to_owned())
            .map_err(|_| crate::SystemError::ClipboardUnavailable)?;
        Ok(())
    }

    /// Read the current clipboard contents (used by the change watcher).
    pub fn get_text(&self) -> Option<String> {
        let mut ctx = copypasta::ClipboardContext::new().ok()?;
        ctx.get_contents().ok()
    }
}
