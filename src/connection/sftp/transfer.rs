//! Streaming transfers with resume, progress, and cancellation (Prompt 3.1).
//!
//! Design notes (quality bar: >1GB files, no full buffering):
//! - All copies are chunked (`chunk_size_for_depth`, default 64 KiB ×
//!   pipeline depth) with `u64` offsets end to end — memory stays flat
//!   regardless of file size.
//! - Resume is offset arithmetic on lengths the caller observed
//!   ([`resume_offset`]); the session layer seeks both ends to that offset.
//! - Progress is a plain callback `(done, total)` so UI, CLI, and tests share
//!   one path; speed is derived in [`TransferProgress`].
//! - Cancellation is cooperative via [`CancelToken`] (atomic flag, `Send +
//!   Sync`), checked once per chunk — transfers abort within one chunk.
//! - `copy_stream` is generic over `AsyncRead`/`AsyncWrite`, so the exact
//!   same code is exercised against in-memory cursors in tests and against
//!   SFTP/local files in production.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mbxt_core::SessionId;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::SftpError;

/// Default chunk size per pipeline slot (64 KiB).
pub const CHUNK_SIZE: usize = 64 * 1024;
/// Upper bound for a single chunk even at high pipeline depths.
pub const MAX_CHUNK_SIZE: usize = 1024 * 1024;

/// Upload or download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Upload,
    Download,
}

/// A planned transfer: endpoints plus the resume offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferPlan {
    pub direction: Direction,
    pub local: PathBuf,
    pub remote: String,
    /// Byte offset to resume from (0 == fresh transfer).
    pub offset: u64,
    /// Total remote/local size when known (drives progress %).
    pub total: Option<u64>,
}

impl TransferPlan {
    pub fn new(direction: Direction, local: &Path, remote: &str, offset: u64) -> Self {
        Self {
            direction,
            local: local.to_path_buf(),
            remote: remote.to_string(),
            offset,
            total: None,
        }
    }

    /// Remaining bytes when `total` is known.
    pub fn remaining(&self) -> Option<u64> {
        self.total.map(|total| total.saturating_sub(self.offset))
    }
}

/// Compute the resume offset from observed lengths.
///
/// - `local_len`: bytes already present at the destination.
/// - `remote_len`: authoritative source size when known (`None` = unknown).
/// - Returns the offset to seek both ends to: `local_len` when it is a
///   strict prefix of the source, else 0 (restart — destination is stale or
///   complete).
pub fn resume_offset(local_len: u64, remote_len: Option<u64>) -> u64 {
    match remote_len {
        Some(remote_len) if local_len < remote_len => local_len,
        _ => 0,
    }
}

/// Chunk size for a pipeline depth (tech_stack R5 knob): 64 KiB per slot,
/// clamped to [`MAX_CHUNK_SIZE`]; depth is clamped to 1..=64.
pub fn chunk_size_for_depth(pipeline_depth: u32) -> usize {
    let depth = pipeline_depth.clamp(1, 64) as usize;
    (CHUNK_SIZE * depth).min(MAX_CHUNK_SIZE)
}

/// Number of chunks to cover `total` bytes at `chunk` size.
pub fn chunk_count(total: u64, chunk: usize) -> u64 {
    if total == 0 {
        return 0;
    }
    total.saturating_add(chunk as u64 - 1) / chunk as u64
}

/// Cooperative cancellation flag (`Send + Sync`, checked per chunk).
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Signal cancellation (idempotent).
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// `true` after [`CancelToken::cancel`].
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// Live progress snapshot (speed derives from `done`/`started`).
#[derive(Debug, Clone)]
pub struct TransferProgress {
    pub done: u64,
    pub total: Option<u64>,
    pub started: Instant,
}

impl TransferProgress {
    pub fn new(total: Option<u64>) -> Self {
        Self {
            done: 0,
            total,
            started: Instant::now(),
        }
    }

