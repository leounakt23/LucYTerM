//! All UI messages. Every variant is handled in `app::update` (exhaustive
//! match, no `_` arm). Payloads are `Clone + Send + Sync` so they can cross
//! from the Tokio runtime into the UI thread.
//!
//! Note: with iced 0.13 the side-effect type is `iced::Task<Message>` —
//! this is the renamed `Command<Message>` from iced ≤0.10 (see tech_stack.md).

use crate::task::TaskEvent;
use mbxt_core::SessionId;

#[cfg(feature = "ssh")]
use crate::connection::sftp::{BrowserSort, FileInfo, SortDir};

/// Session management events (feature matrix §2.2).
#[derive(Debug, Clone)]
pub enum SessionMsg {
    /// Stored sessions were (re)loaded from disk.
    Loaded(Vec<mbxt_core::SessionSpec>),
    Selected(SessionId),
    Created(String),
    Renamed(SessionId, String),
    Deleted(SessionId),
    Tagged(SessionId, String),
    /// Flip SSH X11 forwarding for a session (session-list toggle).
    X11Toggled(SessionId),
    /// Create a session from a full spec (new-session dialog submit).
    CreateDetailed(Box<mbxt_core::SessionSpec>),
    /// Replace a session's spec (edit dialog submit, id preserved).
    UpdateDetailed(SessionId, Box<mbxt_core::SessionSpec>),
}

/// Connection lifecycle events (feature matrix §2.1).
#[derive(Debug, Clone)]
pub enum ConnectionMsg {
    ConnectRequested(SessionId),
    DisconnectRequested(SessionId),
    /// `(session, percent 0..=100)`
    Progress(SessionId, u8),
    Connected(SessionId),
    Disconnected(SessionId),
    Failed(SessionId, String),
}

/// Terminal I/O events (feature matrix §2.3).
#[derive(Debug, Clone)]
pub enum TerminalMsg {
    /// Coalesced output chunk(s) arrived for the grid (≤1/frame).
    Output(SessionId, Vec<u8>),
    /// Raw input bytes destined for the PTY (keyboard, multi-exec).
    Input(SessionId, Vec<u8>),
    Bell(SessionId),
    TitleChanged(SessionId, String),
}

