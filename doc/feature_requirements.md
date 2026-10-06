# Feature Requirements — Linux-Native Remote Computing Application

**Prompt:** 0.1 — Project Vision & Feature Requirements
**Status:** Historical v1 baseline
**Scope:** Strict MobaXterm (Windows) feature parity on Linux. Post-v1 scope is
governed by `ROADMAP.md` and the feature-proposal process.

---

## 1. Overview

This document defines the full feature set, user personas, and success metrics for a Linux-native
remote computing application intended to replicate and surpass MobaXterm's capabilities on Linux.

### 1.1 Scope

- **In scope:** All feature categories in which MobaXterm is a credible competitor: connection
  protocols, session management, terminal emulation, file transfer, bundled tools, tabbed/split UI,
  and secure credential storage.
- **Out of v1 scope:** Features beyond MobaXterm parity (e.g., Mosh support, session sharing,
  config-as-code export, plugin marketplaces). Deferral from v1 is not a commitment or permanent
  rejection; post-v1 candidates are evaluated in `ROADMAP.md`.

### 1.2 Technology assumptions

> Finalized in `doc/tech_stack.md` (Prompt 0.2); the earlier provisional Tauri assumption was
> rejected under the "not web-based" constraint.

| Layer                    | Choice                                                                           | Rationale                                                                                      |
| ------------------------ | -------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Language / core          | **Rust** (Iced GUI, native — no webview)                                         | Native performance, low memory, memory safety.                                                 |
| GUI                      | **Iced** + **wgpu** renderer (X11 & Wayland)                                     | Pure Rust, permissive license, single static binary, custom widgets for the terminal grid.     |
| Terminal emulation       | **vte** parser + custom **wgpu** grid renderer + native PTYs (`nix`)             | vte is the proven Alacritty/WezTerm parser; renderer gives truecolor/60fps without a webview.  |
| SSH/SFTP                 | **russh + russh-sftp** (fallback: spawn system OpenSSH)                          | Pure-Rust, Tokio-native, agent and keyring friendly.                                           |
| Persistence              | **SQLite** (`rusqlite`), encrypted at rest                                       | Single-file session store, easy search/tagging.                                                |
| Secrets                  | **Secret Service API** via `keyring` crate (libsecret / gnome-keyring / kwallet) | Standard Linux credential storage.                                                             |
| X11 / RDP / VNC / serial | Spawn system clients (`XWayland`/X server, `freerdp`, VNC viewer, `picocom`)     | On Linux an X server already exists; bundling one is unnecessary. Reuse battle-tested clients. |

### 1.3 Constraints imposed by the stack

- The system X server already provides what MobaXterm bundles its own X server for; our app only
  needs to set `DISPLAY`/`WAYLAND_DISPLAY` and tunnel X11 channels over SSH (`-X`/`-Y` equivalent).
- The native Iced/wgpu stack makes the <100 MB idle target (M4) achievable — see `doc/tech_stack.md`
  for the memory budget and risk matrix.
- Drag-and-drop is implemented natively in the Iced widget layer (no browser DnD bridge needed).

---

## 2. Feature Matrix

Columns: **Feature** · **MobaXterm?** · **Our App** (target) · **Priority** (P0 must have /
P1 should have / P2 nice to have) · **Notes** (Linux/Iced-Rust implementation approach).

Priorities are assigned by real-world usage: P0 = an admin/engineer cannot adopt the tool without it;
P1 = expected daily; P2 = convenience.

### 2.1 Connection protocols

