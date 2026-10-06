#!/usr/bin/env bash
# Release build script: builds both binaries, reports artifacts and generated
# completions. Desktop file installation is handled by packaging (deb/rpm/
# Flatpak) or manually via desktop-file-install.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

GIT_HASH="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo "==> Building remote-app ${GIT_HASH} (release profile: lto, codegen-units=1)"

cargo build --release --workspace

BIN="target/release"
echo "==> Artifacts:"
echo "    ${BIN}/remote-app"
echo "    ${BIN}/remote-app-headless"

echo "==> Generated shell completions (also embedded; runtime: remote-app-headless completions <shell>):"
find target -path '*/out/completions/*' -type f 2>/dev/null | while read -r f; do
    echo "    ${f}"
done

echo "==> Desktop entry (install target for packaging):"
echo "    assets/remote-app.desktop -> \${prefix}/share/applications/remote-app.desktop"