/// File transfer / SFTP panel events (feature matrix §2.4).
#[derive(Debug, Clone)]
pub enum SftpMsg {
    BrowseRequested(SessionId, String),
    TransferQueued {
        session: SessionId,
        remote_path: String,
        local_path: String,
    },
    /// `(session, done bytes, total bytes)`
    TransferProgress(SessionId, u64, u64),
    TransferCompleted(SessionId),
    TransferFailed(SessionId, String),
    /// Pause one managed transfer (transfer-view button; id is
    /// `TransferId.0` from the transfer manager).
    PauseRequested(u64),
    /// Resume a paused transfer (requeues; offsets recompute on execution).
    ResumeRequested(u64),
    /// Cancel a queued/running transfer (fires its cancel token).
    CancelRequested(u64),
    /// Per-chunk progress from a managed executor
    /// `(transfer id, done bytes, total bytes or None)`.
    ManagerProgress(u64, u64, Option<u64>),
    /// A managed executor finished `(transfer id, outcome)`.
    ManagerFinished(u64, TransferOutcome),
    /// Start queued transfers / fire due retries (scheduler trigger).
    PumpTransfers,
    // -- file browser (Prompt 3.3; ssh-gated: entries are SFTP metadata) --
    /// A directory listing arrived `(session, path, entries)`.
    #[cfg(feature = "ssh")]
    DirectoryListed {
        session: SessionId,
        path: String,
        entries: Vec<FileInfo>,
    },
    /// A directory listing failed `(session, reason)`.
    #[cfg(feature = "ssh")]
    DirectoryFailed(SessionId, String),
    /// Reload the current directory, bypassing the listing cache.
    #[cfg(feature = "ssh")]
    BrowserRefresh(SessionId),
    /// Change the sort column/direction.
    #[cfg(feature = "ssh")]
    BrowserSortChanged(SessionId, BrowserSort, SortDir),
    /// Change the wildcard filter (empty clears).
    #[cfg(feature = "ssh")]
    BrowserFilterChanged(SessionId, String),
    /// Toggle one entry in the bulk selection.
    #[cfg(feature = "ssh")]
    BrowserToggled(SessionId, String),
    /// Select every visible entry.
    #[cfg(feature = "ssh")]
    BrowserSelectAll(SessionId),
    /// Clear the bulk selection.
    #[cfg(feature = "ssh")]
    BrowserClearSelection(SessionId),
    /// Reveal the next lazy page (large directories).
    #[cfg(feature = "ssh")]
    BrowserShowMore(SessionId),
    /// Inline input text changed (mkdir/rename/chmod form).
    #[cfg(feature = "ssh")]
    BrowserInputChanged(SessionId, String),
    /// Confirm the pending inline form.
    #[cfg(feature = "ssh")]
    BrowserInputConfirmed(SessionId),
    /// Dismiss the pending inline form.
    #[cfg(feature = "ssh")]
    BrowserInputCancelled(SessionId),
    /// Open the mkdir form.
    #[cfg(feature = "ssh")]
    BrowserMkdirRequest(SessionId),
    /// Open the rename form for an entry.
    #[cfg(feature = "ssh")]
    BrowserRenameRequest(SessionId, String),
    /// Open the chmod form (applies to the selection).
    #[cfg(feature = "ssh")]
    BrowserChmodRequest(SessionId),
    /// Open the upload form (input holds the local source path).
    #[cfg(feature = "ssh")]
    BrowserUploadRequest(SessionId),
    /// Delete every selected entry.
    #[cfg(feature = "ssh")]
    BrowserDeleteSelected(SessionId),
    /// Queue downloads for every selected file.
    #[cfg(feature = "ssh")]
    BrowserDownloadSelected(SessionId),
    /// Preview a file (first lines) or show properties.
    #[cfg(feature = "ssh")]
    BrowserPreview(SessionId, String),
    /// Preview content arrived `(session, path, text)`.
    #[cfg(feature = "ssh")]
    BrowserPreviewReady(SessionId, String, String),
    /// Dismiss the preview/properties pane.
    #[cfg(feature = "ssh")]
    BrowserPreviewClosed(SessionId),
    /// Show the properties dump for an entry.
    #[cfg(feature = "ssh")]
    BrowserProperties(SessionId, String),
    /// Right-click opened a menu (`None` == background).
    #[cfg(feature = "ssh")]
    BrowserContextOpened(SessionId, Option<String>),
    /// Dismiss the context menu.
    #[cfg(feature = "ssh")]
    BrowserContextClosed(SessionId),
}

/// Structured executor outcome (Prompt 3.2): mirrors the retry classes of
/// the transfer manager without depending on transport error types, so this
/// message stays available in every feature combination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferOutcome {
    Completed,
    Cancelled,
    /// Transient failure worth retrying (carries display message).
    Retryable(String),
    /// Permanent failure (carries display message, never retried).
    Failed(String),
}

/// Macro editor text field (Prompt 5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroEditorField {
    Name,
    Description,
    Tags,
    Text1,
    Text2,
    Number,
    Hotkey,
}

/// Tunnel form field (Prompt 5.3): neutral bind/target slots, mapped per
/// kind on submit (Local: bind→remote, Remote: bind(remote side)→local).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelField {
    Name,
    BindHost,
    BindPort,
    TargetHost,
    TargetPort,
}

/// Tunnel kind picker (Prompt 5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelKind {
    Local,
    Remote,
    Dynamic,
}

/// Port-forwarding events (Prompt 5.3, ssh-gated: definitions reference
/// live SSH sessions).
#[derive(Debug, Clone)]
#[cfg(feature = "ssh")]
pub enum TunnelMsg {
    /// Open the form for a session.
    OpenForm(SessionId),
    /// Close the form without saving.
    CloseForm,
    /// Tunnel form field edited.
    FieldChanged(TunnelField, String),
    /// Tunnel kind picked.
    KindSelected(TunnelKind),
    /// Auto-start checkbox flipped.
    AutoStartToggled,
    /// Save the form (add or update).
    Submit,
    /// Load a definition into the form for editing.
    Edit(uuid::Uuid),
    /// Delete a definition.
    Delete(uuid::Uuid),
    /// Start a defined forward.
    Start(uuid::Uuid),
    /// Stop a running forward.
    Stop(uuid::Uuid),
    /// Async start finished.
    Started(uuid::Uuid, Result<String, String>),
    /// Async stop finished.
    Stopped(uuid::Uuid),
}

