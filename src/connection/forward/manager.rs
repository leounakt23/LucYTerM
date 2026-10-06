//! `ForwardManager`: forward definitions, runtime, stats, traffic logs.
//!
//! One manager process-wide (like `SessionManager`): definitions persist
//! through the session store (`SessionSpec.forwards`), runtime is per
//! `(session, forward)` and dies with disconnect/stop. Auto-start forwards
//! launch on every connect, so reconnects transparently rebuild tunnels.
//!
//! Threading split (MVU discipline): definitions and synchronous state flip
//! on the UI thread; every network round trip (`bind` is local-fast, but
//! `request_remote_forward` is not) runs in `start_async`/`stop_async`,
//! which the update layer drives through iced `Task`s — the UI thread never
//! blocks. Socket/task discipline (quality bar): every listener and every
//! proxied connection is a tracked task; stop aborts the accept task plus
//! all children and drops the listener — no sockets or tasks leak.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use mbxt_core::{ForwardDef, ForwardType, SessionId};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::local::{bind_with_fallback, LogSink};
use super::{ChildTracker, ForwardCounters, ForwardStats};
use crate::connection::actor::SessionManager;
use crate::connection::sftp::CancelToken;

/// Runtime status of one forward.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ForwardStatus {
    Stopped,
    Starting,
    Running,
    Failed(String),
}

impl std::fmt::Debug for RunningForward {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Task handles are not `Debug` — render the plannable state.
        f.debug_struct("RunningForward")
            .field("session", &self.session)
            .field("bound_port", &self.bound_port)
            .field("remote_listen", &self.remote_listen)
            .finish_non_exhaustive()
    }
}

/// Runtime snapshot: definition + live status, stats, and bound port.
/// (`session` is the `u64` session id used across this codebase — the
/// spec's `Uuid` shape adapted.)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PortForward {
    pub id: Uuid,
    pub session: SessionId,
    pub name: Option<String>,
    pub forward_type: ForwardType,
    pub auto_start: bool,
    pub status: ForwardStatus,
    pub created_secs: u64,
    pub statistics: ForwardStats,
    /// Actual local port (differs after bind fallback).
    pub bound_port: Option<u16>,
}

struct RunningForward {
    session: SessionId,
    counters: Arc<ForwardCounters>,
    log: LogSink,
    cancel: CancelToken,
    children: ChildTracker,
    accept_task: Option<tokio::task::JoinHandle<()>>,
    bound_port: Option<u16>,
    created_secs: u64,
    /// Remote listen endpoint for `-R` release (`(bind, requested port)`).
    remote_listen: Option<(String, u32)>,
}

/// Definitions + runtime for every tunnel.
#[derive(Debug, Default)]
pub struct ForwardManager {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    /// Definitions by id (mirrors the session store on every mutation).
    defs: HashMap<Uuid, (SessionId, ForwardDef)>,
    runtime: HashMap<Uuid, RunningForward>,
    /// Last failure per id (survives runtime removal for the panel).
    last_error: HashMap<Uuid, String>,
}

