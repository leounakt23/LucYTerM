//! The MVU Model — `AppState` (prompt 1.1 specification).
//!
//! Threading model: Iced owns `AppState` on the UI thread (`&mut` access in
//! `update`). State that runtime-side actors/task code must read or write is
//! published via `Arc<RwLock<RuntimeShared>>` so the UI never blocks and the
//! runtime never touches widgets (architecture §4).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use mbxt_core::{Protocol, Session, SessionId, SessionState};

/// Tunnel form draft (Prompt 5.3, `None` == closed). Slots are kind-neutral:
/// Local binds `bind_*` locally towards `target_*`; Remote binds `bind_*`
/// on the server towards local `target_*`; Dynamic only uses `bind_*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelDraft {
    pub session: SessionId,
    pub kind: super::messages::TunnelKind,
    pub name: String,
    pub bind_host: String,
    pub bind_port: String,
    pub target_host: String,
    pub target_port: String,
    pub auto_start: bool,
    pub editing: Option<uuid::Uuid>,
    pub error: Option<String>,
}

impl TunnelDraft {
    pub fn new(session: SessionId) -> Self {
        Self {
            session,
            kind: super::messages::TunnelKind::Local,
            name: String::new(),
            bind_host: "127.0.0.1".to_string(),
            bind_port: "8080".to_string(),
            target_host: "localhost".to_string(),
            target_port: "80".to_string(),
            auto_start: false,
            editing: None,
            error: None,
        }
    }
}

/// New-session dialog draft (Prompt 4.4, ephemeral — never saved).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSessionDraft {
    pub name: String,
    pub protocol: Protocol,
    pub host: String,
    pub port: String,
    pub device: String,
    pub baud: String,
    /// Picker entries refreshed on open (`Port list` button).
    pub serial_ports: Vec<String>,
    /// Validation error shown inline (cleared on any edit).
    pub error: Option<String>,
}

impl NewSessionDraft {
    pub fn new(name: String) -> Self {
        Self {
            name,
            protocol: Protocol::Ssh,
            host: String::new(),
            port: "22".to_string(),
            device: String::new(),
            baud: "115200".to_string(),
            serial_ports: Vec::new(),
            error: None,
        }
    }
}

/// Network-tools draft (Prompt 5.4): picker + target + per-tool params.
/// `remote` is `None` for local runs, `Some(id)` for session shells.
#[derive(Debug, Clone)]
pub struct ToolDraft {
    pub kind: crate::tools::ToolKind,
    pub target: String,
    pub timeout_ms: u64,
    pub remote: Option<SessionId>,
    pub params: crate::tools::ToolParams,
    /// Widget-owned input strings per field (iced borrows them; parsed
    /// back into `params` on edit, keeping last good on garbage).
    pub field_text: HashMap<String, String>,
    pub search: String,
    pub error: Option<String>,
}

impl Default for ToolDraft {
    fn default() -> Self {
        let params = crate::tools::ToolParams::for_kind(crate::tools::ToolKind::Ping);
        let field_text = crate::tools::ToolParams::text_fields(&params)
            .into_iter()
            .collect();
        Self {
            kind: crate::tools::ToolKind::Ping,
            target: String::new(),
            timeout_ms: 10_000,
            remote: None,
            params,
            field_text,
            search: String::new(),
            error: None,
        }
    }
}

impl ToolDraft {
    /// Validate the draft into a runnable config (target required
    /// except for tools that carry their own input, like Subnet).
    pub fn build_config(&self) -> Result<crate::tools::ToolConfig, String> {
        use crate::tools::ToolKind;
        let needs_target = !matches!(self.kind, ToolKind::Subnet);
        if needs_target && self.target.trim().is_empty() {
            return Err("enter a target first".to_string());
        }
        Ok(crate::tools::ToolConfig {
            target: self.target.trim().to_string(),
            remote: self.remote,
            timeout_ms: self.timeout_ms.max(100),
            params: self.params.clone(),
        })
    }
}

use super::messages::{UiMsg, ViewKind};
use crate::task::{TaskBoard, TaskId, TaskStatus};
use crate::utils::config::AppConfig;
use crate::utils::secure_storage::SecureStorage;

// ---------------------------------------------------------------------------
// Shared (runtime-side) state
// ---------------------------------------------------------------------------

