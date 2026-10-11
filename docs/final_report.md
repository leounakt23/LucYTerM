# Final Report: Remote App v1.0

> Status 2026-10-07: working tree at Phase 6. Repository version is
> `0.1.0`; no v1.0 tag has been cut. Every number below was measured in
> this repository on the stated machine; targets are never copied into
> results, and unmeasured rows stay UNMEASURED.

## Project Summary

- Report date: 2026-10-07
- Release tag: none (no v1.0 release cut; version `0.1.0`)
- Lines of code: 37,189 Rust lines (`src` + `crates` + `tests` +
  `benches` + `xtask`, `wc -l`, this working tree)
- Dependencies: 852 lock entries (`Cargo.lock`); `cargo deny check`
  licenses/bans/sources passes; `cargo audit` reports 5 unpatched
  advisories (see Security Posture)
- Toolchain: Rust 1.85.1 pinned (`rust-toolchain.toml`), MSRV 1.80

## Features Delivered

- Phase 1: workspace + Elm app state, AES-256-GCM + Argon2id config
  store, main window (sidebar/content/statusbar). Gate: full-feature
  build passes.
- Phase 2: SSH password/key/keyboard-interactive plus ssh-agent
  (`authenticate_via_agent`, shared SSH+SFTP), VTE grid/parser,
  wgpu renderer, session actors. Live SSH stays opt-in; GSSAPI is
  optional and feature-gated.
- Phase 3: SFTP list/stat/upload/download/resume, transfer manager
  (concurrency, throttling, pause/resume), file browser + transfer
  views, resume/overwrite policy documented in
  `docs/src/user/file-transfers.md`.
- Phase 4: X11 forwarding, RDP/VNC via managed external processes
  (secret-free argv, stdin passwords, exit-status events), Telnet,
  serial; honest per-protocol Cargo features incl. `gssapi`; 9/9
  matrix rows compile.
- Phase 5: multi-exec broadcast, macro record/playback with
  variables, local/remote/dynamic SOCKS5 forwarding, nine network
  tools (native WHOIS/HTTP/bandwidth backends) behind the hub tab
  and `run_tool` dispatcher; 5 fuzz targets build with clean
  1000-run smokes (nightly, CI parity).
- Phase 6 (this report): file-based custom themes + toolbar
  switching, hermetic SSH loopback tests, loopback throughput
  bench, RDP/VNC session creation, packaging static validation.
- Phase 6 amendment (2026-10-11): terminal rendering has fixture
  coverage — `tests/fixtures/terminal_{vt100,vt100_scroll,colors}.sh`
  feed deterministic escape streams into a real `Terminal` and assert
  anchor cells (CUP/ED/EL, scroll overflow, 16/256/truecolor fg+bg,
  attributes); pixel-level proof stays display-only.

## Performance Metrics

Machine: AMD Ryzen AI 7 350, 30 GB RAM, Ubuntu 24.04. `bench`
profile is release-optimized; `dev` is unoptimized + debuginfo.

| Metric (target) | Measured | Method |
|---|---|---|
| Cold start <2 s | **PASS: 320 ms median** (release; 317 ms debug) | 5 runs, PID-verified first mapped window, Xvfb 1280x800, AMD iGPU, release profile |
| Idle memory <100 MB | **PARTIAL: 194 MB default, 81 MB on single-driver Vulkan path** (release, 60 s idle) | smaps profile (2026-10-10): app heap only 16 MB — the RSS is the graphics stack. Default `Backends::all()` loaded the full GL/GLX vendor stack for nothing; app now defaults to `WGPU_BACKEND=vulkan` (measured 222→194 MB, hybrid NVIDIA dGPU + AMD iGPU box). Vulkan-loader ICD zoo dominates the rest: with `VK_DRIVER_FILES` trimmed, lavapipe-only = 133 MB, radv-only (real AMD iGPU) = **81 MB PASS**. VmRSS==VmHWM (stable, no leak); the target passes on single-driver systems, misses on multi-driver systems unless ICDs are trimmed — recorded, not faked |
| Terminal 60 FPS | UNMEASURED | No frame-time method headless; render path proven working (screenshots) |
| SSH connect <3 s (LAN) | PASS (loopback) | Hermetic `tests/ssh_loopback.rs`: full password handshake + shell echo in 0.09 s for both tests (asserts <3 s); no LAN server available |
| Transfer >100 MB/s (LAN) | 12.7 GB/s (loopback) | `transfer_loopback` criterion bench, 10×1 s sender blasts, median ≈101.7 Gbit/s, byte-exact vs drain; loopback only, not LAN |
| 10 sessions <500 MB | UNMEASURED | Needs GUI + servers |
| CLI startup (informational, no target) | <10 ms, ~9 MB RSS | `remote-app-headless --version`, median of 5, dev profile; NOT the GUI cold start |
| GUI launch + main window | PASS (visual) | `docs/assets/main-window-xvfb.png`: toolbar, sidebar, welcome tab, status bar all render under Xvfb |
| Tools hub run in-app | PASS (visual) | `docs/assets/tools-hub-active-xvfb.png` (tab) + `tools-run-xvfb.png` (subnet run: correct network/broadcast/range/hosts in history) |

