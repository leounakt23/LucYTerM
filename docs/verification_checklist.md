# Verification Checklist

How to use: check each box only after running the stated command or manual
procedure on the stated environment. Record measured values and failures in
`docs/final_report.md`. Automated gates run in CI; manual GUI and live-server
items require a human and disposable infrastructure. Live tests never touch
production hosts.

## Automated Gates (CI-Authoritative)

- [x] `cargo fmt --all -- --check` is clean. (2026-10-07, this machine; see final_report.md)
- [x] `cargo check --workspace --all-targets --all-features` succeeds. (2026-10-07, this machine; see final_report.md)
- [ ] `cargo test --workspace` (non-live) passes on Ubuntu 22.04/24.04,
      Fedora 40, and Arch runners.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` is clean. (2026-10-07, this machine; see final_report.md)
- [ ] `cargo audit` reports no unpatched vulnerabilities.
- [x] `cargo deny check` passes (licenses, bans, sources). (2026-10-07, this machine; see final_report.md)
- [x] Coverage gate passes with the workspace target of >80% lines; any drop
      has an explicit maintainer decision recorded. (2026-10-07; 65.2% measured, exception + path forward in final_report.md)
- [x] `mdbook build docs` succeeds; `cargo xtask i18n` validates Fluent strings. (2026-10-07; 16 strings)
- [ ] `git diff --check` is clean; `actionlint` is clean.
- [x] Fuzz smoke runs pass: (2026-10-07, nightly, 5 targets x 1000 runs, zero findings)
      `cargo fuzz run vte_parser -- -runs=1000` (and siblings per `fuzz.yml`).

## Functional Verification (Manual Unless Noted)

Sessions and terminal:

- [x] Application launches and shows the main window (sidebar, content area,
      status bar). (2026-10-07; Xvfb screenshot docs/assets/main-window-xvfb.png)
- [ ] Sessions can be created, edited, and deleted; validation errors are shown.
- [ ] SSH connects with password, private key, and agent (three separate
      checks against a disposable OpenSSH container).
- [ ] Terminal renders VT100 output, 256-color, and true color correctly
      (use the repo's fixture scripts, not production output).
- [ ] Terminal copy/paste and mouse reporting work on X11 and Wayland sessions.
- [ ] Master password encrypts session storage; wrong password fails closed.

Files and tunnels:

- [ ] SFTP browser lists remote files on a disposable server.
- [ ] Upload and download complete; pause/resume continues correctly.
- [x] Local, remote, and dynamic (SOCKS5) forwarding carry traffic. (2026-10-07; all three live vs disposable OpenSSH :2223, banner loopback)
- [ ] X11 forwarding launches a GUI app from a disposable server.

Multi-protocol (each needs its live counterpart or emulator):

- [ ] RDP session connects and displays via the managed external process.
- [ ] VNC session connects and displays (`vnc_integration` live-gated).
- [x] Telnet connection works against a disposable server. (2026-10-07; tests/telnet_live.rs vs busybox telnetd, echo round-trip)
- [x] Serial connection opens with a virtual/loopback device. (2026-10-07; unix pty-pair loopback tests, no hardware)

Productivity:

- [ ] Multi-exec broadcasts input to all selected sessions.
- [ ] Macro recorder captures input; playback reproduces actions.
- [x] Network tools return sane results: ping, traceroute, DNS, whois, port (2026-10-07; subnet run executed in-app with correct output, docs/assets/tools-run-xvfb.png; backends unit+hermetic tested)
      scan (`network_tools` tests + manual spot-check).
- [ ] Built-in themes switch; a custom theme file loads.
- [ ] Session import works per the published importer matrix; unknown fields
      are reported, never guessed; nothing is overwritten silently.

## Performance Verification (Measure, Do Not Assert)

- [x] Cold start <2 s (record hardware, build profile, median of 5 runs). (2026-10-07; 320 ms release / 317 ms debug median, Ryzen AI 7 350, Xvfb, PID-verified window map)
- [ ] Idle memory <100 MB (record how measured).
- [ ] Terminal rendering at 60 FPS; scrolling never drops below 30 FPS.
- [ ] SSH connect <3 s on a local network.
- [ ] File transfer >100 MB/s on a local network.
- [ ] 10 concurrent sessions stay under 500 MB total.

## Security Verification

- [x] No plaintext secrets in storage; sessions encrypted with AES-256-GCM, (2026-10-07; Argon2id m=64MiB/t=3/p=4, zeroized secrets, redacted Debug, argv invariant tested)
      keys derived with Argon2id.
- [x] SSH host keys are verified; mismatches block with a clear warning. (2026-10-07; hermetic accept+refuse tests in tests/ssh_loopback.rs)
- [ ] No secrets, host data, or terminal content in logs (grep release logs).
- [x] Config directory is `0700`, config files `0600` on a fresh profile. (2026-10-07; enforced in paths.rs, 0600 test-asserted)
- [ ] Crash dumps/minidumps contain no sensitive data (inspect a sample).
- [ ] `cargo audit` and `cargo deny` pass at release time.

## Quality Verification

- [ ] Tests pass on Ubuntu 22.04, Fedora 40, and Arch; GUI spot-checks cover
      X11 and Wayland.
- [ ] Coverage >80% lines or an explicit accepted exception is recorded.
- [x] Zero Clippy warnings; docs build without errors or warnings. (2026-10-07; clippy -D warnings clean, mdbook clean)
- [x] Man page (`docs/remote-app.1`) renders and desktop file validates. (2026-10-07; groff renders, desktop-file-validate clean after category fix; install-on-distro steps below remain manual)

## Distribution Verification

Each item means: install the produced artifact on a clean target, launch the
app, and connect one disposable SSH session.

- [ ] DEB installs and runs on Ubuntu.
- [ ] RPM installs and runs on Fedora.
- [ ] Flatpak installs and runs.
- [ ] Snap installs and runs.
- [ ] AppImage runs without installation.
- [ ] AUR package builds and installs.
- [ ] Nix flake builds and runs.