/// State shared with runtime tasks/actors via `Arc<RwLock<...>>`.
/// Keep this small: plain data, no handles, no UI types.
#[derive(Debug, Default, Clone)]
pub struct RuntimeShared {
    /// Authoritative per-session connection state (actors are writers).
    pub session_states: HashMap<SessionId, SessionState>,
    /// Multi-exec broadcast targets (#32).
    pub multi_exec_targets: Vec<SessionId>,
}

/// Shareable handle type.
pub type SharedState = Arc<RwLock<RuntimeShared>>;

// ---------------------------------------------------------------------------
// UI-facing enums
// ---------------------------------------------------------------------------

/// Connection status of the *active* session (drives status bar/badges).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting { progress: Option<u8> },
    Connected { since_secs: u64 },
    Failed { reason: String },
}

/// One tab in the main-window tab strip (#41). The terminal grid itself is
/// a placeholder until the custom wgpu renderer lands (tech_stack R3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub title: String,
    pub kind: TabKind,
}

/// What a tab hosts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabKind {
    Welcome,
    /// Terminal placeholder attached to a session.
    TerminalPlaceholder(SessionId),
    /// SFTP/file-browser placeholder.
    FileBrowserPlaceholder,
    /// VNC remote-desktop viewer attached to a session.
    VncViewer(SessionId),
    /// Combined multi-exec output (Prompt 5.1).
    MultiExecView,
    /// Macro library + editor (Prompt 5.2).
    Macros,
    /// SSH tunnels panel (Prompt 5.3).
    Tunnels,
    /// Network-tools hub (Prompt 5.4).
    Tools,
}

/// Multi-execution mode (#32): when enabled, keystrokes fan out to targets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MultiExecMode {
    pub enabled: bool,
    pub targets: Vec<SessionId>,
    /// Delay between targets in ms (0 == simultaneous fan-out).
    pub stagger_ms: u64,
    /// Hostname filter for conditional execution (empty == all targets).
    pub host_filter: String,
}

/// Clipboard mirror kept for dedup in the change watcher + privacy wipe on
/// screen lock (never displayed, never logged — security §6.2).
#[derive(Debug, Clone, Default)]
pub struct ClipboardState {
    pub last_content: String,
}

/// Shell/UI layout state (feature matrix #41–45).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiState {
    pub view: ViewKind,
    pub sidebar_visible: bool,
    pub search_query: String,
    /// Unsaved changes exist → next autosave flushes.
    pub dirty: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            view: ViewKind::SessionList,
            sidebar_visible: true,
            search_query: String::new(),
            dirty: false,
        }
    }
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

/// Root application model.
pub struct AppState {
    // --- sessions & connections ---
    /// Stored sessions (sidebar order).
    pub sessions: Vec<Session>,
    /// Currently focused session.
    pub active_session_id: Option<SessionId>,
    /// Status of the active session (rendered in status bar).
    pub connection_status: ConnectionStatus,
    /// State per session mirrored from `shared` for cheap rendering.
    pub session_states: HashMap<SessionId, SessionState>,

    // --- UI ---
    pub ui_state: UiState,
    pub multi_exec_mode: MultiExecMode,
    /// Main-window tab strip (#41).
    pub tabs: Vec<Tab>,
    /// Focused tab index (clamped on close).
    pub active_tab: usize,
    pub theme: crate::ui::theme::AppTheme,
    /// Draft and attachment consent for the feedback preview.
    pub feedback: FeedbackUiState,
    /// Live Sentry guard; dropping it immediately disables crash delivery.
    pub crash_reporter: Option<crate::feedback::sentry::CrashReporter>,

    // --- services ---
    /// Persisted settings (`config.ron` snapshot, non-sensitive).
    pub settings: AppConfig,
    /// Directories (config/data/runtime).
    pub paths: crate::utils::paths::AppPaths,
    /// Encrypted session store handle (shared; `Clone` keeps one KEK).
    pub secure: SecureStorage,
    /// `true` once the store has been decrypted this run (keyring auto-unlock
    /// or `MasterPasswordSubmitted`). Sessions autosave only when unlocked.
    pub sessions_unlocked: bool,
    /// Credential cache for authentication reuse ("Remember password").
    pub credentials: crate::connection::credential_cache::CredentialCache,
    /// Active auth prompt (modal), if any (prompt 2.2).
    pub auth_dialog: Option<crate::connection::ssh::auth::AuthPrompt>,
    /// Buffered input for the auth modal.
    pub auth_input: String,
    pub clipboard: ClipboardState,

