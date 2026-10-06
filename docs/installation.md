# Installation

## Packages

Use the DEB package on Debian, Ubuntu, Mint, Pop!_OS, or elementary. Use the
RPM package on Fedora, RHEL, CentOS Stream, or OpenSUSE. Arch users can build
the `packaging/arch/PKGBUILD`; Nix users can run `nix run` from
`packaging/nix`.

```text
sudo apt install ./remote-app_*.deb
sudo rpm -Uvh ./remote-app-*.rpm
makepkg -si -p packaging/arch/PKGBUILD
nix run github:LucYTerM/mbxt?dir=packaging/nix
```

## Portable Formats

Flatpak and Snap are sandboxed and require network access plus desktop display
portals. AppImage requires no installation; mark it executable and run it:

```text
chmod +x remote-app-*.AppImage
./remote-app-*.AppImage
```

## Source

Install the pinned Rust toolchain and system development packages listed in
the distribution recipe, then run `cargo build --locked --release`.

## Trust

Verify `SHA256SUMS` before installing. Release signatures are published with
the artifact when the release signing key is configured. SSH host keys remain
subject to known_hosts verification after installation.
