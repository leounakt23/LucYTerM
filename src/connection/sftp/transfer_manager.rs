//! `TransferManager`: concurrent transfer queue with throttling, pause/resume,
//! and retry (Prompt 3.2).
//!
//! Split of responsibilities (architecture §3):
//! - The manager owns the **state machine**: records, FIFO queue, slot
//!   accounting, retry backoff. It is runtime-agnostic and fully
//!   headless-testable — no sockets, no tasks spawned here.
//! - I/O lives in `session.rs` (`get_file`/`put_file`) and is driven by iced
//!   `Task`s in `app::update`, which call the `on_*` hooks below. Progress
//!   fans out through the existing `SessionManager` event bus, so the current
//!   session-keyed UI keeps working while the manager adds per-transfer
//!   control.
//!
//! Handle discipline (quality bar): each transfer owns a [`CancelToken`];
//! pause/cancel fire it, and the executor future drops its file handles on
//! scope exit — nothing leaks across pause/cancel/finish. Resume requeues
//! with a fresh token; the session layer recomputes the resume offset from
//! destination length, so resumed transfers continue where they stopped.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use mbxt_core::SessionId;

use super::transfer::{Backoff, Direction, Transfer, TransferId, TransferStatus};
use super::SftpError;

/// Concurrency + bandwidth + retry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferConfig {
    /// Max simultaneous running transfers (default 4).
    pub max_concurrent: usize,
    /// Global bandwidth cap in bytes/sec (`None` == unlimited).
    pub global_throttle_bps: Option<u64>,
    /// Retries after the first failure (default 3).
    pub max_retries: u32,
    /// Retry backoff schedule.
    pub backoff: Backoff,
}

impl Default for TransferConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            global_throttle_bps: None,
            max_retries: 3,
            backoff: Backoff::default(),
        }
    }
}

/// Which transfers to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferFilter {
    All,
    Active,
    Pending,
    Terminal,
    Failed,
    BySession(SessionId),
}

impl TransferFilter {
    fn matches(&self, transfer: &Transfer) -> bool {
        match self {
            Self::All => true,
            Self::Active => transfer.status.is_active(),
            Self::Pending => transfer.status == TransferStatus::Pending,
            Self::Terminal => transfer.status.is_terminal(),
            Self::Failed => matches!(transfer.status, TransferStatus::Failed(_)),
            Self::BySession(session) => transfer.session == *session,
        }
    }
}

/// Outcome of a finished executor task (drives update-layer messaging).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishOutcome {
    /// Terminal state reached; start queued transfers.
    Done,
    /// Retryable failure; a delayed requeue happened — the update layer
    /// should schedule a pump after `RetryDelay`.
    RetryScheduled { delay: std::time::Duration },
}

/// Concurrent transfer queue (process-wide singleton + isolated instances).
#[derive(Debug)]
pub struct TransferManager {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    config: TransferConfig,
    transfers: HashMap<TransferId, Transfer>,
    queue: VecDeque<TransferId>,
    /// Earliest (re)start time for delayed retries.
    not_before: HashMap<TransferId, Instant>,
}

impl TransferManager {
    /// Isolated manager (tests + embedding).
    pub fn new(config: TransferConfig) -> Self {
        Self {
            inner: Mutex::new(Inner {
                config,
                transfers: HashMap::new(),
                queue: VecDeque::new(),
                not_before: HashMap::new(),
            }),
        }
    }

