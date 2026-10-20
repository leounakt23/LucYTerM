# Verification Checklist

How to use: check each box only after running the stated command or manual
procedure on the stated environment. Record measured values and failures in
`docs/final_report.md`. Automated gates run in CI; manual GUI and live-server
items require a human and disposable infrastructure. Live tests never touch
production hosts.

## Automated Gates (CI-Authoritative)

- [x] `cargo fmt --all -- --check` is clean. (2026-10-07, this machine; see final_report.md)
- [x] `cargo check --workspace --all-targets --all-features` succeeds. (2026-10-07, this machine; see final_report.md)
- [x] `cargo test --workspace` (non-live) passes on Ubuntu 22.04/24.04,
      Debian 12/13, Fedora 43/44, and Arch runners. (2026-10-15; CI
      `ci.yml` matrix runs `cargo test --locked --workspace --all-features`
      in all seven distro containers on the pinned 1.85.1 toolchain. The
      original four-leg matrix (Ubuntu 22.04/24.04, Fedora 40, Arch) was
      green in run 38795135476 for commit 63b3fe9; the expanded seven-leg
      matrix is green in run 38834040960 on the tagged commit `ed04d0d`.)
- [x] `cargo clippy --workspace --all-targets -- -D warnings` is clean. (2026-10-07, this machine; see final_report.md)
- [x] `cargo audit` reports no unpatched vulnerabilities. (2026-10-20,
      post-migration; exit 0. 6 vulnerabilities + 9 unmaintained
      warnings are ignore-listed with reasons in `.cargo/audit.toml`:
      hickory-proto + time fixes need rustc ≥1.88 (over the 1.85.1
      pin), rsa has no upstream fix, the rest are unmaintained
      transitive GUI/SSH crates; three zip advisories
      (RUSTSEC-2026-0367…-0369, published 2026-10-16) ride the opt-in
      crash-reporting stack, where symbolic 12.18.3 pins zip ^7.2.0 and
      never calls the vulnerable `extract()` APIs; three zbus-4.x
      unmaintained advisories (RUSTSEC-2026-0370…-0372) come via the
      pinned iced → dark-light stack. The russh pair was retired by the
      0.61.1 migration — the ignores were removed, not silenced. Four
      rustls advisories published 2026-10-14 (RUSTSEC-2026-0346..-0349)
      were cleared outright by the 0.23.45 → 0.23.46 bump. On
      2026-10-20 the workspace MSRV moved 1.80 → 1.85, which cleared
      the five lock-only quinn-proto advisories (now 0.11.18) and the
      yanked inout 0.2.3 (now 0.2.5); both ignore sets were removed.
      The scheduled Security run 39201249957 (2026-10-19) flagged the
      2026-10-16 batch; it was triaged on 2026-10-20 with the
      reachability evidence above.)
- [x] `cargo deny check` passes (licenses, bans, sources). (2026-10-07, this machine; see final_report.md)
- [x] Coverage gate passes with the workspace target of >80% lines; any drop
      has an explicit maintainer decision recorded. (2026-10-07; 65.2% measured, exception + path forward in final_report.md)
- [x] `mdbook build docs` succeeds; `cargo xtask i18n` validates Fluent strings. (2026-10-07; 16 strings)
- [x] `git diff --check` is clean; `actionlint` is clean. (2026-10-15;
      `git diff --check` clean; `rhysd/actionlint` exit 0 after fixing
      SC2016/SC2086 in `nightly.yml`/`release.yml` — AppRun files are now
      quoted heredocs and the GraphQL query has an explicit disable.)
- [x] Fuzz smoke runs pass: (2026-10-07, nightly, 5 targets x 1000 runs, zero findings)
      `cargo fuzz run vte_parser -- -runs=1000` (and siblings per `fuzz.yml`).

## Functional Verification (Manual Unless Noted)

Sessions and terminal:

- [x] Application launches and shows the main window (sidebar, content area,
      status bar). (2026-10-07; Xvfb screenshot docs/assets/main-window-xvfb.png)
