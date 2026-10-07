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

## Performance Metrics

Machine: AMD Ryzen AI 7 350, 30 GB RAM, Ubuntu 24.04. `bench`
profile is release-optimized; `dev` is unoptimized + debuginfo.

| Metric (target) | Measured | Method |
|---|---|---|
| Cold start <2 s | UNMEASURED | No display server in this environment; GUI cannot launch headless |
| Idle memory <100 MB | UNMEASURED | Same reason as above |
| Terminal 60 FPS | UNMEASURED | Same reason as above |
| SSH connect <3 s (LAN) | PASS (loopback) | Hermetic `tests/ssh_loopback.rs`: full password handshake + shell echo in 0.09 s for both tests (asserts <3 s); no LAN server available |
| Transfer >100 MB/s (LAN) | 12.7 GB/s (loopback) | `transfer_loopback` criterion bench, 10×1 s sender blasts, median ≈101.7 Gbit/s, byte-exact vs drain; loopback only, not LAN |
| 10 sessions <500 MB | UNMEASURED | Needs GUI + servers |
| CLI startup (informational, no target) | <10 ms, ~9 MB RSS | `remote-app-headless --version`, median of 5, dev profile; NOT the GUI cold start |

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
- Secrets: passwords travel over stdin pipes or zeroized memory,
  never argv, logs, or fixtures (argv invariant is unit-tested).

## Test Coverage

- Line coverage: **5.6%** (`cargo llvm-cov --workspace
  --all-features`, nightly toolchain for instrumentation; stable
  1.85 cannot instrument the `tiny-xlib` dependency).
  Tooling caveat (verified): the report is non-responsive — theme
  and WHOIS unit tests demonstrably ran, yet both files report
  0.00%, and per-file values are identical across different test
  selections. The true value is higher but unverified.
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
  VNC against a real server and RDP against a real server stay
  unverified (no server image vetted yet).
- `MBXT_*_TEST_*` suites without servers, VNC/RDP live, and
  key/agent live logins stay ignored/skipped by default.
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

## Future Work

See [`ROADMAP.md`](../ROADMAP.md) (Now/Next/Later/Won't-Do) and
[`docs/roadmap_details.md`](roadmap_details.md). Only milestone-assigned work
with an owner is committed scope.

## Acknowledgments

Single-maintainer project; no co-maintainers are listed anywhere by
policy (see the honesty rules in `MASTER_PROMPT.md`). Inspiration:
MobaXterm (feature model only — no code or assets shared).