    /// Fraction 0.0–1.0 when `total` is known.
    pub fn fraction(&self) -> Option<f64> {
        self.total.map(|total| {
            if total == 0 {
                1.0
            } else {
                (self.done.min(total) as f64) / (total as f64)
            }
        })
    }

    /// Bytes per second since construction.
    pub fn speed_bps(&self) -> f64 {
        let secs = self.started.elapsed().as_secs_f64();
        if secs <= 0.0 {
            0.0
        } else {
            self.done as f64 / secs
        }
    }
}

/// Human-readable throughput (`1.5 MiB/s`).
pub fn format_speed(bytes_per_sec: f64) -> String {
    const UNITS: [&str; 5] = ["B/s", "KiB/s", "MiB/s", "GiB/s", "TiB/s"];
    let mut value = bytes_per_sec.max(0.0);
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", value as u64, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Stream `reader` → `writer` in chunks, invoking `on_progress(done, total)`
/// per chunk and polling `cancel` per chunk.
///
/// Returns total bytes copied. Never buffers more than one chunk: safe for
/// multi-gigabyte files on a flat memory profile.
pub async fn copy_stream<R, W>(
    reader: &mut R,
    writer: &mut W,
    total: Option<u64>,
    chunk_size: usize,
    cancel: &CancelToken,
    on_progress: impl FnMut(u64, Option<u64>) + Send,
) -> Result<u64, SftpError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    copy_stream_limited(reader, writer, total, chunk_size, cancel, None, on_progress).await
}

/// [`copy_stream`] with an optional bandwidth cap (`bytes/sec`).
///
/// Throttling is a per-chunk pace delay: after each chunk the task sleeps
/// until the elapsed time catches up with `done / limit` (token-bucket-lite
/// with a one-chunk burst). `None` disables pacing entirely (same path as
/// [`copy_stream`]). Cancellation is still checked per chunk, so a throttled
/// transfer aborts promptly.
pub async fn copy_stream_limited<R, W>(
    reader: &mut R,
    writer: &mut W,
    total: Option<u64>,
    chunk_size: usize,
    cancel: &CancelToken,
    limit_bps: Option<u64>,
    mut on_progress: impl FnMut(u64, Option<u64>) + Send,
) -> Result<u64, SftpError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let _profile = crate::utils::profiling::operation("sftp.copy_stream");
    let chunk_size = chunk_size.max(1);
    let mut buf = vec![0u8; chunk_size];
    let mut done: u64 = 0;
    let started = Instant::now();
    loop {
        if cancel.is_cancelled() {
            return Err(SftpError::Cancelled);
        }
        let n = reader.read(&mut buf).await.map_err(SftpError::io)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n]).await.map_err(SftpError::io)?;
        done += n as u64;
        if let Some(delay) = throttle_delay(done, started, limit_bps) {
            tokio::time::sleep(delay).await;
        }
        on_progress(done, total);
    }
    writer.flush().await.map_err(SftpError::io)?;
    Ok(done)
}

/// Pace delay for throttling: how long to sleep so `done` bytes in `elapsed`
/// average down to `limit_bps`. Returns `None` when unthrottled or already
/// within budget.
pub fn throttle_delay(done: u64, started: Instant, limit_bps: Option<u64>) -> Option<Duration> {
    let limit = limit_bps?;
    if limit == 0 {
        return None;
    }
    let target = Duration::from_secs_f64(done as f64 / limit as f64);
    let elapsed = started.elapsed();
    if target > elapsed {
        Some(target - elapsed)
    } else {
        None
    }
}

/// Per-transfer knobs (pipeline depth from settings + bandwidth cap).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferOptions {
    /// SFTP request pipeline depth (chunk sizing, tech_stack R5).
    pub pipeline_depth: u32,
    /// Bandwidth cap in bytes/sec (`None` == unlimited).
    pub throttle_bps: Option<u64>,
}