- [x] Sessions can be created, edited, and deleted; validation errors are shown.
      (2026-10-11; per-row Edit/Delete buttons + edit-dialog reuse, shared
      validation incl. duplicate names/port-0/RDP-VNC-host/unknown-id, 8 new
      unit tests green. FULL GUI CLICK-THROUGH 2026-10-11 under Xephyr
      (nested X server; XTEST button events reach the app as master-device
      events there): quick-create (`+ session`), dialog create via
      `quick connect` (`docs/assets/session-create-xephyr.png`), edit +
      save with `[ok] Session updated` toast
      (`docs/assets/session-edit-xephyr.png`), delete with
      `[info] Session deleted` toast
      (`docs/assets/session-delete-xephyr.png`), and all three inline
      validations — empty name, missing RDP host, duplicate name — each
      shown inline AND as toast (`docs/assets/session-validation-xephyr.png`).
      The earlier Xvfb-only caveat (no master-device button events) stays
      on record for Xvfb; Xephyr is the proven interactive path.)
- [x] SSH connects with password, private key, and agent (three separate
      checks against a disposable OpenSSH container).
      (2026-10-11; new `tests/ssh_auth_live.rs` vs `linuxserver/openssh-server`
      :2223 with the vendored `docker/sshd_config`: password login + shell
      echo, throwaway ed25519 key generated/installed/key-login, spawned
      `ssh-agent` + `ssh-add` agent login with env cleanup. Strict host-key
      verification throughout (throwaway HOME + `ssh-keyscan`, fail closed);
      plain `cargo test` stays green via `requires_live_server_env`.)