| #   | Feature                                              | MobaXterm?                | Our App                    | Priority | Notes                                                                                                                                    |
| --- | ---------------------------------------------------- | ------------------------- | -------------------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | SSH (v2, password / key / keyboard-interactive auth) | ✅                        | ✅                         | **P0**   | `russh` for in-process sessions (needed for SFTP browser, forwarding, multi-exec). Fallback path: spawn system `ssh` for exotic options. |
| 2   | Telnet                                               | ✅                        | ✅                         | **P1**   | Pure-Rust client (`rust-telnet`); legacy devices only, so P1.                                                                            |
| 3   | RDP                                                  | ✅                        | ✅                         | **P1**   | Spawn `freerdp` (`xfreerdp`) in a window; embed later via `wlr-screencopy`/pipewire if needed. No Rust RDP stack is production-ready.    |
| 4   | VNC                                                  | ✅                        | ✅                         | **P1**   | Spawn system VNC viewer (`remote-viewer`, `vncviewer`) with session pre-auth from stored config.                                         |
| 5   | FTP                                                  | ✅                        | ✅                         | **P1**   | `supftp`/`async-ftp` crate; plaintext FTP only where required.                                                                           |
| 6   | SFTP                                                 | ✅                        | ✅                         | **P0**   | `russh-sftp` over the same `russh` connection as the shell — one auth, two channels.                                                     |
| 7   | X11 forwarding (GUI apps over SSH)                   | ✅ (bundled X server)     | ✅ (uses system X/Wayland) | **P0**   | Set `DISPLAY`/`WAYLAND_DISPLAY`; enable SSH X11 channel forwarding in `russh`; `-X`/`-Y` trusted/untrusted modes.                        |
| 8   | Serial (COM/tty)                                     | ✅                        | ✅                         | **P1**   | `serialport` crate for raw IO piped into the PTY layer; baud/parity/flow-control UI.                                                     |
| 9   | Custom / arbitrary command session                   | ✅ (shell, cygwin, Mosh…) | ✅                         | **P0**   | Session type that spawns any binary with args (`docker attach`, `cu`, `socat`, …) through a native PTY (`nix` `openpty`).                |

### 2.2 Session management

| #   | Feature                                 | MobaXterm? | Our App | Priority | Notes                                                                                        |
| --- | --------------------------------------- | ---------- | ------- | -------- | -------------------------------------------------------------------------------------------- |
| 10  | Save / edit sessions                    | ✅         | ✅      | **P0**   | SQLite rows per session; JSON blob for protocol-specific fields.                             |
| 11  | Duplicate / clone sessions              | ✅         | ✅      | **P1**   | Copy row with new name; trivial with SQLite.                                                 |
| 12  | Folders / groups (tree)                 | ✅         | ✅      | **P0**   | Self-referencing `parent_id` in sessions table; tree UI in sidebar.                          |
| 13  | Tags                                    | ✅         | ✅      | **P1**   | `tags` join table; tag filter in search.                                                     |
| 14  | Search sessions                         | ✅         | ✅      | **P0**   | `FTS5` full-text index over name/host/tags/notes; instant sidebar filter.                    |
| 15  | Connection history                      | ✅         | ✅      | **P1**   | Append-only `history` table with timestamps; sorted recents list.                            |
| 16  | Favorites / pinned sessions             | ✅         | ✅      | **P1**   | Boolean/pin-order column; favorites section atop sidebar.                                    |
| 17  | Import (PuTTY/OpenSSH config/SecureCRT) | ✅ (PuTTY) | ✅      | **P1**   | Importers for `known_hosts`, `ssh_config`, PuTTY `.reg`; one-way import only (parity scope). |

### 2.3 Terminal

| #   | Feature                               | MobaXterm? | Our App | Priority | Notes                                                                                                  |
| --- | ------------------------------------- | ---------- | ------- | -------- | ------------------------------------------------------------------------------------------------------ |
| 18  | Full VT100/xterm ANSI emulation       | ✅         | ✅      | **P0**   | `vte` parser (Alacritty/WezTerm lineage) covers VT100/220, xterm, ECMA-48; validated against `vttest`. |
| 19  | True color (24-bit)                   | ✅         | ✅      | **P0**   | Grid renderer truecolor + terminal `COLORTERM=truecolor`; PTY env passthrough.                         |
| 20  | Mouse support (apps: vim, htop)       | ✅         | ✅      | **P0**   | xterm mouse-mode encoding → PTY byte stream; wheel scrolling over alternate screen.                    |
| 21  | Scrollback (configurable lines, 10k+) | ✅         | ✅      | **P0**   | Grid model scrollback ring buffer; cap memory by configurable line count.                              |
| 22  | In-terminal search                    | ✅         | ✅      | **P0**   | In-house search over the grid/scrollback model; highlight-all + next/prev.                             |
| 23  | Copy / paste, selection modes         | ✅         | ✅      | **P0**   | OSC 52 + system clipboard integration; middle-click paste on X11.                                      |
| 24  | Font ligatures / fallback fonts       | ✅         | ✅      | **P2**   | wgpu glyph-atlas renderer with harfbuzz-style shaping added late (staged per tech_stack.md R3).        |
| 25  | URL detection / click-to-open         | ✅         | ✅      | **P1**   | Regex detection over grid cells; open via `xdg-open`.                                                  |

### 2.4 File transfers