/// Network-tools hub events (Prompt 5.4): draft edits, run control,
/// and streamed runner events.
#[derive(Debug, Clone)]
pub enum ToolMsg {
    /// Tool picker selection (resets the params form to kind defaults).
    KindSelected(crate::tools::ToolKind),
    /// Target host/URL/CIDR edited.
    TargetChanged(String),
    /// Run on this host.
    ScopeLocal,
    /// Run on a session shell.
    ScopeRemote(SessionId),
    /// Per-tool param field edited (`field`, `value`).
    ParamChanged(String, String),
    /// Start the configured run.
    RunRequested,
    /// Cancel a running run.
    CancelRequested(u64),
    /// Drop finished runs from history.
    ClearHistory,
    /// History search box edited.
    SearchChanged(String),
    /// One runner event for run `id`.
    Event(u64, crate::tools::ToolEvent),
}

/// Macro recording & playback events (Prompt 5.2).
#[derive(Debug, Clone)]
pub enum MacroMsg {
    /// Start recording (active session) / stop and keep.
    RecordToggle,
    /// Pause / resume the active recording.
    RecordPause,
    /// Play a macro on the active session.
    Play(uuid::Uuid),
    /// Play a macro across multi-exec targets (leader context).
    PlayOnTargets(uuid::Uuid),
    /// Stop the running player (cancels immediately).
    Stop,
    /// Continue after a wait sleep / timeout guard.
    Advance(SessionId),
    /// Variable prompt text edited.
    PromptInputChanged(String),
    /// Answer the pending variable prompt.
    PromptAnswer(String),
    /// Abort the pending variable prompt (stops the play).
    PromptCancel,
    /// Confirm a held destructive macro.
    ConfirmPlay,
    /// Discard a held destructive macro.
    DiscardPlay,
    /// A bound hotkey fired (`F5`–`F8`).
    HotkeyPressed(String),
    /// Rescan the macros directory.
    Refresh,
    /// Library load finished `(macros, errors)`.
    LibraryLoaded(Vec<crate::macros::Macro>, Vec<String>),
    /// Open the editor (`None` = new macro).
    EditorOpened(Option<uuid::Uuid>),
    /// Close the editor without saving.
    EditorClosed,
    /// Select a step for editing.
    EditorSelected(usize),
    /// Editor field edited.
    EditorFieldChanged(MacroEditorField, String),
    /// Append a built step of this kind.
    EditorAddStep(crate::macros::MacroStepKind),
    /// Delete a step.
    EditorDeleteStep(usize),
    /// Move a step (`true` = up).
    EditorMoveStep(usize, bool),
    /// Save the draft to the library.
    EditorSave,
    /// Dry-run the draft against a headless terminal.
    EditorTest,
    /// Dry-run finished (report text for the preview pane).
    EditorTestDone(String),
    /// Duplicate a macro.
    EditorDuplicate(uuid::Uuid),
    /// Delete a macro.
    EditorDelete(uuid::Uuid),
    /// Export a macro bundle to the data dir.
    EditorExport(uuid::Uuid),
    /// Export finished (path for the notification).
    EditorExported(String),
    /// Playback progress `(session, completed, total)`.
    MacroProgress {
        session: SessionId,
        current_step: usize,
        total_steps: usize,
    },
}

#[cfg(feature = "vnc")]
use crate::connection::vnc::input::ScalingMode;

