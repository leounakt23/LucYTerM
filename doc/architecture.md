# Project Architecture & Design Patterns

**Prompt:** 0.3 — Project Architecture & Design Patterns
**Status:** Accepted v1
**References:** `doc/feature_requirements.md` (feature matrix, M1–M8 metrics), `doc/tech_stack.md`
(stack decisions; note: KDF finalized to **Argon2id** here per security review — see §6.1).

---

## 1. Component diagram

Five layers; dependencies point **downward only** (UI → Logic → Connection → Storage/System).
Cross-layer communication happens exclusively through channels and traits — no layer reaches
sideways into another's internals.

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│  UI LAYER  (main thread — Iced + wgpu, 60 fps render loop)                  │
│                                                                             │
│  ┌──────────┐ ┌──────────┐ ┌─────────────┐ ┌───────────┐ ┌──────────────┐   │
│  │ TabBar   │ │ Terminal │ │ SessionTree │ │ SftpPanel │ │ SettingsView │   │
│  │ widget   │ │ widget*  │ │ widget      │ │ widget    │ │ widget       │   │
│  └────┬─────┘ └────┬─────┘ └──────┬──────┘ └─────┬─────┘ └──────┬───────┘   │
│       └────────────┴───────┬──────┴───────────────┴──────────────┘           │
│                     views()│  ▲ Message                                       │
│                   ┌────────┴─────────┐   ┌──────────────────────────────┐    │
│                   │  App (MVU root)  │   │ Custom widgets:              │    │
│                   │  - state: Model  │   │  *TerminalGrid (wgpu atlas)  │    │
│                   │  - update(Msg)   │◄──┤  *SplitPane layout manager   │    │
│                   │  - view() -> El  │   └──────────────────────────────┘    │
│                   └───────┬──────────┘                                       │
└───────────────────────────┼──────────────────────────────────────────────────┘
              Command(Msg)  │  ▲ broadcast: UiEvent (coalesced ~60 Hz)
┌───────────────────────────▼──────────────────────────────────────────────────┐
│  APPLICATION LOGIC LAYER  (Tokio runtime, multi-thread)                      │
│                                                                              │
│  ┌───────────────┐  ┌──────────────┐  ┌────────────────┐  ┌───────────────┐  │
│  │ SessionManager│  │ ActionRouter │  │ EventBus       │  │ MacroEngine   │  │
│  │ (singleton)   │  │ (Command     │  │ (tokio broadcast│ │ (record/replay│  │
│  │  owns sessions│  │  pattern)    │  │  Observer)     │  │  multi-exec)  │  │
│  └───────┬───────┘  └──────┬───────┘  └────────────────┘  └───────────────┘  │
│          │   ┌─────────────┼──────────────┐                                  │
│          ▼   ▼             ▼              ▼                                  │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐                          │
│  │ Session actors│ │ TransferQueue│ │ ForwardMgr   │   (one actor task per    │
│  │ (1 per conn) │ │ (bulk/resume)│ │ (L/R/D fwd)  │    open session)         │
│  └──────┬───────┘ └──────┬───────┘ └──────┬───────┘                          │
└─────────┼────────────────┼────────────────┼──────────────────────────────────┘
          │                │                │
┌─────────▼────────────────▼────────────────▼──────────────────────────────────┐
│  CONNECTION LAYER                                                            │
│                                                                              │
│  ConnectionFactory ──creates──► Box<dyn Connection>                          │
│                                   ├── SshConn     (russh: shell/exec/sftp/   │
│                                   │                fwd/agent/X11 channels)    │
│                                   ├── TelnetConn  (rust-telnet)              │
│                                   ├── SerialConn  (nix termios + serialport) │
│                                   ├── SpawnConn   (RDP/VNC/custom: nix fork  │
│                                   │                /exec + PTY capture)       │
│                                   └── LocalShell  (child PTY)                │
│  AuthStrategy (Strategy pattern): Password │ KeyFile │ Agent │ KbdInteractive │
└──────────────────────────────────────────────────────────────────────────────┘
          │                                   │
