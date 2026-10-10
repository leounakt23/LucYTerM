# MASTER PROMPT: Linux-Native Remote Computing Application

> You are an expert software architect and developer. Your task is to build a
> complete, production-ready Linux-native remote computing application inspired
> by MobaXterm, using the specifications below. Execute phases sequentially;
> do not proceed until the current phase's gates pass.

## Honesty Rules (Binding)

- Never claim a version, platform, integration, or metric that was not
  measured in this repository. Current version is `0.1.0`; there is no v1.0
  release yet. Performance numbers are targets until measured and recorded in
  `docs/final_report.md`.
- Single-maintainer project (bus factor 1). Do not invent co-maintainers,
  moderators, chat rooms, social accounts, funding channels, or partnerships.
- Response times and roadmaps are best-effort targets, not SLAs or contracts.
- Where this prompt differs from earlier idealized sketches, the table in
  "Technology Stack" and the structure below are authoritative.

## Technology Stack (Authoritative)

| Layer    | Crate / Tool                                                                                                                  | Notes                                                                        |
| -------- | ----------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Language | Rust 1.85.1 pinned (`rust-toolchain.toml`), MSRV 1.80, edition 2021                                                           | Earlier sketches said "latest stable"; pin it. The pin now gates some security fixes (docs/maintenance.md).                               |
| GUI      | `iced` 0.13.1 (`tokio`, `advanced`), `wgpu` 24                                                                                | Earlier sketches said 0.12; 0.13.1 is implemented.                           |
| Async    | `tokio` 1.44 with a curated subset (`rt-multi-thread`, `macros`, `sync`, `net`, `time`, `io-util`, `signal`, `fs`, `process`) | Not "full features".                                                         |
| SSH/SFTP | `russh` 0.45, `russh-sftp` 2.0                                                                                                | Key handling lives in `russh`; there is no separate `russh-keys` dependency. 0.45 carries open advisories; planned migration to 0.61.1 (docs/maintenance.md). |
| Crypto   | `argon2` 0.5.3, `aes-gcm` 0.10.3, `zeroize` 1.8.1, `secrecy` 0.10.3, `memsec` 0.7                                             | AES-256-GCM only; no `chacha20poly1305` dependency exists.                   |
| Terminal | `vte` 0.13.0 parser + custom grid/atlas/renderer                                                                              | —                                                                            |
| Config   | `serde` 1.0 + `ron` 0.8 (+ `rmp-serde` 1.3 compat)                                                                            | —                                                                            |
| Logging  | `tracing` + `tracing-subscriber` (+ `tracing-appender`)                                                                       | —                                                                            |
| CLI      | `clap` 4.5 (no-derive subset) + `clap_complete`                                                                               | —                                                                            |
| System   | `nix` 0.29, `directories` 5.0, `serialport` 4.7, `dbus` 0.9, `keyring` 3.2, `copypasta` 0.10                                  | —                                                                            |
| Feedback | `reqwest` 0.12, `sentry` 0.36, `minidumper` 0.8 (all opt-in, feature-gated)                                                   | —                                                                            |

Protocol gates (Cargo features): `ssh` (implies `sftp`), `telnet`, `serial`,
`rdp`, `vnc`, `x11`, `gssapi`. RDP runs via an external process with X11
embedding; it is not a vendored FreeRDP binding. VNC/Telnet/serial live behind
their gates with live-server tests opt-in.

## Project Structure (Authoritative)