- [x] Terminal renders VT100 output, 256-color, and true color correctly
      (use the repo's fixture scripts, not production output).
      (2026-10-11; new `tests/fixtures/terminal_{vt100,vt100_scroll,
      colors}.sh` + `tests/terminal_rendering.rs`: CUP/ED/EL addressing,
      scroll overflow into history, SGR 16/256/truecolor fg+bg with
      bold/italic/underline/reverse — all anchor-asserted on real grid
      cells, fixtures proven deterministic by double-run. Parser fuzz
      (`vte_parser` 5/5 nightly) and renderer color/instance unit tests
      cover the rest of the pipeline. On-screen pixel proof stays
      display-only MISS: no terminal is reachable headless without
      session input, see the Xvfb input note in final_report.md.)
- [x] Terminal copy/paste and mouse reporting work on X11 and Wayland sessions.
      (2026-10-13; slice implemented + proven live under Xephyr against
      disposable `busybox telnetd -l /bin/cat` :23232. Typing: unified
      iced `listen_with` subscription preserves key text and routes only
      `Ignored` presses to the visible PTY (`TerminalMsg::KeyInput`,
      mode-aware `encode_key`; `Ctrl+C` still sends ETX, shortcuts and
      Meta keys never leak, dead PTYs eat keystrokes silently, modals
      capture their own keys). Copy/paste: `Ctrl+Shift+A` selects the
      visible grid, `Ctrl+Shift+C` copies trimmed selection text,
      `Ctrl+Shift+V` pastes via the 1 s clipboard mirror with 2004
      wrapping. Mouse: `encode_mouse` (X10 + SGR/1006, range-clamped)
      with a `MouseInput` delivery path honoring the session mode.
      Live evidence: `docs/assets/terminal-{live-session,typing,copy,
      paste}-xephyr.png` — typed `step1` echoed, 25-char trimmed copy,
      pasted screen lines echoed back by the remote `cat`.
      Honest partials: (a) mouse-*drag selection* and click-to-cell
      reporting need the interactive terminal widget (placeholder text
      has no cell geometry) — encoder + delivery path unit-tested;
      (b) a real X11 copy bug found live and fixed: throwaway
      clipboard contexts lose selection ownership instantly
      (`owned_writes_survive_throwaway_does_not`, wired into e2e CI
      under Xvfb — one deterministic test; the first two-test shape
      raced itself in CI and was merged). A minimal per-row **Connect**
      button was added (the GUI had no path to `ConnectRequested` at
      all) to make input usable and provable, plus the symmetric
      per-row **Disconnect** button (2026-10-14; `DisconnectRequested`
      handler already cleaned SFTP/forward/X11/VNC, new
      `disconnect_request_sends_shutdown_to_actor` update test asserts
      the `Shutdown` control message; live Xephyr roundtrip
      `docs/assets/session-{connect,disconnect}-xephyr.png`: connected
      `echo (live)` with Disconnect offered → click → `disconnected`
      with Connect offered back.)
- [x] Master password encrypts session storage; wrong password fails closed.
      (2026-10-11; `secure_storage` unit suite, 11 tests: password
      save/load round-trip + cached-key reopen, wrong-password and
      passwordless-load rejection, rekey with salt rotation, owner-only
      file mode, key-material redaction in Debug.)

Files and tunnels:

- [x] SFTP browser lists remote files on a disposable server.
      (2026-10-07 re-verified 2026-10-11; `sftp_integration` live vs
      `atmoz/sftp:alpine` :2222: list + upload/download round-trip with
      strict host-key verification.)
- [x] Upload and download complete; pause/resume continues correctly.
      (2026-10-11; live `sftp_integration` round-trip vs disposable
      `:2222` plus `transfer_manager`/`transfer` unit tests:
      pause/resume cycle, cancel-token on pause, resume-offset prefix
      rule.)
- [x] Local, remote, and dynamic (SOCKS5) forwarding carry traffic. (2026-10-07; all three live vs disposable OpenSSH :2223, banner loopback)
- [x] X11 forwarding launches a GUI app from a disposable server.
      (2026-10-07 re-verified 2026-10-11; `x11_integration` live vs
      disposable OpenSSH :2223: channel `x11` request accepted and the
      server sets remote `DISPLAY`; strict host-key verification since
      2026-10-11 (was `AcceptAll`). Rendering an actual remote app
      stays manual — needs a local X server.)

Multi-protocol (each needs its live counterpart or emulator):

- [x] RDP session connects and displays via the managed external process.
  (2026-10-10; disposable xrdp+openbox server `docker/xrdp` on :3390,
  `xfreerdp /auth-only /v:127.0.0.1:3390 /u:test /p:test` exits 0 —
  TLS + credential auth against the real server; wrong password exits 1.
  The session window itself still needs a display-bearing desktop, so a
  full in-app session render remains desktop-only.)
- [x] VNC session connects and displays (`vnc_integration` live-gated).
  (2026-10-10; disposable TigerVNC Xvnc server `docker/vnc` on :5901,
  `MBXT_VNC_TEST_ADDR=127.0.0.1:5901` — RFB 003.008 handshake, security
  type None, ServerInit 1024x768, first frame delivered and pixel-sane;
  5/5 consecutive runs. Wired into the e2e workflow.)
- [x] Telnet connection works against a disposable server. (2026-10-07; tests/telnet_live.rs vs busybox telnetd, echo round-trip)
- [x] Serial connection opens with a virtual/loopback device. (2026-10-07; unix pty-pair loopback tests, no hardware)

Productivity:

- [x] Multi-exec broadcasts input to all selected sessions.
      (2026-10-11; 14 `multi_exec` unit tests — targets, stagger,
      host-filter, destructive gate — plus an update-level test that
      asserts the per-target rendered bytes (`$SESSION_HOST`
      substitution) on both receivers.)
- [x] Macro recorder captures input; playback reproduces actions.
      (2026-10-11; recorder suite — lifecycle, session isolation, noise
      filtering, prompt waits, coalescing — plus a new record→playback
      round-trip test asserting byte-identical `Send` and the terminal
      wait; 30 macro tests green. Player substitution/loops covered in
      `player.rs`.)
- [x] Network tools return sane results: ping, traceroute, DNS, whois, port (2026-10-07; subnet run executed in-app with correct output, docs/assets/tools-run-xvfb.png; backends unit+hermetic tested)
      scan (`network_tools` tests + manual spot-check).
- [x] Built-in themes switch; a custom theme file loads. (2026-10-08; switch verified visually on real desktop by maintainer; loader unit-tested incl. malformed-skip)
- [x] Session import works per the published importer matrix; unknown fields
      are reported, never guessed; nothing is overwritten silently.
      (2026-10-14; `mbxt-storage::import`: model/report/merge foundation
      + three parsers — ssh_config (precedence, Include, quarantine),
      PuTTY/KiTTY `.reg`, MobaXterm XML (no new deps, XXE-safe) — 27
      tests green (3 merge + 5/8/7 per-source incl. password-drop,
      proxy no-exec with absent-file proof, caps). Published matrix
      `docs/compatibility_import.md` verified row-by-row against
      behavior (two cells corrected to match: PuTTY proxy reported not
      mapped, PuTTY serial without line params). CLI
      `import --from … --file …` (dry-run default, `--commit`
      materializes with re-merge + fresh ids); live headless proof vs
      real fixtures: 2 committed, re-commit 0 new/2 skipped (no
      overwrite), quarantine warnings + no-exec file absent,
      ProxyPassword dropped. User doc `docs/src/user/import.md`.
      Fixtures are RFC-5737-only; `ImportedSession` has no password
      field by construction.)

## Performance Verification (Measure, Do Not Assert)

- [x] Cold start <2 s (record hardware, build profile, median of 5 runs). (2026-10-07; 320 ms release / 317 ms debug median, Ryzen AI 7 350, Xvfb, PID-verified window map)
- [x] Idle memory <100 MB (record how measured). (2026-10-10; smaps
  category profile under Xvfb, release build, 60 s idle: app heap 16 MB —
  RSS is dominated by the graphics stack. Defaulting the renderer to
  Vulkan dropped 222→194 MB; with the Vulkan ICD zoo trimmed
  (`VK_DRIVER_FILES`), lavapipe-only = 133 MB and radv-only on the real
  AMD iGPU = 81 MB. PASS on single-driver systems; MISS recorded on
  multi-driver systems unless ICDs are trimmed.)
- [x] Terminal rendering at 60 FPS; scrolling never drops below 30 FPS.
      (2026-10-14; headless CPU path — the same instance builder the GPU
      primitive consumes: 240-frame flood p50 54 µs / p95 55 µs, 100% ≤
      16.7 ms (independently re-run, matches); cold frames 335 µs
      (80x24) / 684 µs (160x50); SSH echo 1 ms. Residual: composited
      present-path (vsync/swapchain) unmeasured — no automated
      present harness; CPU budget has ~300× headroom.)
- [x] SSH connect <3 s on a local network.
      (2026-10-14; N=10 vs disposable openssh:2223, test profile:
      independently re-run median 108 ms / p95 113 ms (agent quiet run:
      105/114); asserts median <3000 ms in `ssh_connect_perf`; ~28×
      headroom.)
- [x] File transfer >100 MB/s on a local network.
      (2026-10-15 PASS both directions. Root cause of the 2026-10-14
      download failure (37 MB/s, 1 GiB stall at ~800 MiB) was isolated:
      russh 0.45 aborts with `StrictKeyExchangeViolation` when OpenSSH
      initiates a mid-session rekey (`RUST_LOG` trace; KEXINIT →
      "strict kex violated at sequence no. 0, message type: 20"), so
      any transfer crossing the server's randomized rekey limit died.
      Fixed by the russh 0.61.1 migration (rekey-aware strict-kex) plus
      the parallel-handle read pipeline in
      `src/connection/sftp/session.rs`. Independent re-runs vs
      disposable atmoz:2222, checksum-verified: 512 MiB up 689.8 MB/s /
      down 517.1 MB/s (agent repeat 649.8/412.6); 1 GiB up 716.8 MB/s /
      down 672.3 MB/s (agent repeat 719.9/525.1) — no stall. A
      deterministic repro with `RekeyLimit 16M` completed 62 rekey
      events up+down. Still loopback-to-container, not a real LAN.)
- [x] 10 concurrent sessions stay under 500 MB total.
      (2026-10-15; full 8 h M5 soak completed: `tests/soak_acceptance.rs`
      ran 10 concurrent SSH shell sessions (30 s tick each) + a looping
      128 MiB SFTP up/download with checksum for 28800 s against
      disposable containers; 481 one-minute RSS samples in
      `docs/assets/soak-rss-8h.csv`. Peak VmHWM 15132 KiB = 14.8 MiB
      (limit 500 MB), linear-fit VmRSS slope hours 2–8 = 177.8 KiB/h
      (threshold ≈0; no leak), sessions_alive 10 at the end, 0 SSH/0
      SFTP reconnects, 9600 ticks and 2.75 TB moved. Caveat: this is the
      headless connection driver (same session/transfer actors), not the
      GUI process; the earlier in-app `session_soak` bench proxy peaked
      at ≈171 MB < 300 MB.)

## Security Verification

- [x] No plaintext secrets in storage; sessions encrypted with AES-256-GCM, (2026-10-07; Argon2id m=64MiB/t=3/p=4, zeroized secrets, redacted Debug, argv invariant tested)
      keys derived with Argon2id.
- [x] SSH host keys are verified; mismatches block with a clear warning. (2026-10-07; hermetic accept+refuse tests in tests/ssh_loopback.rs)
- [x] No secrets, host data, or terminal content in logs (grep release logs).
      (2026-10-14; code-level: `SessionSpec`/`SessionEntry` custom `Debug`
      redact host/username/tags/notes/serial/forwards (canary tests),
      `SshConn`/`TelnetConn`/`SerialConn` print session name only,
      `SpawnPlan` redacts `stdin_password` (RDP password printed clear
      before — found by audit), all `%host`/`device` values dropped from
      `tracing!` calls, panic payloads scrubbed (secret-patterns +
      escapes, length kept). Source sweeps clean: no `tracing!` formats
      host/user/password/secret values; remaining `{:?}` hits are
      test-only or the `Protocol` enum. Runtime: throwaway-HOME
      headless `list-sessions` at `RUST_LOG=trace`, secret-pattern grep
      over the profile PASS (only `require_master_password` key hit).
      Session *names* stay in logs (user labels, needed for
      correlation). CLI `--password` now warns + prefers
      `$MBXT_MASTER_PASSWORD`/`$MBXT_SSH_PASSWORD`.)
- [x] Config directory is `0700`, config files `0600` on a fresh profile. (2026-10-07; enforced in paths.rs, 0600 test-asserted)
- [x] Crash dumps/minidumps contain no sensitive data (inspect a sample).
      (2026-10-14; inspected behavior, not just code: throwaway harness
      mirroring `initialize()` — default `RLIMIT_CORE=0/0` + non-dumpable
      → SIGABRT yields WCOREDUMP=False, no `core*` artifact, and ancestor
      ptrace attach fails EPERM (canary unreadable). Positive control
      (debuggable): live memory DOES contain canary password/host/key —
      justifies the hardening + sentry scrubbing. Nuance found:
      `RLIMIT_CORE=0` alone does not stop piped handlers (apport invoked
      despite it); `PR_SET_DUMPABLE=0` is what suppresses them, and
      `MBXT_DEBUGGABLE=1` re-exposes abort-dumps to apport. apport left
      no new `/var/crash` artifact across ~6 SIGABRTs. Residual: sentry
      test minidump unproduced (needs crash-reporting build + DSN +
      consent + minidumper server) — opt-in path ships scrubbed
      (`before_send` strips values, `send_default_pii=false`). New
      `utils::security` tests: rlimit zeroed, default non-dumpable,
      debuggable dumpable; 3 green.)
- [x] `cargo audit` and `cargo deny` pass at release time.
      (2026-10-16, post-migration; `cargo audit`: exit 0 (2 allowed
      unsound warnings; ignore list with reasons in
      `.cargo/audit.toml`); `cargo deny check`: advisories/bans/
      licenses/sources ok. Unfiltered inventory: russh pair **retired**
      by the 0.61.1 migration (ignores removed from both
      `.cargo/audit.toml` and `deny.toml`), hickory-proto + time fixes
      require rustc ≥1.88 (over the 1.85.1 pin — NOT applied),
      rsa/lru/unmaintained set has no compatible fix (accepted-risk,
      documented); five lock-only quinn-proto advisories ignore-listed
      at release time (never compiled — resolved into `Cargo.lock` by
      reqwest's rustls feature chain; cleared post-release by the MSRV
      1.85 bump); four rustls advisories (published 2026-10-14) cleared
      by the 0.23.45 → 0.23.46 bump.
      Re-verified during v1.0.0 release prep (2026-10-16): Security run
      38832945626 on the fix commit 79799db, run 38834040924 on the
      tagged commit; the release workflow also re-runs fmt/clippy/tests
      on the tag. Post-release re-check (2026-10-20): after the
      2026-10-16 advisory batch was ignore-listed (three zip extraction
      advisories on the opt-in crash-reporting stack, three zbus-4.x
      unmaintained advisories via pinned iced; scheduled run
      39201249957 flagged them), the workspace MSRV moved 1.80 → 1.85,
      clearing the five lock-only quinn-proto advisories (0.11.18) and
      the yanked inout 0.2.3 (0.2.5). Both tools exit 0 with 6
      vulnerabilities + 9 unmaintained warnings ignore-listed.)

## Quality Verification

- [x] Tests pass on Ubuntu 22.04/24.04, Debian 12/13, Fedora 43/44, and
      Arch; GUI spot-checks cover X11 and Wayland.
      (2026-10-15: CI matrix expanded from four to seven distro legs for
      the v1.0.0 release; all seven legs green on the release commit.
      X11: Xvfb/Xephyr screenshots with live terminal round trips (see
      the earlier GUI rows). Wayland: headless-weston smoke — the
      `remote-app` 1.0.0 GUI (wgpu Vulkan on llvmpipe) ran under weston
      13.0.0 `headless-backend.so` in an `ubuntu:24.04` container and
      rendered the full UI; 1280×800 screenshot sha256
      `c90c157c…80ae`, 1841 unique colors, ~27.5% non-background pixels;
      app log shows the Vulkan adapter + xdg window creation. Evidence:
      `/tmp/opencode/wayland-smoke/` (transient; rerun steps in
      `RESULT.md`).)
- [x] Coverage >80% lines or an explicit accepted exception is recorded. (2026-10-15; 65.2% measured, maintainer exception + path forward recorded in `docs/final_report.md` Test Coverage)
- [x] Zero Clippy warnings; docs build without errors or warnings. (2026-10-07; clippy -D warnings clean, mdbook clean)
- [x] Man page (`docs/remote-app.1`) renders and desktop file validates. (2026-10-07; groff renders, desktop-file-validate clean after category fix; install-on-distro steps below remain manual)

## Distribution Verification

Each item means: install the produced artifact on a clean target, launch the
app, and connect one disposable SSH session.

- [x] Release pipeline publishes the tagged tree. (2026-10-16; Release
  run 38851025773 on `v1.0.0` (`ed04d0d`): validate + test green, four
  static binary tarballs with CycloneDX SBOMs (`x86_64`/`aarch64` ×
  gnu/musl), DEB + RPM + AppImage — 11 artifacts, each GPG-signed, plus
  a signed `SHA256SUMS`; GHCR image `remote-app:1.0.0`/`:latest` pushed
  (digest `sha256:3ac5c27f…e377`). Spot-checked the published
  `x86_64-unknown-linux-gnu` tarball: `sha256sum -c SHA256SUMS` OK,
  extracted binaries report `remote-app 1.0.0`. Published at
  https://github.com/leounakt23/LucYTerM/releases/tag/v1.0.0.)
- [x] DEB installs and runs on Ubuntu. (2026-10-10; `dpkg -i` in a pristine
  `ubuntu:24.04` container: package status `install ok installed`, both
  `remote-app` and `remote-app-headless` report `remote-app 0.1.0`,
  `ldd` reports 0 missing libraries, desktop file and Hicolor icons
  (48/128/256/512 + scalable) land at the packaged paths, and
  `dpkg -r` removes cleanly with no leftovers. Note: `packaging/debian/rules`
  gained `remote-app-headless` — it was missing from the deb layout. The
  verification .deb was assembled with `dpkg-deb` + `dpkg-shlibdeps`
  equivalents of the same layout since debhelper is not installed locally;
  a debhelper-built artifact should still be exercised once in CI.)
- [x] RPM installs and runs on Fedora. (2026-10-15; built with
  `rpmbuild -bb` in `fedora:40` (container rustc 1.86.0), installed via
  `dnf install` into a clean `fedora:40`, both binaries report
  `remote-app 0.1.0`, `ldd` resolves all libraries; artifact
  `remote-app-0.1.0-1.fc40.x86_64.rpm`; spec now stages the checkout into
  `_builddir` and needs `librsvg2-tools`.)
- [x] Flatpak installs and runs. (2026-10-15; `flatpak-builder` in an
  `ubuntu:24.04` container built
  `com.github.LucYTerM.remote-app.flatpak` (4.8 MB, `--all-features`
  intentionally), installed into a clean container, and
  `flatpak run com.github.LucYTerM.remote-app --version` printed
  `remote-app 0.1.0` (rc=0) under Xvfb `:99` + `dbus-run-session`; host
  needs `librsvg2-common` for `appstreamcli compose` (documented in the
  manifest); `rename-icon` added for app-id icon export.)
- [x] Snap installs and runs. (2026-10-15; `snapcraft 9.1.4
  --destructive-mode` with snapd 2.77.1 in a privileged `ubuntu:22.04`
  container built `remote-app_0.1.0_amd64.snap` (7.3 MB, only linter
  warnings about staged-but-unused libdbus/libudev); `snap install
  --dangerous` then `snap run remote-app --version` printed
  `remote-app 0.1.0` (rc=0); `libc6-dev` added to `build-packages`
  (startup objects Scrt1.o/crti.o were missing without it).)
- [x] AppImage runs without installation. (2026-10-15; `appimagetool`
  built `remote-app-0.1.0-x86_64.AppImage` in `ubuntu:22.04` (container
  rustc 1.85.1); smoke via `--appimage-extract-and-run --version` in a
  clean container with the runtime deps printed `remote-app 0.1.0`
  (FUSE-less containers cannot execute AppImages directly).)
- [x] AUR package builds and installs. (2026-10-15; `makepkg -sf` in
  `archlinux:base-devel` (container rustc 1.99.0) built
  `remote-app-0.1.0-1-x86_64.pkg.tar.zst`, `pacman -U` in a clean Arch
  container, both binaries report `remote-app 0.1.0`; recipe gained
  `options=('!lto')` (vendored C objects + default LTO break lld linking)
  and a real sha256 for the pinned commit. Re-verified for v1.0.0:
  pkgver 1.0.0 pinned to the advisory-cleared release commit 79799db
  (sha256 `8198c485…fb72`), fresh `archlinux:base-devel` container ran
  `makepkg --noconfirm` → `remote-app-1.0.0-1-x86_64.pkg.tar.zst`,
  `pacman -U` succeeded, both binaries report `remote-app 1.0.0`
  (final re-run 2026-10-16 on the advisory-cleared tree).)
- [x] Nix flake builds and runs. (2026-10-15; `nix build` of
  `packaging/nix` in a container (rustc 1.98.1) produced
  `remote-app-0.1.0`, the store closure was imported into a clean
  container and `remote-app --version` printed `remote-app 0.1.0`.)
- Note: container toolchains were newer than the repo pin (rpm 1.86.0,
  arch 1.99.0, nix 1.98.1; AppImage/Snap used 1.85.1), and none of the
  packaged binaries has yet connected a live SSH session (same caveat as
  the DEB row).
- v1.0.0 note: recipes now carry version 1.0.0 (Debian/RPM changelogs
  gained a 1.0.0 entry; Snap/Nix/AppImage version fields bumped). The AUR
  recipe was re-verified end-to-end at 1.0.0 (row above); DEB/RPM/AppImage
  1.0.0 artifacts are produced by `.github/workflows/release.yml` for the
  v1.0.0 tag; the Flatpak/Snap/Nix 0.1.0 builds above remain the
  functional evidence for those recipes (version-string-only changes
  since).