┌──────────▼───────────────┐   ┌───────────────▼───────────────────────────────┐
│  STORAGE LAYER           │   │  SYSTEM INTEGRATION LAYER                     │
│                          │   │                                               │
│  SessionStore (SQLite +  │   │  Keyring     (Secret Service via dbus)        │
│  AES-256-GCM envelope)   │   │  Clipboard   (OSC 52 + wl-clipboard/X11 CLIP) │
│  ConfigStore (RON files) │   │  Notifier    (dbus notifications portal)      │
│  HistoryStore (FTS5)     │   │  SignalGuard (SIGINT/SIGCHLD/SIGHUP → events) │
│  CryptoVault (Argon2id)  │   │  SchemeWatch (dark/light portal watcher)      │
└──────────────────────────┘   └───────────────────────────────────────────────┘
```

\* The terminal widget is the only custom wgpu widget in v1 (tech_stack.md R3 staged plan).

**Layer rules (enforced by module visibility + clippy):**
1. UI never touches russh/nix/dbus types — it sees only `core::` domain types and `UiEvent`s.
2. Connection layer never blocks the UI thread; it never calls UI code directly (events only).
3. Storage and System layers are dependency-free leaves (no upward calls except via events).

---

## 2. Module breakdown (Cargo workspace)

```text
mbxt/
├── crates/
│   ├── core/        # Domain types, zero framework deps
│   ├── terminal/    # vte front-end: grid, scrollback, search, wgpu widget
│   ├── connections/ # Transport trait + protocol impls + factory + auth strategies
│   ├── storage/     # SQLite stores, CryptoVault, config
│   └── system/      # dbus, clipboard, notifications, signals
└── app/             # Iced binary: MVU wiring, views/widgets, runtime startup
```

| Crate | Contents | Responsibilities | Depends on |
|---|---|---|---|
| `core` | `Session`, `SessionSpec`, `Protocol`, `AuthMethod`, `UiEvent`, `Action`, error types | Shared vocabulary; **no I/O**. Pure data + traits. | serde, thiserror |
| `terminal` | `Grid`, `Scrollback`, `ScreenModes`, `Searcher`, `TerminalGrid` widget, glyph atlas | Parse vte callbacks into grid diffs; render; bracketed paste; mouse encoding; OSC 7 capture | core, vte, wgpu, iced |
| `connections` | `Connection` trait, `ConnectionFactory`, `SshConn`, `TelnetConn`, `SerialConn`, `SpawnConn`, `AuthStrategy` impls, `ForwardMgr` | Open/own byte streams + control channels per session; SFTP subsystem; port forwarding; keep-alive/reconnect | core, russh, russh-sftp, nix, tokio |
| `storage` | `SessionStore`, `HistoryStore`, `ConfigStore`, `CryptoVault` | Encrypted persistence, FTS search, config load/save (RON), key derivation | core, rusqlite, aes-gcm, argon2, zeroize |
| `system` | `Keyring`, `Clipboard`, `Notifier`, `SignalGuard`, `SchemeWatch` | OS/desktop integration; all dbus/nix syscalls isolated here | core, dbus, nix |
| `app` | `App` (MVU root), `Message`, views/widgets, `ActionRouter`, `SessionManager`, `EventBus` wiring, `main()` | Composition root; owns singletons; converts `UiEvent` → `Message`; converts user intent → `Action`/`Command` | all crates |

---

## 3. Design patterns

### 3.1 Model-View-Update (Elm) — UI state

- **Model:** single `App` struct tree (`sessions: SessionTree`, `tabs: Vec<TabState>`,
  `layout: SplitTree`, `sftp: SftpState`, `settings: Settings`).
- **Message:** one `enum Message` with per-widget sub-enums (`Msg::Terminal(TerminalMsg)`, …).
- **Update:** pure-ish state transitions; side effects are returned as `iced::Command`s, never
  performed inline — keeps `update()` testable without a GPU or network.
- Session output does **not** create one Message per PTY chunk (would drown the loop at M3 loads).
  `terminal` coalesces grid diffs and emits at most one `Msg::TerminalRedraw(tab_id)` per frame.

### 3.2 Command pattern — async actions

`enum Action` is the queued, serializable unit of work: `Connect(SessionId)`, `Disconnect`,
`StartTransfer { session, plan }`, `OpenForward { spec }`, `MultiExec { targets, bytes }`, …
`ActionRouter` consumes them on the Tokio runtime. Two entry points produce the same action type:
(a) `iced::Command::perform` from `update()` for user-triggered work, (b) an internal mpsc queue for
programmatic/replayed work (macros, reconnect). Benefits: undo-able surface later, macro engine can
reuse the exact same actions, and every action is one `tracing` span for diagnostics.

### 3.3 Observer / Event bus — inter-module communication

- `EventBus` = `tokio::sync::broadcast<UiEvent>` (bounded, 1024; lagging consumers get `Lagged`
  and resync — terminal redraws are idempotent full-frame refreshes).
- Producers: session actors (`SessionEvent::Output`, `StateChanged`, `Bell`, `Osc7(cwd)`), transfer
  queue (`Progress`, `Done`, `Failed`), system layer (`SchemeChanged`, `Signal`).
- Consumer: a single `app` task maps events → `Message` and pushes into the Iced loop via its
  mailbox — the *only* bridge from Tokio-land into the UI thread.
- Subscribers that don't affect UI (logger, macro recorder) subscribe directly on the runtime and
  never cross into UI.

### 3.4 Factory pattern — protocol connections

`ConnectionFactory::create(&SessionSpec) -> Result<Box<dyn Connection>>` matches on `Protocol`:
SSH → `SshConn`; Telnet → `TelnetConn`; Serial → `SerialConn`; RDP/VNC/custom → `SpawnConn`
(`fork`/`exec` + PTY capture, window spawned externally per feature matrix #3/#4). New protocols are
added by adding one module + one match arm; nothing upstream changes (open/closed principle).

### 3.5 Strategy pattern — authentication

```rust
#[async_trait]
pub trait AuthStrategy {
    async fn authenticate(&self, ctx: &AuthContext<'_>) -> Result<AuthOutcome, ConnError>;
}
```
Implementations: `Password`, `KeyFile` (with passphrase), `Agent` (`SSH_AUTH_SOCK`), and a
`KeyboardInteractive` chain. Selection order and per-session overrides live in `SessionSpec`;
fallthrough rules (e.g., agent → password prompt) are data, not code paths.

### 3.6 Singleton — application-wide resources

Deliberately scoped to `app`'s composition root, constructed once in `main()` **before** the runtime
spins up, then shared as `Arc` handles (no global mutable state, no `lazy_static` races):

- `ConfigStore` (RON config, hot-reloaded via `notify`)
- `SessionStore` + `CryptoVault` (unlocked once with master password)
- `SessionManager` (session actor registry)
- `EventBus` (cloneable handle)
- `tracing` subscriber (set once, non-blocking writer)

---

## 4. Concurrency model

```text
MAIN THREAD (UI)                      TOKIO MULTI-THREAD RUNTIME
┌─────────────────────────┐           ┌─────────────────────────────────────────┐
│ Iced event loop         │  mpsc     │  ActionRouter task                      │
│  update(Msg)            │──────────►│   validates + dispatches Actions        │
│  view() -> Element      │ Action    │        │                                │
│  wgpu render @ vsync    │◄──────────┤        ▼                                │
└───────────▲─────────────┘ UiEvent   │  Session actor (per session)            │
            │ broadcast (via bridge)  │   ├─ PTY reader task ──┐                 │
            │                         │   ├─ SSH channel tasks ─┼─► russh conn    │
┌───────────┴─────────────┐           │   └─ keepalive ticker ──┘                 │
│ Bridge task:            │           │  TransferQueue (bounded workers, 4)       │
│  broadcast UiEvent      │           │  ForwardMgr listeners (L/R/D)             │
│  → Message into mailbox │           │  MacroEngine (record from event bus,      │
└─────────────────────────┘           │   replay via Action queue)                │
                                      └─────────────────────────────────────────┘
```

Channel inventory (all typed; no raw byte channels across module boundaries):

| Channel | Type | Direction | Purpose |
|---|---|---|---|
| `action_tx` | `mpsc<Action>` (bounded 256) | UI → ActionRouter | User-initiated work |
| `event_tx` | `broadcast<UiEvent>` (1024) | Runtime → UI bridge | Output chunks, state, progress |
| `ctl_tx` | `mpsc<SessionCtl>` | Router → session actor | Connect/disconnect/resize/kill |
| `reply_rx` | `oneshot<Result<…>>` | Action → caller | "Did it work?" for modal ops (e.g., connect handshake) |
| PTY pipe | byte stream task | session actor → terminal grid | Output; grid diffs coalesced to 1 event/frame |

Rules:
1. **The UI thread never blocks.** All I/O lives on Tokio; the only cross-thread data is `Message`s.
2. **Backpressure is explicit:** bounded channels; when the action queue is full, UI shows a busy
   state instead of silently queuing unboundedly.
3. **One actor per session** owns its connection — no shared mutable `SshHandle`; forwarding and
   SFTP are tasks under the same actor, so disconnect ordering is trivial (drop the actor).
4. **Cancellation:** dropping a session's `ctl` sender aborts its task tree (cancellation-safety
   reviewed per task; transfers checkpoint resume offsets first — feature #30).

---

## 5. Sequence diagrams

### 5.1 Connect (SSH, master password already unlocked)

```text
User        UI (update)        Router/Actor        SshConn(russh)      Storage/System
 │ click session │                  │                    │                    │
 ├──────────────►│ Action::Connect  │                    │                    │
 │               ├──mpsc───────────►│ spawn actor        │                    │
 │               │                  ├──authenticate─────►│ AuthStrategy chain │
 │               │                  │                    ├──agent/keyring───► │
 │               │                  │◄───AuthOutcome─────│                    │
 │               │                  ├──pty request──────►│                    │
 │               │                  │◄──channel open─────│                    │
 │               │                  ├──store history─────────────────────────► │
 │               │◄─UiEvent::Connected(oneshot ack)──────│                    │
 │◄─tab opens, first prompt paint─│                    │                    │
 │  ...output...  │◄─broadcast TerminalOutput (≤1/frame, coalesced diffs)      │
```

Failure path: `AuthOutcome::Failed` → `UiEvent::Error(user_friendly_msg)` + prompt dialog for
password/keyboard-interactive retry; every attempt logged via `tracing` with session span.

### 5.2 File transfer (SFTP, drag-and-drop, resumable)

```text
User        UI                  TransferQueue        SshConn SFTP chan      Disk
 │ drop file(s)  │                    │                     │                 │
 ├──────────────►│ Action::StartTransfer{plan}              │                 │
 │               ├──mpsc─────────────►│ enqueue (4 workers) │                 │
 │               │                    │ stat remote file ─────────────────►    │
 │               │                    │ ◄─ existing size ── │                 │
 │               │                    │ open local .part ──────────────────────►│
 │               │                    │ seek offset = existing (resume #30)      │
 │               │                    │ loop: read 256 KiB ──write pkt──►        │
 │               │                    │      (pipeline depth 20)                │
 │               │◄─UiEvent::Progress {done, total, bps} ──│                 │
 │ progress bar  │                    │ on error: persist offset, emit Failed   │
 │               │                    │ on success: rename .part, set perms ────►│
 │               │◄─UiEvent::TransferDone ──│                │                 │
```

### 5.3 Multi-exec (broadcast input to N sessions)

```text
User        UI (Terminal widget)   MacroEngine        Session actors (1..N)
 │ toggle targets, type "df -h\n" │                  │                    │
 ├──────────────►│ capture input bytes                │                    │
 │               ├──Action::MultiExec{targets,bytes}──►                   │
 │               │                    │ fan-out ctl:Write(bytes) per target    │
 │               │                    ├───┬───────┬───────┬──► actors 1..N        │
 │               │                    │   ▼       ▼       ▼                       │
 │               │◄─broadcast TerminalOutput(tab_k, diffs) for each response   │
 │ N panes update, tinted borders per target state (#32)                     │
```

Macro replay reuses the exact same `Action::MultiExec` with recorded timing offsets (feature #33).

---

## 6. Security architecture

### 6.1 Secret lifecycle

```text
master password ──Argon2id(m=64 MiB, t=3, p=1, random 16B salt)──► KEK (32B, memory only)
                                                                      │
keyring secret ────────── (optional) session passwords/keys ────────  │
                                                                      ▼
sessions.db record ──AES-256-GCM(plaintext, AAD=record_id||version)──► ciphertext + 96-bit random nonce + tag
```

- **KDF:** Argon2id per OWASP current guidance. *(Supersedes tech_stack.md's scrypt note — same
  RustCrypto family, one-line swap; documented in tech_stack decision record.)*
- **Encryption:** AES-256-GCM per record; AAD binds ciphertext to its row id/version so records
  can't be swapped between rows; key rotation = re-encrypt under new KEK with version bump.
- **Keyring split:** where a Secret Service is available, per-session secrets live in the keyring
  (#49) and the DB stores only references; the DB itself is still encrypted as defense-in-depth.
- **Unlocked state:** KEK held in `Zeroizing<[u8; 32]>`; lock action (manual, idle timeout, app
  exit) drops it and clears cached plaintexts.

### 6.2 Memory protection

- `zeroize` on: master password, KEK, passphrases, any decrypted secret buffer (wiped on drop).
- Secrets never stored in `String` — always `Zeroizing<Vec<u8>>`/`secrecy::SecretVec` so they
  can't be accidentally logged or cloned.
- `mlock` (via `libc::mlock` wrapped in `system`) on the KEK page where permitted; `madvise(MADV_DONTDUMP)`
  on secret regions so core dumps never contain keys.
- Log discipline: `tracing` events that could carry payloads use `%` (Display) of redacted wrappers
  only; a compile-time `Redacted<T>` type makes it a type error to log a secret.

### 6.3 File & process permissions

- `~/.config/mbxt/` and `~/.local/state/mbxt/` created with mode `0700`; `sessions.db` `0600`
  (enforced at open time — self-heal if the user relaxed them, with a warning).
- umask pinned `077` for the app's lifetime; temp files (`.part` transfers) in the same protected dir.
- Spawned children (freerdp, picocom) receive secrets via env/fd, not argv (argv is world-readable
  via `/proc`), and only for the duration needed.
- Crash safety: panic hook redacts any captured locals it prints; telemetry payloads are
  allow-listed field sets, never raw buffers.

---

## 7. Error handling strategy

```rust
// one error enum per crate, thiserror-generated; no anyhow inside libraries
#[derive(Debug, thiserror::Error)]
pub enum ConnError {
    #[error("authentication failed for {user}@{host}")]
    AuthFailed { user: String, host: String },
    #[error("network unreachable: {0}")]
    Network(#[from] std::io::Error),
    #[error("server rejected channel open: {reason}")]
    ChannelOpenRefused { reason: String },
    // ...
}
```

- **Libraries** (`core`, `connections`, `storage`, `system`): `thiserror` enums, `?`-propagation,
  `#[from]` for cause chains. No panics across the FFI/actor boundary (`catch_unwind` at task roots
  converts panics into `UiEvent::Error` — feeds M6 crash-free accounting).
- **Application boundary** (`app/main`): errors become `UiEvent::Error(AppProblem)` where
  `AppProblem { title, detail, hint, retry: Option<Action> }` — the retry action lets the UI offer
  "Reconnect", "Retry transfer" (resume), etc., directly from the error toast.
- **Logging:** every error logged once at creation site with `tracing::error!` inside the session
  span (`span!(parent: session_span, …)`), so `journalctl`/log file reconstructs which
  session/host/transfer failed. UI shows the friendly message, never the raw chain.
- **User-facing mapping table** lives in `app/src/errors.rs` — one place to review all strings
  users can see; includes hints for the common cases (bad passphrase, host key changed, dialout
  group missing).

---

## 8. Testing hooks (architecture-mandated)

| Seam | Mechanism |
|---|---|
| Terminal | `vttest` harness drives `Grid` directly (no GPU needed); golden-image tests for the renderer run only on CI machines with Vulkan lavapipe. |
| Connections | `Connection` trait mocked with in-memory byte streams; russh conformance suite runs against a real OpenSSH container in CI. |
| Storage | Crypto KATs (NIST vectors) + tamper tests; DB tests on `:memory:` SQLite. |
| UI | `update()` is a pure function of `(Model, Message)` — property tests replay random message streams for invariant violations (e.g., a tab that exists in UI but not in SessionManager). |