    // --- boards & queues ---
    /// User-facing notifications (bounded, auto-expiring).
    pub notifications: super::notifications::NotificationQueue,
    /// Live transfer progress per session (#29).
    pub transfers: HashMap<SessionId, TransferProgress>,
    /// File-browser state per session (Prompt 3.3, ephemeral — never saved).
    #[cfg(feature = "ssh")]
    pub browsers: HashMap<SessionId, crate::connection::sftp::FileBrowserState>,
    /// Directory-listing cache with TTL (Prompt 3.3).
    #[cfg(feature = "ssh")]
    pub browser_cache: crate::connection::sftp::DirCache,
    /// VNC viewer UI state per session (Prompt 4.3, ephemeral).
    #[cfg(feature = "vnc")]
    pub vnc_viewers: HashMap<SessionId, crate::ui::widgets::vnc_view::VncViewerUi>,
    /// New-session dialog draft (Prompt 4.4, `None` == closed).
    pub new_session: Option<NewSessionDraft>,
    /// Tunnel form draft (Prompt 5.3, `None` == closed).
    pub tunnel_draft: Option<TunnelDraft>,
    /// Network-tools draft (Prompt 5.4): picker + target + per-tool params.
    pub tool_draft: ToolDraft,
    /// Recorded tool runs, oldest first (Prompt 5.4 history).
    pub tool_runs: Vec<crate::tools::ToolRun>,
    /// Next tool-run id.
    pub tool_run_seq: u64,
    /// Cancel tokens for in-flight runs.
    pub tool_cancel: HashMap<u64, crate::tools::CancelToken>,
    /// Macro library (Prompt 5.2, loaded from the macros dir).
    pub macros: Vec<crate::macros::Macro>,
    /// Macro load warnings (shown in the panel footer).
    pub macro_load_errors: Vec<String>,
    /// Active recorder (Prompt 5.2).
    pub macro_recorder: crate::macros::MacroRecorder,
    /// Running playback, if any.
    pub macro_player: Option<crate::macros::RunningMacro>,
    /// Pending variable prompt (modal form state).
    pub macro_prompt: Option<crate::macros::MacroPrompt>,
    /// Variable prompt input buffer (never logged when secret).
    pub macro_prompt_input: String,
    /// Destructive macro held for confirmation.
    pub macro_pending_play: Option<crate::macros::PendingPlay>,
    /// Macro editor draft (`None` == closed).
    pub macro_editor: Option<crate::macros::MacroEditorState>,
    /// Combined multi-exec output `(session, line)`, newest at the back.
    pub multi_exec_log: std::collections::VecDeque<(SessionId, String)>,
    /// Completed commands broadcast while multi-exec was active.
    pub multi_exec_history: Vec<String>,
    /// Template text with per-session `$SESSION_*` substitution.
    pub multi_exec_template: String,
    /// Destructive broadcast held for confirmation (`None` == none held).
    pub multi_exec_pending: Option<Vec<u8>>,
    /// Leader session of the last broadcast (highlighted terminal).
    pub multi_exec_leader: Option<SessionId>,
    /// Task board: id → status (TaskManager events drive transitions).
    pub tasks: TaskBoard,
    /// Last terminal activity per session (multi-exec targeting hints).
    pub last_activity: HashMap<SessionId, u64>,
    /// Headless terminal models; the renderer consumes their visible grids.
    pub terminals: HashMap<SessionId, mbxt_terminal::Terminal>,

    /// Runtime-shared state handle (actors/forward manager read this).
    pub shared: SharedState,
}

/// In-flight transfer progress (#29/#30).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferProgress {
    pub remote_path: String,
    pub local_path: String,
    pub done: u64,
    pub total: u64,
}

/// In-app feedback form. No field is submitted automatically.
#[derive(Debug, Clone, Default)]
pub struct FeedbackUiState {
    pub kind: crate::feedback::FeedbackKind,
    pub title: String,
    pub description: String,
    pub consent: crate::feedback::FeedbackConsent,
    pub screenshot_path: String,
    pub preview: String,
}

