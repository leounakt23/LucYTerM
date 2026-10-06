# Remote App

Remote App is a native Rust desktop client for secure remote computing. It
combines SSH/SFTP, Telnet, serial, VNC, RDP, terminal rendering, forwarding,
macros, multi-exec, and network diagnostics in one application.

## Highlights

- Native Iced/wgpu interface with terminal scrollback, search, themes, and keyboard shortcuts.
- SSH host-key verification, encrypted session storage, Argon2id, AES-256-GCM, and zeroized secrets.
- Resumable SFTP transfers with bounded memory, progress, retry, and throttling.
- Local/remote network tools, port forwarding, X11, VNC, Telnet, serial, and macro replay.
- Linux DEB, RPM, Flatpak, Snap, AppImage, Arch, Nix, and reproducible tarball recipes.

## Quick Install

Download a signed release and verify `SHA256SUMS`, then choose a package for
your distribution. See the [installation guide](docs/installation.md).

```text
sudo apt install ./remote-app_*.deb
sudo rpm -Uvh ./remote-app-*.rpm
```

## Documentation

- [User and developer guide](docs/src/SUMMARY.md)
- [Installation](docs/installation.md)
- [Security threat model](docs/threat_model.md)
- [Performance and profiling](docs/performance.md)
- [Reproducible builds](docs/reproducible_builds.md)
- [Security policy](SECURITY.md)
- [Public roadmap](ROADMAP.md)
- [Long-term vision](docs/vision.md)

Build the book with `cargo xtask docs` after installing mdBook. The current
repository uses a text-first documentation workflow; screenshots and terminal
recordings belong under `docs/assets/` when captured on supported desktop
environments.

## Development

```text
cargo fetch --locked
cargo check --workspace --all-targets --all-features
cargo test --workspace
cargo fmt --all -- --check
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for standards and the developer guide
for architecture and release procedures.

## Community and License

Please use [SUPPORT.md](SUPPORT.md) for questions and [SECURITY.md](SECURITY.md)
for private vulnerability reports. Propose and vote on features in GitHub
Discussions; accepted large features follow the
[RFC process](docs/feature_proposals/README.md). Remote App is licensed under
MIT OR Apache-2.0. See `Cargo.toml` and the dependency policy for
acknowledgments and third-party license handling.
