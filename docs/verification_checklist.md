# Verification Checklist

How to use: check each box only after running the stated command or manual
procedure on the stated environment. Record measured values and failures in
`docs/final_report.md`. Automated gates run in CI; manual GUI and live-server
items require a human and disposable infrastructure. Live tests never touch
production hosts.

## Automated Gates (CI-Authoritative)

- [ ] `cargo fmt --all -- --check` is clean.
- [ ] `cargo check --workspace --all-targets --all-features` succeeds.
- [ ] `cargo test --workspace` (non-live) passes on Ubuntu 22.04/24.04,
      Fedora 40, and Arch runners.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- [ ] `cargo audit` reports no unpatched vulnerabilities.
- [ ] `cargo deny check` passes (licenses, bans, sources).
- [ ] Coverage gate passes with the workspace target of >80% lines; any drop
      has an explicit maintainer decision recorded.
- [ ] `mdbook build docs` succeeds; `cargo xtask i18n` validates Fluent strings.
- [ ] `git diff --check` is clean; `actionlint` is clean.
- [ ] Fuzz smoke runs pass:
      `cargo fuzz run vte_parser -- -runs=1000` (and siblings per `fuzz.yml`).

## Functional Verification (Manual Unless Noted)

Sessions and terminal:

- [ ] Application launches and shows the main window (sidebar, content area,
      status bar).
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
- [ ] Local, remote, and dynamic (SOCKS5) forwarding carry traffic.
- [ ] X11 forwarding launches a GUI app from a disposable server.

Multi-protocol (each needs its live counterpart or emulator):

- [ ] RDP session connects and displays via the managed external process.
- [ ] VNC session connects and displays (`vnc_integration` live-gated).
- [ ] Telnet connection works against a disposable server.
- [ ] Serial connection opens with a virtual/loopback device.

Productivity:

- [ ] Multi-exec broadcasts input to all selected sessions.
- [ ] Macro recorder captures input; playback reproduces actions.
- [ ] Network tools return sane results: ping, traceroute, DNS, whois, port
      scan (`network_tools` tests + manual spot-check).
- [ ] Built-in themes switch; a custom theme file loads.
- [ ] Session import works per the published importer matrix; unknown fields
      are reported, never guessed; nothing is overwritten silently.

## Performance Verification (Measure, Do Not Assert)

- [ ] Cold start <2 s (record hardware, build profile, median of 5 runs).
- [ ] Idle memory <100 MB (record how measured).
- [ ] Terminal rendering at 60 FPS; scrolling never drops below 30 FPS.
- [ ] SSH connect <3 s on a local network.
- [ ] File transfer >100 MB/s on a local network.
- [ ] 10 concurrent sessions stay under 500 MB total.

## Security Verification

- [ ] No plaintext secrets in storage; sessions encrypted with AES-256-GCM,
      keys derived with Argon2id.
- [ ] SSH host keys are verified; mismatches block with a clear warning.
- [ ] No secrets, host data, or terminal content in logs (grep release logs).
- [ ] Config directory is `0700`, config files `0600` on a fresh profile.
- [ ] Crash dumps/minidumps contain no sensitive data (inspect a sample).
- [ ] `cargo audit` and `cargo deny` pass at release time.

## Quality Verification

- [ ] Tests pass on Ubuntu 22.04, Fedora 40, and Arch; GUI spot-checks cover
      X11 and Wayland.
- [ ] Coverage >80% lines or an explicit accepted exception is recorded.
- [ ] Zero Clippy warnings; docs build without errors or warnings.
- [ ] Man page (`docs/remote-app.1`) and desktop file install and validate.

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