/// Remote-desktop viewer events (Prompt 4.3, vnc-gated).
#[derive(Debug, Clone)]
#[cfg(feature = "vnc")]
pub enum VncMsg {
    /// Open (or focus) the viewer tab for a session.
    OpenViewer(SessionId),
    /// Close the viewer tab and drop the VNC session.
    CloseViewer(SessionId),
    /// 15 fps repaint trigger (the widget polls the latest snapshot).
    FrameTickAll,
    /// Pointer moved `(session, buttons, position)` — buttons ride along so
    /// drags keep working between click events.
    PointerMoved(SessionId, u8, (u16, u16)),
    /// Pointer buttons changed `(session, mask, position)`.
    PointerButton(SessionId, u8, (u16, u16)),
    /// Wheel notch `(session, position, up)`.
    PointerWheel(SessionId, (u16, u16), bool),
    /// Key press/release `(session, keysym, down)`.
    KeyEvent(SessionId, u32, bool),
    /// Single tap `(session, keysym)` — down+up in one task.
    KeyTap(SessionId, u32),
    /// Send the Ctrl+Alt+Del chord.
    SendCad(SessionId),
    /// Push the local clipboard mirror to the remote side.
    SendClipboard(SessionId),
    /// Switch Fit / 1:1 scaling.
    SetScaling(SessionId, ScalingMode),
    /// Toggle window fullscreen for the viewer.
    ToggleFullscreen(SessionId),
    /// Window id for the fullscreen toggle `(session, id, entering)`.
    GotWindowId(SessionId, Option<iced::window::Id>, bool),
    /// VNC handshake completed.
    Connected(SessionId),
    /// VNC connect/handshake failed.
    ConnectFailed(SessionId, String),
    /// Transient viewer error (input send failed, …).
    ViewerError(SessionId, String),
}

/// Pure UI actions (feature matrix §2.6).
#[derive(Debug, Clone)]
pub enum UiMsg {
    Quit,
    ToggleSidebar,
    ThemeToggled,
    ReleaseChannelChanged(crate::utils::config::ReleaseChannel),
    TelemetryToggled(bool),
    CrashReportingToggled(bool),
    CheckForUpdates,
    SearchChanged(String),
    ViewChanged(ViewKind),
    MultiExecToggled(bool),
    /// Set the target set for multi-execution broadcasts (#32).
    MultiExecTargets(Vec<SessionId>),
    /// Flip multi-exec on/off (Ctrl+Shift+M); enabling with no targets
    /// auto-selects connected sessions.
    MultiExecToggle,
    /// Toggle one session in the target set (sidebar checkboxes).
    MultiExecToggleTarget(SessionId),
    /// Quick-select: all connected sessions.
    MultiExecSelectAll,
    /// Quick-select: connected SSH-family sessions.
    MultiExecSelectSsh,
    /// Quick-select: connected sessions carrying a tag.
    MultiExecSelectTag(String),
    /// Quick-select: connected sessions with open tabs.
    MultiExecSelectTabs,
    /// Drop every target.
    MultiExecClearTargets,
    /// Stagger between targets in ms (0 == simultaneous).
    MultiExecStaggerChanged(u64),
    /// Hostname filter for conditional execution (empty == all).
    MultiExecFilterChanged(String),
    /// Template text edited (per-session `$SESSION_*` substitution).
    MultiExecTemplateChanged(String),
    /// Broadcast the template once (rendered per target).
    MultiExecTemplateSend,
    /// Confirm a held destructive broadcast.
    MultiExecConfirmBroadcast,
    /// Discard a held destructive broadcast.
    MultiExecDiscardBroadcast,
    /// Open/focus the combined-output view tab.
    MultiExecOpenView,
    /// A staggered delivery failed (prune + notify).
    MultiExecTargetFailed(SessionId, String),
    /// Toolbar: create a new session (default placeholder config).
    NewSession,
    /// Toolbar: quick-connect flow — opens the new-session dialog.
    QuickConnect,
    /// Open the new-session dialog (protocol/host/device picker).
    NewSessionDialogOpened,
    /// Load an existing session into the dialog for editing.
    EditSession(SessionId),
    /// Close the dialog without creating.
    NewSessionDialogClosed,
    /// Dialog field edited.
    NewSessionFieldChanged(NewSessionField, String),
    /// Dialog protocol picked.
    NewSessionProtocolSelected(mbxt_core::Protocol),
    /// Refresh the serial device list.
    NewSessionSerialRefresh,
    /// Create from the dialog draft.
    NewSessionSubmitted,
    /// Toolbar / shortcut: open the settings view.
    OpenSettings,
    /// Toolbar: open the macro library tab.
    OpenMacrosView,
    /// Toolbar: open the tunnels tab.
    OpenTunnelsView,
    /// Toolbar: open the network-tools tab.
    OpenToolsView,
    /// `Ctrl+T`: open a new tab for the active session (or welcome).
    NewTab,
    /// `Ctrl+W`: close the active tab.
    CloseActiveTab,
    /// Click a tab in the tab strip.
    SelectTab(usize),
    /// Master password entered (startup prompt or unlock dialog, prompt 1.2).
    /// Triggers store decrypt on the blocking pool; success routes the
    /// sessions through `SessionMsg::Loaded`.
    MasterPasswordSubmitted(String),
    /// Unlock failed — wrong password or corrupted store.
    MasterPasswordRejected(String),
    /// "Report Issue": bundle recent logs + diagnostics for a bug report.
    ReportIssue,
    /// Open the feedback form without collecting or transmitting data.
    OpenFeedback,
    FeedbackKindChanged(crate::feedback::FeedbackKind),
    FeedbackTitleChanged(String),
    FeedbackDescriptionChanged(String),
    FeedbackLogsToggled(bool),
    FeedbackBreadcrumbsToggled(bool),
    FeedbackScreenshotToggled(bool),
    FeedbackScreenshotPathChanged(String),
    /// Auth dialog text input changed (prompt 2.2 modal).
    AuthInputChanged(String),
    /// Auth dialog OK: submit the buffered answer.
    AuthSubmit,
    /// Auth dialog Cancel: dismiss without answering.
    AuthCancel,
}

