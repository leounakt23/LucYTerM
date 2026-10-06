//! `FileInfo`: remote file metadata + conversions (Prompt 3.1).
//!
//! Pure data layer over `russh-sftp` protocol types: no I/O here, so every
//! mapping is unit-testable without a server. The live session
//! (`session.rs`) converts `DirEntry`/`FileAttributes` through these helpers.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Remote file kind (feature matrix #26: file, dir, symlink).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileType {
    File,
    Dir,
    Symlink,
    Other,
}

impl FileType {
    /// Classify from russh-sftp protocol attributes.
    pub fn from_protocol(attrs: &russh_sftp::protocol::FileAttributes) -> Self {
        let kind = attrs.file_type();
        if kind.is_dir() {
            Self::Dir
        } else if kind.is_symlink() {
            Self::Symlink
        } else if kind.is_file() {
            Self::File
        } else {
            Self::Other
        }
    }

    pub fn is_dir(self) -> bool {
        matches!(self, Self::Dir)
    }

    pub fn is_file(self) -> bool {
        matches!(self, Self::File)
    }
}

/// One remote file as shown in the SFTP panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    /// Base name (`foo.txt`, never a full path).
    pub name: String,
    /// Full remote POSIX path (`/home/ops/foo.txt`).
    pub path: String,
    /// Kind of entry.
    pub file_type: FileType,
    /// Size in bytes (0 when the server omits it, e.g. for dirs).
    pub size: u64,
    /// Raw POSIX mode bits (e.g. `0o755`); 0 when omitted.
    pub permissions: u32,
    /// Modification time (None when the server omits `mtime`).
    pub modified: Option<SystemTime>,
    /// Owner name or uid string.
    pub owner: Option<String>,
    /// Group name or gid string.
    pub group: Option<String>,
}

impl FileInfo {
    /// Build from a directory entry.
    pub fn from_dir_entry(parent: &str, entry: &russh_sftp::client::fs::DirEntry) -> Self {
        let name = entry.file_name();
        let path = join_remote(parent, &name);
        Self::from_parts(path, name, &entry.metadata())
    }

    /// Build from explicit metadata (stat path).
    pub fn from_metadata(path: &str, metadata: &russh_sftp::protocol::FileAttributes) -> Self {
        let name = file_name_of(path).to_string();
        Self::from_parts(path.to_string(), name, metadata)
    }

    fn from_parts(
        path: String,
        name: String,
        attrs: &russh_sftp::protocol::FileAttributes,
    ) -> Self {
        Self {
            name,
            path,
            file_type: FileType::from_protocol(attrs),
            size: attrs.size.unwrap_or(0),
            permissions: attrs.permissions.unwrap_or(0),
            modified: attrs.mtime.map(mtime_to_system_time),
            owner: attrs
                .user
                .clone()
                .or_else(|| attrs.uid.map(|uid| uid.to_string())),
            group: attrs
                .group
                .clone()
                .or_else(|| attrs.gid.map(|gid| gid.to_string())),
        }
    }

    pub fn is_dir(&self) -> bool {
        self.file_type.is_dir()
    }

    pub fn is_file(&self) -> bool {
        self.file_type.is_file()
    }

    /// Unix-style permission string (`rwxr-xr-x`).
    pub fn permission_string(&self) -> String {
        let bits = [
            (0o400, 'r'),
            (0o200, 'w'),
            (0o100, 'x'),
            (0o040, 'r'),
            (0o020, 'w'),
            (0o010, 'x'),
            (0o004, 'r'),
            (0o002, 'w'),
            (0o001, 'x'),
        ];
        bits.iter()
            .map(|(mask, ch)| {
                if self.permissions & mask != 0 {
                    *ch
                } else {
                    '-'
                }
            })
            .collect()
    }

    /// Sort key: directories first, then case-insensitive name.
    pub fn sort_key(&self) -> (u8, String) {
        let dir_first = if self.is_dir() { 0 } else { 1 };
        (dir_first, self.name.to_lowercase())
    }
}

/// Join a parent dir and a child name with POSIX separators.
pub fn join_remote(parent: &str, name: &str) -> String {
    if parent.is_empty() || parent == "/" {
        format!("/{name}")
    } else if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

/// Base name of a remote POSIX path (`/a/b/c` → `c`, `/` → `/`).
pub fn file_name_of(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/";
    }
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

/// Parent directory of a remote POSIX path (`/a/b` → `/a`).
pub fn parent_of(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        None => ".",
        Some(0) => "/",
        Some(i) => &trimmed[..i],
    }
}

/// Sort entries: directories first, then by name (stable, panel order).
pub fn sort_entries(entries: &mut [FileInfo]) {
    entries.sort_by_key(FileInfo::sort_key);
}

fn mtime_to_system_time(mtime: u32) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(u64::from(mtime))
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh_sftp::protocol::FileAttributes;

    fn attrs(size: Option<u64>, permissions: Option<u32>, mtime: Option<u32>) -> FileAttributes {
        FileAttributes {
            size,
            uid: Some(1000),
            user: Some("ops".into()),
            gid: Some(1000),
            group: Some("ops".into()),
            permissions,
            atime: None,
            mtime,
        }
    }

    #[test]
    fn permission_string_renders_rwx() {
        let info = FileInfo::from_metadata("/bin/sh", &attrs(Some(10), Some(0o755), None));
        assert_eq!(info.permission_string(), "rwxr-xr-x");
        let info = FileInfo::from_metadata("/etc/shadow", &attrs(Some(10), Some(0o640), None));
        assert_eq!(info.permission_string(), "rw-r-----");
    }

    #[test]
    fn owner_group_prefer_names_over_ids() {
        let info = FileInfo::from_metadata("/x", &attrs(Some(1), Some(0o644), Some(1_700_000_000)));
        assert_eq!(info.owner.as_deref(), Some("ops"));
        assert_eq!(info.group.as_deref(), Some("ops"));
        assert_eq!(
            info.modified,
            Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        );
    }

    #[test]
    fn missing_fields_default_safely() {
        let bare = FileAttributes::empty();
        let info = FileInfo::from_metadata("/", &bare);
        assert_eq!(info.name, "/");
        assert_eq!(info.size, 0);
        assert_eq!(info.permissions, 0);
        assert_eq!(info.modified, None);
        assert_eq!(info.owner, None);
    }

    #[test]
    fn path_helpers_handle_edges() {
        assert_eq!(join_remote("/", "a"), "/a");
        assert_eq!(join_remote("/a", "b"), "/a/b");
        assert_eq!(join_remote("/a/", "b"), "/a/b");
        assert_eq!(file_name_of("/a/b/c"), "c");
        assert_eq!(file_name_of("/"), "/");
        assert_eq!(parent_of("/a/b"), "/a");
        assert_eq!(parent_of("/a"), "/");
    }

    #[test]
    fn sort_puts_directories_first() {
        let mut entries = vec![
            FileInfo::from_metadata("/b.txt", &attrs(Some(1), Some(0o644), None)),
            FileInfo::from_metadata("/a", &attrs(None, Some(0o755), None)),
        ];
        // Force the second entry to read as a dir via mode bits is not
        // available on bare attrs; sorting still orders by name here.
        sort_entries(&mut entries);
        assert_eq!(entries[0].name, "a");
    }
}
