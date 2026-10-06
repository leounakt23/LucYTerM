//! External-event subscriptions (iced `Subscription` — architecture §3.3).
//!
//! All runtime-side event sources are bridged into the MVU loop here:
//! - OS termination signals (SIGHUP/SIGINT/SIGTERM)
//! - 30 s autosave heartbeat
//! - Clipboard change polling (copypasta; X11/Wayland have no push API)
//! - TaskManager lifecycle events (broadcast → `Message::Task`)
//! - Network status via NetworkManager (linux; native dbus watch TODO)
//!
//! API note: built against iced 0.13 (`Subscription::run_with_id` +
//! `iced::stream::channel`). The prompt referenced iced 0.10's `Command`;
//! in 0.13 that type is `iced::Task` (tech_stack.md decision).

use std::time::Duration;

use iced::{stream, Subscription};
use tokio::sync::broadcast;

// Resilient across iced point releases: some expose an inherent `send` on
// the channel Sender, others require the SinkExt trait.
#[allow(unused_imports)]
use iced::futures::sink::SinkExt;

use super::messages::{Message, SystemEvent};
use crate::task::TaskManager;

/// Batch of all external-event subscriptions.
pub fn subscriptions(_app: &super::state::AppState) -> Subscription<Message> {
    #[allow(unused_mut)]
    let mut subs = vec![
        shutdown_signals(),
        autosave_tick(),
        clipboard_watcher(),
        task_events(),
        session_events(),
        // Keyboard shortcuts (prompt 1.4: Ctrl+T/W/B/K/,).
        crate::ui::keyboard::subscription(),
        // Auth prompts → modal (prompt 2.2).
        auth_prompts(),
    ];
    #[cfg(target_os = "linux")]
    subs.push(network_status());
    #[cfg(feature = "vnc")]
    subs.push(vnc_frame_tick());
    Subscription::batch(subs)
}

/// VNC repaint ticks (~15 fps) while any viewer is connected. The widget
/// polls the latest snapshot, so ticks only trigger re-render; with no
/// viewers the stream idles at 1 Hz.
#[cfg(feature = "vnc")]
fn vnc_frame_tick() -> Subscription<Message> {
    Subscription::run_with_id(
        0x9C9,
        stream::channel(8, |mut output| async move {
            loop {
                let active = crate::connection::vnc::VncManager::shared().has_any();
                if active {
                    if output
                        .send(Message::Vnc(super::messages::VncMsg::FrameTickAll))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(66)).await;
                } else {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }),
    )
}

fn session_events() -> Subscription<Message> {
    Subscription::run_with_id(
        0x5E55,
        stream::channel(64, |mut output| async move {
            let mut rx = crate::connection::actor::SessionManager::shared().subscribe();
            loop {
                let message = match rx.recv().await {
                    Ok(mbxt_core::UiEvent::SessionStateChanged { session, state }) => {
                        use mbxt_core::SessionState;
                        match state {
                            SessionState::Connecting => Message::Connection(
                                super::messages::ConnectionMsg::Progress(session, 0),
                            ),
                            SessionState::Connected => Message::Connection(
                                super::messages::ConnectionMsg::Connected(session),
                            ),
                            SessionState::Disconnected => Message::Connection(
                                super::messages::ConnectionMsg::Disconnected(session),
                            ),
                            SessionState::Failed(reason) => Message::Connection(
                                super::messages::ConnectionMsg::Failed(session, reason),
                            ),
                        }
                    },
                    Ok(mbxt_core::UiEvent::TerminalOutput { session, bytes }) => {
                        Message::Terminal(super::messages::TerminalMsg::Output(session, bytes))
                    },
                    Ok(mbxt_core::UiEvent::TransferProgress {
                        session,
                        done,
                        total,
                    }) => Message::Sftp(super::messages::SftpMsg::TransferProgress(
                        session, done, total,
                    )),
                    Ok(mbxt_core::UiEvent::Error(error)) => {
                        tracing::warn!(%error, "session actor error");
                        Message::Ignored
                    },
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "session event bridge lagged");
                        continue;
                    },
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if output.send(message).await.is_err() {
                    break;
                }
            }
        }),
    )
}

