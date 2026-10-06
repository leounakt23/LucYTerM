# Technology Stack Selection & Justification

**Prompt:** 0.2 — Technology Stack Selection & Justification
**Status:** Accepted v1 (supersedes the provisional Tauri assumption in `doc/feature_requirements.md` §1.2)

---

## 0. Binding constraints

| # | Constraint | Consequence for the stack |
|---|---|---|
| C1 | Linux-native — **not Electron, not web-based** | No Electron, no Tauri, no webview renderer. The terminal grid must be rendered by our own native widget. |
| C2 | X11 **and** Wayland support | Use a GPU abstraction that targets both (wgpu → Vulkan/GL via X11/Wayland surfaces); never talk to Xlib directly in UI code. |
| C3 | Single static binary | Every hard dependency must be pure Rust or statically linkable (musl target). No C/C++ UI toolkit that pulls LGPL shared objects. |
| C4 | Permissive license (MIT / Apache-2.0) | Excludes LGPL/GPL-linked UI stacks (Qt widgets under LGPL, libssh under LGPL) from *static* distribution models. |

---

## 1. Language

**Choice: Rust** (edition 2021, stable channel)

| Language | Verdict | Key reasons |
|---|---|---|
| **Rust** | ✅ **Chosen** | Memory safety without GC; fearless concurrency (SSH channels, SFTP transfers, PTY pumps are heavily concurrent); Cargo with reproducible lockfile; crate ecosystem covers our entire dependency graph in pure Rust. Adopted at scale: Linux kernel, Android, Windows driver/model code — evidence of long-term viability. Default dual MIT/Apache-2.0 licensing satisfies C4. |
| C++ | ❌ Rejected | Peak performance is equal, but ~70% of serious security bugs in Chromium and MS Office are memory-safety bugs (project postmortems) — unacceptable for an app whose core value is secure credential handling. No unified package manager; static-linking a modern UI stack under a permissive license is painful. |
| Go | ❌ Rejected | GC and goroutine stacks add baseline memory (against our <100 MB idle / <300 MB peak targets); the GC must be kept out of the render loop, which fights the runtime. GUI story requires cgo (breaks C3) or web wrappers. Excellent for CLI tooling, wrong tool for a latency-sensitive GUI daemon hybrid. |
| Zig | ❌ Rejected | Pre-1.0, unstable std/ABI, package ecosystem far too small to source SSH, SFTP, crypto, and GUI from Zig crates today. Re-evaluate as a future FFI/CLI companion language, not the core. |

**Why this matters for our metrics:** zero-GC, no-runtime overhead is what makes M4 (idle < 100 MB) and
M3 (60 fps terminal rendering) achievable in the same process that runs dozens of SSH channels.

---

## 2. GUI Framework

**Choice: Iced** (retained-mode, Elm-architecture) + **wgpu** renderer

| Framework | Verdict | Key reasons |
|---|---|---|
| **Iced** | ✅ **Chosen** | Pure Rust (C3), renders via wgpu → Vulkan/Metal/GL on both X11 and Wayland (C2), MIT license (C4). Proven at desktop scale: it is the foundation of System76's COSMIC desktop environment — the largest production Rust GUI deployment on Linux. Elm-style `Message → update → view` maps cleanly to terminal session events (PTY output chunks, connection state, transfer progress). Supports custom widgets — required, because the terminal grid *is* a custom widget. |
| egui | ❌ Rejected | Immediate-mode redraws the entire UI every frame by design; with 10+ sessions of live output the per-frame cost and input latency are worse than a retained tree that repaints dirty regions. Fine prototype ergonomics, weaker long-term fit for a terminal-centric app. |
| Qt (via `cxx-qt`/bindings) | ❌ Rejected | The most mature toolkit, but: C++ core violates the spirit of C1/C3, LGPL forces dynamic linking or license gymnastics (C4), and every binding layer (`cpp_core`, `cxx-qt`, `qmetaobject-rs`) is a maintenance tax on a small team. |
| Tauri / GTK-rs / Slint | ❌ Rejected | Tauri: web-based — excluded by C1 outright (and was the basis of the earlier provisional assumption, now superseded). GTK-rs: mature and pleasant, but LGPL dynamic-linking (C3/C4) and a C dependency we don't need. Slint: permissive and native, but its widget model targets embedded/declarative UIs; custom high-throughput text-grid rendering is not its strength. |

**Renderer:** custom **wgpu**-based terminal renderer (instanced quad per cell, one texture atlas for
glyphs). This is the same architecture Alacritty and WezTerm validated on Linux (both hold 60 fps with
glxgears-style load). Risks in §8.

