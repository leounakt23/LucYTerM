//! `SftpSession`: live SFTP channel over russh (Prompt 3.1).
//!
//! The wrapper owns a `russh_sftp::client::SftpSession` and exposes the
//! prompt-mandated surface: directory listing, stat, file management
//! (delete/rename/mkdir/rmdir/chmod), resumable streaming transfers with
//! progress + cancellation, and recursive directory up/download.
//!
//! Streaming discipline: every transfer seeks both ends to the resume offset
//! and pumps fixed-size chunks through [`transfer::copy_stream`] — memory
//! stays flat for multi-gigabyte files. `Send + Sync` holds (the inner
//! session is `Send + Sync`), so handles share across the session manager.

use std::path::Path;
use std::sync::Arc;

use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use super::file_info::FileInfo;
use super::transfer::{self, CancelToken, TransferOptions};
use super::SftpError;

/// Live SFTP session (cheap to clone: shares the underlying channel).
#[derive(Clone)]
pub struct SftpSession {
    inner: Arc<russh_sftp::client::SftpSession>,
}

impl std::fmt::Debug for SftpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SftpSession").finish_non_exhaustive()
    }
}

impl SftpSession {
    /// Wrap an already-negotiated SFTP stream (subsystem channel).
    ///
    /// Mirrors `russh_sftp::client::SftpSession::new`: performs the SFTP
    /// handshake (`init`/`version`) over `stream`.
    pub async fn new<S>(stream: S) -> Result<Self, SftpError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let inner = russh_sftp::client::SftpSession::new(stream)
            .await
            .map_err(SftpError::from_sftp)?;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    /// Wrap an existing client session (reconnect path reuses handshakes).
    pub fn from_client(session: russh_sftp::client::SftpSession) -> Self {
        Self {
            inner: Arc::new(session),
        }
    }

    /// List a remote directory (sorted: dirs first, then by name).
    pub async fn list_directory(&self, path: &str) -> Result<Vec<FileInfo>, SftpError> {
        let read_dir = self
            .inner
            .read_dir(path)
            .await
            .map_err(SftpError::from_sftp)?;
        let mut entries: Vec<FileInfo> = read_dir
            .map(|entry| FileInfo::from_dir_entry(path, &entry))
            .collect();
        super::file_info::sort_entries(&mut entries);
        Ok(entries)
    }

    /// Stat one remote path.
    pub async fn stat(&self, path: &str) -> Result<FileInfo, SftpError> {
        let metadata = self
            .inner
            .metadata(path)
            .await
            .map_err(SftpError::from_sftp)?;
        Ok(FileInfo::from_metadata(path, &metadata))
    }

    /// Delete a remote file.
    pub async fn delete(&self, path: &str) -> Result<(), SftpError> {
        self.inner
            .remove_file(path)
            .await
            .map_err(SftpError::from_sftp)
    }