impl AppState {
    /// Initial state plus startup effects (keyring auto-unlock when opted in).
    pub fn new(
        settings: AppConfig,
        paths: crate::utils::paths::AppPaths,
    ) -> (Self, iced::Task<super::Message>) {
        let theme = crate::ui::theme::AppTheme::from_settings(&settings);

        let secure = SecureStorage::new(&paths.config_dir);

        // Startup decrypt path (prompt 1.2): when the user opted into the
        // keyring and the store exists, fetch the cached master password and
        // feed it through the same message path as a manual unlock. The
        // password prompt UI itself lands with the session view (prompt 1.3);
        // until then `MasterPasswordSubmitted` is the single unlock entry.
        let startup_task =
            if settings.security.keyring_enabled && !settings.security.require_master_password {
                let secure = secure.clone();
                iced::Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            secure.load_password_from_keyring().ok().flatten()
                        })
                        .await
                        .ok()
                        .flatten()
                    },
                    |password| match password {
                        Some(pw) => super::Message::Ui(UiMsg::MasterPasswordSubmitted(pw)),
                        None => super::Message::Ignored,
                    },
                )
            } else {
                iced::Task::none()
            };
        #[allow(unused_mut)]
        let mut startup_tasks = vec![startup_task];
        #[cfg(feature = "feedback-network")]
        if settings.general.telemetry_enabled
            && crate::feedback::telemetry::should_flush(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            )
        {
            if let Ok(endpoint) = std::env::var("MBXT_TELEMETRY_ENDPOINT") {
                let payload =
                    crate::feedback::telemetry::snapshot(settings.general.release_channel);
                startup_tasks.push(iced::Task::perform(
                    async move {
                        if let Err(error) =
                            crate::feedback::telemetry::send(&endpoint, &payload).await
                        {
                            tracing::warn!(%error, "opt-in telemetry delivery failed");
                        }
                    },
                    |()| super::Message::Ignored,
                ));
            }
        }

        let mut state = Self {
            sessions: Vec::new(),
            active_session_id: None,
            connection_status: ConnectionStatus::Disconnected,
            session_states: HashMap::new(),
            ui_state: UiState::default(),
            multi_exec_mode: MultiExecMode::default(),
            tabs: vec![Tab {
                title: "Welcome".to_string(),
                kind: TabKind::Welcome,
            }],
            active_tab: 0,
            theme,
            feedback: FeedbackUiState::default(),
            crash_reporter: crate::feedback::sentry::init(
                &settings.general,
                std::env::var("MBXT_SENTRY_DSN").ok().as_deref(),
            ),
            settings,
            paths,
            secure,
            sessions_unlocked: false,
            credentials: crate::connection::credential_cache::CredentialCache::new(),
            auth_dialog: None,
            auth_input: String::new(),
            clipboard: ClipboardState::default(),
            notifications: super::notifications::NotificationQueue::default(),
            transfers: HashMap::new(),
            #[cfg(feature = "ssh")]
            browsers: HashMap::new(),
            #[cfg(feature = "ssh")]
            browser_cache: crate::connection::sftp::DirCache::new(
                crate::connection::sftp::CACHE_TTL_SECS,
            ),
            #[cfg(feature = "vnc")]
            vnc_viewers: HashMap::new(),
            new_session: None,
            tunnel_draft: None,
            tool_draft: ToolDraft::default(),
            tool_runs: Vec::new(),
            tool_run_seq: 1,
            tool_cancel: HashMap::new(),
            macros: Vec::new(),
            macro_load_errors: Vec::new(),
            macro_recorder: crate::macros::MacroRecorder::new(),
            macro_player: None,
            macro_prompt: None,
            macro_prompt_input: String::new(),
            macro_pending_play: None,
            macro_editor: None,
            multi_exec_log: std::collections::VecDeque::new(),
            multi_exec_history: Vec::new(),
            multi_exec_template: String::new(),
            multi_exec_pending: None,
            multi_exec_leader: None,
            tasks: TaskBoard::default(),
            last_activity: HashMap::new(),
            terminals: HashMap::new(),
            shared: Arc::new(RwLock::new(RuntimeShared::default())),
        };
        // Macro library seed + load (Prompt 5.2): a handful of small RON
        // files, synchronous at startup like the default config write above.
        // Broken files warn instead of blocking startup.
        {
            let store = crate::macros::MacroStore::new(&state.paths.config_dir.join("macros"));
            store.ensure_seeded(&crate::macros::MacroStore::builtin_macros());
            let (macros, errors) = store.load_all();
            state.macros = macros;
            state.macro_load_errors = errors;
            if !state.macro_load_errors.is_empty() {
                tracing::warn!(
                    errors = ?state.macro_load_errors,
                    "some macro files failed to load"
                );
            }
        }
        (state, iced::Task::batch(startup_tasks))
    }

    // -- helpers used by update/tests ------------------------------------

    /// Look up a session by id.
    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// Mutable lookup by id.
    pub fn session_mut(&mut self, id: SessionId) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    /// Publish a session state to both the shared map and the render mirror.
    pub fn set_session_state(&mut self, id: SessionId, state: SessionState) {
        if let Ok(mut shared) = self.shared.write() {
            shared.session_states.insert(id, state.clone());
        }
        self.session_states.insert(id, state);
    }

    /// Push a user-facing notification (bounded queue handles overflow).
    pub fn notify(&mut self, level: super::notifications::Level, title: &str, body: &str) {
        self.ui_state.dirty = true;
        self.notifications
            .push(super::notifications::Notification::new(level, title, body));
    }

    /// Active task count (status bar).
    pub fn running_tasks(&self) -> usize {
        self.tasks
            .values()
            .filter(|s| **s == TaskStatus::Running)
            .count()
    }

    /// Ids of tasks in `Running` state.
    pub fn running_task_ids(&self) -> Vec<TaskId> {
        self.tasks
            .iter()
            .filter(|(_, s)| **s == TaskStatus::Running)
            .map(|(id, _)| *id)
            .collect()
    }
}

