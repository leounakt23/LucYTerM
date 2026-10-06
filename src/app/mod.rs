//! Application layer: MVU composition root (architecture §3.1).
//!
//! Exports (prompt 1.1):
//! - `Message` / event enums — [`messages`]
//! - `AppState`, `UiState`, `ConnectionStatus` — [`state`]
//! - `update(state, message) -> Task<Message>` — [`update`]
//! - external-event subscriptions — [`subscriptions`]
//! - `Notification` / `NotificationQueue` — [`notifications`]
//!
//! `iced::Task` is the iced-0.13 name for `Command<Message>` (the prompt's
//! 0.10-era terminology); see `doc/tech_stack.md` for the stack decision.

pub mod messages;
pub mod multi_exec;
pub mod notifications;
pub mod state;
pub mod subscriptions;
mod update;

pub use messages::Message;
pub use state::{AppState, ConnectionStatus, RuntimeShared, SharedState, UiState};

use crate::utils::config::AppConfig;

/// Start the Iced application (blocks until the window closes).
pub fn run(config: AppConfig, paths: crate::utils::paths::AppPaths) -> iced::Result {
    iced::application("Remote App", update::update, crate::ui::view)
        .theme(|state: &AppState| state.theme.to_iced())
        .subscription(subscriptions::subscriptions)
        .run_with(move || AppState::new(config, paths))
}
