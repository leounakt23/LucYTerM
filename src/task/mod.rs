//! Task manager: schedules async work and reports progress via messages
//! (architecture §3.2 Command pattern + §4 concurrency model).
//!
//! - Work runs on the Tokio runtime (Iced's runtime for the GUI binary).
//! - Lifecycle events flow over a bounded `tokio::sync::broadcast` channel;
//!   the UI bridge (see `app::subscriptions`) forwards them as `Message::Task`.
//! - `TaskManager::shared()` is the sanctioned singleton handle for
//!   application-wide scheduling (architecture §3.6).
//!
//! Scheduling from `update()` (which runs on the UI thread, *outside* any
//! Tokio context) must go through `app::update::spawn_task`, which wraps the
//! spawn in `iced::Task::perform` so the actual `tokio::spawn` happens on the
//! runtime.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use tokio::sync::broadcast;

/// Unique task identifier.
pub type TaskId = u64;

/// Broadcast channel capacity. Slow subscribers get `Lagged` and resync —
/// task events are informational only.
const EVENT_CAPACITY: usize = 256;

/// Lifecycle/progress events for scheduled tasks.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskEvent {
    Started(TaskId),
    /// `(task, fraction 0.0..=1.0, human label)`
    Progress(TaskId, f32, String),
    Completed(TaskId),
    Failed(TaskId, String),
}

/// Schedules futures on the Tokio runtime and broadcasts their lifecycle.
#[derive(Debug)]
pub struct TaskManager {
    tx: broadcast::Sender<TaskEvent>,
    next_id: AtomicU64,
}

impl TaskManager {
    /// The process-wide singleton (constructed on first use).
    pub fn shared() -> &'static Self {
        static SHARED: OnceLock<TaskManager> = OnceLock::new();
        SHARED.get_or_init(|| Self {
            tx: broadcast::channel(EVENT_CAPACITY).0,
            next_id: AtomicU64::new(1),
        })
    }

    /// Subscribe to task lifecycle events (UI bridge uses this).
    pub fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.tx.subscribe()
    }

    /// Allocate the next task id.
    pub fn next_id(&self) -> TaskId {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Emit a raw event (progress emitters use this).
    pub fn emit(&self, event: TaskEvent) {
        // No subscribers is normal (headless mode); broadcast error is fine.
        let _ = self.tx.send(event);
    }

    /// Spawn `fut` on the Tokio runtime. Must be called from within a Tokio
    /// context (use `app::update::spawn_task` from the UI thread).
    ///
    /// The future's `Err` becomes [`TaskEvent::Failed`]; `Ok` becomes
    /// [`TaskEvent::Completed`]. All log lines emitted inside the task are
    /// correlated via an `info_span!("task", id)` (tracing spans, prompt 1.3).
    pub fn spawn<F, T>(&self, fut: F) -> TaskHandle
    where
        F: std::future::Future<Output = Result<T, String>> + Send + 'static,
        T: Send + 'static,
    {
        use tracing::Instrument;
        let id = self.next_id();
        let tx = self.tx.clone();
        let _ = tx.send(TaskEvent::Started(id));
        let span = tracing::info_span!("task", task_id = id);
        let join = tokio::spawn(
            async move {
                tracing::debug!("task started");
                match fut.await {
                    Ok(_) => {
                        let _ = tx.send(TaskEvent::Completed(id));
                    },
                    Err(err) => {
                        tracing::warn!("task failed: {err}");
                        let _ = tx.send(TaskEvent::Failed(id, err));
                    },
                }
            }
            .instrument(span),
        );
        TaskHandle { id, join }
    }

    /// Emit progress for `id` from inside a running task.
    pub fn progress(&self, id: TaskId, fraction: f32, label: impl Into<String>) {
        let fraction = fraction.clamp(0.0, 1.0);
        self.emit(TaskEvent::Progress(id, fraction, label.into()));
    }
}

/// Cancellable handle to a scheduled task.
pub struct TaskHandle {
    /// Task id (usable for progress reporting / cancellation requests).
    pub id: TaskId,
    join: tokio::task::JoinHandle<()>,
}

impl TaskHandle {
    /// Abort the underlying task (cancellation-safety: transfers checkpoint
    /// resume offsets before returning — feature matrix #30).
    pub fn abort(&self) {
        self.join.abort();
    }

    /// Wait for task completion (test/diagnostics use).
    pub async fn join(self) {
        let _ = self.join.await;
    }
}

/// Live status of tasks as tracked in app state.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Running,
    Failed(String),
}

/// Convenience type alias for the task board kept in [`crate::app`].
pub type TaskBoard = HashMap<TaskId, TaskStatus>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    async fn recv_with_timeout(rx: &mut broadcast::Receiver<TaskEvent>) -> TaskEvent {
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("event within timeout")
            .expect("channel open")
    }

    /// Receive broadcast events until `want` arrives, skipping events from
    /// other tests sharing the process-wide bus (parallel `cargo test`).
    async fn recv_until(
        rx: &mut broadcast::Receiver<TaskEvent>,
        want: impl Fn(&TaskEvent) -> bool,
    ) -> TaskEvent {
        for _ in 0..32 {
            let event = recv_with_timeout(rx).await;
            if want(&event) {
                return event;
            }
        }
        panic!("expected event not seen among shared-bus traffic");
    }

    #[tokio::test]
    async fn spawn_reports_started_and_completed() {
        let manager = TaskManager::shared();
        let mut rx = manager.subscribe();

        let handle = manager.spawn(async { Ok::<(), String>(()) });
        let id = handle.id;
        assert_eq!(
            recv_until(&mut rx, |e| *e == TaskEvent::Started(id)).await,
            TaskEvent::Started(id)
        );
        assert_eq!(
            recv_until(&mut rx, |e| *e == TaskEvent::Completed(id)).await,
            TaskEvent::Completed(id)
        );
        handle.join().await;
    }

    #[tokio::test]
    async fn spawn_reports_failure_with_message() {
        let manager = TaskManager::shared();
        let mut rx = manager.subscribe();

        let handle = manager.spawn(async { Err::<(), _>("boom".to_string()) });
        let id = handle.id;
        assert_eq!(
            recv_until(&mut rx, |e| *e == TaskEvent::Started(id)).await,
            TaskEvent::Started(id)
        );
        assert_eq!(
            recv_until(&mut rx, |e| *e == TaskEvent::Failed(id, "boom".to_string())).await,
            TaskEvent::Failed(id, "boom".to_string())
        );
        handle.join().await;
    }

    #[tokio::test]
    async fn progress_events_are_clamped_and_delivered() {
        let manager = TaskManager::shared();
        let mut rx = manager.subscribe();
        let id = manager.next_id();

        manager.progress(id, 1.7, "over");
        assert_eq!(
            recv_with_timeout(&mut rx).await,
            TaskEvent::Progress(id, 1.0, "over".to_string())
        );

        manager.progress(id, -0.5, "under");
        assert_eq!(
            recv_with_timeout(&mut rx).await,
            TaskEvent::Progress(id, 0.0, "under".to_string())
        );
    }

    #[tokio::test]
    async fn abort_cancels_running_task() {
        let manager = TaskManager::shared();
        let handle = manager.spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok::<(), String>(())
        });
        handle.abort();
        // Join returns promptly after abort (cancelled tasks yield None).
        tokio::time::timeout(Duration::from_secs(2), handle.join())
            .await
            .expect("aborted task finishes promptly");
    }
}
