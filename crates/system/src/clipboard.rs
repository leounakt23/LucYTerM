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

/// Persistent clipboard owner (X11/Wayland selections die with the
/// process/connection that owns them — see `throwaway_set_survives_reread`).
/// `Clipboard::set_text` above claims the selection on a throwaway context
/// and loses it on drop; every *write* path must go through an owner that
/// lives as long as the content should survive. Reads stay throwaway
/// (no ownership needed).
#[derive(Default)]
pub struct ClipboardOwner {
    ctx: Option<copypasta::ClipboardContext>,
}

impl std::fmt::Debug for ClipboardOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardOwner")
            .field("active", &self.ctx.is_some())
            .finish()
    }
}

impl ClipboardOwner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Claim the selection with `text` and keep owning it. Fails only when
    /// no clipboard backend exists (headless); never panics.
    pub fn set_text(&mut self, text: &str) -> Result<(), crate::SystemError> {
        let ctx = match self.ctx.as_mut() {
            Some(ctx) => ctx,
            None => {
                self.ctx = Some(
                    copypasta::ClipboardContext::new()
                        .map_err(|_| crate::SystemError::ClipboardUnavailable)?,
                );
                self.ctx.as_mut().expect("just created")
            },
        };
        ctx.set_contents(text.to_owned())
            .map_err(|_| crate::SystemError::ClipboardUnavailable)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// X11 ownership probe: set via a throwaway context (as `set_text`
    /// does), then re-read from a fresh one. Needs a display clipboard
    /// (Xephyr :98 or real X11); ignored otherwise. Proves the documented
    /// ownership loss — writes must go through `ClipboardOwner`.
    #[test]
    #[ignore = "needs a display clipboard (Xephyr :98 or real X11)"]
    fn throwaway_set_loses_ownership() {
        let probe = format!("mbxt-clip-probe-{}", std::process::id());
        Clipboard::new().set_text(&probe).expect("set_text works");
        let back = Clipboard::new().get_text();
        assert_ne!(
            back.as_deref(),
            Some(probe.as_str()),
            "throwaway write must NOT survive (else the owner is unnecessary)"
        );
    }

    /// Owner-kept writes survive re-reads from a fresh context.
    #[test]
    #[ignore = "needs a display clipboard (Xephyr :98 or real X11)"]
    fn owner_set_survives_reread() {
        let probe = format!("mbxt-clip-owned-{}", std::process::id());
        let mut owner = ClipboardOwner::new();
        owner.set_text(&probe).expect("owner set works");
        // The owner stays alive while a *fresh* context reads back.
        let back = Clipboard::new().get_text();
        assert_eq!(
            back.as_deref(),
            Some(probe.as_str()),
            "owned clipboard content must survive"
        );
        // Debug never leaks content (there is none stored, but assert the shape).
        assert_eq!(format!("{owner:?}"), "ClipboardOwner { active: true }");
    }
}