| #   | Feature                                             | MobaXterm? | Our App | Priority | Notes                                                                          |
| --- | --------------------------------------------------- | ---------- | ------- | -------- | ------------------------------------------------------------------------------ |
| 26  | SFTP side-panel browser (remote tree + local tree)  | ✅         | ✅      | **P0**   | Reuses the session's `russh` handle; dual-pane native UI; async chunked reads. |
| 27  | Drag-and-drop upload / download                     | ✅         | ✅      | **P0**   | Native Iced drag-and-drop → SFTP put/get; show progress per file.              |
| 28  | File permissions / ownership editing                | ✅         | ✅      | **P1**   | SFTP `SETSTAT` for mode/uid/gid; chmod octal editor dialog.                    |
| 29  | Bulk operations (multi-select, queue)               | ✅         | ✅      | **P1**   | Work queue with concurrency limit (e.g., 4 parallel transfers), cancel/retry.  |
| 30  | Resume interrupted transfers                        | ✅         | ✅      | **P1**   | SFTP `APPEND`/offset-based continue; local `.part` temp files.                 |
| 31  | Follow terminal `cd` (browser tracks cwd via OSC 7) | ✅         | ✅      | **P2**   | Parse OSC 7 cwd reports from PTY to sync the SFTP panel.                       |

### 2.5 Tools

| #   | Feature                                          | MobaXterm?        | Our App | Priority | Notes                                                                                                                                       |
| --- | ------------------------------------------------ | ----------------- | ------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| 32  | Multi-execution (broadcast input to N sessions)  | ✅ ("Multi-exec") | ✅      | **P0**   | Fan-out keystrokes from the focused terminal widget to N PTYs; color-tint borders of targeted sessions.                                     |
| 33  | Macro recording & replay                         | ✅                | ✅      | **P1**   | Record PTY input byte stream with timestamps; replay with variable delays; bind to shortcuts.                                               |
| 34  | Local port forwarding (-L)                       | ✅                | ✅      | **P0**   | `russh` `direct-tcpip`; TCP listener via `tokio`; per-session forward config UI.                                                            |
| 35  | Remote port forwarding (-R)                      | ✅                | ✅      | **P1**   | `forwarded-tcpip` channel handling; requires `russh` server-side request support.                                                           |
| 36  | Dynamic forwarding (SOCKS5, -D)                  | ✅                | ✅      | **P1**   | Local SOCKS5 server in Rust that maps connections to `direct-tcpip` channels.                                                               |
| 37  | Network tools: ping, traceroute, nslookup, whois | ✅                | ✅      | **P1**   | Spawn system binaries (`ping`, `traceroute`/`mtr`, `dig`/`resolvectl`, `whois`) into terminal tabs; parse-free, display raw.                |
| 38  | SSH agent support & agent forwarding             | ✅                | ✅      | **P0**   | Read `SSH_AUTH_SOCK`; forward the agent socket to remote (agent channel in `russh`); also support agent-less key auth from encrypted store. |
| 39  | Keep-alive / auto-reconnect                      | ✅                | ✅      | **P0**   | SSH keepalive requests + exponential backoff reconnect; banner when session drops.                                                          |
| 40  | Jump host / proxy jump chains                    | ✅                | ✅      | **P1**   | Chain `russh` connections (channel-in-channel) matching `ProxyJump` semantics.                                                              |

### 2.6 UI/UX

| #   | Feature                                             | MobaXterm? | Our App | Priority | Notes                                                                                                          |
| --- | --------------------------------------------------- | ---------- | ------- | -------- | -------------------------------------------------------------------------------------------------------------- |
| 41  | Tabbed interface (reorderable, close-other, detach) | ✅         | ✅      | **P0**   | Native tab strip; detach = new Iced window sharing the same backend state.                                     |
| 42  | Split views (horizontal/vertical panes)             | ✅         | ✅      | **P1**   | CSS-grid layout tree persisted per workspace; drag splitters.                                                  |
| 43  | Theming: dark/light app + terminal color schemes    | ✅         | ✅      | **P0**   | Follow freedesktop color-scheme portal via dbus-rs; terminal schemes (iTerm-style `.itermcolors`/JSON import). |
| 44  | Customizable keyboard shortcuts                     | ✅         | ✅      | **P1**   | Shortcut map in config; conflicts detected; defaults match common Linux habits (Ctrl+Shift+C/V).               |
| 45  | Customizable layout / panel visibility              | ✅         | ✅      | **P1**   | Sidebar/SFTP panel/toolbox toggles; per-workspace layout persistence.                                          |
| 46  | Notifications (long job done, disconnect)           | ✅         | ✅      | **P2**   | Desktop notifications via portal (`ashpd` crate); bell (BEL) triggers.                                         |