/// SIGHUP/SIGINT/SIGTERM → `Message::System(ShutdownSignal)` (graceful
/// shutdown persists state before exiting; see `update::handle_system`).
fn shutdown_signals() -> Subscription<Message> {
    Subscription::run_with_id(
        0xA11,
        stream::channel(8, |mut output| async move {
            mbxt_system::signals::wait_for_shutdown().await;
            let _ = output
                .send(Message::System(SystemEvent::ShutdownSignal))
                .await;
        }),
    )
}

/// 30-second heartbeat: autosave tick + notification expiry (prompt 1.1:
/// "auto-save state every 30 seconds").
fn autosave_tick() -> Subscription<Message> {
    iced::time::every(Duration::from_secs(30)).map(|_| Message::AutosaveTick)
}

/// Clipboard change watcher. X11/Wayland provide no clipboard push events,
/// so we poll `copypasta` once per second and emit on change. The first
/// read establishes the baseline (and seeds the privacy mirror).
fn clipboard_watcher() -> Subscription<Message> {
    Subscription::run_with_id(
        0xC11,
        stream::channel(8, |mut output| async move {
            let mut last: Option<String> = None;
            loop {
                let content = tokio::task::spawn_blocking(|| {
                    mbxt_system::clipboard::Clipboard::new().get_text()
                })
                .await
                .ok()
                .flatten();

                if let Some(text) = content {
                    if last.as_deref() != Some(text.as_str()) {
                        last = Some(text.clone());
                        if output.send(Message::ClipboardChanged(text)).await.is_err() {
                            break; // UI gone — stop watching.
                        }
                    }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }),
    )
}

/// Forwards `TaskManager` broadcast events into the MVU loop.
/// `Lagged` means the UI was slow; task events are informational, so we
/// drop the backlog and continue (task board self-heals on the next event).
fn task_events() -> Subscription<Message> {
    Subscription::run_with_id(
        0x7A5,
        stream::channel(64, |mut output| async move {
            let mut rx: broadcast::Receiver<crate::task::TaskEvent> =
                TaskManager::shared().subscribe();
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if output.send(Message::Task(event)).await.is_err() {
                            break;
                        }
                    },
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "task event bridge lagged; resyncing");
                    },
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }),
    )
}

/// Auth prompt bridge (prompt 2.2): providers broadcast prompts; this
/// forwards them as `Message::AuthPrompt` so the modal renders and the
/// user's `AuthResponse` resolves the provider's oneshot via `PromptBridge`.
fn auth_prompts() -> Subscription<Message> {
    Subscription::run_with_id(
        0xA77,
        stream::channel(16, |mut output| async move {
            let mut rx = crate::connection::ssh::auth::PromptBridge::shared().subscribe();
            loop {
                match rx.recv().await {
                    Ok(prompt) => {
                        if output.send(Message::AuthPrompt(prompt)).await.is_err() {
                            break;
                        }
                    },
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "auth prompt bridge lagged; resyncing");
                    },
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }),
    )
}

/// Network reachability via NetworkManager (linux only, "if available").
///
/// TODO(prompt 1.2+): replace the `nmcli` poll with a native dbus-rs watch
/// on `org.freedesktop.NetworkManager::StateChanged` and screen-lock events
/// from `org.freedesktop.ScreenSaver` / `org.gnome.ScreenSaver` once the
/// system-integration layer lands (tech_stack §3.5); polling via the system
/// binary keeps behavior parity-safe in the meantime.
#[cfg(target_os = "linux")]
fn network_status() -> Subscription<Message> {
    Subscription::run_with_id(
        0xDB1,
        stream::channel(8, |mut output| async move {
            let mut last: Option<bool> = None;
            loop {
                let online = tokio::process::Command::new("nmcli")
                    .args(["-t", "-f", "STATE", "general"])
                    .output()
                    .await
                    .ok()
                    .map(|out| String::from_utf8_lossy(&out.stdout).contains("connected"))
                    .unwrap_or(true); // no nmcli → assume online, stay quiet

                if last != Some(online) {
                    last = Some(online);
                    let _ = output
                        .send(Message::System(SystemEvent::NetworkStatusChanged {
                            online,
                        }))
                        .await;
                }
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        }),
    )
}