    /// Rename (move) a remote file or directory.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), SftpError> {
        self.inner
            .rename(from, to)
            .await
            .map_err(SftpError::from_sftp)
    }

    /// Create a remote directory (single level).
    pub async fn create_directory(&self, path: &str) -> Result<(), SftpError> {
        self.inner
            .create_dir(path)
            .await
            .map_err(SftpError::from_sftp)
    }

    /// Remove an empty remote directory.
    pub async fn remove_directory(&self, path: &str) -> Result<(), SftpError> {
        self.inner
            .remove_dir(path)
            .await
            .map_err(SftpError::from_sftp)
    }

    /// Set POSIX mode bits (`0o755`) on a remote path.
    pub async fn chmod(&self, path: &str, mode: u32) -> Result<(), SftpError> {
        use russh_sftp::protocol::FileAttributes;
        let metadata = FileAttributes {
            permissions: Some(mode),
            ..FileAttributes::empty()
        };
        self.inner
            .set_metadata(path, metadata)
            .await
            .map_err(|err| SftpError::protocol(err.to_string()))
    }

    /// Close the SFTP channel (idempotent; the SSH session owns teardown).
    pub async fn close(&self) -> Result<(), SftpError> {
        self.inner.close().await.map_err(SftpError::from_sftp)
    }

    /// Read at most `max_bytes` from the start of `remote` (preview path).
    ///
    /// Bounded by construction: safe against multi-gigabyte files — only the
    /// prefix is ever transferred.
    pub async fn read_prefix(&self, remote: &str, max_bytes: u64) -> Result<Vec<u8>, SftpError> {
        use tokio::io::AsyncReadExt as _;
        let mut file = self
            .inner
            .open(remote)
            .await
            .map_err(SftpError::from_sftp)?;
        let mut limited = (&mut file).take(max_bytes);
        let mut out = Vec::new();
        limited.read_to_end(&mut out).await.map_err(SftpError::io)?;
        Ok(out)
    }

    // -- streaming transfers ------------------------------------------------

    /// Download `remote` → `local` with resume + progress + cancellation.
    ///
    /// Resume: when `local` already holds a strict prefix of the remote file,
    /// both ends seek to that offset; otherwise the transfer restarts from 0
    /// (stale/complete destinations are truncated).
    pub async fn get_file(
        &self,
        remote: &str,
        local: &Path,
        pipeline_depth: u32,
        cancel: &CancelToken,
        on_progress: impl FnMut(u64, Option<u64>) + Send,
    ) -> Result<u64, SftpError> {
        self.get_file_with_options(
            remote,
            local,
            &TransferOptions {
                pipeline_depth,
                throttle_bps: None,
            },
            cancel,
            on_progress,
        )
        .await
    }

    /// [`SftpSession::get_file`] with full [`TransferOptions`] (bandwidth cap).
    pub async fn get_file_with_options(
        &self,
        remote: &str,
        local: &Path,
        options: &TransferOptions,
        cancel: &CancelToken,
        mut on_progress: impl FnMut(u64, Option<u64>) + Send,
    ) -> Result<u64, SftpError> {
        let total = self
            .inner
            .metadata(remote)
            .await
            .map_err(SftpError::from_sftp)?
            .size;
        let local_len = fs::metadata(local).await.map(|m| m.len()).unwrap_or(0);
        let offset = transfer::resume_offset(local_len, total);

        let mut remote_file = self
            .inner
            .open(remote)
            .await
            .map_err(SftpError::from_sftp)?;
        remote_file
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(SftpError::io)?;

        let mut local_file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(offset == 0)
            .open(local)
            .await
            .map_err(SftpError::io)?;
        local_file
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(SftpError::io)?;

        if offset > 0 {
            on_progress(offset, total);
        }
        let chunk = transfer::chunk_size_for_depth(options.pipeline_depth);
        let copied = transfer::copy_stream_limited(
            &mut remote_file,
            &mut local_file,
            total,
            chunk,
            cancel,
            options.throttle_bps,
            &mut on_progress,
        )
        .await?;
        remote_file.shutdown().await.map_err(SftpError::io)?;
        Ok(offset + copied)
    }

    /// Upload `local` → `remote` with resume + progress + cancellation.
    pub async fn put_file(
        &self,
        local: &Path,
        remote: &str,
        pipeline_depth: u32,
        cancel: &CancelToken,
        on_progress: impl FnMut(u64, Option<u64>) + Send,
    ) -> Result<u64, SftpError> {
        self.put_file_with_options(
            local,
            remote,
            &TransferOptions {
                pipeline_depth,
                throttle_bps: None,
            },
            cancel,
            on_progress,
        )
        .await
    }

    /// [`SftpSession::put_file`] with full [`TransferOptions`] (bandwidth cap).
    pub async fn put_file_with_options(
        &self,
        local: &Path,
        remote: &str,
        options: &TransferOptions,
        cancel: &CancelToken,
        mut on_progress: impl FnMut(u64, Option<u64>) + Send,
    ) -> Result<u64, SftpError> {
        use russh_sftp::protocol::OpenFlags;

        let local_len = fs::metadata(local).await.map_err(SftpError::io)?.len();
        let remote_len = self
            .inner
            .metadata(remote)
            .await
            .map(|m| m.size.unwrap_or(0))
            .unwrap_or(0);
        let offset = transfer::resume_offset(remote_len, Some(local_len));
        let total = Some(local_len);

        let mut local_file = fs::File::open(local).await.map_err(SftpError::io)?;
        local_file
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(SftpError::io)?;

        let flags = if offset == 0 {
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE
        } else {
            OpenFlags::WRITE | OpenFlags::CREATE
        };
        let mut remote_file = self
            .inner
            .open_with_flags(remote, flags)
            .await
            .map_err(SftpError::from_sftp)?;
        remote_file
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(SftpError::io)?;

        if offset > 0 {
            on_progress(offset, total);
        }
        let chunk = transfer::chunk_size_for_depth(options.pipeline_depth);
        let copied = transfer::copy_stream_limited(
            &mut (&mut local_file).take(local_len.saturating_sub(offset)),
            &mut remote_file,
            total,
            chunk,
            cancel,
            options.throttle_bps,
            &mut on_progress,
        )
        .await?;
        remote_file.shutdown().await.map_err(SftpError::io)?;
        Ok(offset + copied)
    }

    // -- bulk (recursive) operations ----------------------------------------

    /// Recursively download `remote` → `local` (files stream; dirs created).
    ///
    /// Returns `(files, bytes)`. Progress fires per file chunk as
    /// `(done, file total)`; callers aggregate across files. Iterative
    /// work-stack (no async recursion) so cancellation checks stay per entry.
    pub async fn download_dir(
        &self,
        remote: &str,
        local: &Path,
        pipeline_depth: u32,
        cancel: &CancelToken,
        on_progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> Result<(u64, u64), SftpError> {
        fs::create_dir_all(local).await.map_err(SftpError::io)?;
        let mut stack = vec![(remote.to_string(), local.to_path_buf())];
        let mut files = 0u64;
        let mut bytes = 0u64;
        while let Some((remote_dir, local_dir)) = stack.pop() {
            if cancel.is_cancelled() {
                return Err(SftpError::Cancelled);
            }
            let entries = self.list_directory(&remote_dir).await?;
            for entry in entries {
                if cancel.is_cancelled() {
                    return Err(SftpError::Cancelled);
                }
                let local_path = local_dir.join(&entry.name);
                if entry.is_dir() {
                    fs::create_dir_all(&local_path)
                        .await
                        .map_err(SftpError::io)?;
                    stack.push((entry.path, local_path));
                } else if entry.is_file() {
                    let done = self
                        .get_file(
                            &entry.path,
                            &local_path,
                            pipeline_depth,
                            cancel,
                            &mut *on_progress,
                        )
                        .await?;
                    files += 1;
                    bytes += done;
                }
            }
        }
        Ok((files, bytes))
    }

    /// Recursively upload `local` → `remote` (dirs created remotely).
    ///
    /// Returns `(files, bytes)`. Iterative work-stack like [`Self::download_dir`].
    pub async fn upload_dir(
        &self,
        local: &Path,
        remote: &str,
        pipeline_depth: u32,
        cancel: &CancelToken,
        on_progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> Result<(u64, u64), SftpError> {
        let _ = self.create_directory(remote).await;
        let mut stack = vec![(local.to_path_buf(), remote.to_string())];
        let mut files = 0u64;
        let mut bytes = 0u64;
        while let Some((local_dir, remote_dir)) = stack.pop() {
            if cancel.is_cancelled() {
                return Err(SftpError::Cancelled);
            }
            let mut dir = fs::read_dir(&local_dir).await.map_err(SftpError::io)?;
            while let Some(entry) = dir.next_entry().await.map_err(SftpError::io)? {
                if cancel.is_cancelled() {
                    return Err(SftpError::Cancelled);
                }
                let file_type = entry.file_type().await.map_err(SftpError::io)?;
                let remote_path =
                    transfer::join_remote(&remote_dir, &entry.file_name().to_string_lossy());
                if file_type.is_dir() {
                    let _ = self.create_directory(&remote_path).await;
                    stack.push((entry.path(), remote_path));
                } else if file_type.is_file() {
                    let done = self
                        .put_file(
                            &entry.path(),
                            &remote_path,
                            pipeline_depth,
                            cancel,
                            &mut *on_progress,
                        )
                        .await?;
                    files += 1;
                    bytes += done;
                }
            }
        }
        Ok((files, bytes))
    }
}
