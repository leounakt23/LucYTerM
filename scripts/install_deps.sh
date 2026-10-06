#!/usr/bin/env bash
# Install build dependencies for the current Linux distribution, then
# bootstrap the Rust toolchain if missing.
set -euo pipefail

APT_PKGS=(build-essential pkg-config libdbus-1-dev libudev-dev)
DNF_PKGS=(gcc gcc-c++ make pkgconf-pkg-config dbus-devel systemd-devel)
PAC_PKGS=(base-devel dbus libudev)
PKGS=""

if command -v apt-get >/dev/null 2>&1; then
    PKGS="${APT_PKGS[*]}"
    sudo apt-get update
    sudo apt-get install -y ${APT_PKGS[@]}
elif command -v dnf >/dev/null 2>&1; then
    PKGS="${DNF_PKGS[*]}"
    sudo dnf install -y ${DNF_PKGS[@]}
elif command -v pacman >/dev/null 2>&1; then
    PKGS="${PAC_PKGS[*]}"
    sudo pacman -S --needed ${PAC_PKGS[@]}
else
    echo "Unsupported distribution; install manually: ${APT_PKGS[*]}" >&2
    exit 1
fi
echo "==> Installed system packages: ${PKGS}"

# Rust toolchain (stable) via rustup.
if ! command -v cargo >/dev/null 2>&1; then
    echo "==> Installing Rust toolchain (stable) via rustup"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    export PATH="$HOME/.cargo/bin:$PATH"
fi

# Useful dev tooling (non-fatal if any fails).
cargo install --locked cargo-audit || true

rustc --version
cargo --version
echo "==> Ready. Run: scripts/build.sh"