## Security Posture

- Storage crypto: AES-256-GCM; keys from Argon2id
  (m=64 MiB, t=3, p=4); secrets zeroized, redacted `Debug` impls.
- Host keys: OpenSSH `known_hosts` check, fail-closed on error
  (`check_server_key` returns false); hermetic test pins both the
  accept and the refuse path.
- Permissions: config dirs `0700` + owner check at init
  (`paths.rs`); store file `0600` (test-asserted).
- `cargo deny check` (licenses/bans/sources): passes. Advisories DB
  is unparsable by the MSRV-pinned deny 0.18.3; `cargo audit`
  covers advisories instead.
- `cargo audit`: 5 vulnerabilities with no fix inside the pinned
  stack + MSRV 1.80: hickory-proto RUSTSEC-2026-0119, rsa
  RUSTSEC-2023-0071, russh RUSTSEC-2026-0154, russh-cryptovec
  RUSTSEC-2026-0153, time RUSTSEC-2026-0009. Fixing any of them
  requires a toolchain or stack bump; reassess at release time.
  8 unmaintained-crate warnings (instant, paste, …) are transitive
  via pinned GUI/SSH crates.
- GitHub's advisory service is broader than the RustSec mirror that
  `cargo audit` consumes: an inventory on 2026-10-10 counted 21
  advisories (6 high, 12 medium, 3 low) against locked versions —
  16 against `russh` 0.45.0 plus `hickory-resolver`,
  `hickory-proto`, `time`, and `lru`. Every reachable entry needs a
  malicious or compromised SSH peer (or an accepted host-key
  mismatch); the impact class is client-side denial of service, not
  code execution or credential disclosure. Blockers are recorded
  precisely: `russh` 0.61.1 clears the high-severity set on the
  current rustc 1.85 and is blocked only by its breaking API
  migration, while `russh` 0.63.2 and `time` 0.3.47 / `hickory`
  0.26 additionally need rustc ≥ 1.88/1.89. Per-advisory inventory:
  `docs/threat_model.md`; ordered migration: `docs/maintenance.md`.
- Secrets: passwords travel over stdin pipes or zeroized memory,
  never argv, logs, or fixtures (argv invariant is unit-tested).

## Test Coverage

- Line coverage: **65.2%** (`cargo llvm-cov --workspace
  --all-features`, llvm-cov 0.9.1, nightly toolchain for
  instrumentation; e.g. theme 90.6%, whois 73.3%). The earlier 5.6%
  was a broken-tool artifact (cargo-llvm-cov 0.6.21 reported
  identical per-file numbers across different test selections;
  verified non-responsive and replaced).
- Explicit exception (maintainer decision): the >80% line target is
  not met and cannot be met headless — `src/ui` and much of
  `src/app` need a display server, and live-protocol paths need
  disposable servers. Covered well: parsers, crypto, transfers,
  terminal grid, protocol unit logic (per-file 15–86% where
  measured). Path forward: Xvfb GUI harness + disposable-server
  E2E in CI before any release claim.
- Test counts (all-features suite, all passing): 318 lib tests +
  integration targets incl. 2 hermetic SSH loopback tests, a canned
  RFB handshake test, and a busybox-telnet round-trip test.