/// Editable new-session dialog field (Prompt 4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewSessionField {
    Name,
    Host,
    Port,
    Device,
    Baud,
}

/// Views reachable from the shell (feature matrix #45).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    SessionList,
    Settings,
    Sftp,
    About,
    Feedback,
}

/// External/system events arriving via subscriptions (architecture §3.3).
#[derive(Debug, Clone)]
pub enum SystemEvent {
    /// SIGHUP/SIGINT/SIGTERM (graceful shutdown path).
    ShutdownSignal,
    /// NetworkManager reachability transition (linux, via nmcli/dbus).
    NetworkStatusChanged { online: bool },
    /// Screen lock event: privacy action (clear clipboard cache).
    ScreenLocked,
}

/// Root message enum: one variant per event domain.
#[derive(Debug, Clone)]
pub enum Message {
    Session(SessionMsg),
    Connection(ConnectionMsg),
    Terminal(TerminalMsg),
    Sftp(SftpMsg),
    Macro(MacroMsg),
    Tool(ToolMsg),
    #[cfg(feature = "ssh")]
    Tunnel(TunnelMsg),
    #[cfg(feature = "vnc")]
    Vnc(VncMsg),
    Ui(UiMsg),
    System(SystemEvent),
    /// Task manager lifecycle events forwarded by the subscription bridge.
    Task(TaskEvent),
    /// A task was accepted for scheduling on the runtime.
    TaskScheduled(crate::task::TaskId),
    /// 30-second autosave tick (or explicit save-on-change flush).
    AutosaveTick,
    /// Result of a persisted snapshot write.
    Persisted(Result<(), String>),
    /// Issue bundle collection finished (prompt 1.3 "Report Issue").
    IssueBundleReady(Result<String, String>),
    /// Result of a signed channel-manifest check.
    UpdateChecked(Result<String, String>),
    /// A provider needs user input (password / passphrase / OTP) — show the
    /// auth modal (prompt 2.2).
    AuthPrompt(crate::connection::ssh::auth::AuthPrompt),
    /// The user answered prompt `id` (async wait resolves in the provider).
    AuthResponse {
        id: u64,
        value: String,
    },
    /// Clipboard contents changed (polled via copypasta).
    ClipboardChanged(String),
    /// Explicitly ignored effect (placeholder arms).
    Ignored,
}
