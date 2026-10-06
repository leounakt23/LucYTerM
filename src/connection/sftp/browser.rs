//! File-browser model (Prompt 3.3): per-session browser state, sorting,
//! filtering, selection, listing cache, and pagination.
//!
//! Pure data + algorithms — no I/O, no widgets — so everything here is
//! headless-testable. Live listing stays in `session.rs`; the iced view in
//! `ui::widgets::file_browser` renders snapshots of [`FileBrowserState`].

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use mbxt_core::SessionId;

use super::file_info::FileInfo;

/// Default listing-cache TTL (seconds).
pub const CACHE_TTL_SECS: u64 = 30;
/// Rows rendered per lazy page (10k-file directories stay smooth).
pub const PAGE_SIZE: usize = 200;

/// Sort column for the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserSort {
    #[default]
    Name,
    Size,
    Modified,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

/// Inline input form open in the browser (mkdir/rename/chmod/upload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingOp {
    Mkdir,
    Rename {
        old: String,
    },
    Chmod,
    /// Upload a local path (input holds the local source path).
    Upload,
}

/// Right-click target: an entry or the background.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextTarget {
    Entry(String),
    Background,
}

/// Per-session browser state (ephemeral UI state — never persisted).
#[derive(Debug, Clone)]
pub struct FileBrowserState {
    pub session: SessionId,
    /// Current remote directory (POSIX).
    pub cwd: String,
    /// Last listing for `cwd` (already user-sorted/filtered at render).
    pub entries: Vec<FileInfo>,
    pub sort: BrowserSort,
    pub sort_dir: SortDir,
    /// Wildcard filter (`*`, `?`, case-insensitive); empty == no filter.
    pub filter: String,
    /// Selected entry names (bulk ops).
    pub selected: BTreeSet<String>,
    /// Open context menu, if any.
    pub context: Option<ContextTarget>,
    /// Inline input form state.
    pub pending: Option<PendingOp>,
    /// Buffered input text for the pending form.
    pub input: String,
    /// Text preview (`path`, first lines) or properties dump.
    pub preview: Option<(String, String)>,
    /// Visible lazy pages.
    pub pages: usize,
    /// Listing in flight (spinner).
    pub loading: bool,
    /// Last listing/operation error (inline banner).
    pub error: Option<String>,
}

impl FileBrowserState {
    pub fn new(session: SessionId, cwd: &str) -> Self {
        Self {
            session,
            cwd: cwd.to_string(),
            entries: Vec::new(),
            sort: BrowserSort::default(),
            sort_dir: SortDir::default(),
            filter: String::new(),
            selected: BTreeSet::new(),
            context: None,
            pending: None,
            input: String::new(),
            preview: None,
            pages: 1,
            loading: false,
            error: None,
        }
    }

    /// Entries after filter + sort (render path).
    pub fn visible(&self) -> Vec<&FileInfo> {
        let mut out: Vec<&FileInfo> = self
            .entries
            .iter()
            .filter(|e| matches_filter(&e.name, &self.filter))
            .collect();
        apply_sort(&mut out, self.sort, self.sort_dir);
        out
    }