```text
Cargo.toml Cargo.lock build.rs rust-toolchain.toml Cross.toml deny.toml
src/
  main.rs lib.rs
  app/            # messages, state, update, subscriptions, multi_exec, notifications
  bin/            # remote-app-headless entry (cli.rs)
  connection/     # actor, credential_cache, ssh/, sftp/, forward/, x11/,
                  # vnc/, telnet/, serial/
  terminal/       # atlas, renderer
  tools/          # ping, traceroute, dns, whois, port_scanner, subnet,
                  # http_client, bandwidth
  ui/             # main_window, terminal_widget, file_browser, theme,
                  # keyboard, drag_drop, context_menu, auth_dialog,
                  # new_session_dialog, transfer_view, tunnels_panel,
                  # multi_exec_view, multi_exec_toolbar, macro_editor,
                  # macro_panel, vnc_view, widgets/
  utils/          # config, crypto, error, i18n, keyring_bridge, logging,
                  # paths, profiling, secrets, secure_storage, security,
                  # single_instance
  macro/ security/ task/ feedback/ test_utils/
crates/           # core, terminal, connections, storage, system, cli
xtask/            # docs, i18n, release tasks
tests/            # config_integration, forward_integration, network_tools,
                  # properties, sftp_integration, snapshots (+ snapshots/,
                  # fixtures/); vnc_integration, x11_integration (live-gated)
benches/          # performance (Criterion)
fuzz/fuzz_targets/# config_parser, macro_parser, ping_parser, subnet_parser,
                  # vte_parser
packaging/        # debian, rpm, flatpak, snap, appimage, arch, nix, scripts/
docs/             # mdBook source + canonical policy docs; src/ is the book
doc/              # architecture, feature_requirements (historical v1
                  # baseline), tech_stack
rfcs/ docs/feature_proposals/  # RFC process (both valid; rfcs/ canonical)
locales/en-US/app.ftl  assets/  docker/  Cross.toml
.github/workflows/# ci, lint, security, coverage, bench, fuzz, e2e, nightly,
                  # release, docs, pr-automation, release-drafter, stale,
                  # all-contributors
```

## Implementation Phases (Execute in Order)

### 1. Foundation

Set up the workspace above with the pinned toolchain. Implement application
state with Elm architecture (`src/app/`), secure configuration with
AES-256-GCM + Argon2id (`src/utils/crypto.rs`, `secure_storage.rs`), error
handling (`thiserror`/`anyhow`), logging (`tracing`), and the main window with
sidebar, content area, and status bar (`src/ui/main_window.rs`).

Gate: `cargo build --locked --workspace --all-features` succeeds.

### 2. SSH and Terminal

Implement the SSH client (password, key, agent, keyboard-interactive, optional
GSSAPI) in `src/connection/ssh/` + `crates/connections`, the VTE terminal
model/grid (`crates/terminal`), and the wgpu glyph-atlas renderer
(`src/terminal/`). Support multiple concurrent sessions via session actors.

Gate: unit + property tests for parsers/terminal pass; live SSH tests remain
opt-in behind `MBXT_*_TEST_*` variables.

### 3. SFTP and Files

Implement the SFTP client (list, upload, download, resume), the transfer
manager (concurrency, throttling, pause/resume), and the file browser UI with
drag-and-drop, context menus, and sorting (`src/connection/sftp/`,
`src/ui/file_browser.rs`, `transfer_view.rs`).

Gate: `sftp_integration` non-live cases pass; resume/overwrite semantics
documented.

### 4. Multi-Protocol

Add X11 forwarding with SSH channel integration, RDP via managed child process

- X11 embedding, VNC, Telnet, and serial — each behind its Cargo feature with
  diagnostics and tests that do not silently connect anywhere.

Gate: feature-matrix builds (`--no-default-features` + each gate) compile;
live VNC/X11 tests only run with explicit server configuration.

### 5. Advanced Features

Multi-exec broadcast, macro record/playback with variable substitution, local /
remote / dynamic SOCKS5 forwarding, and network tools (ping, traceroute, DNS,
whois, port scanner, HTTP client, subnet calculator).

Gate: `forward_integration`, `network_tools`, macro/config parser fuzz targets
build and pass smoke runs.

### 6. Polish and Production

Theming (built-in + custom), performance budgets (<2 s startup, <100 MB idle,
60 FPS — all targets until measured), threat-model hardening, packaging for
DEB/RPM/Flatpak/Snap/AppImage/AUR/Nix, test pyramid (>80% line-coverage
target), full CI/CD, beta program, and community files.

Gate: the verification checklist in `docs/verification_checklist.md` is
executed and `docs/final_report.md` records measured values.

## Quality Requirements

- No `unsafe` unless necessary, documented with a safety comment,
  platform-isolated.
- No `unwrap`/`expect` on untrusted input or I/O paths; typed errors only.
- Public APIs documented; behavior changes ship tests + docs in the same PR.
- Secrets never touch logs, screenshots, fixtures, or commit messages.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo audit`, `cargo deny`, mdBook build, and `git diff --check` are gates.

## Deliverables

Complete source in `src/` + `crates/`, configs (`Cargo.toml`, `build.rs`,
workflows), packaging for all listed formats, mdBook docs, test suite with
coverage report, automated CI/CD, and community files.

## Execution Instructions

After each phase: build, test, clippy, fmt, then commit with a Conventional
Commit message. See `docs/getting_started_for_developers.md` for the rebuild
walkthrough. Begin with Phase 1: foundation and build system.