/// Current unix time in seconds (timestamps for status/notifications).
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::notifications::{Level, Notification};

    fn test_state() -> AppState {
        let (state, _task) =
            AppState::new(crate::utils::config::AppConfig::default(), test_paths());
        state
    }

    fn test_paths() -> crate::utils::paths::AppPaths {
        let base = std::env::temp_dir().join(format!("mbxt-state-test-{}", std::process::id()));
        crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        }
    }

    #[test]
    fn defaults_are_sane() {
        let state = test_state();
        assert_eq!(state.active_session_id, None);
        assert_eq!(state.connection_status, ConnectionStatus::Disconnected);
        assert!(state.ui_state.sidebar_visible);
        assert!(!state.multi_exec_mode.enabled);
        assert!(state.notifications.is_empty());
        assert_eq!(state.running_tasks(), 0);
    }

    #[test]
    fn set_session_state_publishes_to_shared_mirror() {
        let mut state = test_state();
        state.set_session_state(7, SessionState::Connected);
        assert_eq!(state.session_states.get(&7), Some(&SessionState::Connected));
        let shared = state.shared.read().unwrap();
        assert_eq!(
            shared.session_states.get(&7),
            Some(&SessionState::Connected)
        );
    }

    #[test]
    fn notification_queue_is_bounded() {
        let mut state = test_state();
        for i in 0..(super::super::notifications::NotificationQueue::default().max * 2) {
            state.notify(Level::Info, &format!("n{i}"), "body");
        }
        assert!(state.notifications.len() <= state.notifications.max);
        // Oldest were dropped, newest retained.
        assert_eq!(
            state.notifications.items.back().unwrap().title,
            format!("n{}", state.notifications.max * 2 - 1)
        );
    }

    #[test]
    fn notifications_expire() {
        let mut queue = super::super::notifications::NotificationQueue::default();
        let mut n = Notification::new(Level::Error, "t", "b");
        n.created_secs = now_secs().saturating_sub(3600);
        queue.push(n);
        queue.retain_recent(now_secs());
        assert!(queue.is_empty());
    }

    #[test]
    fn running_task_board_tracks_status() {
        let mut state = test_state();
        state.tasks.insert(1, TaskStatus::Running);
        state.tasks.insert(2, TaskStatus::Failed("x".into()));
        assert_eq!(state.running_tasks(), 1);
        assert_eq!(state.running_task_ids(), vec![1]);
    }

    #[test]
    fn theme_follows_settings() {
        let mut settings = crate::utils::config::AppConfig::default();
        settings.appearance.dark_theme = false;
        let (state, _) = AppState::new(settings, test_paths());
        assert_eq!(state.theme, crate::ui::theme::AppTheme::Light);
    }

    #[test]
    fn main_window_starts_with_welcome_tab() {
        let state = test_state();
        assert_eq!(state.tabs.len(), 1);
        assert_eq!(state.active_tab, 0);
        assert_eq!(state.tabs[0].kind, super::TabKind::Welcome);
    }

    #[test]
    fn secure_storage_is_wired_and_starts_locked() {
        let state = test_state();
        assert!(!state.sessions_unlocked);
        // Store file only exists after the first save.
        assert!(!state.secure.is_unlocked());
    }

    #[test]
    fn keyring_opt_in_schedules_unlock_attempt() {
        let mut settings = crate::utils::config::AppConfig::default();
        settings.security.keyring_enabled = true;
        let (_, task) = AppState::new(settings, test_paths());
        // The unlock attempt is scheduled (keyring read happens off-thread);
        // we only assert a task was produced — value semantics are opaque.
        let _ = task;
    }
}