### 2.7 Security

| #   | Feature                              | MobaXterm?         | Our App | Priority | Notes                                                                                                               |
| --- | ------------------------------------ | ------------------ | ------- | -------- | ------------------------------------------------------------------------------------------------------------------- |
| 47  | Encrypted session storage            | ✅                 | ✅      | **P0**   | SQLite encrypted with AES-256-GCM (`ring`/`age`-style); key derived from master password (Argon2id).                |
| 48  | Master password                      | ✅                 | ✅      | **P0**   | Optional but recommended; unlock prompt at launch; change re-encrypts DB.                                           |
| 49  | Keyring integration (Secret Service) | ✅ (Windows vault) | ✅      | **P0**   | `keyring` crate → gnome-keyring/KWallet via Secret Service; store keys/passwords per-session.                       |
| 50  | SSH private key management           | ✅                 | ✅      | **P1**   | Support agent keys + key files (passphrase-protected, OpenSSH format); never write passphrases to disk unencrypted. |

**Priority totals (50 features):** P0 = 24 · P1 = 23 · P2 = 3.

---

## 3. User Personas

### 3.1 System Administrator — "Priya"

- **Profile:** Manages 50–500 Linux servers across on-prem and cloud; works in terminals all day.
- **Typical workflows:**
  1. Morning pass: open saved session group "prod-web", multi-exec `df -h && free -m` across 12 hosts.
  2. Patch night: SSH jump chain bastion → app tier; run playbooks; tail logs in split view.
  3. Pull a failing log via the SFTP panel drag-and-drop; edit locally; push back.
  4. Fix a stuck service over serial console (IPMI/USB-serial) when the network path is down.
- **Must-have features:** #1 SSH, #12 groups, #18–23 terminal core, #32 multi-exec, #34/38
  forwarding & agent, #39 keep-alive/reconnect, #47–49 encrypted storage & keyring.
- **Adoption blocker if missing:** Multi-exec and session groups — without them, triaging dozens of
  hosts means N separate windows and N logins.

### 3.2 Developer / DevOps Engineer — "Marcus"

