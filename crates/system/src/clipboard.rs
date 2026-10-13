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

    /// Clipboard ownership round-trip (single test by design: two tests
    /// would race — Rust runs them in parallel threads against the same
    /// selection and steal each other's ownership, which failed CI once).
    /// Needs a display clipboard (Xvfb in e2e CI, Xephyr :98 or real X11
    /// locally); ignored otherwise.
    #[test]
    #[ignore = "needs a display clipboard (Xvfb in e2e CI, Xephyr :98, real X11)"]
    fn owned_writes_survive_throwaway_does_not() {
        let pid = std::process::id();
        // 1. Throwaway write (as `Clipboard::set_text` does) loses
        //    ownership on drop: re-read must NOT return the probe. This
        //    documents why every write path uses `ClipboardOwner`.
        let lost = format!("mbxt-clip-probe-{pid}");
        Clipboard::new().set_text(&lost).expect("set_text works");
        assert_ne!(
            Clipboard::new().get_text().as_deref(),
            Some(lost.as_str()),
            "throwaway write must NOT survive (else the owner is unnecessary)"
        );
        // 2. Owner-kept write survives a re-read from a fresh context.
        let kept = format!("mbxt-clip-owned-{pid}");
        let mut owner = ClipboardOwner::new();
        owner.set_text(&kept).expect("owner set works");
        // The owner stays alive while a *fresh* context reads back.
        assert_eq!(
            Clipboard::new().get_text().as_deref(),
            Some(kept.as_str()),
            "owned clipboard content must survive"
        );
        // Debug never leaks content (there is none stored, but assert the shape).
        assert_eq!(format!("{owner:?}"), "ClipboardOwner { active: true }");
    }
}
