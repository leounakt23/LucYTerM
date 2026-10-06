//! Notifications: bounded, auto-expiring user-facing messages (architecture
//! §7 error handling — every handler error becomes a notification).

use std::collections::VecDeque;
use uuid::Uuid;

/// Notification severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

impl Level {
    /// Human prefix used in the status-bar rendering (native toast/portal
    /// notification mapping lands with `mbxt-system::Notifier`).
    pub fn label(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Success => "ok",
            Level::Warning => "warn",
            Level::Error => "error",
        }
    }
}

/// One user-facing notification.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Unique id (dismissal target).
    pub id: Uuid,
    pub level: Level,
    pub title: String,
    pub body: String,
    /// Creation time (unix secs) for expiry.
    pub created_secs: u64,
}

impl Notification {
    pub fn new(level: Level, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            level,
            title: title.into(),
            body: body.into(),
            created_secs: super::state::now_secs(),
        }
    }

    /// Error notification helper (error-handling strategy §7).
    pub fn error(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new(Level::Error, title, body)
    }

    /// One-line rendering for the status bar / toast queue.
    pub fn display(&self) -> String {
        format!("[{}] {}: {}", self.level.label(), self.title, self.body)
    }
}

/// Bounded FIFO with time-based expiry (oldest dropped on overflow).
#[derive(Debug, Clone)]
pub struct NotificationQueue {
    pub items: VecDeque<Notification>,
    /// Maximum retained notifications.
    pub max: usize,
    /// Seconds before a notification auto-expires.
    pub ttl_secs: u64,
}

impl Default for NotificationQueue {
    fn default() -> Self {
        Self {
            items: VecDeque::new(),
            max: 100,
            ttl_secs: 15,
        }
    }
}

impl NotificationQueue {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Push a notification; drops the oldest when over capacity.
    pub fn push(&mut self, notification: Notification) {
        if self.items.len() >= self.max {
            self.items.pop_front();
        }
        self.items.push_back(notification);
    }

    /// Drop expired notifications (called on autosave/heartbeat ticks).
    pub fn retain_recent(&mut self, now_secs: u64) {
        let ttl = self.ttl_secs;
        self.items
            .retain(|n| now_secs.saturating_sub(n.created_secs) < ttl);
    }

    /// Render all current notifications (view helper).
    pub fn display(&self) -> Vec<String> {
        self.items.iter().map(Notification::display).collect()
    }

    /// Dismiss by id (user click).
    pub fn dismiss(&mut self, id: Uuid) -> bool {
        let before = self.items.len();
        self.items.retain(|n| n.id != id);
        self.items.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_level_and_text() {
        let n = Notification::new(Level::Warning, "Upload", "disk almost full");
        let text = n.display();
        assert!(text.contains("[warn]"));
        assert!(text.contains("Upload"));
        assert!(text.contains("disk almost full"));
    }

    #[test]
    fn push_drops_oldest_on_overflow() {
        let mut queue = NotificationQueue {
            max: 2,
            ..Default::default()
        };
        queue.push(Notification::new(Level::Info, "1", ""));
        queue.push(Notification::new(Level::Info, "2", ""));
        queue.push(Notification::new(Level::Info, "3", ""));
        let titles: Vec<&str> = queue.items.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["2", "3"]);
    }

    #[test]
    fn dismiss_removes_only_matching_id() {
        let mut queue = NotificationQueue::default();
        let a = Notification::new(Level::Info, "a", "");
        let b = Notification::new(Level::Info, "b", "");
        let id_a = a.id;
        queue.push(a);
        queue.push(b);
        assert!(queue.dismiss(id_a));
        assert!(!queue.dismiss(id_a));
        assert_eq!(queue.items[0].title, "b");
    }
}