impl Default for TransferOptions {
    fn default() -> Self {
        Self {
            pipeline_depth: 20,
            throttle_bps: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Transfer records (Prompt 3.2 manager layer)
// ---------------------------------------------------------------------------

/// Stable identifier for one queued transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransferId(pub u64);

impl std::fmt::Display for TransferId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "transfer #{}", self.0)
    }
}

static NEXT_TRANSFER_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate a fresh [`TransferId`] (process-wide, lock-free).
pub fn next_transfer_id() -> TransferId {
    TransferId(NEXT_TRANSFER_ID.fetch_add(1, Ordering::SeqCst))
}

/// Lifecycle state of one transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferStatus {
    Pending,
    InProgress,
    Paused,
    Completed,
    Failed(String),
    Cancelled,
}

impl TransferStatus {
    /// `true` for Completed/Failed/Cancelled (no further transitions).
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed(_) | Self::Cancelled)
    }

    /// `true` while queued or running (counts toward nothing terminal).
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::InProgress | Self::Paused)
    }

    /// Short UI label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "queued",
            Self::InProgress => "transferring",
            Self::Paused => "paused",
            Self::Completed => "done",
            Self::Failed(_) => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// One managed transfer: endpoints, progress, status, and retry bookkeeping.
#[derive(Debug, Clone)]
pub struct Transfer {
    pub id: TransferId,
    pub session: SessionId,
    pub direction: Direction,
    pub local_path: PathBuf,
    pub remote_path: String,
    pub status: TransferStatus,
    pub done: u64,
    pub total: Option<u64>,
    /// Attempts consumed (1 == first try).
    pub attempts: u32,
    /// Bandwidth cap in bytes/sec.
    pub throttle_bps: Option<u64>,
    pub started: Option<Instant>,
    pub updated: Instant,
    /// Cooperative cancel for the in-flight task (fired on pause/cancel).
    pub cancel: CancelToken,
}

impl Transfer {
    pub fn new(
        session: SessionId,
        direction: Direction,
        local_path: &Path,
        remote_path: &str,
        total: Option<u64>,
        throttle_bps: Option<u64>,
    ) -> Self {
        Self {
            id: next_transfer_id(),
            session,
            direction,
            local_path: local_path.to_path_buf(),
            remote_path: remote_path.to_string(),
            status: TransferStatus::Pending,
            done: 0,
            total,
            attempts: 0,
            throttle_bps,
            started: None,
            updated: Instant::now(),
            cancel: CancelToken::new(),
        }
    }

    /// Fraction 0.0–1.0 when `total` is known.
    pub fn fraction(&self) -> Option<f64> {
        self.total.map(|total| {
            if total == 0 {
                1.0
            } else {
                (self.done.min(total) as f64) / (total as f64)
            }
        })
    }

    /// Current throughput over the run so far.
    pub fn speed_bps(&self) -> f64 {
        match self.started {
            Some(started) => {
                let secs = started.elapsed().as_secs_f64();
                if secs <= 0.0 {
                    0.0
                } else {
                    self.done as f64 / secs
                }
            },
            None => 0.0,
        }
    }

    /// Estimated seconds remaining (`None` without total/speed).
    pub fn eta_secs(&self) -> Option<u64> {
        let total = self.total?;
        let speed = self.speed_bps();
        if speed <= 0.0 {
            return None;
        }
        Some(((total.saturating_sub(self.done) as f64) / speed).ceil() as u64)
    }

    /// One-line UI summary (`name (done/total · speed · eta)`).
    pub fn summary(&self) -> String {
        let arrow = match self.direction {
            Direction::Upload => "↑",
            Direction::Download => "↓",
        };
        let name = self
            .local_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.remote_path.clone());
        let progress = match self.total {
            Some(total) => format!("{} / {} bytes", self.done, total),
            None => format!("{} bytes", self.done),
        };
        let eta = self
            .eta_secs()
            .map(|s| format!(", eta {s}s"))
            .unwrap_or_default();
        format!(
            "{arrow} {name} ({progress} · {}{eta} · {})",
            format_speed(self.speed_bps()),
            self.status.label()
        )
    }
}

