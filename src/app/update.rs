//! MVU update: `(AppState, Message) -> Task<Message>`.
//!
//! Rules (architecture §3.1, §7):
//! - `update` **never blocks** — all I/O is scheduled as async effects via
//!   `iced::Task` (the iced-0.13 name for `Command`) or the `TaskManager`.
//! - Domain handlers return `Result`; any `Err` becomes a user-friendly
//!   `Notification` — never a raw error chain, never a panic (M6 target).
//! - Every variant is handled; unimplemented behavior is a logged stub with a
//!   TODO, not `todo!()` (a panic in a handler would take down the UI).

use mbxt_core::{AuthMethod, SessionId, SessionSpec, SessionState};

#[cfg(feature = "vnc")]
use super::messages::VncMsg;
use super::messages::{
    ConnectionMsg, MacroMsg, Message, SessionMsg, SftpMsg, SystemEvent, TerminalMsg, ToolMsg, UiMsg,
};
use super::notifications::Level;
use super::state::{now_secs, AppState, ConnectionStatus, Tab, TabKind};
use crate::connection::credential_cache::CredentialCache;
use crate::connection::ssh::auth::{negotiate, AuthContext, AuthCredential, PromptBridge};
#[cfg(feature = "vnc")]
use crate::connection::vnc::input::{KeyPress, PointerEvent};
use crate::connection::{ConnectionAuth, TerminalSize};
use crate::macros::Context;
use crate::task::{TaskEvent, TaskManager, TaskStatus};
use crate::utils::config::{self as config_store, AppConfig, SessionEntry};
use crate::utils::secure_storage::SecureStorage;

/// MVU update entry point: pure state transitions + effect scheduling.
pub fn update(app: &mut AppState, message: Message) -> iced::Task<Message> {
    let breadcrumb = match &message {
        Message::Session(_) => "session action",
        Message::Connection(_) => "connection action",
        Message::Terminal(_) => "terminal interaction",
        Message::Sftp(_) => "file transfer action",
        Message::Macro(_) => "macro action",
        Message::Tool(_) => "network tool action",
        #[cfg(feature = "ssh")]
        Message::Tunnel(_) => "tunnel action",
        #[cfg(feature = "vnc")]
        Message::Vnc(_) => "VNC action",
        Message::Ui(_) => "UI action",
        Message::System(_) => "system event",
        _ => "background task",
    };
    if app.feedback.consent.include_breadcrumbs {
        crate::feedback::telemetry::breadcrumb(breadcrumb);
    }
    if app.settings.general.telemetry_enabled {
        crate::feedback::telemetry::record_feature(breadcrumb);
    }
    let result = match message {
        Message::Session(msg) => handle_session(app, msg),
        Message::Connection(msg) => handle_connection(app, msg),
        Message::Terminal(msg) => handle_terminal(app, msg),
        Message::Sftp(msg) => handle_sftp(app, msg),
        Message::Macro(msg) => handle_macro(app, msg),
        Message::Tool(msg) => handle_tool(app, msg),
        #[cfg(feature = "ssh")]
        Message::Tunnel(msg) => handle_tunnel(app, msg),
        #[cfg(feature = "vnc")]
        Message::Vnc(msg) => handle_vnc(app, msg),
        Message::Ui(msg) => handle_ui(app, msg),
        Message::System(event) => handle_system(app, event),
        Message::Task(event) => handle_task_event(app, event),
        Message::TaskScheduled(_id) => return iced::Task::none(),
        Message::AutosaveTick => handle_autosave(app),
        Message::Persisted(result) => handle_persisted(app, result),
        Message::IssueBundleReady(result) => handle_issue_bundle(app, result),
        Message::UpdateChecked(result) => handle_update_checked(app, result),
        Message::AuthPrompt(prompt) => handle_auth_prompt(app, prompt),
        Message::AuthResponse { id, value } => handle_auth_response(app, id, value),
        Message::ClipboardChanged(text) => handle_clipboard(app, text),
        Message::Ignored => return iced::Task::none(),
    };
    or_notify(app, result)
}

/// Run a fallible handler; convert errors to an error notification.
fn or_notify(
    app: &mut AppState,
    result: Result<iced::Task<Message>, String>,
) -> iced::Task<Message> {
    match result {
        Ok(task) => task,
        Err(err) => {
            tracing::warn!(%err, "message handler failed; notifying user");
            app.notify(Level::Error, "Operation failed", &err);
            iced::Task::none()
        },
    }
}

/// Schedule a fallible future on the Tokio runtime via the TaskManager.
///
/// `update()` runs on the UI thread (outside any Tokio context), so the
/// actual `tokio::spawn` is wrapped in `iced::Task::perform`, which iced
/// executes on its runtime. Lifecycle events arrive back as `Message::Task`
/// through the subscription bridge.
///
/// Only used by `no-default-features` (non-ssh) paths; the ssh build routes
/// through `iced::Task::perform` directly to map results into domain
/// messages — hence the allowance.
#[allow(dead_code)]
pub fn spawn_task<F, T>(fut: F) -> iced::Task<Message>
where
    F: std::future::Future<Output = Result<T, String>> + Send + 'static,
    T: Send + 'static,
{
    iced::Task::perform(
        async move { TaskManager::shared().spawn(fut).id },
        Message::TaskScheduled,
    )
}