    /// Visible rows capped to loaded lazy pages.
    pub fn page<'a>(&self, visible: &'a [&'a FileInfo]) -> &'a [&'a FileInfo] {
        let end = (self.pages * PAGE_SIZE).min(visible.len());
        &visible[..end]
    }

    /// Toggle one entry in the selection.
    pub fn toggle(&mut self, name: &str) {
        if !self.selected.remove(name) {
            self.selected.insert(name.to_string());
        }
    }

    /// Select every currently visible entry.
    pub fn select_all_visible(&mut self) {
        let names: Vec<String> = self.visible().iter().map(|e| e.name.clone()).collect();
        self.selected.extend(names);
    }

    /// Drop selections that no longer exist (after refresh).
    pub fn prune_selection(&mut self) {
        let names: BTreeSet<String> = self.entries.iter().map(|e| e.name.clone()).collect();
        self.selected.retain(|name| names.contains(name));
    }
}

/// Sort borrowed entries in place.
///
/// Name sort keeps directories first (panel convention); size/mtime sort
/// purely by value so large/recent files surface regardless of kind.
pub fn apply_sort(entries: &mut [&FileInfo], sort: BrowserSort, dir: SortDir) {
    entries.sort_by(|a, b| {
        let order = match sort {
            BrowserSort::Name => (b.file_type.is_dir().cmp(&a.file_type.is_dir()))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            BrowserSort::Size => a.size.cmp(&b.size),
            BrowserSort::Modified => a.modified.cmp(&b.modified),
        };
        match dir {
            SortDir::Asc => order,
            SortDir::Desc => order.reverse(),
        }
    });
}

/// Case-insensitive wildcard match (`*` any run, `?` one char).
/// Empty pattern matches everything; patterns without wildcards degrade to
/// substring search.
pub fn matches_filter(name: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    if !pattern.contains(['*', '?']) {
        return name.to_lowercase().contains(&pattern.to_lowercase());
    }
    glob_match(&name.to_lowercase(), &pattern.to_lowercase())
}

fn glob_match(text: &str, pattern: &str) -> bool {
    let (mut ti, mut pi) = (0, 0);
    let (text, pattern) = (text.as_bytes(), pattern.as_bytes());
    let (mut star, mut match_idx) = (None, 0);
    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == b'?' || pattern[pi] == text[ti]) {
            ti += 1;
            pi += 1;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            star = Some(pi);
            match_idx = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }
    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }
    pi == pattern.len()
}

/// Breadcrumb segments for `path`: `(label, full_path)` from root to leaf.
pub fn breadcrumbs(path: &str) -> Vec<(String, String)> {
    let mut crumbs = vec![("/".to_string(), "/".to_string())];
    let mut accumulated = String::new();
    for part in path.split('/').filter(|p| !p.is_empty()) {
        accumulated.push('/');
        accumulated.push_str(part);
        crumbs.push((part.to_string(), accumulated.clone()));
    }
    crumbs
}

/// TTL cache for directory listings (reduces SFTP round trips on
/// back-and-forth navigation; refresh bypasses it).
#[derive(Debug, Default)]
pub struct DirCache {
    ttl: Duration,
    entries: HashMap<(SessionId, String), (Instant, Vec<FileInfo>)>,
}