- Live servers (2026-10-07, disposable docker services on loopback,
  strict host-key learning, zero user-config pollution): SFTP
  list + round-trip (`atmoz/sftp:alpine :2222`) pass; local, remote,
  and dynamic SOCKS5 forwarding (`linuxserver/openssh-server :2223`
  with vendored `docker/sshd_config`) pass; X11 handshake sets
  remote `DISPLAY` (pass); Telnet echo via `busybox telnetd` passes.
- Live amendment (2026-10-11): SSH auth triad added as
  `tests/ssh_auth_live.rs` vs the same disposable OpenSSH :2223 —
  password login + shell echo, throwaway ed25519 key
  (generated/installed/key-login), and spawned-`ssh-agent` login —
  all with strict host-key verification; wired into the e2e workflow
  (`MBXT_SSH_TEST_*`). SFTP and X11 suites re-verified green the same
  day; X11 now verifies strictly too (was `AcceptAll`).
  VNC against a real server passes (2026-10-10, disposable TigerVNC
  Xvnc container `docker/vnc`, `RFB 003.008` handshake + first frame,
  5/5 runs; `BlacklistTimeout=0` keeps nc healthchecks from poisoning
  the listener — a throttled listener was observed and worked around).
  RDP authentication against a real server passes (2026-10-10,
  disposable xrdp+openbox container `docker/xrdp`, `xfreerdp /auth-only`
  exit 0 with test:test; wrong password exits 1).
- `MBXT_*_TEST_*` suites without servers, RDP live-in-CI (server
  exists; the Rust side spawns the external `xfreerdp` client, so CI
  wiring stays manual for now), and key/agent live logins stay
  ignored/skipped by default.
- Platforms exercised: Ubuntu 24.04 x86_64 only. Fedora/Arch/
  Windows targets compile-check in CI; this machine cannot run them.

## Known Limitations

The following are true at the time of writing and must not be edited into
successes without evidence:

- Version is `0.1.0`; no v1.0 release has been cut.
- Single maintainer (bus factor 1); backup seats vacant.
- Private GitHub vulnerability reporting returned 404 during review; enabling
  it (or a verified private contact) is a release blocker.
- CI covers Ubuntu 22.04/24.04, Fedora 40, and Arch; Debian and
  current/previous Fedora need release smoke tests until permanent jobs exist.
- Local full-test linking is constrained on Windows-GNU hosts without
  `dlltool.exe`; CI is authoritative for the full suite.
- No official chat, social, blog, Weblate, funding, or meeting cadence exists.
- RDP is an external-process integration, not a native binding; VNC, serial,
  and some protocol paths require live counterparts to verify.
- Performance numbers are targets until measured per the checklist.
- NEW (Phase 6): GUI cold start, idle memory, FPS, and 10-session memory
  are UNMEASURED (no display server here); coverage tooling is
  non-responsive as documented above; RDP viewer windows are external
  (in-tab X11 embedding needs a custom embed widget and is not done);
  fuzz smokes and coverage need the nightly toolchain (CI parity);
  Nix flake and install-on-distro packaging steps were statically
  validated only.
- NEW (2026-10-11): GUI button click-through is unverified under Xvfb:
  XTEST button events arrive only as slave-device raw events (0 master
  device ButtonPress over 6 measured clicks) while winit 0.30 selects
  XI2 on master devices, so release-to-publish never completes for
  buttons inside scrollables (toolbar buttons, text focus, typing, and
  shortcuts all work live). Session CRUD button paths are unit-tested;
  the click-through needs a real display. Local reproduction note: a
  running Xvfb (e.g. its `/tmp/.X11-unix/X99` socket) breaks the X11
  negative tests (`missing_socket_*`, `without_x_server`,
  `unreachable_display`) — stop Xvfb before `cargo test`.

## Future Work

See [`ROADMAP.md`](../ROADMAP.md) (Now/Next/Later/Won't-Do) and
[`docs/roadmap_details.md`](roadmap_details.md). Only milestone-assigned work
with an owner is committed scope.

## Acknowledgments

Single-maintainer project; no co-maintainers are listed anywhere by
policy (see the honesty rules in `MASTER_PROMPT.md`). Inspiration:
MobaXterm (feature model only — no code or assets shared).