---

## 3. Networking

### 3.1 SSH — **Choice: russh** (pure Rust, Tokio-native)

| Library | Verdict | Key reasons |
|---|---|---|
| **russh** | ✅ **Chosen** | Pure Rust (C3), MIT/Apache (C4), built on Tokio so SSH channels compose with our async runtime natively. Exposes the primitives we need: shell + exec channels, PTY requests, `direct-tcpip`/`forwarded-tcpip` (L/R/D forwarding), X11 and agent channel forwarding, keyboard-interactive auth. Successor of thrussh with an active maintainer and real production users. |
| libssh (C) | ❌ Rejected | Mature and fast, but LGPL (C4 static-linking problem), unsafe FFI boundary in a security-critical component, and a sync C API that needs a blocking-pool bridge into Tokio. Kept as a documented **fallback escape hatch** if russh hits a blocker against exotic servers (spawn the system OpenSSH binary — zero code, always correct). |
| `async-ssh`/`thrussh` | ❌ Rejected | thrussh is russh's predecessor and is effectively in maintenance mode; `async-ssh` is not a maintained general-purpose client. |

### 3.2 SFTP — **Choice: russh-sftp**

Runs the SFTP subsystem over the same authenticated russh connection as the shell — one auth, two
channels, which is exactly what the SFTP side-panel feature (#26) requires. Async request pipelining
(overlapping read/write requests, window management) is how we reach the throughput needed for the
1 GB-resume acceptance test in §4.1 of `feature_requirements.md`.

### 3.3 Terminal emulation — **Choice: vte parser + custom wgpu renderer**

- **vte** (from the Alacritty project): the most battle-tested VT/xterm parser in Rust — it is the
  parser inside Alacritty and is reused by WezTerm. Writing our own parser is the classic trap;
  using vte means we inherit years of real-world escape-sequence bug fixes and `vttest` behavior.
- Our layer on top: grid/screen model (scrollback, alternate screen, modes), OSC 7 cwd capture
  (#31), bracketed paste, mouse-mode encoding.
- **Renderer:** custom wgpu widget — glyph atlas + instanced cell quads; dirty-region tracking so a
  repaint only touches the damaged rows (keeps M3 at 60 fps even with `yes > /dev/null` flooding in
  one of many sessions).

Rejected alternative: reusing `alacritty_terminal` wholesale — it bundles its own event model and
term structure that fight Iced's; vte alone is the right-cut abstraction.

### 3.4 Async runtime — **Choice: Tokio**

De-facto standard (most-downloaded async runtime on crates.io by a wide margin; used by AWS SDK,
Discord, etc.). Multi-threaded work-stealing scheduler fits our workload: many small IO tasks
(SSH channels, SFTP, PTY pumps) + a few CPU tasks (crypto KDF, grid diffs). `tokio::task::spawn_blocking`
covers the rare blocking call (dbus sync calls, file IO).

### 3.5 System integration

| Crate | Use | Notes |
|---|---|---|
| **nix** | Unix syscalls: `openpty` for session PTYs, termios for serial sessions (#8), signals, `fork`/`exec` for custom-command sessions (#9) and protocol spawners (RDP/VNC/#3/#4) | Thin, safe wrappers over libc; no runtime cost. |
| **dbus-rs** | Desktop integration: Secret Service keyring access, dark/light color-scheme portal (#43), desktop notifications (#46) | Sync API → wrap in `spawn_blocking`. Alternative noted in risk matrix: `zbus` (pure-Rust async) is the drop-in replacement if dbus-rs friction grows. |

---

## 4. Cryptography

**Choice: RustCrypto family** (pure Rust, permissive, no C dependency — keeps C3/C4 intact)

| Concern | Crate | Justification |
|---|---|---|
| Symmetric encryption (session DB at rest, #47) | `aes-gcm` (AES-256-GCM, AEAD) | RustCrypto's flagship AEAD; constant-time AES backends incl. AES-NI. Rejected: `ring`/`aws-lc-rs` — excellent but C/asm cores complicate the pure-musl static build and carry heavier build toolchains. |
| KDF (master password → key, #48) | `argon2` (**Argon2id** — finalized in `doc/architecture.md` §6.1, superseding the provisional scrypt choice) | Memory-hard; parameters tuned to ~100 ms on reference hardware (m=64 MiB, t=3, p=1). Same RustCrypto family as the rest of the crypto stack. |
| SSH host/user key algorithms | `ed25519-dalek` (+ RSA/ECDSA via russh's crypto stack) | ed25519 is the modern default for SSH keys; dalek is the reference Rust implementation. |
| Randomness | `getrandom` / `rand_core` | OS entropy source (`getrandom(2)`); no userspace PRNG for key material. |
| Signature/verdict | — | All MIT/Apache-2.0; several RustCrypto AEADs have undergone formal verification/audits (AES-GCM implementations were formally verified in the RustCrypto verification campaign). |

**Verification plan:** known-answer tests (NIST vectors) for AES-GCM and Argon2 in CI; property
test
that an encrypted DB round-trips and that a tampered ciphertext fails AEAD verification.

---

## 5. Serialization

**Choice: Serde + RON (human-readable) + MessagePack (binary)**

| Format | Crate | Used for |
|---|---|---|
| RON | `ron` | Session store export/import, app config, workspace layouts. Rust-flavored but human-readable, supports comments and round-tripping — users can inspect/diff their config. |
| MessagePack | `rmp-serde` | Internal wire format (UI ↔ backend task messages if split), cache blobs, transfer manifests for resume support (#30). Compact, schema-flexible, fast. |
| (JSON) | `serde_json` | Import compatibility only (#17: PuTTY/ssh_config importers emit JSON-shaped intermediates). Not a primary format. |

Rejected: TOML (poor programmatic round-trip — comments are destroyed on rewrite), bincode (fine
internally but weaker cross-language tooling than MessagePack).

---

## 6. Build & Packaging

### 6.1 Single static binary (C3)

- Primary target: `x86_64-unknown-linux-musl` (and `aarch64-unknown-linux-musl`) → one self-contained
  binary; all core deps (russh, vte, iced, RustCrypto) are pure Rust.
- **wgpu:** loads the system Vulkan/GL driver at runtime via `dlopen` — driver code is *not* linked,
  so the binary stays static and portable across machines/drivers. GLSL/WGSL shaders are compiled
  into the binary.
- **DNS under musl:** glibc NSS is unavailable when statically linked; use `hickory-resolver`
  (pure-Rust DNS) so hostname resolution works in the static build.
- **glibc fallback build** (`x86_64-unknown-linux-gnu`) kept for distro packages where musl
  performance/compat quirks matter.

### 6.2 Distribution formats

| Format | Notes |
|---|---|
| AppImage | Flagship download — single-file, matches the static-binary constraint. |
| Flatpak | Sandboxed store distribution; portal access for notifications/schemes. |
| `.deb` / `.rpm` | Distro-native; depends on system Vulkan loader at runtime only. |
| AUR / nix | Community-maintained derivations; CI publishes a version stamp. |

### 6.3 Dependency hygiene & CI

- **`cargo audit`** in CI (weekly scheduled job + on every PR): fails the build on known
  RUSTSEC advisories touching our dependency graph; advisory DB pulled fresh per run.
- **`cargo deny`**: license check (only MIT/Apache/BSD/ISC/Unicode-3.0 accepted — enforces C4),
  duplicate-crate detection, advisory checking as a second layer.
- **Renovate/Dependabot**: automated bump PRs, batched monthly except security patches.
- **`Cargo.lock` committed**: releases are reproducible; `cargo vet`-style review for new
  transitive deps with network/crypto capability (lightweight `cargo vet` adoption planned).
- CI matrix: {x86_64, aarch64} × {musl static, gnu} + clippy + `rustfmt --check` + tests incl.
  crypto KATs and vttest harness.

---

## 7. Dependency list (main crates)

Versions are latest stable at time of writing; `Cargo.lock` is authoritative at build time.

| Crate | Version | Purpose | License |
|---|---|---|---|
| `tokio` | 1.47 (features: full minus process-priv bits as needed) | Async runtime | MIT |
| `russh` | 0.5x | SSH client: channels, PTY, forwarding, agent/X11 channels | MIT/Apache-2.0 |
| `russh-sftp` | 2.x | SFTP subsystem client | MIT/Apache-2.0 |
| `vte` | 0.13 | VT/xterm parser | MIT/Apache-2.0 |
| `iced` | 0.13 | GUI framework (retained, Elm-style) | MIT |
| `wgpu` | 25 | GPU renderer (Vulkan/GL, X11+Wayland) | MIT/Apache-2.0 |
| `nix` | 0.30 | Unix syscalls (pty, termios, signals) | MIT |
| `dbus` | 0.9 | Desktop integration (Secret Service, portals) | MIT/Apache-2.0 |
| `serde` | 1.0 (+derive) | Serialization framework | MIT/Apache-2.0 |
| `ron` | 0.9 | Human-readable config/session format | MIT/Apache-2.0 |
| `rmp-serde` | 1.3 | MessagePack binary serialization | MIT |
| `aes-gcm` | 0.10 | Session-store encryption (AES-256-GCM) | MIT/Apache-2.0 |
| `argon2` | 0.5 | Master-password KDF (Argon2id) | MIT/Apache-2.0 |
| `ed25519-dalek` | 2.x | SSH key algorithm support | BSD-3 |
| `getrandom` | 0.3 | OS entropy | MIT/Apache-2.0 |
| `rusqlite` | 0.34 (bundled) | Session/history database | MIT |
| `keyring` | 3.x | Secret Service / KWallet integration | MIT/Apache-2.0 |
| `hickory-resolver` | 0.25 | Pure-Rust DNS (musl-safe) | MIT/Apache-2.0 |
| `directories` | 6 | XDG base-dir handling | MIT/Apache-2.0 |
| `tracing` / `tracing-subscriber` | 0.1 / 0.3 | Structured logging | MIT |
| `thiserror` / `anyhow` | 2 / 1 | Error handling (lib / app) | MIT/Apache-2.0 |
| `serialport` | 4.x | Serial sessions (#8) | MPL-2.0 ⚠ (check link model; userspace only) |
| CI tools: `cargo-audit`, `cargo-deny`, `clippy`, `rustfmt` | latest | Supply-chain & lint gates | MIT/Apache-2.0 |

> Note: `serialport` links libudev (LGPL) on Linux — acceptable as it is dynamically linked system
> dependency usage (not distributed in our binary), but flagged for the license review checklist.
> If static purity matters more, `nix`+raw termios is the fallback path.

---

## 8. Risk matrix

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | **russh maturity**: fewer real-world audits than OpenSSH; occasional breaking API changes between minors | Medium | High | Conformance suite in CI against OpenSSH (channel, forwarding, agent, X11 matrix); pinned `Cargo.lock`; documented fallback: spawn system `ssh` for unsupported servers; security review before v1.0 (ties to M6). |
| R2 | **Iced pre-1.0 API churn** (0.x releases break APIs) | High | Medium | Pin versions; wrap framework-specific types behind a thin app-layer façade so widget code is isolated; track Iced master in a weekly CI build to catch breakage early. COSMIC's ongoing usage is the strongest stabilizing force. |
| R3 | **Custom wgpu terminal renderer effort/perf** — biggest engineering risk | Medium | High | Ship in stages: (1) atlas+instanced quads, benchmark vs. Alacritty baseline early; (2) dirty-region optimization; (3) only then ligatures/shaping (P2 #24). If perf or effort derails, fallback renderer: `iced`'s text pipeline (slower but functional) unblocks everything else. |
| R4 | Static musl build vs. GPU stack | Low | Medium | wgpu dlopen's drivers at runtime (verified approach); CI builds musl target on every PR so breakage is caught immediately. |
| R5 | SFTP throughput below OpenSSH's `sftp` client | Medium | Medium | Pipelined async requests (depth ~20) + benchmark gate in CI (1 GB transfer acceptance test); window-size tuning is config-exposed. |
| R6 | dbus/Secret Service absent (headless, minimal WM) | Medium | Low | Graceful degradation: `keyring` unavailable → fall back to master-password-encrypted file store (#47/#48) and surface the downgrade in UI. |
| R7 | dbus-rs sync API in async context | Medium | Low | Confine to `spawn_blocking`; if it becomes a drag, swap to `zbus` (pure-Rust, async, same protocol) — isolated behind a trait. |
| R8 | libudev LGPL linkage via `serialport` | Low | Low | Keep dynamic system linkage (not redistributed); license review checklist item; termios fallback documented. |
| R9 | X11/Wayland behavioral differences (clipboard, drag-drop, HiDPI) | Medium | Medium | CI smoke tests on Xvfb *and* a Wayland compositor (Sway headless); use wgpu + standard clipboard/portal crates rather than raw X11 calls; drag-drop is in-app native (no portal dependency for core flow). |
| R10 | Scope creep beyond parity | Medium | Medium | `doc/feature_requirements.md` matrix is the contract; new features require a matrix row + priority decision first. |

---

## 9. Decision record

| Date | Decision |
|---|---|
| Prompt 0.1 | Provisional stack: Tauri/webview (later found to violate the not-web-based constraint). |
| Prompt 0.2 | **Final for core:** Rust + Iced/wgpu + vte + russh/russh-sftp + Tokio + RustCrypto + nix/dbus-rs. Tauri formally rejected under constraint C1; `doc/feature_requirements.md` §1.2/§1.3 updated accordingly. |