impl DirCache {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            ttl: Duration::from_secs(ttl_secs),
            entries: HashMap::new(),
        }
    }

    /// Fresh cached listing, if any.
    pub fn get(&self, session: SessionId, path: &str) -> Option<Vec<FileInfo>> {
        self.entries
            .get(&(session, path.to_string()))
            .filter(|(at, _)| at.elapsed() < self.ttl)
            .map(|(_, entries)| entries.clone())
    }

    /// Store a listing (resets selection-relevant staleness upstream).
    pub fn insert(&mut self, session: SessionId, path: &str, entries: Vec<FileInfo>) {
        self.entries
            .insert((session, path.to_string()), (Instant::now(), entries));
    }

    /// Drop one directory (refresh path).
    pub fn invalidate(&mut self, session: SessionId, path: &str) {
        self.entries.remove(&(session, path.to_string()));
    }

    /// Drop every cached directory of a session (disconnect/reconnect path —
    /// a stale listing must never survive a reconnect).
    pub fn invalidate_session(&mut self, session: SessionId) {
        self.entries.retain(|(s, _), _| *s != session);
    }

    /// Cached paths (tests/diagnostics).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh_sftp::protocol::FileAttributes;
    use std::time::{Duration, UNIX_EPOCH};

    fn info(name: &str, size: u64, mtime: u64) -> FileInfo {
        FileInfo::from_metadata(
            &format!("/home/ops/{name}"),
            &FileAttributes {
                size: Some(size),
                uid: None,
                user: None,
                gid: None,
                group: None,
                permissions: Some(0o644),
                atime: None,
                mtime: Some(mtime as u32),
            },
        )
    }

    #[test]
    fn sort_name_size_modified() {
        let entries = [
            info("b.txt", 200, 30),
            info("a.txt", 100, 20),
            info("c.txt", 300, 10),
        ];
        let mut refs: Vec<&FileInfo> = entries.iter().collect();
        apply_sort(&mut refs, BrowserSort::Name, SortDir::Asc);
        assert_eq!(
            refs.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["a.txt", "b.txt", "c.txt"]
        );
        apply_sort(&mut refs, BrowserSort::Size, SortDir::Desc);
        assert_eq!(refs[0].name, "c.txt");
        apply_sort(&mut refs, BrowserSort::Modified, SortDir::Asc);
        assert_eq!(refs[0].name, "c.txt");
    }

    #[test]
    fn filter_wildcard_and_substring() {
        assert!(matches_filter("photo.JPG", ""));
        assert!(matches_filter("photo.jpg", "*.jpg"));
        assert!(matches_filter("photo.jpg", "PHOTO*"));
        assert!(matches_filter("ab", "a?"));
        assert!(!matches_filter("abc", "a?"));
        assert!(matches_filter("report-final.pdf", "report"));
        assert!(!matches_filter("notes.txt", "*.pdf"));
    }

    #[test]
    fn breadcrumbs_cover_root_and_leaf() {
        assert_eq!(breadcrumbs("/"), vec![("/".to_string(), "/".to_string())]);
        let crumbs = breadcrumbs("/home/ops/docs");
        assert_eq!(crumbs.len(), 4);
        assert_eq!(
            crumbs[3],
            ("docs".to_string(), "/home/ops/docs".to_string())
        );
    }

    #[test]
    fn selection_toggle_and_prune() {
        let mut state = FileBrowserState::new(1, "/");
        state.entries = vec![info("a", 1, 1), info("b", 2, 2)];
        state.toggle("a");
        assert!(state.selected.contains("a"));
        state.toggle("a");
        assert!(!state.selected.contains("a"));
        state.toggle("gone");
        state.prune_selection();
        assert!(state.selected.is_empty());
        state.select_all_visible();
        assert_eq!(state.selected.len(), 2);
    }

    #[test]
    fn cache_ttl_and_invalidation() {
        let mut cache = DirCache::new(60);
        assert!(cache.is_empty());
        cache.insert(1, "/home", vec![info("a", 1, 1)]);
        assert!(cache.get(1, "/home").is_some());
        assert!(cache.get(1, "/tmp").is_none());
        cache.invalidate(1, "/home");
        assert!(cache.get(1, "/home").is_none());
        cache.insert(1, "/a", vec![]);
        cache.insert(2, "/b", vec![]);
        cache.invalidate_session(1);
        assert_eq!(cache.len(), 1);

        let mut expired = DirCache::new(0);
        expired.insert(1, "/x", vec![info("a", 1, 1)]);
        assert!(expired.get(1, "/x").is_none(), "zero TTL never hits");
    }

    #[test]
    fn pagination_caps_rows() {
        let mut state = FileBrowserState::new(1, "/");
        state.entries = (0..500).map(|i| info(&format!("f{i:03}"), i, i)).collect();
        let first_page = {
            let visible: Vec<&FileInfo> = state.visible();
            state.page(&visible).len()
        };
        assert_eq!(first_page, PAGE_SIZE);
        state.pages = 3;
        let full = {
            let visible: Vec<&FileInfo> = state.visible();
            state.page(&visible).len()
        };
        assert_eq!(full, 500);
    }

    #[test]
    fn modified_none_sorts_before_some() {
        let mut with_time = info("new", 1, 100);
        with_time.modified = Some(UNIX_EPOCH + Duration::from_secs(100));
        let timeless = FileInfo::from_metadata(
            "/home/ops/old",
            &FileAttributes {
                size: Some(1),
                uid: None,
                user: None,
                gid: None,
                group: None,
                permissions: None,
                atime: None,
                mtime: None,
            },
        );
        let entries = [with_time, timeless];
        let mut refs: Vec<&FileInfo> = entries.iter().collect();
        apply_sort(&mut refs, BrowserSort::Modified, SortDir::Asc);
        assert_eq!(refs[0].name, "old");
    }
}