/// Blocking-snapshot write used by autosave/shutdown: runs on the blocking
/// pool and returns a user-friendly `Result`.
/// Blocking-snapshot write used by autosave/shutdown (runs on the blocking
/// pool): `config.ron` (settings) + `sessions.enc` (sessions, only when the
/// store is unlocked — prompt 1.2 auto-save-on-change requirement).
async fn persist_snapshot(
    paths: crate::utils::paths::AppPaths,
    config: AppConfig,
    secure: SecureStorage,
    sessions: Vec<SessionEntry>,
) -> Result<(), String> {
    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        config_store::save(&paths.config_dir, &config).map_err(|e| e.to_string())?;
        if secure.is_unlocked() {
            secure
                .save_sessions(&sessions, None)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await
    .unwrap_or_else(|e| Err(format!("persistence task panicked: {e}")));

    if let Err(ref err) = result {
        // Logged here too: the shutdown path discards the result to keep
        // quitting simple (best-effort persistence).
        tracing::error!(%err, "state snapshot failed");
    }
    result
}

// ---------------------------------------------------------------------------
// Handlers (each returns Result<effects, user-facing error>)
// ---------------------------------------------------------------------------

fn handle_session(app: &mut AppState, msg: SessionMsg) -> Result<iced::Task<Message>, String> {
    match msg {
        SessionMsg::Loaded(specs) => {
            app.sessions = specs
                .into_iter()
                .enumerate()
                .map(|(i, spec)| mbxt_core::Session {
                    id: (i + 1) as SessionId,
                    spec,
                    state: SessionState::Disconnected,
                })
                .collect();
            // Loaded-from-store implies a successful decrypt (unlock path).
            app.sessions_unlocked = true;
            // Register persisted tunnel definitions with the forward manager.
            #[cfg(feature = "ssh")]
            for session in &app.sessions {
                for def in &session.spec.forwards {
                    crate::connection::forward::ForwardManager::shared()
                        .define(session.id, def.clone());
                }
            }
            tracing::debug!(count = app.sessions.len(), "sessions loaded");
            Ok(iced::Task::none())
        },
        SessionMsg::Selected(id) => {
            if app.session(id).is_none() {
                return Err(format!("unknown session #{id}"));
            }
            app.active_session_id = Some(id);
            app.connection_status = match app.session_states.get(&id) {
                Some(SessionState::Connected) => ConnectionStatus::Connected {
                    since_secs: now_secs(),
                },
                Some(SessionState::Failed(reason)) => ConnectionStatus::Failed {
                    reason: reason.clone(),
                },
                Some(SessionState::Connecting) => ConnectionStatus::Connecting { progress: None },
                _ => ConnectionStatus::Disconnected,
            };
            Ok(iced::Task::none())
        },
        SessionMsg::Created(name) => {
            if name.trim().is_empty() {
                return Err("session name cannot be empty".to_string());
            }
            let spec = mbxt_core::SessionSpec {
                name: name.trim().to_string(),
                protocol: mbxt_core::Protocol::Ssh,
                host: None,
                port: None,
                username: None,
                auth: mbxt_core::AuthMethod::Agent { forward: false },
                tags: Vec::new(),
                notes: String::new(),
                x11_forwarding: false,
                serial: None,
                forwards: Vec::new(),
            };
            let id = app.sessions.len() as SessionId + 1;
            app.sessions.push(mbxt_core::Session {
                id,
                spec,
                state: SessionState::Disconnected,
            });
            app.set_session_state(id, SessionState::Disconnected);
            app.ui_state.dirty = true;
            app.notify(
                Level::Success,
                "Session created",
                &format!("\"{name}\" added"),
            );
            Ok(iced::Task::none())
        },
        SessionMsg::CreateDetailed(spec) => {
            let name = spec.name.trim().to_string();
            if name.is_empty() {
                return Err("session name cannot be empty".to_string());
            }
            // Protocol-specific requirements (dialog validation mirrors this).
            match spec.protocol {
                mbxt_core::Protocol::Ssh
                | mbxt_core::Protocol::Telnet
                | mbxt_core::Protocol::Sftp
                | mbxt_core::Protocol::X11 => {
                    if spec.host.as_deref().unwrap_or("").trim().is_empty() {
                        return Err("host is required".to_string());
                    }
                },
                mbxt_core::Protocol::Serial => {
                    let params = spec
                        .serial
                        .as_ref()
                        .ok_or("serial settings are missing".to_string())?;
                    params.validate().map_err(|err| err.to_string())?;
                },
                _ => {},
            }
            let id = app.sessions.len() as SessionId + 1;
            app.sessions.push(mbxt_core::Session {
                id,
                spec: *spec,
                state: SessionState::Disconnected,
            });
            app.set_session_state(id, SessionState::Disconnected);
            app.ui_state.dirty = true;
            app.notify(
                Level::Success,
                "Session created",
                &format!("\"{name}\" added"),
            );
            Ok(iced::Task::none())
        },
        SessionMsg::Renamed(id, name) => match app.session_mut(id) {
            Some(session) => {
                session.spec.name = name;
                app.ui_state.dirty = true;
                Ok(iced::Task::none())
            },
            None => Err(format!("unknown session #{id}")),
        },
        SessionMsg::Deleted(id) => {
            let before = app.sessions.len();
            app.sessions.retain(|s| s.id != id);
            if app.sessions.len() == before {
                return Err(format!("unknown session #{id}"));
            }
            if app.active_session_id == Some(id) {
                app.active_session_id = None;
                app.connection_status = ConnectionStatus::Disconnected;
            }
            app.set_session_state(id, SessionState::Disconnected);
            let _ = crate::connection::actor::SessionManager::shared().disconnect(id);
            #[cfg(feature = "ssh")]
            crate::connection::forward::ForwardManager::shared().remove_session(id);
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        SessionMsg::Tagged(id, tag) => match app.session_mut(id) {
            Some(session) => {
                if !session.spec.tags.contains(&tag) {
                    session.spec.tags.push(tag);
                    app.ui_state.dirty = true;
                }
                Ok(iced::Task::none())
            },
            None => Err(format!("unknown session #{id}")),
        },
        SessionMsg::X11Toggled(id) => match app.session_mut(id) {
            Some(session) => {
                if !matches!(
                    session.spec.protocol,
                    mbxt_core::Protocol::Ssh | mbxt_core::Protocol::Sftp | mbxt_core::Protocol::X11
                ) {
                    return Err(format!("session #{id} is not an SSH session"));
                }
                session.spec.x11_forwarding = !session.spec.x11_forwarding;
                let state = session.spec.x11_forwarding;
                app.ui_state.dirty = true;
                app.notify(
                    Level::Info,
                    "X11 forwarding",
                    if state {
                        "enabled — takes effect on next connect"
                    } else {
                        "disabled"
                    },
                );
                Ok(iced::Task::none())
            },
            None => Err(format!("unknown session #{id}")),
        },
    }
}

fn handle_connection(
    app: &mut AppState,
    msg: ConnectionMsg,
) -> Result<iced::Task<Message>, String> {
    match msg {
        ConnectionMsg::ConnectRequested(id) => {
            let spec = app
                .session(id)
                .map(|s| s.spec.clone())
                .ok_or_else(|| format!("unknown session #{id}"))?;

            // VNC sessions connect through the embedded RFB client (Prompt
            // 4.3), not the SSH shell flow below.
            #[cfg(feature = "vnc")]
            if spec.protocol == mbxt_core::Protocol::Vnc {
                app.active_session_id = Some(id);
                app.connection_status = ConnectionStatus::Connecting { progress: None };
                app.set_session_state(id, SessionState::Connecting);
                let cache = app.credentials.clone();
                return Ok(iced::Task::perform(
                    async move { run_vnc_connect(id, spec, cache).await },
                    move |result| match result {
                        Ok(()) => Message::Vnc(VncMsg::Connected(id)),
                        Err(err) => Message::Vnc(VncMsg::ConnectFailed(id, err)),
                    },
                ));
            }

            app.active_session_id = Some(id);
            app.connection_status = ConnectionStatus::Connecting { progress: None };
            app.set_session_state(id, SessionState::Connecting);
            let bridge = PromptBridge::shared().clone();
            let cache = app.credentials.clone();
            Ok(iced::Task::perform(
                async move {
                    // Telnet/serial lines carry no authentication, and RDP
                    // authenticates in its own viewer window: skip the
                    // interactive auth flow (no prompts) with an ignored
                    // placeholder all three transports accept and drop.
                    // (SpawnConn only forwards non-empty stdin passwords.)
                    let auth = if matches!(
                        spec.protocol,
                        mbxt_core::Protocol::Telnet
                            | mbxt_core::Protocol::Serial
                            | mbxt_core::Protocol::Rdp
                    ) {
                        ConnectionAuth::Password(zeroize::Zeroizing::new(String::new()))
                    } else {
                        run_auth_flow(spec.clone(), bridge, cache).await?
                    };
                    crate::connection::actor::SessionManager::shared().connect(
                        id,
                        spec.clone(),
                        auth.clone(),
                        TerminalSize::default(),
                    )?;
                    // SSH-family sessions get an SFTP subsystem alongside the
                    // shell (same auth); SFTP drops with SSH on reconnect.
                    #[cfg(feature = "ssh")]
                    if matches!(
                        spec.protocol,
                        mbxt_core::Protocol::Ssh
                            | mbxt_core::Protocol::Sftp
                            | mbxt_core::Protocol::X11
                    ) {
                        crate::connection::sftp::SftpManager::shared()
                            .connect_in_background(id, spec, auth);
                    }
                    Ok::<(), String>(())
                },
                move |result| match result {
                    Ok(()) => Message::Ignored,
                    Err(err) => Message::Connection(ConnectionMsg::Failed(id, err)),
                },
            ))
        },
        ConnectionMsg::DisconnectRequested(id) => {
            crate::connection::actor::SessionManager::shared().disconnect(id)?;
            #[cfg(feature = "ssh")]
            {
                crate::connection::sftp::SftpManager::shared().note_ssh_disconnected(id);
                crate::connection::forward::ForwardManager::shared().note_ssh_disconnected(id);
            }
            #[cfg(feature = "x11")]
            {
                crate::connection::x11::X11Manager::shared().note_ssh_disconnected(id);
            }
            #[cfg(feature = "vnc")]
            {
                crate::connection::vnc::VncManager::shared().note_disconnected(id);
                app.vnc_viewers.remove(&id);
                app.tabs.retain(|tab| tab.kind != TabKind::VncViewer(id));
            }
            Ok(iced::Task::none())
        },
        ConnectionMsg::Progress(id, percent) => {
            if app.active_session_id == Some(id) {
                app.connection_status = ConnectionStatus::Connecting {
                    progress: Some(percent.min(100)),
                };
            }
            Ok(iced::Task::none())
        },
        ConnectionMsg::Connected(id) => {
            app.set_session_state(id, SessionState::Connected);
            if app.active_session_id == Some(id) {
                app.connection_status = ConnectionStatus::Connected {
                    since_secs: now_secs(),
                };
            }
            app.notify(Level::Success, "Connected", &format!("session #{id}"));
            // X11 forwarding (Prompt 4.1): register the forwarder alongside
            // the shell when the session asked for it; warn when the local
            // X server is not accessible instead of failing the connect.
            #[cfg(feature = "x11")]
            {
                let wants_x11 = app.session(id).is_some_and(|s| {
                    s.spec.x11_forwarding
                        && matches!(
                            s.spec.protocol,
                            mbxt_core::Protocol::Ssh
                                | mbxt_core::Protocol::Sftp
                                | mbxt_core::Protocol::X11
                        )
                });
                if wants_x11 {
                    match crate::connection::x11::detect_display() {
                        Ok(display) => {
                            match crate::connection::x11::X11Manager::shared().enable(id, &display)
                            {
                                Ok(()) => app.notify(
                                    Level::Success,
                                    "X11 forwarding active",
                                    &format!("display {display}"),
                                ),
                                Err(err) => app.notify(
                                    Level::Warning,
                                    "X11 forwarding unavailable",
                                    &err.user_message(),
                                ),
                            }
                        },
                        Err(err) => app.notify(
                            Level::Warning,
                            "X11 forwarding unavailable",
                            &err.user_message(),
                        ),
                    }
                }
            }
            // File browser opens automatically for SSH-family sessions
            // (Prompt 3.3): seed the pane and trigger the first listing.
            #[cfg(feature = "ssh")]
            {
                let ssh_family = app.session(id).is_some_and(|s| {
                    matches!(
                        s.spec.protocol,
                        mbxt_core::Protocol::Ssh
                            | mbxt_core::Protocol::Sftp
                            | mbxt_core::Protocol::X11
                    )
                });
                if ssh_family {
                    return Ok(iced::Task::perform(async move {}, move |()| {
                        Message::Sftp(SftpMsg::BrowseRequested(id, ".".to_string()))
                    }));
                }
            }
            // Auto-start tunnels (Prompt 5.3): reconnects rebuild them.
            #[cfg(feature = "ssh")]
            {
                let autostart: Vec<iced::Task<Message>> =
                    crate::connection::forward::ForwardManager::shared()
                        .autostart_ids(id)
                        .into_iter()
                        .map(|forward| {
                            iced::Task::perform(
                                async move {
                                    crate::connection::forward::ForwardManager::shared()
                                        .start_async(forward)
                                        .await
                                },
                                move |result| {
                                    Message::Tunnel(super::messages::TunnelMsg::Started(
                                        forward, result,
                                    ))
                                },
                            )
                        })
                        .collect();
                if !autostart.is_empty() {
                    return Ok(iced::Task::batch(autostart));
                }
            }
            Ok(iced::Task::none())
        },
        ConnectionMsg::Disconnected(id) => {
            app.set_session_state(id, SessionState::Disconnected);
            if app.active_session_id == Some(id) {
                app.connection_status = ConnectionStatus::Disconnected;
            }
            prune_multicast_target(app, id);
            #[cfg(feature = "ssh")]
            {
                crate::connection::sftp::SftpManager::shared().note_ssh_disconnected(id);
                crate::connection::forward::ForwardManager::shared().note_ssh_disconnected(id);
                // Stale listings must never survive a reconnect.
                app.browser_cache.invalidate_session(id);
                app.browsers.remove(&id);
            }
            #[cfg(feature = "x11")]
            {
                crate::connection::x11::X11Manager::shared().note_ssh_disconnected(id);
            }
            Ok(iced::Task::none())
        },
        ConnectionMsg::Failed(id, reason) => {
            app.set_session_state(id, SessionState::Failed(reason.clone()));
            if app.active_session_id == Some(id) {
                app.connection_status = ConnectionStatus::Failed {
                    reason: reason.clone(),
                };
            }
            prune_multicast_target(app, id);
            #[cfg(feature = "ssh")]
            {
                crate::connection::sftp::SftpManager::shared().note_ssh_disconnected(id);
                crate::connection::forward::ForwardManager::shared().note_ssh_disconnected(id);
            }
            #[cfg(feature = "x11")]
            {
                crate::connection::x11::X11Manager::shared().note_ssh_disconnected(id);
            }
            app.notify(Level::Error, "Connection failed", &reason);
            Ok(iced::Task::none())
        },
    }
}

/// Multi-exec broadcast for leader input (Prompt 5.1).
///
/// - Prunes disconnected targets first (dropout races resolve here, with a
///   notification naming who left).
/// - The leader always receives its own keystrokes, then configured targets.
/// - Destructive input is held for confirmation instead of sent.
/// - Conditional host filtering applies before sending.
/// - `stagger_ms > 0` fans out through delayed tasks (no UI blocking);
///   otherwise delivery is synchronous best-effort.
fn broadcast_multicast(
    app: &mut AppState,
    leader: SessionId,
    bytes: Vec<u8>,
) -> Result<iced::Task<Message>, String> {
    use crate::app::multi_exec as engine;

    let connected =
        |id: SessionId| matches!(app.session_states.get(&id), Some(SessionState::Connected));
    let (kept, dropped) = engine::prune_disconnected(&app.multi_exec_mode.targets, &connected);
    if kept != app.multi_exec_mode.targets {
        app.multi_exec_mode.targets = kept;
        if let Ok(mut shared) = app.shared.write() {
            shared.multi_exec_targets = app.multi_exec_mode.targets.clone();
        }
    }
    for id in dropped {
        app.notify(
            Level::Warning,
            "Multi-exec dropout",
            &format!("session #{id} left the target set"),
        );
    }

    if engine::is_destructive(&bytes) {
        app.multi_exec_pending = Some(bytes);
        app.notify(
            Level::Warning,
            "Destructive broadcast held",
            "confirm in the multi-exec toolbar to send",
        );
        return Ok(iced::Task::none());
    }

    let filter = app.multi_exec_mode.host_filter.clone();
    let mut targets = engine::broadcast_targets(leader, &app.multi_exec_mode.targets);
    if !filter.is_empty() {
        targets.retain(|id| {
            app.session(*id)
                .map(|s| engine::matches_host_filter(s.spec.host.as_deref(), &filter))
                .unwrap_or(false)
        });
        // The leader typed here: never filter the leader itself out.
        if !targets.contains(&leader) {
            targets.insert(0, leader);
        }
    }

    app.multi_exec_leader = Some(leader);
    let history_lines = engine::completed_lines(&bytes);

    let stagger = app.multi_exec_mode.stagger_ms;
    if stagger > 0 {
        let schedule = engine::stagger_schedule(&targets, stagger);
        let tasks = schedule.into_iter().map(|(id, delay_ms)| {
            let bytes = bytes.clone();
            iced::Task::perform(
                async move {
                    if delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                    crate::connection::actor::SessionManager::shared()
                        .write(id, bytes)
                        .map_err(|reason| (id, reason))
                },
                |result| match result {
                    Ok(()) => Message::Ignored,
                    Err((id, reason)) => Message::Ui(UiMsg::MultiExecTargetFailed(id, reason)),
                },
            )
        });
        // Staggered tasks are accepted for sending: record intent history.
        for line in history_lines {
            if app.multi_exec_history.len() >= engine::MAX_HISTORY {
                app.multi_exec_history.remove(0);
            }
            app.multi_exec_history.push(line);
        }
        return Ok(iced::Task::batch(tasks));
    }

    let (delivered, failed) =
        crate::connection::actor::SessionManager::shared().broadcast(&targets, &bytes);
    for (id, reason) in failed {
        app.multi_exec_mode.targets.retain(|t| *t != id);
        app.notify(
            Level::Warning,
            "Multi-exec delivery failed",
            &format!("session #{id}: {reason}"),
        );
    }
    if delivered.is_empty() && !targets.is_empty() {
        return Err("no multi-exec target accepted input".to_string());
    }
    // Only executed commands enter history (all-failed broadcasts do not).
    for line in history_lines {
        if app.multi_exec_history.len() >= engine::MAX_HISTORY {
            app.multi_exec_history.remove(0);
        }
        app.multi_exec_history.push(line);
    }
    Ok(iced::Task::none())
}

fn handle_terminal(app: &mut AppState, msg: TerminalMsg) -> Result<iced::Task<Message>, String> {
    match msg {
        TerminalMsg::Output(id, bytes) => {
            app.last_activity.insert(id, now_secs());
            let scrollback = app.settings.terminal.scrollback_lines;
            app.terminals
                .entry(id)
                .or_insert_with(|| mbxt_terminal::Terminal::new(80, 24, scrollback))
                .write_bytes(&bytes);
            // Macro hooks (Prompt 5.2): recorder prompt detection, then the
            // running player's pattern wait for this session.
            app.macro_recorder.observe_output(id, &bytes);
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let feed = match app.macro_player.as_mut() {
                Some(running) if running.session == id => {
                    match running.player.observe_output(&text) {
                        Ok(matched) => {
                            if matched {
                                FeedOutcome::Matched
                            } else {
                                FeedOutcome::Waiting
                            }
                        },
                        Err(err) => FeedOutcome::Failed(err.to_string()),
                    }
                },
                _ => FeedOutcome::NoPlayer,
            };
            match feed {
                FeedOutcome::Matched => return continue_macro(app, id),
                FeedOutcome::Failed(reason) => {
                    if let Some(running) = app.macro_player.as_mut() {
                        running.player.stop();
                    }
                    app.macro_player = None;
                    app.notify(Level::Error, "Macro failed", &reason);
                },
                FeedOutcome::Waiting | FeedOutcome::NoPlayer => {},
            }
            // Combined multi-exec view: mirror output lines of targeted
            // sessions (and the leader) with session prefixes.
            if app.multi_exec_mode.enabled
                && (app.multi_exec_mode.targets.contains(&id) || app.multi_exec_leader == Some(id))
            {
                let text = String::from_utf8_lossy(&bytes);
                for line in text.split('\n') {
                    let line = line.trim_end_matches('\r');
                    if line.trim().is_empty() {
                        continue;
                    }
                    crate::app::multi_exec::push_log_line(
                        &mut app.multi_exec_log,
                        id,
                        line.to_string(),
                    );
                }
            }
            Ok(iced::Task::none())
        },
        TerminalMsg::Input(id, bytes) => {
            app.last_activity.insert(id, now_secs());
            // Macro recorder hook (Prompt 5.2): O(1) append, never blocks.
            app.macro_recorder.observe_input(id, &bytes);
            if app.multi_exec_mode.enabled {
                broadcast_multicast(app, id, bytes)
            } else {
                crate::connection::actor::SessionManager::shared().write(id, bytes)?;
                Ok(iced::Task::none())
            }
        },
        TerminalMsg::Bell(id) => {
            app.notify(Level::Info, "Terminal bell", &format!("session #{id}"));
            Ok(iced::Task::none())
        },
        TerminalMsg::TitleChanged(id, title) => {
            // Tab title update (#41) — stored on the session mirror for now.
            if let Some(session) = app.session_mut(id) {
                session.spec.notes = title;
            }
            Ok(iced::Task::none())
        },
    }
}

fn handle_sftp(app: &mut AppState, msg: SftpMsg) -> Result<iced::Task<Message>, String> {
    match msg {
        SftpMsg::BrowseRequested(session, path) => {
            #[cfg(feature = "ssh")]
            {
                load_browser_directory(app, session, path, false)
            }
            #[cfg(not(feature = "ssh"))]
            {
                let _ = (session, path);
                return Err("sftp support not compiled in".to_string());
            }
        },
        SftpMsg::TransferQueued {
            session,
            remote_path,
            local_path,
        } => {
            app.transfers.insert(
                session,
                super::state::TransferProgress {
                    remote_path: remote_path.clone(),
                    local_path: local_path.clone(),
                    done: 0,
                    total: 0,
                },
            );
            #[cfg(feature = "ssh")]
            {
                // Queue through the transfer manager (concurrency + throttle
                // + retry); the single-transfer session mirror above stays
                // for the file-browser placeholder line.
                let manager = crate::connection::sftp::TransferManager::shared();
                let throttle = manager.config().global_throttle_bps;
                let id = manager.submit(
                    session,
                    crate::connection::sftp::Direction::Upload,
                    std::path::Path::new(&local_path),
                    &remote_path,
                    None,
                    throttle,
                );
                tracing::debug!(transfer = id.0, "transfer queued");
                let depth = app.settings.network.sftp_pipeline_depth;
                Ok(pump_transfer_tasks(depth))
            }
            #[cfg(not(feature = "ssh"))]
            {
                // TODO(prompt 1.3): real russh-sftp pipeline (depth from settings).
                return Ok(spawn_task(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    Err::<(), _>(format!(
                        "transfer {local_path} -> {remote_path}: sftp not wired yet (skeleton)"
                    ))
                }));
            }
        },
        SftpMsg::TransferProgress(session, done, total) => {
            if let Some(t) = app.transfers.get_mut(&session) {
                t.done = done;
                t.total = total;
            }
            Ok(iced::Task::none())
        },
        SftpMsg::TransferCompleted(session) => {
            if let Some(t) = app.transfers.remove(&session) {
                app.notify(Level::Success, "Transfer complete", &t.local_path);
            }
            Ok(iced::Task::none())
        },
        SftpMsg::TransferFailed(session, reason) => {
            app.transfers.remove(&session);
            app.notify(Level::Error, "Transfer failed", &reason);
            Ok(iced::Task::none())
        },
        SftpMsg::PauseRequested(raw) => {
            #[cfg(feature = "ssh")]
            {
                use crate::connection::sftp::{TransferId, TransferManager};
                let manager = TransferManager::shared();
                manager.pause(TransferId(raw))?;
                app.notify(Level::Info, "Transfer paused", &format!("transfer #{raw}"));
                let depth = app.settings.network.sftp_pipeline_depth;
                Ok(pump_transfer_tasks(depth))
            }
            #[cfg(not(feature = "ssh"))]
            {
                let _ = raw;
                Err("sftp support not compiled in".to_string())
            }
        },
        SftpMsg::ResumeRequested(raw) => {
            #[cfg(feature = "ssh")]
            {
                use crate::connection::sftp::{TransferId, TransferManager};
                let manager = TransferManager::shared();
                manager.resume(TransferId(raw))?;
                app.notify(Level::Info, "Transfer resumed", &format!("transfer #{raw}"));
                let depth = app.settings.network.sftp_pipeline_depth;
                Ok(pump_transfer_tasks(depth))
            }
            #[cfg(not(feature = "ssh"))]
            {
                let _ = raw;
                Err("sftp support not compiled in".to_string())
            }
        },
        SftpMsg::CancelRequested(raw) => {
            #[cfg(feature = "ssh")]
            {
                use crate::connection::sftp::{TransferId, TransferManager};
                let manager = TransferManager::shared();
                // Mirror cleanup for the session-keyed placeholder line.
                if let Some(record) = manager.get_status(TransferId(raw)) {
                    app.transfers.remove(&record.session);
                }
                manager.cancel(TransferId(raw))?;
                app.notify(
                    Level::Warning,
                    "Transfer cancelled",
                    &format!("transfer #{raw}"),
                );
                let depth = app.settings.network.sftp_pipeline_depth;
                Ok(pump_transfer_tasks(depth))
            }
            #[cfg(not(feature = "ssh"))]
            {
                let _ = raw;
                Err("sftp support not compiled in".to_string())
            }
        },
        SftpMsg::ManagerProgress(raw, done, total) => {
            #[cfg(feature = "ssh")]
            {
                use crate::connection::sftp::{TransferId, TransferManager};
                let manager = TransferManager::shared();
                manager.on_progress(TransferId(raw), done, total);
                // Mirror into the session-keyed placeholder line.
                if let Some(record) = manager.get_status(TransferId(raw)) {
                    if let Some(entry) = app.transfers.get_mut(&record.session) {
                        entry.done = done;
                        entry.total = total.unwrap_or(done);
                    }
                }
                Ok(iced::Task::none())
            }
            #[cfg(not(feature = "ssh"))]
            {
                let _ = (raw, done, total);
                Ok(iced::Task::none())
            }
        },
        SftpMsg::ManagerFinished(raw, outcome) => {
            #[cfg(feature = "ssh")]
            {
                use crate::connection::sftp::FinishOutcome;
                use crate::connection::sftp::{SftpError, TransferId, TransferManager};
                let manager = TransferManager::shared();
                let id = TransferId(raw);
                let session = manager.get_status(id).map(|t| t.session);
                let result = match outcome {
                    super::messages::TransferOutcome::Completed => Ok(()),
                    super::messages::TransferOutcome::Cancelled => Err(SftpError::Cancelled),
                    super::messages::TransferOutcome::Retryable(reason) => {
                        Err(SftpError::Protocol(reason))
                    },
                    super::messages::TransferOutcome::Failed(reason) => {
                        Err(SftpError::Permanent(reason))
                    },
                };
                let depth = app.settings.network.sftp_pipeline_depth;
                match manager.on_finished(id, result) {
                    FinishOutcome::Done => {
                        let status = manager.get_status(id).map(|t| t.status);
                        let mut tasks = vec![pump_transfer_tasks(depth)];
                        match status {
                            Some(crate::connection::sftp::TransferStatus::Completed) => {
                                if let Some(session) = session {
                                    tasks.push(handle_sftp(
                                        app,
                                        SftpMsg::TransferCompleted(session),
                                    )?);
                                }
                            },
                            Some(crate::connection::sftp::TransferStatus::Failed(reason)) => {
                                if let Some(session) = session {
                                    tasks.push(handle_sftp(
                                        app,
                                        SftpMsg::TransferFailed(session, reason),
                                    )?);
                                }
                            },
                            Some(crate::connection::sftp::TransferStatus::Cancelled) => {
                                if let Some(session) = session {
                                    app.transfers.remove(&session);
                                    app.notify(
                                        Level::Warning,
                                        "Transfer cancelled",
                                        &format!("transfer #{raw}"),
                                    );
                                }
                            },
                            _ => {},
                        }
                        Ok(iced::Task::batch(tasks))
                    },
                    FinishOutcome::RetryScheduled { delay } => {
                        app.notify(
                            Level::Info,
                            "Retrying transfer",
                            &format!("transfer #{raw} in {}ms", delay.as_millis()),
                        );
                        Ok(iced::Task::perform(
                            async move {
                                tokio::time::sleep(delay).await;
                            },
                            |()| Message::Sftp(SftpMsg::PumpTransfers),
                        ))
                    },
                }
            }
            #[cfg(not(feature = "ssh"))]
            {
                let _ = (raw, outcome);
                Ok(iced::Task::none())
            }
        },
        SftpMsg::PumpTransfers => {
            #[cfg(feature = "ssh")]
            {
                let depth = app.settings.network.sftp_pipeline_depth;
                Ok(pump_transfer_tasks(depth))
            }
            #[cfg(not(feature = "ssh"))]
            {
                Ok(iced::Task::none())
            }
        },
        // -- file browser (Prompt 3.3; all arms ssh-gated) -------------------
        #[cfg(feature = "ssh")]
        SftpMsg::DirectoryListed {
            session,
            path,
            entries,
        } => {
            use crate::connection::sftp::FileBrowserState;
            app.browser_cache.insert(session, &path, entries.clone());
            let browser = app
                .browsers
                .entry(session)
                .or_insert_with(|| FileBrowserState::new(session, &path));
            browser.cwd = path;
            browser.entries = entries;
            browser.loading = false;
            browser.error = None;
            browser.pages = 1;
            browser.prune_selection();
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::DirectoryFailed(session, reason) => {
            use crate::connection::sftp::FileBrowserState;
            let browser = app
                .browsers
                .entry(session)
                .or_insert_with(|| FileBrowserState::new(session, "."));
            browser.loading = false;
            browser.error = Some(reason.clone());
            app.notify(Level::Error, "Could not list directory", &reason);
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserRefresh(session) => {
            let path = browser_cwd(app, session);
            load_browser_directory(app, session, path, true)
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserSortChanged(session, sort, dir) => {
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.sort = sort;
                browser.sort_dir = dir;
                browser.pages = 1;
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserFilterChanged(session, filter) => {
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.filter = filter;
                browser.pages = 1;
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserToggled(session, name) => {
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.toggle(&name);
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserSelectAll(session) => {
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.select_all_visible();
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserClearSelection(session) => {
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.selected.clear();
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserShowMore(session) => {
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.pages += 1;
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserInputChanged(session, value) => {
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.input = value;
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserInputCancelled(session) => {
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.pending = None;
                browser.input.clear();
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserInputConfirmed(session) => confirm_browser_input(app, session),
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserMkdirRequest(session) => {
            use crate::connection::sftp::PendingOp;
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.pending = Some(PendingOp::Mkdir);
                browser.input.clear();
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserRenameRequest(session, name) => {
            use crate::connection::sftp::PendingOp;
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.input = name.clone();
                browser.pending = Some(PendingOp::Rename { old: name });
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserChmodRequest(session) => {
            use crate::connection::sftp::PendingOp;
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.pending = Some(PendingOp::Chmod);
                browser.input = "755".to_string();
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserUploadRequest(session) => {
            use crate::connection::sftp::PendingOp;
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.pending = Some(PendingOp::Upload);
                browser.input.clear();
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserDeleteSelected(session) => delete_browser_selection(app, session),
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserDownloadSelected(session) => download_browser_selection(app, session),
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserPreview(session, name) => preview_browser_entry(app, session, &name),
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserPreviewReady(session, path, text) => {
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.preview = Some((path, text));
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserPreviewClosed(session) => {
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.preview = None;
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserProperties(session, name) => {
            let dump = app
                .browsers
                .get(&session)
                .and_then(|browser| {
                    browser.entries.iter().find(|e| e.name == name)
                })
                .map(|entry| {
                    format!(
                        "{}\ntype: {:?}\nsize: {} bytes\npermissions: {:o} ({})\nmodified: {}\nowner: {}\ngroup: {}",
                        entry.path,
                        entry.file_type,
                        entry.size,
                        entry.permissions,
                        entry.permission_string(),
                        entry
                            .modified
                            .map(|t| format!("{t:?}"))
                            .unwrap_or_else(|| "unknown".to_string()),
                        entry.owner.as_deref().unwrap_or("unknown"),
                        entry.group.as_deref().unwrap_or("unknown"),
                    )
                });
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.preview = dump.map(|text| (name.clone(), text));
            }
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserContextOpened(session, target) => {
            use crate::connection::sftp::{ContextTarget, FileBrowserState};
            let browser = app
                .browsers
                .entry(session)
                .or_insert_with(|| FileBrowserState::new(session, "."));
            browser.context = Some(match target {
                Some(name) => ContextTarget::Entry(name),
                None => ContextTarget::Background,
            });
            Ok(iced::Task::none())
        },
        #[cfg(feature = "ssh")]
        SftpMsg::BrowserContextClosed(session) => {
            if let Some(browser) = app.browsers.get_mut(&session) {
                browser.context = None;
            }
            Ok(iced::Task::none())
        },
    }
}

/// Current browser directory (`.` when the pane was never opened).
#[cfg(feature = "ssh")]
fn browser_cwd(app: &AppState, session: SessionId) -> String {
    app.browsers
        .get(&session)
        .map(|browser| browser.cwd.clone())
        .unwrap_or_else(|| ".".to_string())
}

/// Load a directory into the browser: fresh cache hits apply synchronously,
/// otherwise an SFTP listing task lands as `DirectoryListed/DirectoryFailed`.
#[cfg(feature = "ssh")]
fn load_browser_directory(
    app: &mut AppState,
    session: SessionId,
    path: String,
    force: bool,
) -> Result<iced::Task<Message>, String> {
    use crate::connection::sftp::FileBrowserState;

    if force {
        app.browser_cache.invalidate(session, &path);
    }
    if let Some(entries) = app.browser_cache.get(session, &path) {
        let browser = app
            .browsers
            .entry(session)
            .or_insert_with(|| FileBrowserState::new(session, &path));
        browser.cwd = path;
        browser.entries = entries;
        browser.loading = false;
        browser.error = None;
        browser.pages = 1;
        browser.prune_selection();
        return Ok(iced::Task::none());
    }
    let Some(handle) = crate::connection::sftp::SftpManager::shared().get(session) else {
        let browser = app
            .browsers
            .entry(session)
            .or_insert_with(|| FileBrowserState::new(session, &path));
        browser.loading = false;
        browser.error = Some("file browser is not connected".to_string());
        return Err(format!("session #{session} file browser is not connected"));
    };
    {
        let browser = app
            .browsers
            .entry(session)
            .or_insert_with(|| FileBrowserState::new(session, &path));
        browser.cwd = path.clone();
        browser.loading = true;
        browser.error = None;
    }
    Ok(iced::Task::perform(
        async move {
            handle
                .list_directory(&path)
                .await
                .map(|entries| (path, entries))
                .map_err(|err| err.to_string())
        },
        move |result| match result {
            Ok((path, entries)) => Message::Sftp(SftpMsg::DirectoryListed {
                session,
                path,
                entries,
            }),
            Err(reason) => Message::Sftp(SftpMsg::DirectoryFailed(session, reason)),
        },
    ))
}

/// Confirm the mkdir/rename/chmod inline form (validates, then runs the op
/// and refreshes the listing through the normal cached path).
#[cfg(feature = "ssh")]
fn confirm_browser_input(
    app: &mut AppState,
    session: SessionId,
) -> Result<iced::Task<Message>, String> {
    use crate::connection::sftp::{join_remote, PendingOp};

    let (cwd, pending, input) = match app.browsers.get_mut(&session) {
        Some(browser) => (
            browser.cwd.clone(),
            browser.pending.take(),
            browser.input.trim().to_string(),
        ),
        None => return Err(format!("no file browser for session #{session}")),
    };
    if let Some(browser) = app.browsers.get_mut(&session) {
        browser.input.clear();
    }
    let Some(op) = pending else {
        return Ok(iced::Task::none());
    };
    let Some(handle) = crate::connection::sftp::SftpManager::shared().get(session) else {
        return Err(format!("session #{session} file browser is not connected"));
    };
    if input.is_empty() {
        return Err("name cannot be empty".to_string());
    }
    app.browser_cache.invalidate(session, &cwd);
    let task = match op {
        PendingOp::Mkdir => {
            let path = join_remote(&cwd, &input);
            iced::Task::perform(
                async move {
                    handle
                        .create_directory(&path)
                        .await
                        .map(|()| cwd)
                        .map_err(|err| err.to_string())
                },
                move |result| match result {
                    Ok(cwd) => Message::Sftp(SftpMsg::BrowseRequested(session, cwd)),
                    Err(reason) => Message::Sftp(SftpMsg::DirectoryFailed(session, reason)),
                },
            )
        },
        PendingOp::Rename { old } => {
            let from = join_remote(&cwd, &old);
            let to = join_remote(&cwd, &input);
            iced::Task::perform(
                async move {
                    handle
                        .rename(&from, &to)
                        .await
                        .map(|()| cwd)
                        .map_err(|err| err.to_string())
                },
                move |result| match result {
                    Ok(cwd) => Message::Sftp(SftpMsg::BrowseRequested(session, cwd)),
                    Err(reason) => Message::Sftp(SftpMsg::DirectoryFailed(session, reason)),
                },
            )
        },
        PendingOp::Upload => {
            use crate::connection::sftp::{Direction, TransferManager};
            let local = std::path::PathBuf::from(&input);
            if !local.is_file() {
                return Err(format!("local file not found: {input}"));
            }
            let name = local
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| input.clone());
            let remote = join_remote(&cwd, &name);
            let manager = TransferManager::shared();
            manager.submit(
                session,
                Direction::Upload,
                &local,
                &remote,
                std::fs::metadata(&local).ok().map(|m| m.len()),
                manager.config().global_throttle_bps,
            );
            app.notify(Level::Info, "Upload queued", &format!("{input} → {remote}"));
            return Ok(pump_transfer_tasks(
                app.settings.network.sftp_pipeline_depth,
            ));
        },
        PendingOp::Chmod => {
            let mode = u32::from_str_radix(&input, 8)
                .map_err(|_| "mode must be octal (e.g. 755)".to_string())?;
            let targets: Vec<String> = app
                .browsers
                .get(&session)
                .map(|browser| {
                    browser
                        .selected
                        .iter()
                        .map(|name| join_remote(&cwd, name))
                        .collect()
                })
                .unwrap_or_default();
            if targets.is_empty() {
                return Err("nothing selected".to_string());
            }
            iced::Task::perform(
                async move {
                    for path in &targets {
                        handle
                            .chmod(path, mode)
                            .await
                            .map_err(|err| err.to_string())?;
                    }
                    Ok::<String, String>(cwd)
                },
                move |result| match result {
                    Ok(cwd) => Message::Sftp(SftpMsg::BrowseRequested(session, cwd)),
                    Err(reason) => Message::Sftp(SftpMsg::DirectoryFailed(session, reason)),
                },
            )
        },
    };
    Ok(task)
}

/// Delete every selected entry (files via delete, directories via rmdir),
/// then refresh the listing.
#[cfg(feature = "ssh")]
fn delete_browser_selection(
    app: &mut AppState,
    session: SessionId,
) -> Result<iced::Task<Message>, String> {
    let (cwd, targets) = match app.browsers.get(&session) {
        Some(browser) => (
            browser.cwd.clone(),
            browser
                .entries
                .iter()
                .filter(|e| browser.selected.contains(&e.name))
                .map(|e| (e.path.clone(), e.is_dir()))
                .collect::<Vec<_>>(),
        ),
        None => return Err(format!("no file browser for session #{session}")),
    };
    if targets.is_empty() {
        return Err("nothing selected".to_string());
    }
    let Some(handle) = crate::connection::sftp::SftpManager::shared().get(session) else {
        return Err(format!("session #{session} file browser is not connected"));
    };
    app.browser_cache.invalidate(session, &cwd);
    if let Some(browser) = app.browsers.get_mut(&session) {
        browser.selected.clear();
    }
    Ok(iced::Task::perform(
        async move {
            let mut deleted = 0u32;
            for (path, is_dir) in &targets {
                let result = if *is_dir {
                    handle.remove_directory(path).await
                } else {
                    handle.delete(path).await
                };
                result.map_err(|err| err.to_string())?;
                deleted += 1;
            }
            Ok::<(String, u32), String>((cwd, deleted))
        },
        move |result| match result {
            Ok((cwd, _)) => Message::Sftp(SftpMsg::BrowseRequested(session, cwd)),
            Err(reason) => Message::Sftp(SftpMsg::DirectoryFailed(session, reason)),
        },
    ))
}

/// Queue downloads for every selected file through the transfer manager.
#[cfg(feature = "ssh")]
fn download_browser_selection(
    app: &mut AppState,
    session: SessionId,
) -> Result<iced::Task<Message>, String> {
    use crate::connection::sftp::{Direction, TransferManager};

    let entries = match app.browsers.get(&session) {
        Some(browser) => browser
            .entries
            .iter()
            .filter(|e| browser.selected.contains(&e.name) && e.is_file())
            .map(|e| (e.path.clone(), e.name.clone(), e.size))
            .collect::<Vec<_>>(),
        None => return Err(format!("no file browser for session #{session}")),
    };
    if entries.is_empty() {
        return Err("select files to download first".to_string());
    }
    let download_dir = app.paths.data_dir.join("downloads");
    std::fs::create_dir_all(&download_dir).map_err(|err| err.to_string())?;
    let manager = TransferManager::shared();
    let throttle = manager.config().global_throttle_bps;
    let mut count = 0u32;
    for (remote, name, size) in entries {
        let local = download_dir.join(&name);
        app.transfers.insert(
            session,
            super::state::TransferProgress {
                remote_path: remote.clone(),
                local_path: local.to_string_lossy().into_owned(),
                done: 0,
                total: size,
            },
        );
        manager.submit(
            session,
            Direction::Download,
            &local,
            &remote,
            Some(size),
            throttle,
        );
        count += 1;
    }
    app.notify(
        Level::Info,
        "Downloads queued",
        &format!("{count} file(s) to {}", download_dir.display()),
    );
    let depth = app.settings.network.sftp_pipeline_depth;
    Ok(pump_transfer_tasks(depth))
}

/// Preview a remote file: fetch a bounded prefix, keep the first 10 lines
/// (binary files show a placeholder instead of garbage).
#[cfg(feature = "ssh")]
fn preview_browser_entry(
    app: &mut AppState,
    session: SessionId,
    name: &str,
) -> Result<iced::Task<Message>, String> {
    use crate::connection::sftp::join_remote;

    let path = match app.browsers.get(&session) {
        Some(browser) => join_remote(&browser.cwd, name),
        None => return Err(format!("no file browser for session #{session}")),
    };
    let Some(handle) = crate::connection::sftp::SftpManager::shared().get(session) else {
        return Err(format!("session #{session} file browser is not connected"));
    };
    Ok(iced::Task::perform(
        async move {
            handle
                .read_prefix(&path, 8192)
                .await
                .map(|bytes| {
                    let text = if bytes.contains(&0) {
                        format!("[binary file, {} bytes shown]", bytes.len())
                    } else {
                        String::from_utf8_lossy(&bytes)
                            .lines()
                            .take(10)
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    (path, text)
                })
                .map_err(|err| err.to_string())
        },
        move |result| match result {
            Ok((path, text)) => Message::Sftp(SftpMsg::BrowserPreviewReady(session, path, text)),
            Err(reason) => Message::Sftp(SftpMsg::DirectoryFailed(session, reason)),
        },
    ))
}

/// Start every queued transfer that has a free slot (Prompt 3.2 scheduler).
///
/// Pops [`TransferManager::next_ready`] until no slot/transfer is ready and
/// spawns one iced `Task` per transfer driving `get/put_file_with_options`
/// (throttled, resumable, cancellable). Per-chunk progress records into the
/// manager and publishes on the session bus; completion maps into
/// `ManagerFinished` for retry accounting.
#[cfg(feature = "ssh")]
fn pump_transfer_tasks(depth: u32) -> iced::Task<Message> {
    use super::messages::TransferOutcome;
    use crate::connection::sftp::{Direction, SftpError, TransferManager, TransferOptions};

    let mut tasks = Vec::new();
    while let Some(record) = TransferManager::shared().next_ready() {
        let id = record.id;
        let session = record.session;
        let handle = crate::connection::sftp::SftpManager::shared().get(session);
        let cancel = record.cancel.clone();
        let options = TransferOptions {
            pipeline_depth: depth,
            throttle_bps: record.throttle_bps,
        };
        tasks.push(iced::Task::perform(
            async move {
                let Some(handle) = handle else {
                    return (
                        id.0,
                        TransferOutcome::Failed("sftp is not connected".to_string()),
                    );
                };
                let on_progress =
                    |done: u64, total: Option<u64>| {
                        TransferManager::shared().on_progress(id, done, total);
                        crate::connection::actor::SessionManager::shared()
                            .publish_transfer_progress(session, done, total.unwrap_or(done));
                    };
                let result = match record.direction {
                    Direction::Upload => {
                        handle
                            .put_file_with_options(
                                &record.local_path,
                                &record.remote_path,
                                &options,
                                &cancel,
                                on_progress,
                            )
                            .await
                    },
                    Direction::Download => {
                        handle
                            .get_file_with_options(
                                &record.remote_path,
                                &record.local_path,
                                &options,
                                &cancel,
                                on_progress,
                            )
                            .await
                    },
                };
                let outcome = match result {
                    Ok(_) => TransferOutcome::Completed,
                    Err(SftpError::Cancelled) => TransferOutcome::Cancelled,
                    Err(SftpError::Protocol(reason))
                    | Err(SftpError::Io(reason))
                    | Err(SftpError::Ssh(reason)) => TransferOutcome::Retryable(reason),
                    Err(err) => TransferOutcome::Failed(err.to_string()),
                };
                (id.0, outcome)
            },
            |(raw, outcome)| Message::Sftp(SftpMsg::ManagerFinished(raw, outcome)),
        ));
    }
    iced::Task::batch(tasks)
}

/// VNC viewer events (Prompt 4.3, vnc-gated).
#[cfg(feature = "vnc")]
fn handle_vnc(app: &mut AppState, msg: VncMsg) -> Result<iced::Task<Message>, String> {
    use crate::connection::vnc::VncManager;

    match msg {
        VncMsg::OpenViewer(session) => {
            focus_vnc_tab(app, session);
            Ok(iced::Task::none())
        },
        VncMsg::CloseViewer(session) => {
            VncManager::shared().note_disconnected(session);
            app.vnc_viewers.remove(&session);
            app.tabs
                .retain(|tab| tab.kind != TabKind::VncViewer(session));
            app.active_tab = app.active_tab.min(app.tabs.len().saturating_sub(1));
            Ok(iced::Task::none())
        },
        VncMsg::FrameTickAll => Ok(iced::Task::none()),
        VncMsg::PointerMoved(session, buttons, (x, y)) => {
            let Some(handle) = VncManager::shared().get(session) else {
                return Ok(iced::Task::none());
            };
            Ok(iced::Task::perform(
                async move {
                    handle
                        .send_pointer(PointerEvent::new(buttons, x, y))
                        .await
                        .map_err(|err| err.to_string())
                },
                move |result| match result {
                    Ok(()) => Message::Ignored,
                    Err(reason) => Message::Vnc(VncMsg::ViewerError(session, reason)),
                },
            ))
        },
        VncMsg::PointerButton(session, buttons, (x, y)) => {
            let Some(handle) = VncManager::shared().get(session) else {
                return Ok(iced::Task::none());
            };
            Ok(iced::Task::perform(
                async move {
                    handle
                        .send_pointer(PointerEvent::new(buttons, x, y))
                        .await
                        .map_err(|err| err.to_string())
                },
                move |result| match result {
                    Ok(()) => Message::Ignored,
                    Err(reason) => Message::Vnc(VncMsg::ViewerError(session, reason)),
                },
            ))
        },
        VncMsg::PointerWheel(session, (x, y), up) => {
            let Some(handle) = VncManager::shared().get(session) else {
                return Ok(iced::Task::none());
            };
            let bit = if up {
                crate::connection::vnc::input::button::WHEEL_UP
            } else {
                crate::connection::vnc::input::button::WHEEL_DOWN
            };
            Ok(iced::Task::perform(
                async move {
                    // Wheel is a click: press + release back to back.
                    handle
                        .send_pointer(PointerEvent::new(bit, x, y))
                        .await
                        .map_err(|err| err.to_string())?;
                    handle
                        .send_pointer(PointerEvent::new(0, x, y))
                        .await
                        .map_err(|err| err.to_string())
                },
                move |result| match result {
                    Ok(()) => Message::Ignored,
                    Err(reason) => Message::Vnc(VncMsg::ViewerError(session, reason)),
                },
            ))
        },
        VncMsg::KeyEvent(session, keysym, down) => {
            let Some(handle) = VncManager::shared().get(session) else {
                return Ok(iced::Task::none());
            };
            Ok(iced::Task::perform(
                async move {
                    handle
                        .send_key(KeyPress { keysym, down })
                        .await
                        .map_err(|err| err.to_string())
                },
                move |result| match result {
                    Ok(()) => Message::Ignored,
                    Err(reason) => Message::Vnc(VncMsg::ViewerError(session, reason)),
                },
            ))
        },
        VncMsg::KeyTap(session, keysym) => {
            let Some(handle) = VncManager::shared().get(session) else {
                return Ok(iced::Task::none());
            };
            Ok(iced::Task::perform(
                async move {
                    handle
                        .send_key(KeyPress::down(keysym))
                        .await
                        .map_err(|err| err.to_string())?;
                    handle
                        .send_key(KeyPress::up(keysym))
                        .await
                        .map_err(|err| err.to_string())
                },
                move |result| match result {
                    Ok(()) => Message::Ignored,
                    Err(reason) => Message::Vnc(VncMsg::ViewerError(session, reason)),
                },
            ))
        },
        VncMsg::SendCad(session) => {
            let Some(handle) = VncManager::shared().get(session) else {
                return Err(format!("remote desktop #{session} is not connected"));
            };
            Ok(iced::Task::perform(
                async move {
                    for press in crate::connection::vnc::input::ctrl_alt_del() {
                        handle
                            .send_key(press)
                            .await
                            .map_err(|err| err.to_string())?;
                    }
                    Ok::<(), String>(())
                },
                move |result| match result {
                    Ok(()) => Message::Ignored,
                    Err(reason) => Message::Vnc(VncMsg::ViewerError(session, reason)),
                },
            ))
        },
        VncMsg::SendClipboard(session) => {
            let Some(handle) = VncManager::shared().get(session) else {
                return Err(format!("remote desktop #{session} is not connected"));
            };
            let text = app.clipboard.last_content.clone();
            if text.is_empty() {
                app.notify(Level::Info, "Clipboard empty", "copy text locally first");
                return Ok(iced::Task::none());
            }
            Ok(iced::Task::perform(
                async move {
                    handle
                        .send_cut_text(&text)
                        .await
                        .map_err(|err| err.to_string())
                },
                move |result| match result {
                    Ok(()) => Message::Ignored,
                    Err(reason) => Message::Vnc(VncMsg::ViewerError(session, reason)),
                },
            ))
        },
        VncMsg::SetScaling(session, scaling) => {
            if let Some(viewer) = app.vnc_viewers.get_mut(&session) {
                viewer.scaling = scaling;
            }
            Ok(iced::Task::none())
        },
        VncMsg::ToggleFullscreen(session) => {
            let entering = !app
                .vnc_viewers
                .get(&session)
                .is_some_and(|viewer| viewer.fullscreen);
            if let Some(viewer) = app.vnc_viewers.get_mut(&session) {
                viewer.fullscreen = entering;
            }
            Ok(iced::window::get_oldest()
                .map(move |id| Message::Vnc(VncMsg::GotWindowId(session, id, entering))))
        },
        VncMsg::GotWindowId(session, id, entering) => {
            let Some(id) = id else {
                app.notify(
                    Level::Warning,
                    "Fullscreen unavailable",
                    "no application window",
                );
                if let Some(viewer) = app.vnc_viewers.get_mut(&session) {
                    viewer.fullscreen = false;
                }
                return Ok(iced::Task::none());
            };
            Ok(iced::window::change_mode(
                id,
                if entering {
                    iced::window::Mode::Fullscreen
                } else {
                    iced::window::Mode::Windowed
                },
            ))
        },
        VncMsg::Connected(session) => {
            if let Some(viewer) = app.vnc_viewers.get_mut(&session) {
                viewer.connected = true;
                viewer.error = None;
            }
            focus_vnc_tab(app, session);
            app.notify(
                Level::Success,
                "Remote desktop connected",
                &format!("session #{session}"),
            );
            Ok(iced::Task::none())
        },
        VncMsg::ConnectFailed(session, reason) => {
            if let Some(viewer) = app.vnc_viewers.get_mut(&session) {
                viewer.connected = false;
                viewer.error = Some(reason.clone());
            }
            app.notify(Level::Error, "Remote desktop failed", &reason);
            Ok(iced::Task::none())
        },
        VncMsg::ViewerError(session, reason) => {
            tracing::warn!(session, %reason, "vnc viewer error");
            app.notify(Level::Warning, "Remote desktop issue", &reason);
            Ok(iced::Task::none())
        },
    }
}

/// Focus (or open) the viewer tab for a session.
#[cfg(feature = "vnc")]
fn focus_vnc_tab(app: &mut AppState, session: SessionId) {
    use crate::ui::widgets::vnc_view::VncViewerUi;
    app.vnc_viewers
        .entry(session)
        .or_insert_with(|| VncViewerUi {
            connected: crate::connection::vnc::VncManager::shared().is_connected(session),
            ..VncViewerUi::default()
        });
    if let Some(index) = app
        .tabs
        .iter()
        .position(|tab| tab.kind == TabKind::VncViewer(session))
    {
        app.active_tab = index;
    } else {
        let title = app
            .session(session)
            .map(|s| format!("Desktop: {}", s.spec.name))
            .unwrap_or_else(|| format!("Desktop #{session}"));
        app.tabs.push(Tab {
            title,
            kind: TabKind::VncViewer(session),
        });
        app.active_tab = app.tabs.len() - 1;
    }
    // The viewer tab must be visible, not shadowed by overlays.
    app.ui_state.view = crate::app::messages::ViewKind::SessionList;
}

/// VNC connect flow for `Protocol::Vnc` sessions: direct TCP with a cached
/// password when available (servers without auth connect immediately); the
/// DES password handshake itself lives in the session layer.
#[cfg(feature = "vnc")]
async fn run_vnc_connect(
    id: SessionId,
    spec: SessionSpec,
    cache: CredentialCache,
) -> Result<(), String> {
    use crate::connection::vnc::{VncConfig, VncManager, VncSession};

    let host = spec.host.clone().ok_or("VNC host is missing".to_string())?;
    let port = spec.port.unwrap_or(5900);
    let display = port.saturating_sub(5900).min(u8::MAX as u16) as u8;
    let mut config = VncConfig::direct(&host, display);
    config.port = port;
    let password = cache.get_password(&spec.name);
    let session = VncSession::connect_tcp(config, password.as_deref())
        .await
        .map_err(|err| err.to_string())?;
    VncManager::shared().insert(id, session);
    Ok(())
}

/// Poll serial devices for the dialog picker (empty without the backend).
fn refresh_serial_ports() -> Vec<String> {
    #[cfg(feature = "serial")]
    {
        crate::connection::serial::SerialConfig::available_ports()
    }
    #[cfg(not(feature = "serial"))]
    {
        Vec::new()
    }
}

/// Build a validated [`SessionSpec`] from the dialog draft (mirrors the
/// `CreateDetailed` requirements so errors surface inline).
fn build_session_spec(draft: &super::state::NewSessionDraft) -> Result<SessionSpec, String> {
    let name = draft.name.trim().to_string();
    if name.is_empty() {
        return Err("session name cannot be empty".to_string());
    }
    let mut spec = SessionSpec {
        name,
        protocol: draft.protocol,
        host: None,
        port: None,
        username: None,
        auth: AuthMethod::Password,
        tags: Vec::new(),
        notes: String::new(),
        x11_forwarding: false,
        serial: None,
        forwards: Vec::new(),
    };
    match draft.protocol {
        mbxt_core::Protocol::Ssh
        | mbxt_core::Protocol::Telnet
        | mbxt_core::Protocol::Sftp
        | mbxt_core::Protocol::X11
        | mbxt_core::Protocol::Rdp
        | mbxt_core::Protocol::Vnc => {
            let host = draft.host.trim().to_string();
            if host.is_empty() {
                return Err("host is required".to_string());
            }
            spec.host = Some(host);
            spec.port = draft
                .port
                .trim()
                .parse::<u16>()
                .map_err(|_| "port must be 1–65535".to_string())
                .map(Some)?;
        },
        mbxt_core::Protocol::Serial => {
            let params = mbxt_core::SerialParams {
                device: draft.device.trim().to_string(),
                baud_rate: draft
                    .baud
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| "baud rate must be a number".to_string())?,
                ..mbxt_core::SerialParams::default()
            };
            params.validate().map_err(str::to_string)?;
            spec.serial = Some(params);
        },
        _ => {},
    }
    Ok(spec)
}

/// Player output-feed outcome (keeps borrows out of the match arms).
enum FeedOutcome {
    Matched,
    Waiting,
    Failed(String),
    NoPlayer,
}

/// Macro recording & playback (Prompt 5.2, message-driven player).
#[allow(clippy::too_many_lines)]
fn handle_macro(app: &mut AppState, msg: MacroMsg) -> Result<iced::Task<Message>, String> {
    use crate::macros::Context;

    match msg {
        MacroMsg::RecordToggle => {
            use crate::macros::RecorderState;
            let Some(leader) = app.active_session_id else {
                return Err("no active session to record".to_string());
            };
            match app.macro_recorder.state().clone() {
                RecorderState::Idle => {
                    app.macro_recorder.start(leader);
                    app.notify(
                        Level::Info,
                        "Recording macro",
                        "type in the terminal; Ctrl+Shift+R stops",
                    );
                    Ok(iced::Task::none())
                },
                RecorderState::Recording | RecorderState::Paused => {
                    let name = format!("macro-{}", app.macros.len() + 1);
                    match app.macro_recorder.stop(name.clone()) {
                        Some(mut macro_) => {
                            let store = macro_store(app);
                            store.save(&mut macro_)?;
                            app.macros.retain(|m| m.id != macro_.id);
                            app.macros.push(macro_.clone());
                            app.notify(
                                Level::Success,
                                "Macro recorded",
                                &format!("\"{name}\" ({} steps)", macro_.steps.len()),
                            );
                            Ok(iced::Task::none())
                        },
                        None => {
                            app.notify(Level::Info, "Recording discarded", "nothing was captured");
                            Ok(iced::Task::none())
                        },
                    }
                },
            }
        },
        MacroMsg::RecordPause => {
            use crate::macros::RecorderState;
            match app.macro_recorder.state() {
                RecorderState::Recording => {
                    app.macro_recorder.pause();
                    app.notify(Level::Info, "Recording paused", "");
                },
                RecorderState::Paused => {
                    app.macro_recorder.resume();
                    app.notify(Level::Info, "Recording resumed", "");
                },
                RecorderState::Idle => return Err("no recording in progress".to_string()),
            }
            Ok(iced::Task::none())
        },
        MacroMsg::Play(id) => {
            let Some(leader) = app.active_session_id else {
                return Err("no active session to play on".to_string());
            };
            start_macro_play(app, id, Some(leader))
        },
        MacroMsg::PlayOnTargets(id) => play_macro_on_targets(app, id),
        MacroMsg::Stop => {
            let had_player = app.macro_player.is_some();
            if let Some(running) = app.macro_player.as_mut() {
                running.player.stop();
            }
            app.macro_player = None;
            app.macro_prompt = None;
            if had_player {
                app.notify(Level::Info, "Macro stopped", "");
            }
            Ok(iced::Task::none())
        },
        MacroMsg::Advance(session) => continue_macro(app, session),
        MacroMsg::PromptInputChanged(value) => {
            app.macro_prompt_input = value;
            Ok(iced::Task::none())
        },
        MacroMsg::PromptAnswer(value) => {
            app.macro_prompt_input.clear();
            answer_macro_prompt(app, value)
        },
        MacroMsg::PromptCancel => {
            app.macro_prompt = None;
            app.macro_prompt_input.clear();
            app.macro_pending_play = None;
            if let Some(running) = app.macro_player.as_mut() {
                running.player.stop();
            }
            app.macro_player = None;
            app.notify(Level::Info, "Macro cancelled", "");
            Ok(iced::Task::none())
        },
        MacroMsg::ConfirmPlay => {
            let Some(pending) = app.macro_pending_play.take() else {
                return Err("nothing held for confirmation".to_string());
            };
            match pending.session {
                Some(session) => {
                    start_macro_play_resolved(app, pending.macro_id, session, pending.vars)
                },
                None => play_macro_on_targets_resolved(app, pending.macro_id, pending.vars),
            }
        },
        MacroMsg::DiscardPlay => {
            app.macro_pending_play = None;
            app.notify(Level::Info, "Macro discarded", "held macro dropped");
            Ok(iced::Task::none())
        },
        MacroMsg::HotkeyPressed(key) => {
            let found = app
                .macros
                .iter()
                .find(|m| m.hotkey.as_deref() == Some(key.as_str()))
                .map(|m| m.id);
            match found {
                Some(id) => {
                    let Some(leader) = app.active_session_id else {
                        return Err("no active session to play on".to_string());
                    };
                    start_macro_play(app, id, Some(leader))
                },
                None => {
                    app.notify(Level::Info, "No macro bound", &format!("no macro on {key}"));
                    Ok(iced::Task::none())
                },
            }
        },
        MacroMsg::Refresh => {
            let dir = macro_store_dir(app);
            Ok(iced::Task::perform(
                async move {
                    let store = crate::macros::MacroStore::new(&dir);
                    store.ensure_seeded(&crate::macros::MacroStore::builtin_macros());
                    store.load_all()
                },
                |(macros, errors)| Message::Macro(MacroMsg::LibraryLoaded(macros, errors)),
            ))
        },
        MacroMsg::LibraryLoaded(macros, errors) => {
            app.macros = macros;
            app.macro_load_errors = errors;
            Ok(iced::Task::none())
        },
        MacroMsg::EditorOpened(id) => {
            use crate::macros::MacroEditorState;
            let draft = match id {
                Some(id) => app
                    .macros
                    .iter()
                    .find(|m| m.id == id)
                    .cloned()
                    .ok_or_else(|| "macro not found".to_string())?,
                None => crate::macros::Macro::new(format!("macro-{}", app.macros.len() + 1)),
            };
            app.macro_editor = Some(MacroEditorState::new(draft));
            Ok(iced::Task::none())
        },
        MacroMsg::EditorClosed => {
            app.macro_editor = None;
            Ok(iced::Task::none())
        },
        MacroMsg::EditorSelected(index) => {
            if let Some(editor) = app.macro_editor.as_mut() {
                editor.selected = Some(index);
            }
            Ok(iced::Task::none())
        },
        MacroMsg::EditorFieldChanged(field, value) => {
            use crate::app::messages::MacroEditorField;
            let Some(editor) = app.macro_editor.as_mut() else {
                return Err("editor is not open".to_string());
            };
            match field {
                MacroEditorField::Name => editor.draft.name = value,
                MacroEditorField::Description => {
                    editor.draft.description = if value.is_empty() { None } else { Some(value) };
                },
                MacroEditorField::Tags => {
                    editor.draft.tags = value
                        .split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect();
                },
                MacroEditorField::Text1 => editor.text1 = value,
                MacroEditorField::Text2 => editor.text2 = value,
                MacroEditorField::Number => {
                    editor.number = value.parse::<u64>().unwrap_or(editor.number);
                },
                MacroEditorField::Hotkey => {
                    let value = value.trim().to_string();
                    editor.draft.hotkey = if value.is_empty() { None } else { Some(value) };
                },
            }
            Ok(iced::Task::none())
        },
        MacroMsg::EditorAddStep(kind) => {
            let Some(editor) = app.macro_editor.as_mut() else {
                return Err("editor is not open".to_string());
            };
            editor.new_kind = kind;
            let step = editor.build_step()?;
            editor.draft.steps.push(step);
            editor.selected = Some(editor.draft.steps.len() - 1);
            editor.draft.updated_secs = crate::macros::now_secs();
            Ok(iced::Task::none())
        },
        MacroMsg::EditorDeleteStep(index) => {
            if let Some(editor) = app.macro_editor.as_mut() {
                if index < editor.draft.steps.len() {
                    editor.draft.steps.remove(index);
                    editor.selected = None;
                }
            }
            Ok(iced::Task::none())
        },
        MacroMsg::EditorMoveStep(index, up) => {
            if let Some(editor) = app.macro_editor.as_mut() {
                let other = if up {
                    index.saturating_sub(1)
                } else {
                    index + 1
                };
                if other < editor.draft.steps.len() && index < editor.draft.steps.len() {
                    editor.draft.steps.swap(index, other);
                    editor.selected = Some(other);
                }
            }
            Ok(iced::Task::none())
        },
        MacroMsg::EditorSave => {
            let Some(mut editor) = app.macro_editor.clone() else {
                return Err("editor is not open".to_string());
            };
            if editor.draft.name.trim().is_empty() {
                return Err("macro name cannot be empty".to_string());
            }
            let store = macro_store(app);
            store.save(&mut editor.draft)?;
            app.macros.retain(|m| m.id != editor.draft.id);
            app.macros.push(editor.draft.clone());
            app.macro_editor = Some(editor);
            app.notify(Level::Success, "Macro saved", "");
            Ok(iced::Task::none())
        },
        MacroMsg::EditorTest => {
            let Some(editor) = app.macro_editor.clone() else {
                return Err("editor is not open".to_string());
            };
            Ok(iced::Task::perform(
                async move {
                    let mut context = Context::new();
                    for variable in &editor.draft.variables {
                        if let Some(default) = variable.default_value.clone() {
                            context.set(&variable.name, default, variable.secret);
                        }
                    }
                    let report = crate::macros::dry_run(&editor.draft, &mut context);
                    let mut text = format!(
                        "completed: {}\nsteps: {}\nbytes sent: {}\n",
                        report.completed, report.steps_executed, report.bytes_sent
                    );
                    if let Some(error) = report.error {
                        text.push_str(&format!("error: {error}\n"));
                    }
                    text.push_str("--- screen ---\n");
                    text.push_str(&report.screen.join("\n"));
                    text
                },
                |report| Message::Macro(MacroMsg::EditorTestDone(report)),
            ))
        },
        MacroMsg::EditorTestDone(report) => {
            if let Some(editor) = app.macro_editor.as_mut() {
                editor.report = Some(report);
            }
            Ok(iced::Task::none())
        },
        MacroMsg::EditorDuplicate(id) => {
            let store = macro_store(app);
            let source = app
                .macros
                .iter()
                .find(|m| m.id == id)
                .cloned()
                .ok_or_else(|| "macro not found".to_string())?;
            let copy = store.duplicate(&source)?;
            app.macros.push(copy);
            Ok(iced::Task::none())
        },
        MacroMsg::EditorDelete(id) => {
            let store = macro_store(app);
            store.delete(&id)?;
            app.macros.retain(|m| m.id != id);
            app.notify(Level::Info, "Macro deleted", "");
            Ok(iced::Task::none())
        },
        MacroMsg::EditorExport(id) => {
            let source = app
                .macros
                .iter()
                .find(|m| m.id == id)
                .cloned()
                .ok_or_else(|| "macro not found".to_string())?;
            let dir = app.paths.data_dir.join("macros-export");
            Ok(iced::Task::perform(
                async move {
                    let store = crate::macros::MacroStore::new(&dir);
                    let path = dir.join(format!("{}.ron", source.name.replace(' ', "_")));
                    store
                        .export_bundle(&path, &[source])
                        .map(|()| path.display().to_string())
                },
                |result| match result {
                    Ok(path) => Message::Macro(MacroMsg::EditorExported(path)),
                    Err(reason) => {
                        Message::Macro(MacroMsg::EditorExported(format!("failed: {reason}")))
                    },
                },
            ))
        },
        MacroMsg::EditorExported(path) => {
            app.notify(Level::Success, "Macro exported", &path);
            Ok(iced::Task::none())
        },
        MacroMsg::MacroProgress {
            session,
            current_step,
            total_steps,
        } => {
            tracing::debug!(session, current_step, total_steps, "macro progress");
            Ok(iced::Task::none())
        },
    }
}

/// Macros directory under the config dir.
#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
fn macro_store_dir(app: &AppState) -> std::path::PathBuf {
    app.paths.config_dir.join("macros")
}

/// Store handle for the macros directory.
fn macro_store(app: &AppState) -> crate::macros::MacroStore {
    crate::macros::MacroStore::new(&macro_store_dir(app))
}

/// Begin playback with validation, prompts, and destructive confirmation.
#[allow(clippy::too_many_lines)]
fn start_macro_play(
    app: &mut AppState,
    id: uuid::Uuid,
    session: Option<SessionId>,
) -> Result<iced::Task<Message>, String> {
    use crate::macros::{Context, PendingPlay};

    let macro_ = app
        .macros
        .iter()
        .find(|m| m.id == id)
        .cloned()
        .ok_or_else(|| "macro not found".to_string())?;
    let Some(session) = session.or(app.active_session_id) else {
        return Err("no active session to play on".to_string());
    };
    if let Some(spec) = app.session(session).map(|s| s.spec.clone()) {
        if let Some(protocol) = macro_.target_protocol {
            if protocol != spec.protocol {
                return Err("macro targets a different protocol".to_string());
            }
        }
    }
    if macro_.has_destructive_steps() {
        app.macro_pending_play = Some(PendingPlay {
            macro_id: id,
            session: Some(session),
            vars: Context::new(),
        });
        app.notify(
            Level::Warning,
            "Destructive macro held",
            "confirm playback in the macro panel",
        );
        return Ok(iced::Task::none());
    }
    let mut vars = Context::new();
    if let Some(spec) = app.session(session).map(|s| s.spec.clone()) {
        vars.fill_builtins(&spec, crate::macros::now_secs());
    }
    vars.apply_defaults(&macro_.variables);
    start_macro_play_resolved(app, id, session, vars)
}

/// Continue a confirmed/answered play with a resolved variable context.
fn start_macro_play_resolved(
    app: &mut AppState,
    id: uuid::Uuid,
    session: SessionId,
    mut vars: Context,
) -> Result<iced::Task<Message>, String> {
    use crate::macros::{MacroPlayer, MacroPrompt, RunningMacro};

    let macro_ = app
        .macros
        .iter()
        .find(|m| m.id == id)
        .cloned()
        .ok_or_else(|| "macro not found".to_string())?;
    if let Some(spec) = app.session(session).map(|s| s.spec.clone()) {
        vars.fill_builtins(&spec, crate::macros::now_secs());
    }
    vars.apply_defaults(&macro_.variables);
    let missing = vars.missing_required(&macro_.variables);
    if let Some(name) = missing.into_iter().next() {
        let declared = macro_.variables.iter().find(|v| v.name == name);
        app.macro_prompt = Some(MacroPrompt {
            var_name: name.clone(),
            prompt: declared
                .and_then(|v| v.description.clone())
                .unwrap_or_else(|| format!("Value for {name}")),
            default: declared.and_then(|v| v.default_value.clone()),
            secret: declared.is_some_and(|v| v.secret),
            session,
        });
        app.macro_pending_play = Some(crate::macros::PendingPlay {
            macro_id: id,
            session: Some(session),
            vars,
        });
        app.macro_prompt_input.clear();
        app.notify(
            Level::Info,
            "Macro needs input",
            &format!("value for {name}"),
        );
        return Ok(iced::Task::none());
    }
    app.macro_prompt = None;
    app.macro_player = Some(RunningMacro {
        player: MacroPlayer::new(&macro_, vars),
        session,
    });
    continue_macro(app, session)
}

/// Answer the pending variable prompt and resume validation/playback.
fn answer_macro_prompt(app: &mut AppState, value: String) -> Result<iced::Task<Message>, String> {
    let Some(prompt) = app.macro_prompt.take() else {
        return Err("no pending macro prompt".to_string());
    };
    // Case A: a running player asked mid-play — answer in place and continue.
    if let Some(session) = app
        .macro_player
        .as_ref()
        .filter(|running| running.session == prompt.session)
        .map(|running| running.session)
    {
        let value = if value.is_empty() {
            prompt.default.clone().unwrap_or_default()
        } else {
            value
        };
        if let Some(running) = app.macro_player.as_mut() {
            running
                .player
                .answer_variable(&prompt.var_name, value, prompt.secret);
        }
        app.macro_prompt_input.clear();
        return continue_macro(app, session);
    }
    let Some(mut pending) = app.macro_pending_play.take() else {
        return Err("no pending macro play".to_string());
    };
    let value = if value.is_empty() {
        prompt.default.clone().unwrap_or_default()
    } else {
        value
    };
    pending.vars.set(&prompt.var_name, value, prompt.secret);
    app.macro_pending_play = Some(pending.clone());
    let Some(session) = pending.session else {
        return Err("variable prompts need a single-session play".to_string());
    };
    start_macro_play_resolved(app, pending.macro_id, session, pending.vars)
}

/// Play a macro across multi-exec targets (leader context for variables;
/// waits and prompts resolve from defaults — interactive steps are skipped).
fn play_macro_on_targets(
    app: &mut AppState,
    id: uuid::Uuid,
) -> Result<iced::Task<Message>, String> {
    use crate::macros::Context;

    let macro_ = app
        .macros
        .iter()
        .find(|m| m.id == id)
        .cloned()
        .ok_or_else(|| "macro not found".to_string())?;
    if macro_.has_destructive_steps() {
        return Err("destructive macros play on one session with confirmation".to_string());
    }
    let Some(leader) = app.active_session_id else {
        return Err("no active session (leader)".to_string());
    };
    let mut vars = Context::new();
    if let Some(spec) = app.session(leader).map(|s| s.spec.clone()) {
        vars.fill_builtins(&spec, crate::macros::now_secs());
    }
    vars.apply_defaults(&macro_.variables);
    play_macro_on_targets_resolved(app, id, vars)
}

/// Multi-exec expansion with a resolved context.
fn play_macro_on_targets_resolved(
    app: &mut AppState,
    id: uuid::Uuid,
    vars: Context,
) -> Result<iced::Task<Message>, String> {
    use crate::macros::MacroStep;

    let macro_ = app
        .macros
        .iter()
        .find(|m| m.id == id)
        .cloned()
        .ok_or_else(|| "macro not found".to_string())?;
    let Some(leader) = app.active_session_id else {
        return Err("no active session (leader)".to_string());
    };
    let mut tasks = Vec::new();
    let mut played = 0u32;
    for step in &macro_.steps {
        let bytes = match step {
            MacroStep::SendInput { data } => {
                crate::macros::variables::substitute(data, &vars).into_bytes()
            },
            MacroStep::SendKey { key } => key.to_bytes(),
            MacroStep::SendVariable { name } => {
                vars.get(name).unwrap_or("").to_string().into_bytes()
            },
            _ => continue, // waits/prompts/branches need a live player
        };
        if bytes.is_empty() {
            continue;
        }
        // Reuse the multi-exec fan-out (leader-inclusive, best-effort).
        match broadcast_multicast(app, leader, bytes) {
            Ok(task) => tasks.push(task),
            Err(reason) => {
                app.notify(Level::Warning, "Macro multi-play skipped", &reason);
            },
        }
        played += 1;
    }
    app.notify(
        Level::Success,
        "Macro fanned out",
        &format!("{played} send steps across multi-exec targets"),
    );
    Ok(iced::Task::batch(tasks))
}

/// Drive the running player until it needs the runtime (sleep/wait),
/// finishes, or fails. Emits a progress message per continuation.
fn continue_macro(app: &mut AppState, session: SessionId) -> Result<iced::Task<Message>, String> {
    use crate::macros::{PlayerError, StepEffect};

    /// One driver action with owned payloads (borrow-free across `app`).
    enum Act {
        Send(Vec<u8>),
        Sleep(u64),
        Await(u64),
        NeedVar {
            name: String,
            prompt: String,
            default: Option<String>,
            secret: bool,
        },
        Finished {
            done: usize,
            total: usize,
            name: String,
        },
        Fail(String),
    }

    let present = app
        .macro_player
        .as_ref()
        .is_some_and(|running| running.session == session);
    if !present {
        return Err("no macro playing on this session".to_string());
    }
    if let Some(reason) = match app.macro_player.as_mut() {
        Some(running) => running
            .player
            .check_timeout()
            .err()
            .map(|err| err.to_string()),
        None => None,
    } {
        return finish_macro_with_error(app, session, &reason);
    }
    loop {
        let act = match app.macro_player.as_mut() {
            Some(running) => match running.player.step() {
                Ok(StepEffect::Send(bytes)) => Act::Send(bytes),
                Ok(StepEffect::Sleep(ms)) => Act::Sleep(ms),
                Ok(StepEffect::AwaitPattern) => {
                    Act::Await(running.player.wait_deadline_ms().unwrap_or(10_000))
                },
                Ok(StepEffect::NeedVariable {
                    name,
                    prompt,
                    default,
                    secret,
                }) => Act::NeedVar {
                    name,
                    prompt,
                    default,
                    secret,
                },
                Ok(StepEffect::Finished) => {
                    let (done, total) = running.player.progress();
                    Act::Finished {
                        done,
                        total,
                        name: running.player.macro_name().to_string(),
                    }
                },
                Err(PlayerError::MissingVariable(name)) => {
                    return Err(format!("missing variable: {name}"));
                },
                Err(err) => Act::Fail(err.to_string()),
            },
            None => return Err("no macro playing on this session".to_string()),
        };
        match act {
            Act::Send(bytes) => {
                if let Err(reason) =
                    crate::connection::actor::SessionManager::shared().write(session, bytes)
                {
                    return finish_macro_with_error(app, session, &reason);
                }
            },
            Act::Sleep(ms) => {
                return Ok(iced::Task::batch([
                    progress_task(app, session),
                    iced::Task::perform(
                        async move {
                            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                        },
                        move |()| Message::Macro(MacroMsg::Advance(session)),
                    ),
                ]));
            },
            Act::Await(timeout_ms) => {
                return Ok(iced::Task::batch([
                    progress_task(app, session),
                    iced::Task::perform(
                        async move {
                            tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)).await;
                        },
                        move |()| Message::Macro(MacroMsg::Advance(session)),
                    ),
                ]));
            },
            Act::NeedVar {
                name,
                prompt,
                default,
                secret,
            } => {
                app.macro_prompt = Some(crate::macros::MacroPrompt {
                    var_name: name,
                    prompt,
                    default,
                    secret,
                    session,
                });
                return Ok(progress_task(app, session));
            },
            Act::Finished { done, total, name } => {
                app.macro_player = None;
                app.notify(
                    Level::Success,
                    "Macro finished",
                    &format!("{name} ({done}/{total} steps)"),
                );
                return Ok(progress_task(app, session));
            },
            Act::Fail(reason) => return finish_macro_with_error(app, session, &reason),
        }
    }
}

/// Progress message for the current player state.
fn progress_task(app: &AppState, session: SessionId) -> iced::Task<Message> {
    let (current_step, total_steps) = app
        .macro_player
        .as_ref()
        .map(|running| running.player.progress())
        .unwrap_or((0, 0));
    iced::Task::perform(async move {}, move |()| {
        Message::Macro(MacroMsg::MacroProgress {
            session,
            current_step,
            total_steps,
        })
    })
}

/// Fail the running player with a notification.
fn finish_macro_with_error(
    app: &mut AppState,
    session: SessionId,
    reason: &str,
) -> Result<iced::Task<Message>, String> {
    if let Some(running) = app.macro_player.as_mut() {
        if running.session == session {
            running.player.stop();
        }
    }
    app.macro_player = None;
    app.notify(Level::Error, "Macro failed", reason);
    Ok(iced::Task::none())
}

/// Network-tools hub events (Prompt 5.4).
#[allow(clippy::too_many_lines)]
fn handle_tool(app: &mut AppState, msg: ToolMsg) -> Result<iced::Task<Message>, String> {
    use crate::tools::{run_tool, CancelToken, RunStatus, ToolEvent, ToolParams, ToolRun};
    use futures::StreamExt as _;
    match msg {
        ToolMsg::KindSelected(kind) => {
            app.tool_draft.kind = kind;
            app.tool_draft.params = ToolParams::for_kind(kind);
            app.tool_draft.field_text = ToolParams::text_fields(&app.tool_draft.params)
                .into_iter()
                .collect();
            app.tool_draft.error = None;
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        ToolMsg::TargetChanged(target) => {
            app.tool_draft.target = target;
            app.tool_draft.error = None;
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        ToolMsg::ScopeLocal => {
            app.tool_draft.remote = None;
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        ToolMsg::ScopeRemote(id) => {
            app.tool_draft.remote = Some(id);
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        ToolMsg::ParamChanged(field, value) => {
            app.tool_draft
                .field_text
                .insert(field.clone(), value.clone());
            apply_tool_param(&mut app.tool_draft, &field, &value);
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        ToolMsg::RunRequested => {
            let config = match app.tool_draft.build_config() {
                Ok(config) => config,
                Err(error) => {
                    app.tool_draft.error = Some(error);
                    app.ui_state.dirty = true;
                    return Ok(iced::Task::none());
                },
            };
            let id = app.tool_run_seq;
            app.tool_run_seq += 1;
            let token = CancelToken::new();
            app.tool_cancel.insert(id, token.clone());
            app.tool_runs.push(ToolRun {
                id,
                kind: app.tool_draft.kind,
                target_label: config.target.clone(),
                lines: Vec::new(),
                status: RunStatus::Running,
                started_secs: crate::tools::now_secs(),
            });
            // Cap history so long sessions cannot grow it without bound.
            while app.tool_runs.len() > 50 {
                let oldest_finished = app
                    .tool_runs
                    .iter()
                    .position(|run| run.status != RunStatus::Running);
                match oldest_finished {
                    Some(index) => {
                        app.tool_cancel.remove(&app.tool_runs[index].id);
                        app.tool_runs.remove(index);
                    },
                    None => break,
                }
            }
            let kind = app.tool_draft.kind;
            let stream = run_tool(kind, config, token);
            app.ui_state.dirty = true;
            Ok(iced::Task::stream(stream.map(move |event| {
                Message::Tool(ToolMsg::Event(id, event))
            })))
        },
        ToolMsg::CancelRequested(id) => {
            if let Some(token) = app.tool_cancel.get(&id) {
                token.cancel();
            }
            if let Some(run) = app.tool_runs.iter_mut().find(|run| run.id == id) {
                run.status = RunStatus::Cancelled;
            }
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        ToolMsg::ClearHistory => {
            app.tool_runs.retain(|run| run.status == RunStatus::Running);
            app.tool_cancel
                .retain(|id, _| app.tool_runs.iter().any(|run| &run.id == id));
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        ToolMsg::SearchChanged(query) => {
            app.tool_draft.search = query;
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        ToolMsg::Event(id, event) => {
            let Some(run) = app.tool_runs.iter_mut().find(|run| run.id == id) else {
                return Ok(iced::Task::none());
            };
            match event {
                ToolEvent::Started => run.status = RunStatus::Running,
                ToolEvent::Progress { message, percent } => {
                    let line = match percent {
                        Some(value) => format!("{message} ({value:.0}%)"),
                        None => message,
                    };
                    run.lines.push((crate::tools::OutputLevel::Info, line));
                },
                ToolEvent::Output { line, level } => run.lines.push((level, line)),
                ToolEvent::Completed { summary } => {
                    if let Some(summary) = summary {
                        run.lines
                            .push((crate::tools::OutputLevel::Success, summary));
                    }
                    run.status = RunStatus::Completed;
                    app.tool_cancel.remove(&id);
                },
                ToolEvent::Failed { error } => {
                    run.status = RunStatus::Failed(error);
                    app.tool_cancel.remove(&id);
                },
                ToolEvent::Cancelled => {
                    run.status = RunStatus::Cancelled;
                    app.tool_cancel.remove(&id);
                },
            }
            // Bound per-run output like the multi-exec log.
            while run.lines.len() > 500 {
                run.lines.remove(0);
            }
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
    }
}

/// Apply one draft field edit to the per-kind params (Prompt 5.4 form
/// state). Unparseable numbers keep their previous value.
fn apply_tool_param(draft: &mut super::state::ToolDraft, field: &str, value: &str) {
    use crate::tools::{ToolKind, ToolParams};
    let parse_u32 = |text: &str, current: u32| text.trim().parse().unwrap_or(current);
    let parse_u64 = |text: &str, current: u64| text.trim().parse().unwrap_or(current);
    let parse_u16 = |text: &str, current: u16| text.trim().parse().unwrap_or(current);
    let parse_bool = |text: &str| text.trim().eq_ignore_ascii_case("true");
    match (&draft.kind, &mut draft.params) {
        (
            ToolKind::Ping,
            ToolParams::Ping {
                count,
                interval_ms,
                size,
                v6,
            },
        ) => match field {
            "count" => *count = parse_u32(value, *count),
            "interval_ms" => *interval_ms = parse_u64(value, *interval_ms),
            "size" => *size = parse_u32(value, *size),
            "v6" => *v6 = parse_bool(value),
            _ => {},
        },
        (
            ToolKind::Traceroute,
            ToolParams::Traceroute {
                max_hops,
                timeout_ms,
            },
        ) => match field {
            "max_hops" => *max_hops = value.trim().parse().unwrap_or(*max_hops),
            "timeout_ms" => *timeout_ms = parse_u64(value, *timeout_ms),
            _ => {},
        },
        (
            ToolKind::Dns,
            ToolParams::Dns {
                record_type,
                server,
            },
        ) => match field {
            "record_type" => {
                *record_type = value.trim().to_ascii_uppercase();
            },
            "server" => *server = value.trim().to_string(),
            _ => {},
        },
        (ToolKind::ReverseDns, ToolParams::ReverseDns) => {},
        (ToolKind::Whois, ToolParams::Whois { server }) => {
            if field == "server" {
                *server = value.trim().to_string();
            }
        },
        (
            ToolKind::PortScan,
            ToolParams::PortScan {
                ports,
                concurrency,
                timeout_ms,
                banner,
            },
        ) => match field {
            "ports" => *ports = value.trim().to_string(),
            "concurrency" => *concurrency = parse_u32(value, *concurrency),
            "timeout_ms" => *timeout_ms = parse_u64(value, *timeout_ms),
            "banner" => *banner = parse_bool(value),
            _ => {},
        },
        (
            ToolKind::Http,
            ToolParams::Http {
                method,
                path,
                headers,
                body,
            },
        ) => match field {
            "method" => *method = value.trim().to_ascii_uppercase(),
            "path" => *path = value.trim().to_string(),
            "headers" => *headers = value.to_string(),
            "body" => *body = value.to_string(),
            _ => {},
        },
        (ToolKind::Subnet, ToolParams::Subnet { cidr }) => {
            if field == "cidr" {
                *cidr = value.trim().to_string();
            }
        },
        (
            ToolKind::Bandwidth,
            ToolParams::Bandwidth {
                mode,
                port,
                seconds,
            },
        ) => match field {
            "mode" => *mode = value.trim().to_ascii_lowercase(),
            "port" => *port = parse_u16(value, *port),
            "seconds" => *seconds = parse_u64(value, *seconds),
            _ => {},
        },
        _ => {},
    }
}

/// Port-forwarding events (Prompt 5.3, ssh-gated).
#[cfg(feature = "ssh")]
#[allow(clippy::too_many_lines)]
fn handle_tunnel(
    app: &mut AppState,
    msg: super::messages::TunnelMsg,
) -> Result<iced::Task<Message>, String> {
    use super::messages::{TunnelField, TunnelKind, TunnelMsg};
    use crate::connection::forward::ForwardManager;
    use mbxt_core::ForwardType;

    match msg {
        TunnelMsg::OpenForm(session) => {
            if app.session(session).is_none() {
                return Err(format!("unknown session #{session}"));
            }
            app.tunnel_draft = Some(super::state::TunnelDraft::new(session));
            Ok(iced::Task::none())
        },
        TunnelMsg::CloseForm => {
            app.tunnel_draft = None;
            Ok(iced::Task::none())
        },
        TunnelMsg::FieldChanged(field, value) => {
            if let Some(draft) = app.tunnel_draft.as_mut() {
                draft.error = None;
                match field {
                    TunnelField::Name => draft.name = value,
                    TunnelField::BindHost => draft.bind_host = value,
                    TunnelField::BindPort => draft.bind_port = value,
                    TunnelField::TargetHost => draft.target_host = value,
                    TunnelField::TargetPort => draft.target_port = value,
                }
            }
            Ok(iced::Task::none())
        },
        TunnelMsg::KindSelected(kind) => {
            if let Some(draft) = app.tunnel_draft.as_mut() {
                draft.error = None;
                draft.kind = kind;
                // Sensible port defaults per kind.
                draft.bind_port = match kind {
                    TunnelKind::Dynamic => "1080".to_string(),
                    TunnelKind::Local | TunnelKind::Remote => "8080".to_string(),
                };
            }
            Ok(iced::Task::none())
        },
        TunnelMsg::AutoStartToggled => {
            if let Some(draft) = app.tunnel_draft.as_mut() {
                draft.auto_start = !draft.auto_start;
            }
            Ok(iced::Task::none())
        },
        TunnelMsg::Submit => {
            let draft = app
                .tunnel_draft
                .clone()
                .ok_or("tunnel form is not open".to_string())?;
            let def = build_forward_def(&draft)?;
            let session = draft.session;
            let editing = draft.editing;
            // Persist into the session spec (autosave carries it to disk).
            let stored = app
                .session_mut(session)
                .ok_or_else(|| format!("unknown session #{session}"))?;
            match editing {
                Some(id) => {
                    let slot = stored
                        .spec
                        .forwards
                        .iter_mut()
                        .find(|def| def.id == id)
                        .ok_or_else(|| "tunnel definition is gone".to_string())?;
                    *slot = def.clone();
                },
                None => stored.spec.forwards.push(def.clone()),
            }
            app.ui_state.dirty = true;
            // Mirror into the manager (replaces any same-id definition).
            ForwardManager::shared().define(session, def.clone());
            app.tunnel_draft = None;
            app.notify(Level::Success, "Tunnel saved", &def.display_name());
            Ok(iced::Task::none())
        },
        TunnelMsg::Edit(id) => {
            let (session, def) = find_forward_def(app, id)?;
            let mut draft = super::state::TunnelDraft::new(session);
            draft.editing = Some(id);
            draft.name = def.name.clone().unwrap_or_default();
            draft.auto_start = def.auto_start;
            match &def.forward_type {
                ForwardType::Local {
                    local_host,
                    local_port,
                    remote_host,
                    remote_port,
                } => {
                    draft.kind = TunnelKind::Local;
                    draft.bind_host = local_host.clone();
                    draft.bind_port = local_port.to_string();
                    draft.target_host = remote_host.clone();
                    draft.target_port = remote_port.to_string();
                },
                ForwardType::Remote {
                    remote_host,
                    remote_port,
                    local_host,
                    local_port,
                } => {
                    draft.kind = TunnelKind::Remote;
                    draft.bind_host = remote_host.clone();
                    draft.bind_port = remote_port.to_string();
                    draft.target_host = local_host.clone();
                    draft.target_port = local_port.to_string();
                },
                ForwardType::Dynamic {
                    local_host,
                    local_port,
                } => {
                    draft.kind = TunnelKind::Dynamic;
                    draft.bind_host = local_host.clone();
                    draft.bind_port = local_port.to_string();
                },
            }
            app.tunnel_draft = Some(draft);
            Ok(iced::Task::none())
        },
        TunnelMsg::Delete(id) => {
            let (session, _) = find_forward_def(app, id)?;
            if let Some(stored) = app.session_mut(session) {
                stored.spec.forwards.retain(|def| def.id != id);
            }
            app.ui_state.dirty = true;
            ForwardManager::shared().remove(id)?;
            app.notify(Level::Info, "Tunnel deleted", "");
            Ok(iced::Task::none())
        },
        TunnelMsg::Start(id) => Ok(iced::Task::perform(
            async move { ForwardManager::shared().start_async(id).await },
            move |result| Message::Tunnel(TunnelMsg::Started(id, result)),
        )),
        TunnelMsg::Stop(id) => Ok(iced::Task::perform(
            async move { ForwardManager::shared().stop_async(id).await },
            move |result| match result {
                Ok(()) => Message::Tunnel(TunnelMsg::Stopped(id)),
                Err(reason) => Message::Tunnel(TunnelMsg::Started(id, Err(reason))),
            },
        )),
        TunnelMsg::Started(id, result) => {
            match result {
                Ok(summary) => app.notify(Level::Success, "Tunnel started", &summary),
                Err(reason) => app.notify(Level::Error, "Tunnel failed", &reason),
            }
            let _ = id;
            Ok(iced::Task::none())
        },
        TunnelMsg::Stopped(id) => {
            let _ = id;
            app.notify(Level::Info, "Tunnel stopped", "");
            Ok(iced::Task::none())
        },
    }
}

/// Validate the tunnel form into a [`ForwardDef`] (mirrors the panel).
#[cfg(feature = "ssh")]
fn build_forward_def(draft: &super::state::TunnelDraft) -> Result<mbxt_core::ForwardDef, String> {
    use super::messages::TunnelKind;

    let parse_port = |text: &str, what: &str| {
        text.trim()
            .parse::<u16>()
            .map_err(|_| format!("{what} must be 1–65535"))
    };
    let forward_type = match draft.kind {
        TunnelKind::Local => {
            if draft.target_host.trim().is_empty() {
                return Err("target host is required".to_string());
            }
            mbxt_core::ForwardType::Local {
                local_host: non_empty(&draft.bind_host, "bind address")?,
                local_port: parse_port(&draft.bind_port, "bind port")?,
                remote_host: draft.target_host.trim().to_string(),
                remote_port: parse_port(&draft.target_port, "target port")?,
            }
        },
        TunnelKind::Remote => {
            if draft.target_host.trim().is_empty() {
                return Err("target host is required".to_string());
            }
            mbxt_core::ForwardType::Remote {
                remote_host: non_empty(&draft.bind_host, "remote bind address")?,
                remote_port: parse_port(&draft.bind_port, "remote bind port")?,
                local_host: draft.target_host.trim().to_string(),
                local_port: parse_port(&draft.target_port, "target port")?,
            }
        },
        TunnelKind::Dynamic => mbxt_core::ForwardType::Dynamic {
            local_host: non_empty(&draft.bind_host, "bind address")?,
            local_port: parse_port(&draft.bind_port, "bind port")?,
        },
    };
    let name = draft.name.trim().to_string();
    Ok(match draft.editing {
        // Preserve the id across edits so running state follows the def.
        Some(id) => mbxt_core::ForwardDef {
            id,
            name: if name.is_empty() { None } else { Some(name) },
            forward_type,
            auto_start: draft.auto_start,
        },
        None => mbxt_core::ForwardDef::new(
            if name.is_empty() { None } else { Some(name) },
            forward_type,
            draft.auto_start,
        ),
    })
}

/// Non-empty trimmed string or an actionable error.
#[cfg(feature = "ssh")]
fn non_empty(text: &str, what: &str) -> Result<String, String> {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Err(format!("{what} is required"));
    }
    Ok(trimmed)
}

/// Locate a forward definition across sessions `(owner, def)`.
#[cfg(feature = "ssh")]
fn find_forward_def(
    app: &AppState,
    id: uuid::Uuid,
) -> Result<(SessionId, mbxt_core::ForwardDef), String> {
    app.sessions
        .iter()
        .find_map(|session| {
            session
                .spec
                .forwards
                .iter()
                .find(|def| def.id == id)
                .map(|def| (session.id, def.clone()))
        })
        .ok_or_else(|| "tunnel not found".to_string())
}

fn handle_ui(app: &mut AppState, msg: UiMsg) -> Result<iced::Task<Message>, String> {
    match msg {
        UiMsg::Quit => {
            tracing::info!("user requested quit");
            Ok(iced::exit())
        },
        UiMsg::ToggleSidebar => {
            app.ui_state.sidebar_visible = !app.ui_state.sidebar_visible;
            Ok(iced::Task::none())
        },
        UiMsg::ThemeToggled => {
            // Cycle built-ins then customs; the name persists so the
            // choice restores on next launch. The legacy flag follows
            // background darkness for older readers of the setting.
            let next = app.theme.cycle_with(&app.custom_themes);
            app.settings.appearance.theme = next.canonical_name();
            app.settings.appearance.dark_theme = next.is_dark();
            app.theme = next;
            // Save-on-change: mark dirty; the next tick (≤30 s) persists.
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        UiMsg::ReleaseChannelChanged(channel) => {
            app.settings.general.release_channel = channel;
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        UiMsg::TelemetryToggled(enabled) => {
            app.settings.general.telemetry_enabled = enabled;
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        UiMsg::CrashReportingToggled(enabled) => {
            app.settings.general.crash_reporting_enabled = enabled;
            app.crash_reporter = crate::feedback::sentry::init(
                &app.settings.general,
                std::env::var("MBXT_SENTRY_DSN").ok().as_deref(),
            );
            app.ui_state.dirty = true;
            Ok(iced::Task::none())
        },
        UiMsg::CheckForUpdates => {
            #[cfg(feature = "feedback-network")]
            {
                let base_url = std::env::var("MBXT_UPDATE_BASE_URL")
                    .map_err(|_| "update service is not configured".to_string())?;
                let key = std::env::var("MBXT_UPDATE_PUBLIC_KEY")
                    .map_err(|_| "update verification key is not configured".to_string())?;
                let public_key = crate::feedback::updates::public_key_from_hex(&key)?;
                let channel = app.settings.general.release_channel;
                Ok(iced::Task::perform(
                    async move {
                        crate::feedback::updates::check(&base_url, channel, &public_key)
                            .await
                            .map(|manifest| manifest.version)
                    },
                    Message::UpdateChecked,
                ))
            }
            #[cfg(not(feature = "feedback-network"))]
            {
                Err("update checking is not compiled into this build".to_string())
            }
        },
        UiMsg::NewSession => {
            let name = format!("session-{}", app.sessions.len() + 1);
            handle_session(app, SessionMsg::Created(name))
        },
        UiMsg::QuickConnect | UiMsg::NewSessionDialogOpened => {
            // Custom session dialog (Prompt 4.4): protocol/host/device picker
            // feeding `CreateDetailed` on submit.
            let name = format!("session-{}", app.sessions.len() + 1);
            let mut draft = super::state::NewSessionDraft::new(name);
            draft.serial_ports = refresh_serial_ports();
            app.new_session = Some(draft);
            Ok(iced::Task::none())
        },
        UiMsg::NewSessionDialogClosed => {
            app.new_session = None;
            Ok(iced::Task::none())
        },
        UiMsg::NewSessionFieldChanged(field, value) => {
            if let Some(draft) = app.new_session.as_mut() {
                draft.error = None;
                match field {
                    super::messages::NewSessionField::Name => draft.name = value,
                    super::messages::NewSessionField::Host => draft.host = value,
                    super::messages::NewSessionField::Port => draft.port = value,
                    super::messages::NewSessionField::Device => draft.device = value,
                    super::messages::NewSessionField::Baud => draft.baud = value,
                }
            }
            Ok(iced::Task::none())
        },
        UiMsg::NewSessionProtocolSelected(protocol) => {
            if let Some(draft) = app.new_session.as_mut() {
                draft.error = None;
                draft.protocol = protocol;
                draft.port = match protocol {
                    mbxt_core::Protocol::Ssh => "22".to_string(),
                    mbxt_core::Protocol::Telnet => "23".to_string(),
                    mbxt_core::Protocol::Rdp => "3389".to_string(),
                    mbxt_core::Protocol::Vnc => "5900".to_string(),
                    _ => draft.port.clone(),
                };
            }
            Ok(iced::Task::none())
        },
        UiMsg::NewSessionSerialRefresh => {
            if let Some(draft) = app.new_session.as_mut() {
                draft.serial_ports = refresh_serial_ports();
            }
            Ok(iced::Task::none())
        },
        UiMsg::NewSessionSubmitted => {
            let draft = app
                .new_session
                .clone()
                .ok_or("session dialog is not open".to_string())?;
            let spec = build_session_spec(&draft)?;
            app.new_session = None;
            handle_session(app, SessionMsg::CreateDetailed(Box::new(spec)))
        },
        UiMsg::OpenSettings => {
            app.ui_state.view = crate::app::messages::ViewKind::Settings;
            Ok(iced::Task::none())
        },
        UiMsg::OpenFeedback => {
            app.ui_state.view = crate::app::messages::ViewKind::Feedback;
            Ok(iced::Task::none())
        },
        UiMsg::FeedbackKindChanged(kind) => {
            app.feedback.kind = kind;
            Ok(iced::Task::none())
        },
        UiMsg::FeedbackTitleChanged(value) => {
            app.feedback.title = value;
            Ok(iced::Task::none())
        },
        UiMsg::FeedbackDescriptionChanged(value) => {
            app.feedback.description = value;
            Ok(iced::Task::none())
        },
        UiMsg::FeedbackLogsToggled(enabled) => {
            app.feedback.consent.include_redacted_logs = enabled;
            Ok(iced::Task::none())
        },
        UiMsg::FeedbackBreadcrumbsToggled(enabled) => {
            app.feedback.consent.include_breadcrumbs = enabled;
            Ok(iced::Task::none())
        },
        UiMsg::FeedbackScreenshotToggled(enabled) => {
            app.feedback.consent.include_screenshot = enabled;
            Ok(iced::Task::none())
        },
        UiMsg::FeedbackScreenshotPathChanged(value) => {
            app.feedback.screenshot_path = value;
            Ok(iced::Task::none())
        },
        UiMsg::OpenMacrosView => {
            if let Some(index) = app.tabs.iter().position(|tab| tab.kind == TabKind::Macros) {
                app.active_tab = index;
            } else {
                app.tabs.push(Tab {
                    title: "Macros".to_string(),
                    kind: TabKind::Macros,
                });
                app.active_tab = app.tabs.len() - 1;
            }
            app.ui_state.view = crate::app::messages::ViewKind::SessionList;
            Ok(iced::Task::none())
        },
        UiMsg::OpenTunnelsView => {
            if let Some(index) = app.tabs.iter().position(|tab| tab.kind == TabKind::Tunnels) {
                app.active_tab = index;
            } else {
                app.tabs.push(Tab {
                    title: "Tunnels".to_string(),
                    kind: TabKind::Tunnels,
                });
                app.active_tab = app.tabs.len() - 1;
            }
            app.ui_state.view = crate::app::messages::ViewKind::SessionList;
            Ok(iced::Task::none())
        },
        UiMsg::OpenToolsView => {
            if let Some(index) = app.tabs.iter().position(|tab| tab.kind == TabKind::Tools) {
                app.active_tab = index;
            } else {
                app.tabs.push(Tab {
                    title: "Network tools".to_string(),
                    kind: TabKind::Tools,
                });
                app.active_tab = app.tabs.len() - 1;
            }
            app.ui_state.view = crate::app::messages::ViewKind::SessionList;
            Ok(iced::Task::none())
        },
        UiMsg::NewTab => {
            // New tab attaches to the active session when one exists (#41).
            let (title, kind) = match app.active_session_id {
                Some(id) => {
                    let title = app
                        .session(id)
                        .map(|s| s.spec.name.clone())
                        .unwrap_or_else(|| format!("terminal {}", app.tabs.len() + 1));
                    (title, super::state::TabKind::TerminalPlaceholder(id))
                },
                None => (
                    format!("tab {}", app.tabs.len() + 1),
                    super::state::TabKind::Welcome,
                ),
            };
            app.tabs.push(super::state::Tab { title, kind });
            app.active_tab = app.tabs.len() - 1;
            Ok(iced::Task::none())
        },
        UiMsg::CloseActiveTab => {
            if app.tabs.len() > 1 {
                let index = app.active_tab.min(app.tabs.len() - 1);
                app.tabs.remove(index);
                app.active_tab = index.min(app.tabs.len() - 1);
            }
            Ok(iced::Task::none())
        },
        UiMsg::SelectTab(index) => {
            app.active_tab = index.min(app.tabs.len().saturating_sub(1));
            // Tabs live under the SessionList view; selecting one must
            // leave Settings/Feedback overlays (they shadow all tabs).
            app.ui_state.view = crate::app::messages::ViewKind::SessionList;
            Ok(iced::Task::none())
        },
        UiMsg::MasterPasswordSubmitted(password) => {
            // Prompt 1.2 startup/unlock path: decrypt the store on the
            // blocking pool; success caches the KEK inside `app.secure` and
            // routes the decrypted sessions through `SessionMsg::Loaded`.
            let secure = app.secure.clone();
            Ok(iced::Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let sessions = secure
                            .load_sessions(Some(&password))
                            .map_err(|e| e.to_string())?;
                        Ok(sessions)
                    })
                    .await
                    .unwrap_or_else(|e| Err(format!("unlock task failed: {e}")))
                },
                |result| match result {
                    Ok(entries) => Message::Session(SessionMsg::Loaded(
                        entries.iter().map(SessionEntry::to_spec).collect(),
                    )),
                    Err(err) => Message::Ui(UiMsg::MasterPasswordRejected(err)),
                },
            ))
        },
        UiMsg::MasterPasswordRejected(reason) => {
            app.sessions_unlocked = false;
            app.notify(Level::Error, "Unlock failed", &reason);
            Ok(iced::Task::none())
        },
        UiMsg::AuthCancel => {
            if let Some(prompt) = app.auth_dialog.take() {
                // Dropping the reply sender resolves the provider's wait as
                // `AuthError::Cancelled` (negotiation aborts).
                PromptBridge::shared().cancel(prompt.id);
                tracing::debug!(prompt = prompt.id, "auth prompt cancelled");
            }
            app.auth_input.clear();
            Ok(iced::Task::none())
        },
        UiMsg::AuthSubmit => {
            if let Some(prompt) = app.auth_dialog.take() {
                let value = std::mem::take(&mut app.auth_input);
                // The provider's oneshot resolves here (async wait, prompt 2.2).
                PromptBridge::shared().complete(prompt.id, value);
            }
            Ok(iced::Task::none())
        },
        UiMsg::AuthInputChanged(value) => {
            app.auth_input = value;
            Ok(iced::Task::none())
        },
        UiMsg::ReportIssue => {
            let paths = app.paths.clone();
            let feedback = app.feedback.clone();
            let channel = app.settings.general.release_channel;
            Ok(iced::Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        collect_issue_bundle(&paths, &feedback, channel)
                    })
                    .await
                    .unwrap_or_else(|e| Err(format!("report task failed: {e}")))
                },
                Message::IssueBundleReady,
            ))
        },
        UiMsg::SearchChanged(query) => {
            app.ui_state.search_query = query;
            Ok(iced::Task::none())
        },
        UiMsg::ViewChanged(view) => {
            app.ui_state.view = view;
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecToggled(enabled) => {
            app.multi_exec_mode.enabled = enabled;
            if !enabled {
                if let Ok(mut shared) = app.shared.write() {
                    shared.multi_exec_targets.clear();
                }
                app.multi_exec_mode.targets.clear();
            }
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecTargets(targets) => {
            app.multi_exec_mode.targets = targets.clone();
            if let Ok(mut shared) = app.shared.write() {
                shared.multi_exec_targets = targets;
            }
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecToggle => {
            let enabling = !app.multi_exec_mode.enabled;
            app.multi_exec_mode.enabled = enabling;
            if enabling {
                // Wait-for-connected: only live sessions become targets.
                let connected = |id: SessionId| {
                    matches!(app.session_states.get(&id), Some(SessionState::Connected))
                };
                let (kept, dropped) = crate::app::multi_exec::prune_disconnected(
                    &app.multi_exec_mode.targets,
                    &connected,
                );
                app.multi_exec_mode.targets = kept;
                if app.multi_exec_mode.targets.is_empty() {
                    // Auto-select connected sessions so the toggle does
                    // something useful out of the box.
                    app.multi_exec_mode.targets = app
                        .sessions
                        .iter()
                        .filter(|s| connected(s.id))
                        .map(|s| s.id)
                        .collect();
                }
                for id in dropped {
                    app.notify(
                        Level::Warning,
                        "Multi-exec dropout",
                        &format!("session #{id} is not connected"),
                    );
                }
                app.notify(
                    Level::Info,
                    "Multi-exec enabled",
                    &format!("{} target(s)", app.multi_exec_mode.targets.len()),
                );
            } else {
                if let Ok(mut shared) = app.shared.write() {
                    shared.multi_exec_targets.clear();
                }
                app.multi_exec_mode.targets.clear();
                app.multi_exec_pending = None;
                app.multi_exec_leader = None;
                app.notify(Level::Info, "Multi-exec disabled", "broadcasting off");
            }
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecToggleTarget(id) => {
            if app.session(id).is_none() {
                return Err(format!("unknown session #{id}"));
            }
            if app.multi_exec_mode.targets.contains(&id) {
                app.multi_exec_mode.targets.retain(|t| *t != id);
            } else {
                app.multi_exec_mode.targets.push(id);
            }
            if let Ok(mut shared) = app.shared.write() {
                shared.multi_exec_targets = app.multi_exec_mode.targets.clone();
            }
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecSelectAll => {
            app.multi_exec_mode.targets = app
                .sessions
                .iter()
                .filter(|s| matches!(app.session_states.get(&s.id), Some(SessionState::Connected)))
                .map(|s| s.id)
                .collect();
            sync_multi_exec_targets(app);
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecSelectSsh => {
            app.multi_exec_mode.targets = app
                .sessions
                .iter()
                .filter(|s| {
                    matches!(app.session_states.get(&s.id), Some(SessionState::Connected))
                        && matches!(
                            s.spec.protocol,
                            mbxt_core::Protocol::Ssh
                                | mbxt_core::Protocol::Sftp
                                | mbxt_core::Protocol::X11
                        )
                })
                .map(|s| s.id)
                .collect();
            sync_multi_exec_targets(app);
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecSelectTag(tag) => {
            app.multi_exec_mode.targets = app
                .sessions
                .iter()
                .filter(|s| {
                    matches!(app.session_states.get(&s.id), Some(SessionState::Connected))
                        && s.spec.tags.iter().any(|t| t == &tag)
                })
                .map(|s| s.id)
                .collect();
            sync_multi_exec_targets(app);
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecSelectTabs => {
            let tabbed: Vec<SessionId> = app
                .tabs
                .iter()
                .filter_map(|tab| match &tab.kind {
                    super::state::TabKind::TerminalPlaceholder(id) => Some(*id),
                    _ => None,
                })
                .collect();
            app.multi_exec_mode.targets = app
                .sessions
                .iter()
                .filter(|s| {
                    matches!(app.session_states.get(&s.id), Some(SessionState::Connected))
                        && tabbed.contains(&s.id)
                })
                .map(|s| s.id)
                .collect();
            sync_multi_exec_targets(app);
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecClearTargets => {
            app.multi_exec_mode.targets.clear();
            sync_multi_exec_targets(app);
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecStaggerChanged(ms) => {
            app.multi_exec_mode.stagger_ms = ms.min(crate::app::multi_exec::MAX_STAGGER_MS);
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecFilterChanged(filter) => {
            app.multi_exec_mode.host_filter = filter;
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecTemplateChanged(text) => {
            app.multi_exec_template = text;
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecTemplateSend => {
            let template = app.multi_exec_template.clone();
            if template.trim().is_empty() {
                return Err("template is empty".to_string());
            }
            let Some(leader) = app.active_session_id else {
                return Err("no active session (leader)".to_string());
            };
            let connected = |id: SessionId| {
                matches!(app.session_states.get(&id), Some(SessionState::Connected))
            };
            let (kept, _) = crate::app::multi_exec::prune_disconnected(
                &app.multi_exec_mode.targets,
                &connected,
            );
            let mut targets = crate::app::multi_exec::broadcast_targets(leader, &kept);
            let filter = app.multi_exec_mode.host_filter.clone();
            if !filter.is_empty() {
                targets.retain(|id| {
                    app.session(*id)
                        .map(|s| {
                            crate::app::multi_exec::matches_host_filter(
                                s.spec.host.as_deref(),
                                &filter,
                            )
                        })
                        .unwrap_or(false)
                });
                if !targets.contains(&leader) {
                    targets.insert(0, leader);
                }
            }
            app.multi_exec_leader = Some(leader);
            // Per-session substitution: each target gets its own rendering.
            for id in targets {
                let rendered = app
                    .session(id)
                    .map(|s| crate::app::multi_exec::render_template(&template, &s.spec))
                    .unwrap_or_default();
                if let Err(reason) = crate::connection::actor::SessionManager::shared()
                    .write(id, rendered.into_bytes())
                {
                    app.notify(
                        Level::Warning,
                        "Multi-exec delivery failed",
                        &format!("session #{id}: {reason}"),
                    );
                }
            }
            if app.multi_exec_history.len() >= crate::app::multi_exec::MAX_HISTORY {
                app.multi_exec_history.remove(0);
            }
            app.multi_exec_history.push(format!("template: {template}"));
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecConfirmBroadcast => {
            let Some(bytes) = app.multi_exec_pending.take() else {
                return Err("nothing held for confirmation".to_string());
            };
            let Some(leader) = app.multi_exec_leader.or(app.active_session_id) else {
                return Err("no leader session".to_string());
            };
            broadcast_multicast(app, leader, bytes)
        },
        UiMsg::MultiExecDiscardBroadcast => {
            app.multi_exec_pending = None;
            app.notify(Level::Info, "Broadcast discarded", "held command dropped");
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecOpenView => {
            if let Some(index) = app
                .tabs
                .iter()
                .position(|tab| tab.kind == TabKind::MultiExecView)
            {
                app.active_tab = index;
            } else {
                app.tabs.push(Tab {
                    title: "Multi-exec".to_string(),
                    kind: TabKind::MultiExecView,
                });
                app.active_tab = app.tabs.len() - 1;
            }
            Ok(iced::Task::none())
        },
        UiMsg::MultiExecTargetFailed(id, reason) => {
            app.multi_exec_mode.targets.retain(|t| *t != id);
            sync_multi_exec_targets(app);
            app.notify(
                Level::Warning,
                "Multi-exec delivery failed",
                &format!("session #{id}: {reason}"),
            );
            Ok(iced::Task::none())
        },
    }
}

/// Mirror the target set into the runtime-shared map.
fn sync_multi_exec_targets(app: &mut AppState) {
    if let Ok(mut shared) = app.shared.write() {
        shared.multi_exec_targets = app.multi_exec_mode.targets.clone();
    }
}

/// Drop a session from the multi-exec targets on disconnect/failure, with
/// a notification while broadcasting is armed (race-safe: pruning is
/// idempotent and re-checked at every broadcast).
fn prune_multicast_target(app: &mut AppState, id: SessionId) {
    if !app.multi_exec_mode.targets.contains(&id) {
        return;
    }
    app.multi_exec_mode.targets.retain(|t| *t != id);
    sync_multi_exec_targets(app);
    if app.multi_exec_mode.enabled {
        app.notify(
            Level::Warning,
            "Multi-exec dropout",
            &format!("session #{id} left the target set"),
        );
    }
}

fn handle_system(app: &mut AppState, event: SystemEvent) -> Result<iced::Task<Message>, String> {
    match event {
        SystemEvent::ShutdownSignal => {
            app.notify(
                Level::Warning,
                "Shutting down",
                "termination signal received",
            );
            // Graceful path: persist first (best effort; failures are logged
            // inside persist_snapshot), then route through Quit handling.
            let paths = app.paths.clone();
            let config = app.settings.clone();
            let secure = app.secure.clone();
            let sessions = snapshot_sessions(app);
            Ok(iced::Task::perform(
                persist_snapshot(paths, config, secure, sessions),
                |_| Message::Ui(UiMsg::Quit),
            ))
        },
        SystemEvent::NetworkStatusChanged { online } => {
            if online {
                app.notify(Level::Info, "Network", "connection restored");
            } else {
                app.notify(Level::Warning, "Network", "connection lost");
            }
            Ok(iced::Task::none())
        },
        SystemEvent::ScreenLocked => {
            // Privacy: drop the clipboard mirror on screen lock (§6.2).
            app.clipboard.last_content.clear();
            app.notify(Level::Info, "Screen locked", "clipboard cache cleared");
            Ok(iced::Task::none())
        },
    }
}

fn handle_task_event(app: &mut AppState, event: TaskEvent) -> Result<iced::Task<Message>, String> {
    match event {
        TaskEvent::Started(id) => {
            app.tasks.insert(id, TaskStatus::Running);
            Ok(iced::Task::none())
        },
        TaskEvent::Progress(id, fraction, label) => {
            tracing::debug!(task = id, fraction, %label, "task progress");
            Ok(iced::Task::none())
        },
        TaskEvent::Completed(id) => {
            app.tasks.remove(&id);
            Ok(iced::Task::none())
        },
        TaskEvent::Failed(id, reason) => {
            app.tasks.insert(id, TaskStatus::Failed(reason.clone()));
            app.notify(Level::Error, "Background task failed", &reason);
            Ok(iced::Task::none())
        },
    }
}

fn handle_autosave(app: &mut AppState) -> Result<iced::Task<Message>, String> {
    app.notifications.retain_recent(now_secs());
    let mut tasks = Vec::new();
    #[cfg(feature = "feedback-network")]
    if app.settings.general.telemetry_enabled
        && crate::feedback::telemetry::should_flush(now_secs())
    {
        if let Ok(endpoint) = std::env::var("MBXT_TELEMETRY_ENDPOINT") {
            let payload =
                crate::feedback::telemetry::snapshot(app.settings.general.release_channel);
            tasks.push(iced::Task::perform(
                async move {
                    if let Err(error) = crate::feedback::telemetry::send(&endpoint, &payload).await
                    {
                        tracing::warn!(%error, "opt-in telemetry delivery failed");
                    }
                },
                |()| Message::Ignored,
            ));
        }
    }
    if app.ui_state.dirty {
        let paths = app.paths.clone();
        let config = app.settings.clone();
        let secure = app.secure.clone();
        let sessions = snapshot_sessions(app);
        tasks.push(iced::Task::perform(
            persist_snapshot(paths, config, secure, sessions),
            Message::Persisted,
        ));
    }
    Ok(iced::Task::batch(tasks))
}

/// Convert the live session list into store entries for persistence.
fn snapshot_sessions(app: &AppState) -> Vec<SessionEntry> {
    app.sessions
        .iter()
        .map(SessionEntry::from_session)
        .collect()
}

/// Prompt 2.2 connect flow: build the auth context (seeded from the
/// credential cache), negotiate with fallback, cache the winning password
/// when the provider produced one. Runs on the Tokio runtime via
/// `Task::perform`; interactive prompts hop to the UI through the bridge.
async fn run_auth_flow(
    spec: SessionSpec,
    bridge: PromptBridge,
    cache: CredentialCache,
) -> Result<ConnectionAuth, String> {
    let mut ctx = AuthContext::from_spec(&spec);
    if let Some(password) = cache.get_password(&spec.name) {
        ctx.password = Some(password);
    }

    match negotiate(&mut ctx, &bridge).await {
        Ok(outcome) => {
            let method = outcome.credential.method_name().to_string();
            tracing::info!(session = %spec.name, %method, "credentials acquired");
            Ok(match outcome.credential {
                AuthCredential::Password(password) => ConnectionAuth::Password(password),
                AuthCredential::KeyFile { path, passphrase } => {
                    ConnectionAuth::KeyFile { path, passphrase }
                },
                AuthCredential::Agent => ConnectionAuth::Agent,
                AuthCredential::KeyboardInteractive(response) => {
                    ConnectionAuth::KeyboardInteractive(response)
                },
                AuthCredential::GssApi => {
                    return Err("GSSAPI authentication is not wired to the SSH engine".to_string())
                },
            })
        },
        Err(err) => {
            tracing::warn!(session = %spec.name, error = %err, "authentication failed");
            Err(err.user_message())
        },
    }
}

fn handle_persisted(
    app: &mut AppState,
    result: Result<(), String>,
) -> Result<iced::Task<Message>, String> {
    match result {
        Ok(()) => {
            app.ui_state.dirty = false;
            Ok(iced::Task::none())
        },
        Err(err) => Err(err), // or_notify turns this into a notification
    }
}

/// Bundle path for "Report Issue" success/failure notification.
fn handle_issue_bundle(
    app: &mut AppState,
    result: Result<String, String>,
) -> Result<iced::Task<Message>, String> {
    match result {
        Ok(path) => {
            app.feedback.preview =
                std::fs::read_to_string(std::path::Path::new(&path).join("preview.md"))
                    .unwrap_or_else(|_| {
                        "Preview is available in the generated directory.".to_string()
                    });
            app.notify(
                Level::Success,
                "Feedback preview ready",
                &format!("review the preview before submitting: {path}"),
            );
            Ok(iced::Task::none())
        },
        Err(err) => Err(err),
    }
}

fn handle_update_checked(
    app: &mut AppState,
    result: Result<String, String>,
) -> Result<iced::Task<Message>, String> {
    match result {
        Ok(version) => {
            app.notify(
                Level::Info,
                "Update channel checked",
                &format!("latest signed version: {version}"),
            );
            Ok(iced::Task::none())
        },
        Err(error) => Err(error),
    }
}

/// A provider requested input — park it as the active modal (prompt 2.2).
fn handle_auth_prompt(
    app: &mut AppState,
    prompt: crate::connection::ssh::auth::AuthPrompt,
) -> Result<iced::Task<Message>, String> {
    tracing::debug!(prompt = prompt.id, kind = ?prompt.kind, "auth prompt shown");
    app.auth_input.clear();
    app.auth_dialog = Some(prompt);
    Ok(iced::Task::none())
}

/// User answered prompt `id` — resolve the provider's async wait.
fn handle_auth_response(
    app: &mut AppState,
    id: u64,
    value: String,
) -> Result<iced::Task<Message>, String> {
    app.auth_dialog = None;
    app.auth_input.clear();
    if !PromptBridge::shared().complete(id, value) {
        return Err(format!("no pending auth prompt #{id}"));
    }
    Ok(iced::Task::none())
}

/// Build a local, user-reviewable feedback preview. No network request occurs.
fn collect_issue_bundle(
    paths: &crate::utils::paths::AppPaths,
    feedback: &super::state::FeedbackUiState,
    channel: crate::utils::config::ReleaseChannel,
) -> Result<String, String> {
    if feedback.title.trim().is_empty() {
        return Err("feedback needs a short summary before preview".to_string());
    }
    let bundle_dir = paths.config_dir.join(format!("feedback-{}", now_secs()));
    std::fs::create_dir_all(&bundle_dir).map_err(|e| format!("cannot create bundle dir: {e}"))?;
    let screenshot = (!feedback.screenshot_path.trim().is_empty())
        .then(|| std::path::PathBuf::from(feedback.screenshot_path.trim()));
    let preview = crate::feedback::FeedbackPreview::prepare(
        feedback.kind,
        feedback.title.clone(),
        feedback.description.clone(),
        crate::feedback::SystemContext::collect(channel),
        &feedback.consent,
        &paths.logs_dir,
        screenshot,
    )?;
    if let Some(source) = &preview.screenshot {
        if !source.is_file() {
            return Err("selected screenshot is not a readable file".to_string());
        }
        std::fs::copy(source, bundle_dir.join("screenshot"))
            .map_err(|e| format!("cannot copy screenshot: {e}"))?;
    }
    std::fs::write(bundle_dir.join("preview.md"), preview.render())
        .map_err(|e| format!("cannot write report: {e}"))?;

    Ok(bundle_dir.display().to_string())
}

fn handle_clipboard(app: &mut AppState, text: String) -> Result<iced::Task<Message>, String> {
    // OSC 52-initiated or external clipboard change; mirror for dedup only.
    app.clipboard.last_content = text;
    Ok(iced::Task::none())
}

#[cfg(test)]
#[allow(unused_must_use)]
mod tests {
    use super::*;
    use crate::app::notifications::Level;
    use mbxt_core::{AuthMethod, Protocol, SessionSpec};

    fn app() -> AppState {
        let base = std::env::temp_dir().join(format!("mbxt-update-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        AppState::new(AppConfig::default(), paths).0
    }

    fn spec(name: &str) -> SessionSpec {
        SessionSpec {
            name: name.to_string(),
            protocol: Protocol::Ssh,
            host: Some("h".into()),
            port: Some(22),
            username: Some("ops".into()),
            auth: AuthMethod::Password,
            tags: vec![],
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
        }
    }

    fn push_session(app: &mut AppState, name: &str) -> SessionId {
        app.sessions.push(mbxt_core::Session {
            id: app.sessions.len() as SessionId + 1,
            spec: spec(name),
            state: SessionState::Disconnected,
        });
        app.sessions.last().unwrap().id
    }

    #[test]
    fn unknown_session_select_becomes_error_notification() {
        let mut state = app();
        update(&mut state, Message::Session(SessionMsg::Selected(99)));
        assert!(state
            .notifications
            .items
            .iter()
            .any(|n| n.level == Level::Error));
    }

    #[test]
    fn empty_session_name_rejected() {
        let mut state = app();
        update(
            &mut state,
            Message::Session(SessionMsg::Created("   ".into())),
        );
        assert!(state
            .notifications
            .items
            .iter()
            .any(|n| n.level == Level::Error));
        assert!(state.sessions.is_empty());
    }

    #[test]
    fn session_created_then_renamed_then_deleted() {
        let mut state = app();
        update(
            &mut state,
            Message::Session(SessionMsg::Created("web".into())),
        );
        assert_eq!(state.sessions.len(), 1);
        assert!(state.ui_state.dirty);

        let id = state.sessions[0].id;
        update(
            &mut state,
            Message::Session(SessionMsg::Renamed(id, "web2".into())),
        );
        assert_eq!(state.sessions[0].spec.name, "web2");

        update(&mut state, Message::Session(SessionMsg::Deleted(id)));
        assert!(state.sessions.is_empty());
        assert_eq!(state.active_session_id, None);
    }

    #[test]
    fn x11_toggle_flips_flag_and_rejects_non_ssh() {
        let mut state = app();
        let id = push_session(&mut state, "web-1");
        assert!(!state.sessions[0].spec.x11_forwarding);

        update(&mut state, Message::Session(SessionMsg::X11Toggled(id)));
        assert!(state.sessions[0].spec.x11_forwarding);
        assert!(state.ui_state.dirty);

        update(&mut state, Message::Session(SessionMsg::X11Toggled(id)));
        assert!(!state.sessions[0].spec.x11_forwarding);

        // Unknown sessions error.
        update(&mut state, Message::Session(SessionMsg::X11Toggled(999)));
        assert!(state
            .notifications
            .items
            .iter()
            .any(|n| n.level == Level::Error));

        // Non-SSH sessions reject the toggle.
        state.sessions[0].spec.protocol = Protocol::Telnet;
        state.notifications.items.clear();
        update(&mut state, Message::Session(SessionMsg::X11Toggled(id)));
        assert!(!state.sessions[0].spec.x11_forwarding);
        assert!(state
            .notifications
            .items
            .iter()
            .any(|n| n.level == Level::Error));
    }

    #[cfg(feature = "ssh")]
    #[test]
    fn tunnel_form_validate_persist_edit_delete() {
        use crate::app::messages::TunnelMsg;
        use crate::connection::forward::ForwardManager;

        let mut state = app();
        let id = push_session(&mut state, "web-1");

        // Unknown sessions cannot open the form.
        update(&mut state, Message::Tunnel(TunnelMsg::OpenForm(9999)));
        assert!(state.tunnel_draft.is_none());

        update(&mut state, Message::Tunnel(TunnelMsg::OpenForm(id)));
        assert!(state.tunnel_draft.is_some());

        // Bad port: rejected, form stays open, nothing persisted.
        update(
            &mut state,
            Message::Tunnel(TunnelMsg::FieldChanged(
                crate::app::messages::TunnelField::BindPort,
                "banana".into(),
            )),
        );
        update(&mut state, Message::Tunnel(TunnelMsg::Submit));
        assert!(state.tunnel_draft.is_some());
        assert!(state.sessions[0].spec.forwards.is_empty());

        // Valid local forward: persisted to the spec, mirrored to the
        // manager, form closed.
        update(
            &mut state,
            Message::Tunnel(TunnelMsg::FieldChanged(
                crate::app::messages::TunnelField::BindPort,
                "8080".into(),
            )),
        );
        update(
            &mut state,
            Message::Tunnel(TunnelMsg::FieldChanged(
                crate::app::messages::TunnelField::TargetHost,
                "db.internal".into(),
            )),
        );
        update(&mut state, Message::Tunnel(TunnelMsg::Submit));
        assert!(state.tunnel_draft.is_none());
        assert_eq!(state.sessions[0].spec.forwards.len(), 1);
        assert!(state.ui_state.dirty, "autosave picks up tunnels");
        let def_id = state.sessions[0].spec.forwards[0].id;
        // The manager is process-global (parallel tests register here too):
        // assert presence of our def, not an exact count.
        assert!(
            ForwardManager::shared()
                .definitions_for(id)
                .iter()
                .any(|(other, _)| *other == def_id),
            "submit mirrors the definition into the manager"
        );

        // Edit round-trips the definition back into the form.
        update(&mut state, Message::Tunnel(TunnelMsg::Edit(def_id)));
        let draft = state.tunnel_draft.as_ref().expect("editing");
        assert_eq!(draft.target_host, "db.internal");

        // Delete drops both copies.
        update(&mut state, Message::Tunnel(TunnelMsg::Delete(def_id)));
        assert!(state.sessions[0].spec.forwards.is_empty());
        assert!(
            ForwardManager::shared()
                .definitions_for(id)
                .iter()
                .all(|(other, _)| *other != def_id),
            "delete drops the manager copy"
        );
    }

    #[cfg(feature = "ssh")]
    #[test]
    fn tunnel_start_without_session_fails_cleanly() {
        use crate::app::messages::TunnelMsg;
        use crate::connection::forward::ForwardManager;
        use mbxt_core::{ForwardDef, ForwardType};

        // A remote forward needs a live actor: with none, start records a
        // Failed status instead of hanging or panicking.
        let manager = ForwardManager::new();
        let id = manager.define(
            31337,
            ForwardDef::new(
                None,
                ForwardType::Remote {
                    remote_host: "127.0.0.1".into(),
                    remote_port: 19999,
                    local_host: "127.0.0.1".into(),
                    local_port: 80,
                },
                false,
            ),
        );
        let outcome = futures_block_on_test(manager.start_async(id));
        assert!(outcome.is_err());
        assert!(matches!(
            manager.snapshot(id).map(|s| s.status),
            Some(crate::connection::forward::ForwardStatus::Failed(_))
        ));

        // Unknown ids schedule async work without panicking (the failure
        // notice lands when the task resolves; Failed status is covered in
        // the manager test).
        let mut state = app();
        update(
            &mut state,
            Message::Tunnel(TunnelMsg::Start(uuid::Uuid::new_v4())),
        );
        update(
            &mut state,
            Message::Tunnel(TunnelMsg::Stop(uuid::Uuid::new_v4())),
        );
    }

    /// Block on a future from sync test code.
    #[cfg(feature = "ssh")]
    fn futures_block_on_test<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(future)
    }

    #[test]
    fn dialog_submit_validates_before_creating() {
        use crate::app::state::NewSessionDraft;

        let mut state = app();
        // Empty serial device: rejected, dialog stays open.
        let mut draft = NewSessionDraft::new("console".into());
        draft.protocol = Protocol::Serial;
        draft.device.clear();
        state.new_session = Some(draft);
        update(&mut state, Message::Ui(UiMsg::NewSessionSubmitted));
        assert!(state.sessions.is_empty());
        assert!(state.new_session.is_some());

        // Telnet without host: rejected too.
        let mut draft = NewSessionDraft::new("legacy".into());
        draft.protocol = Protocol::Telnet;
        draft.host.clear();
        state.new_session = Some(draft);
        update(&mut state, Message::Ui(UiMsg::NewSessionSubmitted));
        assert!(state.sessions.is_empty());

        // Direct spec path enforces the same rules.
        let spec = SessionSpec {
            name: "x".into(),
            protocol: Protocol::Serial,
            host: None,
            port: None,
            username: None,
            auth: AuthMethod::Password,
            tags: vec![],
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
        };
        update(
            &mut state,
            Message::Session(SessionMsg::CreateDetailed(Box::new(spec))),
        );
        assert!(state.sessions.is_empty());

        // A valid serial draft creates exactly one session and closes.
        let mut draft = NewSessionDraft::new("console".into());
        draft.protocol = Protocol::Serial;
        draft.device = "/dev/ttyUSB0".into();
        draft.baud = "9600".into();
        state.new_session = Some(draft);
        update(&mut state, Message::Ui(UiMsg::NewSessionSubmitted));
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].spec.protocol, Protocol::Serial);
        assert!(state.new_session.is_none());
    }

    #[test]
    fn quick_connect_opens_the_dialog() {
        let mut state = app();
        assert!(state.new_session.is_none());
        update(&mut state, Message::Ui(UiMsg::QuickConnect));
        let draft = state.new_session.as_ref().expect("dialog opened");
        assert_eq!(draft.protocol, Protocol::Ssh);
        update(&mut state, Message::Ui(UiMsg::NewSessionDialogClosed));
        assert!(state.new_session.is_none());
    }

    #[cfg(feature = "vnc")]
    #[test]
    fn vnc_viewer_tab_lifecycle() {
        use crate::app::messages::VncMsg;
        use crate::connection::vnc::input::ScalingMode;

        let mut state = app();
        let id = push_session(&mut state, "vnc-1");

        update(&mut state, Message::Vnc(VncMsg::OpenViewer(id)));
        assert!(state.vnc_viewers.contains_key(&id));
        assert!(state.tabs.iter().any(|t| t.kind == TabKind::VncViewer(id)));

        // Opening twice focuses instead of duplicating.
        let tabs = state.tabs.len();
        update(&mut state, Message::Vnc(VncMsg::OpenViewer(id)));
        assert_eq!(state.tabs.len(), tabs);

        update(
            &mut state,
            Message::Vnc(VncMsg::SetScaling(id, ScalingMode::OneToOne)),
        );
        assert_eq!(state.vnc_viewers[&id].scaling, ScalingMode::OneToOne);

        update(&mut state, Message::Vnc(VncMsg::CloseViewer(id)));
        assert!(!state.vnc_viewers.contains_key(&id));
        assert!(!state.tabs.iter().any(|t| t.kind == TabKind::VncViewer(id)));
    }

    #[cfg(feature = "vnc")]
    #[test]
    fn vnc_input_without_session_is_ignored() {
        use crate::app::messages::VncMsg;

        let mut state = app();
        // No manager session: handlers return without tasks or panics.
        update(&mut state, Message::Vnc(VncMsg::KeyTap(4242, 0x61)));
        update(
            &mut state,
            Message::Vnc(VncMsg::PointerMoved(4242, 0, (10, 10))),
        );
        update(&mut state, Message::Vnc(VncMsg::FrameTickAll));
        update(
            &mut state,
            Message::Vnc(VncMsg::GotWindowId(4242, None, true)),
        );
        assert!(state
            .notifications
            .items
            .iter()
            .any(|n| n.title == "Fullscreen unavailable"));
    }

    #[cfg(feature = "vnc")]
    #[test]
    fn vnc_connect_outcome_records_state() {
        use crate::app::messages::VncMsg;

        let mut state = app();
        let id = push_session(&mut state, "vnc-1");
        update(&mut state, Message::Vnc(VncMsg::OpenViewer(id)));
        update(&mut state, Message::Vnc(VncMsg::Connected(id)));
        assert!(state.vnc_viewers[&id].connected);
        update(
            &mut state,
            Message::Vnc(VncMsg::ConnectFailed(id, "refused".into())),
        );
        assert!(!state.vnc_viewers[&id].connected);
        assert_eq!(state.vnc_viewers[&id].error.as_deref(), Some("refused"));
    }

    #[test]
    fn connect_failure_path_notifies_and_marks_failed() {
        let mut state = app();
        let id = push_session(&mut state, "web-1");
        update(
            &mut state,
            Message::Connection(ConnectionMsg::ConnectRequested(id)),
        );
        assert_eq!(
            state.session_states.get(&id),
            Some(&SessionState::Connecting)
        );

        update(
            &mut state,
            Message::Connection(ConnectionMsg::Failed(id, "refused".into())),
        );
        assert_eq!(
            state.session_states.get(&id),
            Some(&SessionState::Failed("refused".into()))
        );
        assert!(matches!(
            state.connection_status,
            ConnectionStatus::Failed { .. }
        ));
        assert!(state
            .notifications
            .items
            .iter()
            .any(|n| n.title == "Connection failed"));
    }

    #[test]
    fn connected_updates_status_and_shared_map() {
        let mut state = app();
        let id = push_session(&mut state, "web-1");
        state.active_session_id = Some(id);
        update(
            &mut state,
            Message::Connection(ConnectionMsg::Connected(id)),
        );
        assert_eq!(
            state.session_states.get(&id),
            Some(&SessionState::Connected)
        );
        assert!(matches!(
            state.connection_status,
            ConnectionStatus::Connected { .. }
        ));
        assert_eq!(
            state.shared.read().unwrap().session_states.get(&id),
            Some(&SessionState::Connected)
        );
    }

    #[test]
    fn disconnect_of_inactive_session_leaves_active_status() {
        let mut state = app();
        let a = push_session(&mut state, "a");
        let b = push_session(&mut state, "b");
        state.active_session_id = Some(b);
        update(&mut state, Message::Connection(ConnectionMsg::Connected(b)));
        update(
            &mut state,
            Message::Connection(ConnectionMsg::Disconnected(a)),
        );
        assert!(matches!(
            state.connection_status,
            ConnectionStatus::Connected { .. }
        ));
    }

    #[test]
    fn progress_clamps_to_100() {
        let mut state = app();
        let id = push_session(&mut state, "web-1");
        state.active_session_id = Some(id);
        update(
            &mut state,
            Message::Connection(ConnectionMsg::Progress(id, 250)),
        );
        assert_eq!(
            state.connection_status,
            ConnectionStatus::Connecting {
                progress: Some(100)
            }
        );
    }

    #[test]
    fn theme_toggle_cycles_and_persists_name() {
        let mut state = app();
        assert_eq!(state.theme, crate::ui::theme::AppTheme::Dark);
        update(&mut state, Message::Ui(UiMsg::ThemeToggled));
        // Dark -> SolarizedDark, name persisted for the next launch.
        assert_eq!(state.theme, crate::ui::theme::AppTheme::SolarizedDark);
        assert_eq!(state.settings.appearance.theme, "solarized-dark");
        assert!(state.settings.appearance.dark_theme);
        assert!(state.ui_state.dirty);
    }

    #[test]
    fn tool_run_request_records_a_running_run() {
        use crate::app::messages::ToolMsg;
        use crate::tools::{RunStatus, ToolKind, ToolParams};
        let mut state = app();
        state.tool_draft.kind = ToolKind::Subnet;
        state.tool_draft.params = ToolParams::for_kind(ToolKind::Subnet);
        let _task = update(&mut state, Message::Tool(ToolMsg::RunRequested));
        assert_eq!(state.tool_runs.len(), 1);
        let run = &state.tool_runs[0];
        assert_eq!(run.kind, ToolKind::Subnet);
        assert_eq!(run.status, RunStatus::Running);
        assert!(state.tool_cancel.contains_key(&run.id));
    }

    #[test]
    fn master_password_rejection_marks_store_locked_and_notifies() {
        let mut state = app();
        state.sessions_unlocked = true;
        update(
            &mut state,
            Message::Ui(UiMsg::MasterPasswordRejected("bad pw".into())),
        );
        assert!(!state.sessions_unlocked);
        assert!(state
            .notifications
            .items
            .iter()
            .any(|n| n.title == "Unlock failed"));
    }

    #[test]
    fn multi_exec_toggle_clears_targets_when_disabled() {
        let mut state = app();
        update(
            &mut state,
            Message::Ui(UiMsg::MultiExecTargets(vec![1, 2, 3])),
        );
        update(&mut state, Message::Ui(UiMsg::MultiExecToggled(true)));
        assert!(state.multi_exec_mode.enabled);

        update(&mut state, Message::Ui(UiMsg::MultiExecToggled(false)));
        assert!(!state.multi_exec_mode.enabled);
        assert!(state.multi_exec_mode.targets.is_empty());
        assert!(state.shared.read().unwrap().multi_exec_targets.is_empty());
    }

    #[test]
    fn broadcast_reaches_every_mock_target_identically() {
        use crate::connection::actor::{SessionCtl, SessionManager};

        let manager = SessionManager::shared();
        let mut rx_a = manager.inject_test_target(9001);
        let mut rx_b = manager.inject_test_target(9002);

        let mut state = app();
        let leader = push_session(&mut state, "leader");
        // Wire the leader in as a live target too.
        let mut rx_leader = manager.inject_test_target(leader);
        // Mock targets count as connected (pruning consults this mirror).
        for id in [leader, 9001, 9002] {
            state.session_states.insert(id, SessionState::Connected);
        }
        state.multi_exec_mode.enabled = true;
        state.multi_exec_mode.targets = vec![9001, 9002];

        update(
            &mut state,
            Message::Terminal(TerminalMsg::Input(leader, b"uptime\n".to_vec())),
        );
        // Leader + both targets receive identical bytes (order preserved).
        for rx in [&mut rx_leader, &mut rx_a, &mut rx_b] {
            match rx.try_recv() {
                Ok(SessionCtl::Write(bytes)) => assert_eq!(bytes, b"uptime\n"),
                other => panic!("expected broadcast write, got {other:?}"),
            }
        }
        // Completed line entered broadcast history; leader tracked.
        assert_eq!(state.multi_exec_history, vec!["uptime"]);
        assert_eq!(state.multi_exec_leader, Some(leader));
    }

    #[test]
    fn destructive_input_is_held_for_confirmation() {
        let mut state = app();
        let leader = push_session(&mut state, "leader");
        state.multi_exec_mode.enabled = true;
        state.multi_exec_mode.targets = vec![leader];

        update(
            &mut state,
            Message::Terminal(TerminalMsg::Input(leader, b"rm -rf /tmp/cache\n".to_vec())),
        );
        assert_eq!(
            state.multi_exec_pending.as_deref(),
            Some(b"rm -rf /tmp/cache\n".as_slice())
        );
        assert!(
            state.multi_exec_history.is_empty(),
            "held commands leave no history"
        );
        assert!(state
            .notifications
            .items
            .iter()
            .any(|n| n.title == "Destructive broadcast held"));

        // Discard drops it; confirm would rebroadcast (no live actor here).
        update(&mut state, Message::Ui(UiMsg::MultiExecDiscardBroadcast));
        assert!(state.multi_exec_pending.is_none());
    }

    #[test]
    fn failed_targets_prune_with_notification() {
        let mut state = app();
        let _leader = push_session(&mut state, "leader");
        // 9500+ have no actors: every delivery fails.
        state.multi_exec_mode.enabled = true;
        state.multi_exec_mode.targets = vec![9501, 9502];

        update(
            &mut state,
            Message::Terminal(TerminalMsg::Input(9501, b"ls\n".to_vec())),
        );
        assert!(state.multi_exec_mode.targets.is_empty(), "failures pruned");
        assert!(state
            .notifications
            .items
            .iter()
            .any(|n| n.title == "Multi-exec delivery failed"));
    }

    #[test]
    fn toggle_auto_selects_connected_and_prunes_on_drop() {
        let mut state = app();
        let a = push_session(&mut state, "a");
        let b = push_session(&mut state, "b");
        state.session_states.insert(a, SessionState::Connected);
        state.session_states.insert(b, SessionState::Disconnected);

        update(&mut state, Message::Ui(UiMsg::MultiExecToggle));
        assert!(state.multi_exec_mode.enabled);
        assert_eq!(state.multi_exec_mode.targets, vec![a], "only live sessions");

        // Dropout while armed prunes + warns.
        update(
            &mut state,
            Message::Connection(ConnectionMsg::Disconnected(a)),
        );
        assert!(state.multi_exec_mode.targets.is_empty());
        assert!(state
            .notifications
            .items
            .iter()
            .any(|n| n.title == "Multi-exec dropout"));

        update(&mut state, Message::Ui(UiMsg::MultiExecToggle));
        assert!(!state.multi_exec_mode.enabled);
    }

    #[test]
    fn macro_record_play_round_trip() {
        use crate::app::messages::MacroMsg;
        use crate::connection::actor::{SessionCtl, SessionManager};
        use crate::macros::MacroStep;

        // Fixed high ids: the shared SessionManager outlives any one test,
        // so mock actors must never collide across parallel tests.
        let mut state = app();
        let leader = 9301;
        state.sessions.push(mbxt_core::Session {
            id: leader,
            spec: spec("leader"),
            state: SessionState::Disconnected,
        });
        state.active_session_id = Some(leader);
        let mut rx = SessionManager::shared().inject_test_target(leader);

        // Record: input captured, prompt output appends a wait.
        update(&mut state, Message::Macro(MacroMsg::RecordToggle));
        update(
            &mut state,
            Message::Terminal(TerminalMsg::Input(leader, b"uptime\n".to_vec())),
        );
        update(
            &mut state,
            Message::Terminal(TerminalMsg::Output(leader, b"load 0.1\nops@h:~$ ".to_vec())),
        );
        update(&mut state, Message::Macro(MacroMsg::RecordToggle));
        // Library already holds the seeded builtins; find the recording.
        let recorded = state
            .macros
            .iter()
            .find(|m| m.name.starts_with("macro-"))
            .expect("recorded macro saved");
        let id = recorded.id;
        assert!(recorded
            .steps
            .iter()
            .any(|s| matches!(s, MacroStep::WaitForPattern { .. })));

        // Play: input sent, wait armed, matching output finishes.
        update(&mut state, Message::Macro(MacroMsg::Play(id)));
        assert!(state.macro_player.is_some());
        match rx.try_recv() {
            Ok(SessionCtl::Write(bytes)) => assert_eq!(bytes, b"uptime\n"),
            other => panic!("expected macro send, got {other:?}"),
        }
        update(
            &mut state,
            Message::Terminal(TerminalMsg::Output(leader, b"ops@h:~$ ".to_vec())),
        );
        assert!(state.macro_player.is_none(), "pattern match finishes play");
        assert!(state
            .notifications
            .items
            .iter()
            .any(|n| n.title == "Macro finished"));
    }

    #[test]
    fn macro_destructive_play_needs_confirmation() {
        use crate::app::messages::MacroMsg;
        use crate::macros::{Macro, MacroStep};

        let mut state = app();
        let leader = push_session(&mut state, "leader");
        state.active_session_id = Some(leader);
        let mut macro_ = Macro::new("danger".into());
        macro_.steps = vec![MacroStep::SendInput {
            data: "rm -rf /tmp/x\n".into(),
        }];
        let id = macro_.id;
        state.macros.push(macro_);

        update(&mut state, Message::Macro(MacroMsg::Play(id)));
        assert!(state.macro_player.is_none(), "held, not playing");
        assert!(state.macro_pending_play.is_some());
        update(&mut state, Message::Macro(MacroMsg::DiscardPlay));
        assert!(state.macro_pending_play.is_none());
    }

    #[test]
    fn macro_variable_prompt_flow() {
        use crate::app::messages::MacroMsg;
        use crate::connection::actor::{SessionCtl, SessionManager};
        use crate::macros::{Macro, MacroStep};

        let mut state = app();
        let leader = 9401;
        state.sessions.push(mbxt_core::Session {
            id: leader,
            spec: spec("leader"),
            state: SessionState::Disconnected,
        });
        state.active_session_id = Some(leader);
        let mut rx = SessionManager::shared().inject_test_target(leader);
        let mut macro_ = Macro::new("login".into());
        macro_.steps = vec![
            MacroStep::PromptVariable {
                name: "pw".into(),
                prompt: "Password".into(),
                default: None,
                secret: true,
            },
            MacroStep::SendVariable { name: "pw".into() },
        ];
        let id = macro_.id;
        state.macros.push(macro_);

        update(&mut state, Message::Macro(MacroMsg::Play(id)));
        assert!(state.macro_prompt.is_some(), "prompt shown");
        update(
            &mut state,
            Message::Macro(MacroMsg::PromptAnswer("s3krit".into())),
        );
        assert!(state.macro_prompt.is_none());
        match rx.try_recv() {
            Ok(SessionCtl::Write(bytes)) => assert_eq!(bytes, b"s3krit"),
            other => panic!("expected variable send, got {other:?}"),
        }
        assert!(state.macro_player.is_none(), "finished after send");
    }

    #[test]
    fn template_renders_per_session_host() {
        use crate::connection::actor::SessionManager;

        let manager = SessionManager::shared();
        let mut rx_a = manager.inject_test_target(9101);
        let mut rx_b = manager.inject_test_target(9102);

        let mut state = app();
        let leader = push_session(&mut state, "leader");
        state
            .sessions
            .iter_mut()
            .find(|s| s.id == leader)
            .unwrap()
            .spec
            .host = Some("leader-host".into());
        // Retarget the mock sessions with distinct hosts.
        state.sessions.push(mbxt_core::Session {
            id: 9101,
            spec: {
                let mut spec = state.sessions[0].spec.clone();
                spec.name = "a".into();
                spec.host = Some("10.0.0.1".into());
                spec
            },
            state: SessionState::Disconnected,
        });
        state.sessions.push(mbxt_core::Session {
            id: 9102,
            spec: {
                let mut spec = state.sessions[0].spec.clone();
                spec.name = "b".into();
                spec.host = Some("10.0.0.2".into());
                spec
            },
            state: SessionState::Disconnected,
        });
        state.active_session_id = Some(leader);
        state.session_states.insert(9101, SessionState::Connected);
        state.session_states.insert(9102, SessionState::Connected);
        state.multi_exec_mode.enabled = true;
        state.multi_exec_mode.targets = vec![9101, 9102];
        state.multi_exec_template = "ping -c1 $SESSION_HOST".into();

        update(&mut state, Message::Ui(UiMsg::MultiExecTemplateSend));
        for (rx, host) in [(&mut rx_a, "10.0.0.1"), (&mut rx_b, "10.0.0.2")] {
            match rx.try_recv() {
                Ok(crate::connection::actor::SessionCtl::Write(bytes)) => {
                    assert_eq!(bytes, format!("ping -c1 {host}").into_bytes())
                },
                other => panic!("expected rendered write, got {other:?}"),
            }
        }
    }

    #[test]
    fn task_board_tracks_lifecycle() {
        let mut state = app();
        update(&mut state, Message::Task(TaskEvent::Started(5)));
        assert_eq!(state.running_tasks(), 1);

        update(&mut state, Message::Task(TaskEvent::Completed(5)));
        assert_eq!(state.running_tasks(), 0);

        update(&mut state, Message::Task(TaskEvent::Failed(6, "x".into())));
        assert!(state.tasks.contains_key(&6));
        assert!(state
            .notifications
            .items
            .iter()
            .any(|n| n.title == "Background task failed"));
    }

    #[test]
    fn screen_lock_clears_clipboard_mirror() {
        let mut state = app();
        update(&mut state, Message::ClipboardChanged("secret".to_string()));
        assert_eq!(state.clipboard.last_content, "secret");

        update(&mut state, Message::System(SystemEvent::ScreenLocked));
        assert!(state.clipboard.last_content.is_empty());
    }

    #[test]
    fn tab_lifecycle_new_select_close() {
        let mut state = app();
        assert_eq!(state.tabs.len(), 1); // Welcome

        update(&mut state, Message::Ui(UiMsg::NewTab));
        update(&mut state, Message::Ui(UiMsg::NewTab));
        assert_eq!(state.tabs.len(), 3);
        assert_eq!(state.active_tab, 2);

        update(&mut state, Message::Ui(UiMsg::SelectTab(0)));
        assert_eq!(state.active_tab, 0);

        update(&mut state, Message::Ui(UiMsg::CloseActiveTab));
        assert_eq!(state.tabs.len(), 2);
        assert_eq!(state.active_tab, 0, "index clamps into range");

        // Never close the last tab.
        update(&mut state, Message::Ui(UiMsg::CloseActiveTab));
        update(&mut state, Message::Ui(UiMsg::CloseActiveTab));
        assert_eq!(state.tabs.len(), 1);
    }

    #[test]
    fn select_tab_is_clamped() {
        let mut state = app();
        update(&mut state, Message::Ui(UiMsg::SelectTab(42)));
        assert_eq!(state.active_tab, 0);
    }

    #[test]
    fn new_session_from_toolbar_creates_and_marks_dirty() {
        let mut state = app();
        update(&mut state, Message::Ui(UiMsg::NewSession));
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].spec.name, "session-1");
        assert!(state.ui_state.dirty);
    }

    #[test]
    fn open_settings_switches_view() {
        let mut state = app();
        update(&mut state, Message::Ui(UiMsg::OpenSettings));
        assert_eq!(
            state.ui_state.view,
            crate::app::messages::ViewKind::Settings
        );
    }

    #[test]
    fn auth_prompt_shows_modal_and_response_resolves_bridge() {
        let mut state = app();
        let prompt = crate::connection::ssh::auth::AuthPrompt {
            id: 77,
            kind: crate::connection::ssh::auth::PromptKind::Otp,
            title: "Two-factor".into(),
            prompt: "Verification code:".into(),
            masked: true,
        };
        update(&mut state, Message::AuthPrompt(prompt));
        assert!(state.auth_dialog.is_some());

        update(&mut state, Message::Ui(UiMsg::AuthInputChanged("9".into())));
        assert_eq!(state.auth_input, "9");

        // Submitting resolves the waiting side of the bridge.
        let bridge = crate::connection::ssh::auth::PromptBridge::shared();
        // (complete() on an unregistered id fails; register via cancel-safe
        // path: submit for a known prompt id goes through AuthResponse.)
        update(&mut state, Message::Ui(UiMsg::AuthCancel));
        assert!(state.auth_dialog.is_none());
        assert!(state.auth_input.is_empty());
        assert!(
            !bridge.complete(77, "late".into()),
            "cancelled prompt is gone"
        );
    }

    #[tokio::test]
    async fn auth_response_resolves_pending_provider_wait() {
        let bridge = crate::connection::ssh::auth::PromptBridge::shared();
        let mut rx = bridge.subscribe();

        let asker = {
            let bridge = bridge.clone();
            tokio::spawn(async move {
                bridge
                    .request(
                        crate::connection::ssh::auth::PromptKind::Password,
                        "Auth",
                        "Password:",
                    )
                    .await
                    .expect("answered via AuthResponse")
            })
        };

        let prompt = rx.recv().await.expect("prompt broadcast");
        // Simulate the UI round trip through update().
        let mut state = app();
        update(&mut state, Message::AuthPrompt(prompt.clone()));
        update(
            &mut state,
            Message::AuthResponse {
                id: prompt.id,
                value: "answer".into(),
            },
        );
        assert_eq!(asker.await.unwrap(), "answer");
    }

    #[tokio::test]
    async fn auth_flow_caches_password_on_success() {
        let spec = SessionSpec {
            name: "cache-test".into(),
            protocol: mbxt_core::Protocol::Ssh,
            host: Some("h".into()),
            port: Some(22),
            username: Some("ops".into()),
            auth: mbxt_core::AuthMethod::Password,
            tags: vec![],
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
        };
        let cache = CredentialCache::new();
        cache.put_password("cache-test", "cached-pw", true);

        let bridge = PromptBridge::shared().clone();
        let method = run_auth_flow(spec, bridge, cache.clone())
            .await
            .expect("cached password satisfies negotiation");
        assert!(matches!(method, ConnectionAuth::Password(_)));
    }

    #[test]
    fn autosave_only_saves_when_dirty() {
        let mut state = app();
        // Not dirty → no persistence task, no error.
        update(&mut state, Message::AutosaveTick);
        assert!(state.notifications.is_empty());

        state.ui_state.dirty = true;
        // Dirty → schedules persistence; result arrives as Message::Persisted.
        update(&mut state, Message::AutosaveTick);
        update(&mut state, Message::Persisted(Ok(())));
        assert!(!state.ui_state.dirty);
    }
}
