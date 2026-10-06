#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root_dir"

export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || date +%s)}"
targets=(
  x86_64-unknown-linux-gnu
  x86_64-unknown-linux-musl
  aarch64-unknown-linux-gnu
  aarch64-unknown-linux-musl
)

for target in "${targets[@]}"; do
  echo "==> building $target"
  if command -v cross >/dev/null 2>&1; then
    cross build --locked --release --target "$target"
  else
    rustup target add "$target"
    cargo build --locked --release --target "$target"
  fi
done

mkdir -p dist
version="$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"name":"remote-app".*"version":"\([^"]*\)".*/\1/p' | head -n 1)"
for target in "${targets[@]}"; do
  archive="dist/remote-app-${version}-${target}.tar.gz"
  staging="$(mktemp -d)"
  trap 'rm -rf "$staging"' EXIT
  install -Dm755 "target/$target/release/remote-app" "$staging/remote-app-$target/remote-app"
  install -Dm755 "target/$target/release/remote-app-headless" "$staging/remote-app-$target/remote-app-headless"
  install -Dm644 assets/remote-app.desktop "$staging/remote-app-$target/remote-app.desktop"
  install -Dm644 assets/remote-app.metainfo.xml "$staging/remote-app-$target/remote-app.metainfo.xml"
  install -Dm644 docs/remote-app.1 "$staging/remote-app-$target/remote-app.1"
  tar --sort=name --mtime="@$SOURCE_DATE_EPOCH" --owner=0 --group=0 --numeric-owner -czf "$archive" -C "$staging" "remote-app-$target"
  rm -rf "$staging"
  trap - EXIT
done

sha256sum dist/* > dist/SHA256SUMS