    /// Process-wide singleton (mirrors `SessionManager`/`SftpManager`).
    pub fn shared() -> &'static Self {
        static INSTANCE: OnceLock<TransferManager> = OnceLock::new();
        INSTANCE.get_or_init(|| Self::new(TransferConfig::default()))
    }

    /// Queue a transfer; returns its id. Starts immediately when a slot is
    /// free — the caller pumps with [`TransferManager::next_ready`].
    pub fn submit(
        &self,
        session: SessionId,
        direction: Direction,
        local_path: &Path,
        remote_path: &str,
        total: Option<u64>,
        throttle_bps: Option<u64>,
    ) -> TransferId {
        let mut inner = self.inner.lock().expect("transfer registry poisoned");
        let effective = throttle_bps.or(inner.config.global_throttle_bps);
        let transfer = Transfer::new(
            session,
            direction,
            local_path,
            remote_path,
            total,
            effective,
        );
        let id = transfer.id;
        inner.queue.push_back(id);
        inner.transfers.insert(id, transfer);
        id
    }

    /// Number of currently running transfers.
    pub fn active_count(&self) -> usize {
        self.inner
            .lock()
            .expect("transfer registry poisoned")
            .transfers
            .values()
            .filter(|t| t.status == TransferStatus::InProgress)
            .count()
    }

    /// Queued (pending, not started) transfers.
    pub fn queued_count(&self) -> usize {
        self.inner
            .lock()
            .expect("transfer registry poisoned")
            .queue
            .len()
    }

    /// Pop the next startable transfer and mark it `InProgress`.
    ///
    /// Respects the concurrency limit and retry delays. Returns the record
    /// snapshot; the caller spawns the executor and reports back through
    /// [`TransferManager::on_progress`]/[`TransferManager::on_finished`].
    pub fn next_ready(&self) -> Option<Transfer> {
        let mut inner = self.inner.lock().expect("transfer registry poisoned");
        if inner
            .transfers
            .values()
            .filter(|t| t.status == TransferStatus::InProgress)
            .count()
            >= inner.config.max_concurrent.max(1)
        {
            return None;
        }
        let now = Instant::now();
        let position = inner.queue.iter().position(|id| {
            let ready = inner.not_before.get(id).map(|t| now >= *t).unwrap_or(true);
            ready
                && matches!(
                    inner.transfers.get(id).map(|t| &t.status),
                    Some(TransferStatus::Pending)
                )
        })?;
        let id = inner.queue.remove(position).expect("queue position valid");
        let transfer = inner
            .transfers
            .get_mut(&id)
            .expect("queued transfer exists");
        transfer.status = TransferStatus::InProgress;
        transfer.attempts += 1;
        if transfer.started.is_none() {
            transfer.started = Some(Instant::now());
        }
        transfer.updated = Instant::now();
        // Fresh token per attempt; resume recomputes offsets from lengths.
        transfer.cancel = super::transfer::CancelToken::new();
        Some(transfer.clone())
    }

    /// Record chunk progress (executor hook, per chunk).
    pub fn on_progress(&self, id: TransferId, done: u64, total: Option<u64>) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(transfer) = inner.transfers.get_mut(&id) {
                transfer.done = done;
                if total.is_some() {
                    transfer.total = total;
                }
                transfer.updated = Instant::now();
            }
        }
    }

    /// Record a finished executor task.
    ///
    /// - `Ok(())` → `Completed`.
    /// - `Err(Cancelled)` while `Paused` → stays `Paused` (resume requeues).
    /// - `Err(Cancelled)` otherwise → `Cancelled`.
    /// - Retryable error with attempts left → `Pending` + delayed requeue.
    /// - Other errors → `Failed(reason)`.
    pub fn on_finished(&self, id: TransferId, result: Result<(), SftpError>) -> FinishOutcome {
        // Copy policy first: `transfers.get_mut` below borrows `inner`
        // mutably, so config must be owned up front (borrowck discipline).
        let (backoff, max_retries) = {
            let inner = self.inner.lock().expect("transfer registry poisoned");
            (inner.config.backoff, inner.config.max_retries)
        };
        let mut inner = self.inner.lock().expect("transfer registry poisoned");
        let Some(transfer) = inner.transfers.get_mut(&id) else {
            return FinishOutcome::Done;
        };
        transfer.updated = Instant::now();
        match result {
            Ok(()) => {
                transfer.status = TransferStatus::Completed;
                FinishOutcome::Done
            },
            Err(SftpError::Cancelled) if transfer.status == TransferStatus::Paused => {
                FinishOutcome::Done
            },
            Err(SftpError::Cancelled) => {
                transfer.status = TransferStatus::Cancelled;
                FinishOutcome::Done
            },
            Err(err) => {
                let attempts_used = transfer.attempts.saturating_sub(1);
                if backoff.should_retry(&err) && attempts_used < max_retries {
                    let delay = backoff.delay(attempts_used);
                    transfer.status = TransferStatus::Pending;
                    inner.not_before.insert(id, Instant::now() + delay);
                    if !inner.queue.contains(&id) {
                        inner.queue.push_back(id);
                    }
                    FinishOutcome::RetryScheduled { delay }
                } else {
                    transfer.status = TransferStatus::Failed(err.to_string());
                    FinishOutcome::Done
                }
            },
        }
    }

    /// Pause: queued → `Paused` (dequeued); running → `Paused` + cancel fired
    /// (the executor observes it within one chunk and exits; handles drop).
    pub fn pause(&self, id: TransferId) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "transfer registry poisoned".to_string())?;
        let transfer = inner
            .transfers
            .get_mut(&id)
            .ok_or_else(|| format!("unknown {id}"))?;
        match transfer.status {
            TransferStatus::Pending | TransferStatus::InProgress => {
                transfer.status = TransferStatus::Paused;
                transfer.cancel.cancel();
                transfer.updated = Instant::now();
                inner.queue.retain(|queued| *queued != id);
                Ok(())
            },
            _ => Err(format!(
                "{id} is not pausable ({})",
                transfer.status.label()
            )),
        }
    }

    /// Resume: `Paused` → `Pending` requeue (fresh token on next start;
    /// offsets recompute from destination length at execution).
    pub fn resume(&self, id: TransferId) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "transfer registry poisoned".to_string())?;
        let transfer = inner
            .transfers
            .get_mut(&id)
            .ok_or_else(|| format!("unknown {id}"))?;
        if transfer.status != TransferStatus::Paused {
            return Err(format!("{id} is not paused"));
        }
        transfer.status = TransferStatus::Pending;
        transfer.updated = Instant::now();
        inner.not_before.remove(&id);
        inner.queue.push_back(id);
        Ok(())
    }

    /// Cancel: any active transfer → `Cancelled` + cancel fired + dequeued.
    /// Terminal transfers are left alone (use `clear_finished` to drop them).
    pub fn cancel(&self, id: TransferId) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "transfer registry poisoned".to_string())?;
        let transfer = inner
            .transfers
            .get_mut(&id)
            .ok_or_else(|| format!("unknown {id}"))?;
        if transfer.status.is_terminal() {
            return Err(format!("{id} already finished"));
        }
        transfer.status = TransferStatus::Cancelled;
        transfer.cancel.cancel();
        transfer.updated = Instant::now();
        inner.queue.retain(|queued| *queued != id);
        inner.not_before.remove(&id);
        Ok(())
    }

    /// Snapshot one transfer.
    pub fn get_status(&self, id: TransferId) -> Option<Transfer> {
        self.inner
            .lock()
            .expect("transfer registry poisoned")
            .transfers
            .get(&id)
            .cloned()
    }

    /// Snapshot transfers matching `filter` (panel order: active first).
    pub fn list_transfers(&self, filter: TransferFilter) -> Vec<Transfer> {
        let inner = self.inner.lock().expect("transfer registry poisoned");
        let mut out: Vec<Transfer> = inner
            .transfers
            .values()
            .filter(|t| filter.matches(t))
            .cloned()
            .collect();
        out.sort_by_key(|t| (!t.status.is_active(), t.id.0));
        out
    }

    /// Drop terminal records (Completed/Failed/Cancelled) to bound memory.
    pub fn clear_finished(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.transfers.retain(|_, t| !t.status.is_terminal());
            let live: std::collections::HashSet<TransferId> =
                inner.transfers.keys().copied().collect();
            inner.queue.retain(|id| live.contains(id));
            inner.not_before.retain(|id, _| live.contains(id));
        }
    }

    /// Current policy (UI display + executor throttle default).
    pub fn config(&self) -> TransferConfig {
        self.inner
            .lock()
            .expect("transfer registry poisoned")
            .config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn manager(max_concurrent: usize) -> TransferManager {
        TransferManager::new(TransferConfig {
            max_concurrent,
            ..TransferConfig::default()
        })
    }

    fn submit(manager: &TransferManager) -> TransferId {
        manager.submit(
            7,
            Direction::Upload,
            Path::new("/tmp/a.bin"),
            "/remote/a.bin",
            Some(100),
            None,
        )
    }

    #[test]
    fn concurrency_limit_queues_excess() {
        let manager = manager(1);
        let first = submit(&manager);
        let second = submit(&manager);
        assert!(manager.next_ready().is_some());
        assert_eq!(manager.active_count(), 1);
        // Slot occupied: second stays queued.
        assert!(manager.next_ready().is_none());
        assert_eq!(manager.queued_count(), 1);
        manager.on_finished(first, Ok(()));
        let next = manager.next_ready().expect("slot freed");
        assert_eq!(next.id, second);
    }

    #[test]
    fn pause_and_resume_cycle() {
        let manager = manager(4);
        let id = submit(&manager);
        manager.pause(id).expect("pause queued");
        assert_eq!(
            manager.get_status(id).unwrap().status,
            TransferStatus::Paused
        );
        // Paused transfers are not handed out.
        assert!(manager.next_ready().is_none());
        manager.resume(id).expect("resume");
        let started = manager.next_ready().expect("resumed start");
        assert_eq!(started.id, id);
        assert_eq!(started.status, TransferStatus::InProgress);
    }

    #[test]
    fn pause_running_fires_cancel_token() {
        let manager = manager(4);
        let id = submit(&manager);
        let started = manager.next_ready().expect("start");
        assert!(!started.cancel.is_cancelled());
        manager.pause(id).expect("pause running");
        // The stored record's token is fired; the executor sees it in ≤1 chunk.
        assert!(manager.get_status(id).unwrap().cancel.is_cancelled());
        // Executor exits Cancelled while Paused → stays Paused.
        let outcome = manager.on_finished(id, Err(SftpError::Cancelled));
        assert_eq!(outcome, FinishOutcome::Done);
        assert_eq!(
            manager.get_status(id).unwrap().status,
            TransferStatus::Paused
        );
    }

    #[test]
    fn cancel_running_marks_cancelled() {
        let manager = manager(4);
        let id = submit(&manager);
        manager.next_ready().expect("start");
        manager.cancel(id).expect("cancel");
        assert!(manager.get_status(id).unwrap().cancel.is_cancelled());
        let outcome = manager.on_finished(id, Err(SftpError::Cancelled));
        assert_eq!(outcome, FinishOutcome::Done);
        assert_eq!(
            manager.get_status(id).unwrap().status,
            TransferStatus::Cancelled
        );
    }

    #[test]
    fn retryable_failures_requeue_with_backoff() {
        let manager = TransferManager::new(TransferConfig {
            max_concurrent: 4,
            max_retries: 2,
            ..TransferConfig::default()
        });
        let id = submit(&manager);
        manager.next_ready().expect("start");
        let outcome = manager.on_finished(id, Err(SftpError::Io("reset".into())));
        match outcome {
            FinishOutcome::RetryScheduled { delay } => {
                assert!(delay >= std::time::Duration::from_millis(500));
            },
            FinishOutcome::Done => panic!("expected a retry"),
        }
        assert_eq!(
            manager.get_status(id).unwrap().status,
            TransferStatus::Pending
        );
        // Non-retryable errors fail immediately.
        let id2 = submit(&manager);
        manager.next_ready();
        let outcome = manager.on_finished(id2, Err(SftpError::Cancelled));
        assert_eq!(outcome, FinishOutcome::Done);
        assert_eq!(
            manager.get_status(id2).unwrap().status,
            TransferStatus::Cancelled
        );
    }

    #[test]
    fn retries_exhaust_into_failed() {
        let manager = TransferManager::new(TransferConfig {
            max_concurrent: 4,
            max_retries: 0,
            ..TransferConfig::default()
        });
        let id = submit(&manager);
        manager.next_ready().expect("start");
        let outcome = manager.on_finished(id, Err(SftpError::Io("down".into())));
        assert_eq!(outcome, FinishOutcome::Done);
        assert!(matches!(
            manager.get_status(id).unwrap().status,
            TransferStatus::Failed(_)
        ));
    }

    #[test]
    fn list_filters_and_clear_finished() {
        let manager = manager(4);
        let id = submit(&manager);
        assert_eq!(manager.list_transfers(TransferFilter::All).len(), 1);
        assert_eq!(manager.list_transfers(TransferFilter::Pending).len(), 1);
        manager.next_ready();
        manager.on_finished(id, Ok(()));
        assert_eq!(manager.list_transfers(TransferFilter::Terminal).len(), 1);
        assert_eq!(
            manager.list_transfers(TransferFilter::BySession(7)).len(),
            1
        );
        assert!(manager.list_transfers(TransferFilter::Active).is_empty());
        manager.clear_finished();
        assert!(manager.list_transfers(TransferFilter::All).is_empty());
    }

    #[test]
    fn unknown_ids_error_cleanly() {
        let manager = manager(4);
        let missing = TransferId(999_999);
        assert!(manager.pause(missing).is_err());
        assert!(manager.resume(missing).is_err());
        assert!(manager.cancel(missing).is_err());
        assert!(manager.get_status(missing).is_none());
    }
}
