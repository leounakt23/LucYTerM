//! UI layer: Iced views, widgets, theme, and keyboard shortcuts
//! (prompt 1.4). Views are pure mappings from `AppState` to elements —
//! all business logic lives in `app::update`.

pub mod auth_dialog;
pub mod context_menu;
pub mod drag_drop;
pub mod file_browser;
pub mod keyboard;
pub mod macro_editor;
pub mod macro_panel;
pub mod main_window;
pub mod multi_exec_toolbar;
pub mod multi_exec_view;
pub mod new_session_dialog;
pub mod terminal_widget;
pub mod theme;
/// Transfer view re-export path (Prompt 3.2); ssh-gated like the SFTP stack.
#[cfg(feature = "ssh")]
pub mod transfer_view;
pub mod tunnels_panel;
#[cfg(feature = "vnc")]
pub mod vnc_view;
pub mod widgets;

pub use main_window::view;
pub use theme::AppTheme;