impl ForwardManager {
    /// Process-wide singleton (mirrors `SessionManager`).
    pub fn shared() -> &'static Self {
        static INSTANCE: OnceLock<ForwardManager> = OnceLock::new();
        INSTANCE.get_or_init(ForwardManager::default)
    }

    /// Isolated manager (tests + embedding).
    pub fn new() -> Self {
        Self::default()
    }

    // -- definitions (persisted via the session store by the caller) --------

    /// Register a definition; returns its id.
    pub fn define(&self, session: SessionId, def: ForwardDef) -> Uuid {
        let id = def.id;
        if let Ok(mut inner) = self.inner.lock() {
            inner.defs.insert(id, (session, def));
        }
        id
    }

    /// Drop a definition (stops it first).
    pub fn remove(&self, id: Uuid) -> Result<(), String> {
        self.stop_sync(id);
        if let Ok(mut inner) = self.inner.lock() {
            inner.defs.remove(&id);
            inner.last_error.remove(&id);
        }
        Ok(())
    }

    /// Drop every definition and runtime of a deleted session.
    pub fn remove_session(&self, session: SessionId) {
        let ids: Vec<Uuid> = self
            .inner
            .lock()
            .map(|inner| {
                inner
                    .defs
                    .iter()
                    .filter(|(_, (owner, _))| *owner == session)
                    .map(|(id, _)| *id)
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            let _ = self.remove(id);
        }
    }

    /// Definitions for one session (panel listing).
    pub fn definitions_for(&self, session: SessionId) -> Vec<(Uuid, ForwardDef)> {
        self.inner
            .lock()
            .map(|inner| {
                inner
                    .defs
                    .iter()
                    .filter(|(_, (owner, _))| *owner == session)
                    .map(|(id, (_, def))| (*id, def.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    // -- runtime -------------------------------------------------------------

    /// Establish a defined forward (async: binds listeners, requests remote
    /// listens). Called from iced tasks — never blocks the UI thread.
    /// Returns a short human summary for the notification.
    pub async fn start_async(&self, id: Uuid) -> Result<String, String> {
        let (session, def) = self
            .inner
            .lock()
            .map_err(|_| "forward registry poisoned".to_string())?
            .defs
            .get(&id)
            .map(|(session, def)| (*session, def.clone()))
            .ok_or_else(|| "unknown forward".to_string())?;
        self.stop_sync(id);
        let summary = self.establish(session, &def).await?;
        Ok(summary)
    }

    /// Tear a forward down (async: releases remote listens).
    pub async fn stop_async(&self, id: Uuid) -> Result<(), String> {
        let running = self
            .inner
            .lock()
            .map_err(|_| "forward registry poisoned".to_string())?
            .runtime
            .remove(&id);
        if let Some(running) = running {
            shutdown_running(running).await;
        }
        Ok(())
    }

    /// Synchronous stop (disconnect path + `remove`): cancels tasks and
    /// drops sockets now; the server-side listen dies with the connection,
    /// so no release round trip is needed here.
    pub fn stop_sync(&self, id: Uuid) {
        let runtime = self
            .inner
            .lock()
            .map(|mut inner| inner.runtime.remove(&id))
            .unwrap_or(None);
        if let Some(running) = runtime {
            running.cancel.cancel();
            if let Some(task) = running.accept_task {
                task.abort();
            }
            running.children.abort_all();
            running.log.push("tunnel stopped".to_string());
            if running.remote_listen.is_some() {
                let _ = SessionManager::shared().register_forwarded_sink(running.session, None);
            }
        }
    }

    /// Snapshot one forward (panel row).
    pub fn snapshot(&self, id: Uuid) -> Option<PortForward> {
        let inner = self.inner.lock().expect("forward registry poisoned");
        let (session, def) = inner.defs.get(&id)?;
        let (status, statistics, bound_port) = match inner.runtime.get(&id) {
            Some(running) => (
                ForwardStatus::Running,
                running.counters.snapshot(Some(running.created_secs)),
                running.bound_port,
            ),
            None => (
                inner
                    .last_error
                    .get(&id)
                    .map(|reason| ForwardStatus::Failed(reason.clone()))
                    .unwrap_or(ForwardStatus::Stopped),
                ForwardStats {
                    bytes_sent: 0,
                    bytes_received: 0,
                    active_connections: 0,
                    total_connections: 0,
                    start_secs: None,
                },
                None,
            ),
        };
        Some(PortForward {
            id,
            session: *session,
            name: def.name.clone(),
            forward_type: def.forward_type.clone(),
            auto_start: def.auto_start,
            status,
            created_secs: running_created(&inner, &id),
            statistics,
            bound_port,
        })
    }

    /// Snapshots for one session (panel listing).
    pub fn snapshots_for(&self, session: SessionId) -> Vec<PortForward> {
        let ids: Vec<Uuid> = self
            .inner
            .lock()
            .map(|inner| {
                inner
                    .defs
                    .iter()
                    .filter(|(_, (owner, _))| *owner == session)
                    .map(|(id, _)| *id)
                    .collect()
            })
            .unwrap_or_default();
        ids.into_iter().filter_map(|id| self.snapshot(id)).collect()
    }

    /// Traffic-log tail for one forward.
    pub fn log_tail(&self, id: Uuid, count: usize) -> Vec<String> {
        self.inner
            .lock()
            .map(|inner| {
                inner
                    .runtime
                    .get(&id)
                    .map(|running| running.log.tail(count))
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    /// Ids of `auto_start` forwards for a session (connect path).
    pub fn autostart_ids(&self, session: SessionId) -> Vec<Uuid> {
        self.inner
            .lock()
            .map(|inner| {
                inner
                    .defs
                    .iter()
                    .filter(|(_, (owner, def))| *owner == session && def.auto_start)
                    .map(|(id, _)| *id)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Stop every runtime of a session (disconnect/reconnect path — defs
    /// kept, so the next connect re-autostarts).
    pub fn note_ssh_disconnected(&self, session: SessionId) {
        let ids: Vec<Uuid> = self
            .inner
            .lock()
            .map(|inner| {
                inner
                    .runtime
                    .iter()
                    .filter(|(_, running)| running.session == session)
                    .map(|(id, _)| *id)
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            self.stop_sync(id);
        }
    }

    // -- establishment -------------------------------------------------------

    /// Establish a forward, recording failures for the panel. Split from
    /// `establish_inner` because `?` inside the arms would otherwise skip
    /// the recording step on the way out.
    async fn establish(&self, session: SessionId, def: &ForwardDef) -> Result<String, String> {
        let result = self.establish_inner(session, def).await;
        if let Err(reason) = &result {
            if let Ok(mut inner) = self.inner.lock() {
                inner.last_error.insert(def.id, reason.clone());
            }
        }
        result
    }

    async fn establish_inner(
        &self,
        session: SessionId,
        def: &ForwardDef,
    ) -> Result<String, String> {
        let counters = Arc::new(ForwardCounters::default());
        let log = LogSink::new(50);
        let cancel = CancelToken::new();
        let children = ChildTracker::default();
        match def.forward_type.clone() {
            ForwardType::Local {
                local_host,
                local_port,
                remote_host,
                remote_port,
            } => {
                // Port-in-use surfaces here with the fallback suggestion.
                let (listener, actual) = bind_with_fallback(&local_host, local_port)
                    .await
                    .map_err(|err| err.to_string())?;
                if actual != local_port {
                    log.push(format!("port {local_port} in use, bound {actual} instead"));
                }
                let opener = move || {
                    let (remote_host, remote_port) = (remote_host.clone(), remote_port);
                    async move {
                        SessionManager::shared()
                            .open_direct_channel(session, &remote_host, u32::from(remote_port))
                            .await
                            .map_err(super::ForwardError::Ssh)
                    }
                };
                let accept_task = tokio::spawn(local_loop(
                    listener,
                    opener,
                    Arc::clone(&counters),
                    log.clone(),
                    cancel.clone(),
                    children.clone(),
                ));
                self.insert_running(
                    def,
                    session,
                    counters,
                    log,
                    cancel,
                    children,
                    Some(accept_task),
                    Some(actual),
                    None,
                );
                Ok(format!("listening on {local_host}:{actual}"))
            },
            ForwardType::Dynamic {
                local_host,
                local_port,
            } => {
                let (listener, actual) = bind_with_fallback(&local_host, local_port)
                    .await
                    .map_err(|err| err.to_string())?;
                if actual != local_port {
                    log.push(format!("port {local_port} in use, bound {actual} instead"));
                }
                let accept_task = tokio::spawn(dynamic_loop(
                    listener,
                    session,
                    Arc::clone(&counters),
                    log.clone(),
                    cancel.clone(),
                    children.clone(),
                ));
                self.insert_running(
                    def,
                    session,
                    counters,
                    log,
                    cancel,
                    children,
                    Some(accept_task),
                    Some(actual),
                    None,
                );
                Ok(format!("SOCKS proxy on {local_host}:{actual}"))
            },
            ForwardType::Remote {
                remote_host,
                remote_port,
                local_host,
                local_port,
            } => {
                let (tx, rx) = mpsc::unbounded_channel();
                SessionManager::shared()
                    .register_forwarded_sink(session, Some(tx))
                    .map_err(|err| format!("session #{session} has no live connection ({err})"))?;
                let bound = SessionManager::shared()
                    .request_remote_forward(session, &remote_host, u32::from(remote_port))
                    .await
                    .inspect_err(|_| {
                        let _ = SessionManager::shared().register_forwarded_sink(session, None);
                    })?;
                // The sender half lives in the runtime entry: dropping it on
                // stop ends the accept loop even mid-idle.
                let summary = format!("remote {remote_host}:{bound} → {local_host}:{local_port}");
                let accept_task = tokio::spawn(remote_loop(
                    rx,
                    remote_host.clone(),
                    u32::from(remote_port),
                    local_host,
                    local_port,
                    Arc::clone(&counters),
                    log.clone(),
                    cancel.clone(),
                    children.clone(),
                ));
                self.insert_running(
                    def,
                    session,
                    counters,
                    log,
                    cancel,
                    children,
                    Some(accept_task),
                    Some(bound as u16),
                    Some((remote_host, u32::from(remote_port))),
                );
                Ok(summary)
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_running(
        &self,
        def: &ForwardDef,
        session: SessionId,
        counters: Arc<ForwardCounters>,
        log: LogSink,
        cancel: CancelToken,
        children: ChildTracker,
        accept_task: Option<tokio::task::JoinHandle<()>>,
        bound_port: Option<u16>,
        remote_listen: Option<(String, u32)>,
    ) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.last_error.remove(&def.id);
            inner.runtime.insert(
                def.id,
                RunningForward {
                    session,
                    counters,
                    log,
                    cancel,
                    children,
                    accept_task,
                    bound_port,
                    created_secs: crate::macros::now_secs(),
                    remote_listen,
                },
            );
        }
    }
}

fn running_created(inner: &Inner, id: &Uuid) -> u64 {
    inner
        .runtime
        .get(id)
        .map(|running| running.created_secs)
        .unwrap_or(0)
}

/// Stop a runtime: cancel, abort tasks, best-effort `-R` release.
async fn shutdown_running(mut running: RunningForward) {
    running.cancel.cancel();
    if let Some(task) = running.accept_task.take() {
        task.abort();
    }
    running.children.abort_all();
    if let Some((bind, port)) = running.remote_listen.take() {
        running.log.push("releasing remote listen".to_string());
        let _ = SessionManager::shared()
            .cancel_remote_forward(running.session, &bind, port)
            .await;
        let _ = SessionManager::shared().register_forwarded_sink(running.session, None);
    }
    running.log.push("tunnel stopped".to_string());
}

/// Local accept loop (owned task; children tracked for abort-on-stop).
async fn local_loop<O, OpenFut>(
    listener: tokio::net::TcpListener,
    opener: O,
    counters: Arc<ForwardCounters>,
    log: LogSink,
    cancel: CancelToken,
    children: ChildTracker,
) where
    O: Fn() -> OpenFut + Send + Sync + 'static,
    OpenFut: std::future::Future<Output = Result<mbxt_connections::ForwardStream, super::ForwardError>>
        + Send,
{
    let _ =
        super::local::run_local_forward(listener, opener, counters, log, cancel, children).await;
}

/// Dynamic accept loop: SOCKS handshake per peer, upstream via channels.
async fn dynamic_loop(
    listener: tokio::net::TcpListener,
    session: SessionId,
    counters: Arc<ForwardCounters>,
    log: LogSink,
    cancel: CancelToken,
    children: ChildTracker,
) {
    let opener = move |host: String, port: u16| async move {
        SessionManager::shared()
            .open_direct_channel(session, &host, u32::from(port))
            .await
            .map_err(super::dynamic::SocksError::Protocol)
    };
    let _ = super::dynamic::run_dynamic_forward(listener, opener, counters, log, cancel, children)
        .await;
}

/// Remote accept loop: server channels → local dial.
async fn remote_loop(
    rx: mpsc::UnboundedReceiver<mbxt_connections::ForwardedTcpIp>,
    expected_host: String,
    expected_port: u32,
    local_host: String,
    local_port: u16,
    counters: Arc<ForwardCounters>,
    log: LogSink,
    cancel: CancelToken,
    children: ChildTracker,
) {
    let dial = move || {
        let (local_host, local_port) = (local_host.clone(), local_port);
        async move {
            tokio::net::TcpStream::connect((local_host.as_str(), local_port))
                .await
                .map_err(super::ForwardError::io)
        }
    };
    let _ = super::remote::run_remote_forward(
        rx,
        expected_host,
        expected_port,
        dial,
        counters,
        log,
        cancel,
        children,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(forward_type: ForwardType) -> ForwardDef {
        ForwardDef::new(None, forward_type, false)
    }

    #[test]
    fn define_snapshot_remove_lifecycle() {
        let manager = ForwardManager::new();
        let id = manager.define(
            11,
            def(ForwardType::Local {
                local_host: "127.0.0.1".into(),
                local_port: 18080,
                remote_host: "db".into(),
                remote_port: 5432,
            }),
        );
        let snapshot = manager.snapshot(id).expect("defined");
        assert_eq!(snapshot.session, 11);
        assert_eq!(snapshot.status, ForwardStatus::Stopped);
        assert_eq!(snapshot.bound_port, None);
        assert_eq!(manager.snapshots_for(11).len(), 1);
        assert!(manager.snapshots_for(12).is_empty());
        manager.remove(id).expect("remove");
        assert!(manager.snapshot(id).is_none());
    }

    #[test]
    fn unknown_ids_error_cleanly() {
        let manager = ForwardManager::new();
        assert!(manager.snapshot(uuid::Uuid::new_v4()).is_none());
        assert!(manager.log_tail(uuid::Uuid::new_v4(), 5).is_empty());
        manager.remove(uuid::Uuid::new_v4()).expect("idempotent");
    }

    #[test]
    fn failed_starts_record_actionable_status() {
        // No live session: the remote request fails fast, and the panel
        // shows why (local binds succeed without any session, so `-R` is
        // the honest failure probe here).
        let manager = ForwardManager::new();
        let id = manager.define(
            999_111,
            def(ForwardType::Remote {
                remote_host: "127.0.0.1".into(),
                remote_port: 19999,
                local_host: "127.0.0.1".into(),
                local_port: 80,
            }),
        );
        let outcome = futures_block_on_test(manager.start_async(id));
        assert!(outcome.is_err());
        assert!(matches!(
            manager.snapshot(id).map(|s| s.status),
            Some(ForwardStatus::Failed(_))
        ));
    }

    /// Block on a future from sync test code (tests run on runtimes where
    /// this is legal; production paths stay async Tasks).
    fn futures_block_on_test<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(future)
    }

    #[test]
    fn port_forward_snapshot_round_trips_through_ron() {
        let forward = PortForward {
            id: uuid::Uuid::new_v4(),
            session: 3,
            name: Some("db".into()),
            forward_type: ForwardType::Dynamic {
                local_host: "127.0.0.1".into(),
                local_port: 1080,
            },
            auto_start: true,
            status: ForwardStatus::Running,
            created_secs: 1_700_000_000,
            statistics: ForwardStats {
                bytes_sent: 10,
                bytes_received: 20,
                active_connections: 1,
                total_connections: 2,
                start_secs: Some(1_700_000_100),
            },
            bound_port: Some(1080),
        };
        let text = ron::ser::to_string_pretty(&forward, ron::ser::PrettyConfig::default()).unwrap();
        assert_eq!(ron::from_str::<PortForward>(&text).unwrap(), forward);
    }
}