/// Retry backoff: exponential `base_ms * 2^attempt`, capped at `cap_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    pub base_ms: u64,
    pub cap_ms: u64,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            base_ms: 500,
            cap_ms: 30_000,
        }
    }
}

impl Backoff {
    /// Delay before attempt `attempt` (0-based: first retry waits `base_ms`).
    pub fn delay(&self, attempt: u32) -> Duration {
        let shift = attempt.min(10);
        let ms = self.base_ms.saturating_mul(1 << shift).min(self.cap_ms);
        Duration::from_millis(ms)
    }

    /// Whether `err` is worth retrying (transient network/protocol).
    /// Cancellation and missing-channel errors never retry: the former is
    /// user intent, the latter needs an explicit reconnect first.
    pub fn should_retry(&self, err: &SftpError) -> bool {
        matches!(
            err,
            SftpError::Protocol(_) | SftpError::Io(_) | SftpError::Ssh(_)
        )
    }
}

/// Join a remote parent and a child name with POSIX separators.
pub fn join_remote(parent: &str, name: &str) -> String {
    super::file_info::join_remote(parent, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn resume_only_when_destination_is_a_strict_prefix() {
        assert_eq!(resume_offset(0, Some(100)), 0);
        assert_eq!(resume_offset(40, Some(100)), 40);
        assert_eq!(resume_offset(100, Some(100)), 0, "complete → restart");
        assert_eq!(resume_offset(120, Some(100)), 0, "stale → restart");
        assert_eq!(resume_offset(40, None), 0, "unknown total → restart");
    }

    #[test]
    fn chunk_math_covers_edges() {
        assert_eq!(chunk_count(0, 1024), 0);
        assert_eq!(chunk_count(1, 1024), 1);
        assert_eq!(chunk_count(1024, 1024), 1);
        assert_eq!(chunk_count(1025, 1024), 2);
        assert_eq!(chunk_size_for_depth(0), CHUNK_SIZE);
        assert_eq!(chunk_size_for_depth(1), CHUNK_SIZE);
        assert_eq!(chunk_size_for_depth(20), MAX_CHUNK_SIZE);
        // u64 offsets cover >1GiB without overflow.
        assert_eq!(chunk_count(2 * 1024 * 1024 * 1024, CHUNK_SIZE), 32768);
    }

    #[test]
    fn progress_fraction_and_speed() {
        let mut progress = TransferProgress::new(Some(100));
        assert_eq!(progress.fraction(), Some(0.0));
        progress.done = 50;
        assert_eq!(progress.fraction(), Some(0.5));
        assert!(progress.speed_bps() >= 0.0);
        assert_eq!(TransferProgress::new(Some(0)).fraction(), Some(1.0));
        assert_eq!(format_speed(512.0), "512 B/s");
        assert_eq!(format_speed(1536.0), "1.5 KiB/s");
    }

    #[tokio::test]
    async fn copy_stream_reports_progress_per_chunk() {
        let data = vec![7u8; 200_000];
        let mut reader = Cursor::new(data.clone());
        let mut writer = Vec::new();
        let cancel = CancelToken::new();
        let mut calls = Vec::new();
        let done = copy_stream(
            &mut reader,
            &mut writer,
            Some(data.len() as u64),
            64 * 1024,
            &cancel,
            |done, total| calls.push((done, total)),
        )
        .await
        .unwrap();
        assert_eq!(done, data.len() as u64);
        assert_eq!(writer, data);
        assert!(!calls.is_empty());
        assert_eq!(calls.last().unwrap().0, data.len() as u64);
    }

    #[tokio::test]
    async fn copy_stream_is_cancellable_within_one_chunk() {
        let data = vec![9u8; 1024 * 1024];
        let mut reader = Cursor::new(data);
        let mut writer = Vec::new();
        let cancel = CancelToken::new();
        cancel.cancel();
        let err = copy_stream(
            &mut reader,
            &mut writer,
            None,
            64 * 1024,
            &cancel,
            |_, _| {},
        )
        .await
        .unwrap_err();
        assert!(matches!(err, SftpError::Cancelled));
    }

    #[tokio::test]
    async fn empty_stream_copies_zero_bytes() {
        let mut reader = Cursor::new(Vec::new());
        let mut writer = Vec::new();
        let done = copy_stream(
            &mut reader,
            &mut writer,
            Some(0),
            1024,
            &CancelToken::new(),
            |_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(done, 0);
    }

    #[test]
    fn transfer_ids_are_unique_and_ordered() {
        let a = next_transfer_id();
        let b = next_transfer_id();
        assert_ne!(a, b);
        assert!(b.0 > a.0);
        assert_eq!(format!("{a}"), format!("transfer #{}", a.0));
    }

    #[test]
    fn transfer_status_lifecycle_predicates() {
        assert!(TransferStatus::Pending.is_active());
        assert!(TransferStatus::InProgress.is_active());
        assert!(TransferStatus::Paused.is_active());
        assert!(!TransferStatus::Completed.is_active());
        assert!(TransferStatus::Completed.is_terminal());
        assert!(TransferStatus::Failed("x".into()).is_terminal());
        assert!(TransferStatus::Cancelled.is_terminal());
        assert!(!TransferStatus::Pending.is_terminal());
    }

    #[test]
    fn transfer_fraction_eta_and_summary() {
        let mut transfer = Transfer::new(
            1,
            Direction::Upload,
            Path::new("/tmp/a.bin"),
            "/remote/a.bin",
            Some(1000),
            None,
        );
        assert_eq!(transfer.fraction(), Some(0.0));
        transfer.done = 500;
        assert_eq!(transfer.fraction(), Some(0.5));
        assert_eq!(transfer.eta_secs(), None, "no clock yet");
        transfer.started = Some(Instant::now());
        let summary = transfer.summary();
        assert!(summary.contains("↑") && summary.contains("a.bin"));
        let download = Transfer::new(
            1,
            Direction::Download,
            Path::new("/tmp/b.bin"),
            "/remote/b.bin",
            None,
            None,
        );
        assert!(download.summary().contains("↓"));
    }

    #[test]
    fn throttle_delay_paces_to_budget() {
        assert_eq!(throttle_delay(0, Instant::now(), None), None);
        assert_eq!(throttle_delay(1000, Instant::now(), Some(0)), None);
        // Nothing sent yet on a fresh timer: within budget, no wait.
        assert_eq!(throttle_delay(0, Instant::now(), Some(1024)), None);
        // A megabyte "sent" instantly at 1 B/s is wildly over budget.
        let wait = throttle_delay(1_000_000, Instant::now(), Some(1)).expect("must wait");
        assert!(wait > Duration::from_secs(100));
    }

    #[test]
    fn backoff_grows_exponentially_with_cap() {
        let backoff = Backoff::default();
        assert_eq!(backoff.delay(0), Duration::from_millis(500));
        assert_eq!(backoff.delay(1), Duration::from_millis(1000));
        assert_eq!(backoff.delay(2), Duration::from_millis(2000));
        assert!(backoff.delay(100) <= Duration::from_millis(30_000));
        assert!(backoff.should_retry(&SftpError::Io("reset".into())));
        assert!(backoff.should_retry(&SftpError::Protocol("x".into())));
        assert!(!backoff.should_retry(&SftpError::Cancelled));
        assert!(!backoff.should_retry(&SftpError::NotConnected));
    }

    #[tokio::test]
    async fn limited_copy_matches_plain_copy_without_cap() {
        let data = vec![3u8; 50_000];
        let mut reader = Cursor::new(data.clone());
        let mut writer = Vec::new();
        let done = copy_stream_limited(
            &mut reader,
            &mut writer,
            Some(data.len() as u64),
            8192,
            &CancelToken::new(),
            None,
            |_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(done, data.len() as u64);
        assert_eq!(writer, data);
    }
}