- **Profile:** Builds services; SSHes into dev/staging clusters and containers; uses tunnels daily.
- **Typical workflows:**
  1. Dynamic SOCKS forward (#36) to reach a staging-only service in the browser.
  2. Local forward (-L) from laptop port 5432 to a private Postgres.
  3. Tail logs in one split pane while running tests in another.
  4. Jump from session history/recents to the host he used yesterday.
- **Must-have features:** #34–36 all forwarding modes, #15 history, #42 split views, #21 scrollback,
  #25 URL detection, #9 custom sessions (`docker exec`/`kubectl exec` via arbitrary command).
- **Adoption blocker if missing:** Reliable local/dynamic forwarding with a simple UI.

### 3.3 Network Engineer — "Sofia"

- **Profile:** Operates routers/switches/firewalls; lives in Telnet/serial consoles; changes configs
  in batch.
- **Typical workflows:**
  1. Serial console to a brand-new switch for initial config (#8).
  2. Telnet/SSH to legacy gear; record a macro of the config-push keystrokes, replay per device (#33).
  3. Multi-exec `show interface status` across access switches (#32).
  4. Quick ping/traceroute/nslookup from built-in tools while diagnosing (#37).
- **Must-have features:** #2 Telnet, #8 serial, #32 multi-exec, #33 macros, #37 network tools.
- **Adoption blocker if missing:** Solid serial/telnet sessions — web-only SSH tools fail this
  persona outright.

### 3.4 Security Analyst — "Devin"

- **Profile:** Pen-tests and incident response; jumps through bastions; wary of credential hygiene.
- **Typical workflows:**
  1. Chain jump hosts to reach isolated segments (#40).
  2. Keep credentials in the system keyring, never on disk in plaintext (#49); master password for
     the session store (#48).
  3. VNC/RDP into a victim-analysis VM (#3, #4).
  4. Use SOCKS through a pivot for scanning tools (#36).
- **Must-have features:** #47–49 security stack, #36 SOCKS, #40 jump chains, #3/#4 RDP/VNC,
  #38 agent (but _selective_ forwarding).
- **Adoption blocker if missing:** Trust: encrypted-at-rest storage and keyring integration audited
  against plaintext-write regressions.

### 3.5 Student / Hobbyist — "Yuki"

- **Profile:** Learning Linux; connects to a Raspberry Pi, a college VPS, and lab VMs.
- **Typical workflows:**
  1. First-run wizard → saved session "pi@raspberrypi.local"; double-click to connect.
  2. Copy/paste commands from a tutorial with confidence (bracketed paste, clipboard safety).
  3. Drag a file onto the window to upload it (#27) instead of learning `scp` flags.
  4. Switch to dark theme; increase font size.
- **Must-have features:** #1 SSH, #10 save, #23 copy/paste, #27 drag-and-drop, #43 theming, plus a
  forgiving import from `ssh_config` (#17) so existing college lab config "just appears".
- **Adoption blocker if missing:** Zero-config first connect and easy file transfer.

---

## 4. Success Metrics & Acceptance Criteria

Each metric includes a measurement method so acceptance is testable, not aspirational.

| ID  | Metric                             | Target                         | Measurement method                                                                                            | Acceptance criteria                                                                |
| --- | ---------------------------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| M1  | Cold launch time                   | < 2 s                          | Time from `exec` to first interactive frame; median of 10 runs on a reference laptop (i5-8xxx, NVMe, Wayland) | Median < 2.0 s and p95 < 2.5 s                                                     |
| M2  | Connect time (SSH to first prompt) | < 3 s                          | Timestamped from connect click to PTY first output; LAN to a reference server                                 | Median < 3.0 s including auth + PTY spawn                                          |
| M3  | Render performance                 | 60 fps                         | Compositor frame timings (`frame_callback` deltas) during `yes > /dev/null` flood + scrolling                 | ≥ 95% of frames < 16.7 ms; no input echo latency > 50 ms                           |
| M4  | Idle memory                        | < 100 MB                       | RSS after launch with 1 open session, 5 min settle                                                            | < 100 MB (native Iced/wgpu stack; budget in `doc/tech_stack.md`)                   |
| M5  | Heavy usage memory                 | < 300 MB                       | 10 sessions + 1 active SFTP transfer of 1 GB + full scrollback                                                | < 300 MB, no unbounded growth over 8 h soak                                        |
| M6  | Crash-free rate                    | > 99.9%                        | Panic hook + telemetry (opt-in): crashes / (crashes + clean exits) over a release cycle                       | ≥ 99.9% per release; zero data-loss crashes of the session DB                      |
| M7  | Adoption                           | 1000+ GitHub stars in 6 months | GitHub API                                                                                                    | ≥ 1000 stars at month 6 post-v1.0; secondary: ≥ 100 issues/PRs from external users |
| M8  | Feature parity gate                | All P0 shipped                 | Matrix §2                                                                                                     | v1.0 ships 24/24 P0 items; each verified by a functional test                      |

### 4.1 Acceptance tests per P0 area (summary)

- **Protocols:** automated round-trip tests against OpenSSH/Telnet fixtures; manual matrix for
  RDP/VNC/serial against reference devices.
- **Terminal:** pass `vttest` suites 1–6; truecolor test pattern renders exact RGB; paste with
  newlines triggers bracketed-paste.
- **File transfer:** 1 GB transfer resumable after kill -9; permission edits verified via `stat`.
- **Security:** review checklist — no plaintext secrets written anywhere (audited via `strace`
  sample + DB hex dump); master password change re-encrypts; keyring round-trip on
  gnome-keyring _and_ KWallet.

---

## 5. Risks & Mitigations

| #   | Risk                                                         | Impact                                  | Mitigation                                                                                                                                                                                    |
| --- | ------------------------------------------------------------ | --------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 5.1 | No production-grade Rust RDP/VNC stack                       | P1 protocols depend on external clients | Spawn `freerdp`/VNC viewers; session UI owns auth config so UX stays consistent                                                                                                               |
| 5.2 | Remote forwarding in `russh` less battle-tested than OpenSSH | P1 #35                                  | Ship behind a toggle; document `-R` fallback via spawned `ssh`                                                                                                                                |
| 5.3 | Serial devices need group permissions on Linux               | Onboarding friction                     | First-run diagnostic that detects `dialout` group membership and prints the fix                                                                                                               |
| 5.4 | X11 apps on pure-Wayland sessions                            | #7 may fail for some users              | Support XWayland (default) and document X11 mode; Wayland-native forwarding is explicitly out of parity scope                                                                                 |
| 5.5 | **Custom wgpu terminal renderer effort/perf (M3 60 fps)**    | Biggest engineering risk                | Staged delivery per `doc/tech_stack.md` R3: glyph-atlas + instanced quads first, dirty-region optimization second, ligatures last; fallback to Iced's text pipeline if perf or effort derails |
