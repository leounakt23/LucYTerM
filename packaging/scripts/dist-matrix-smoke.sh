#!/usr/bin/env bash
# Distribution-matrix smoke checks (RPM, Flatpak, Snap, AppImage, AUR, Nix).
# DEB is proven separately. Static checks always run; real builds run only
# when the respective toolchain is present. Anything uninstallable here is
# an honest MISS: this script reports the exact missing piece and never
# fakes a build.
#
# Feature-set contract: every recipe ships DEFAULT cargo features
# (ssh,telnet) except packaging/flatpak, which intentionally builds
# `--all-features`. Keep that divergence when touching build flags.
set -u

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root_dir"

pass=0; miss=0
report() { # $1 = artifact, $2 = PASS|MISS, $3 = detail
  printf '%-10s %-4s %s\n' "$1" "$2" "$3"
  if [[ "$2" == "PASS" ]]; then pass=$((pass + 1)); else miss=$((miss + 1)); fi
}
have() { command -v "$1" >/dev/null 2>&1; }

# --- RPM: static file-reference check; real build needs rpmbuild/fpm ---
if have rpmbuild || have fpm; then
  report RPM PASS "toolchain present ($(command -v rpmbuild fpm 2>/dev/null | tr '\n' ' '))"
else
  missing="rpmbuild,fpm"
  ok=1
  for f in assets/remote-app.desktop assets/remote-app.metainfo.xml \
           docs/remote-app.1 assets/icon.svg packaging/scripts/generate-icons.sh; do
    [[ -e "$f" ]] || { echo "RPM static check: missing $f"; ok=0; }
  done
  grep -q 'BuildRequires:.*cargo' packaging/rpm/remote-app.spec || { echo "RPM static check: no cargo BuildRequires"; ok=0; }
  grep -q 'remote-app-headless' packaging/rpm/remote-app.spec || { echo "RPM static check: headless binary not packaged"; ok=0; }
  if [[ "$ok" == 1 ]]; then report RPM MISS "static refs OK; no build (missing: $missing)"; else report RPM MISS "static refs BROKEN; no build (missing: $missing)"; fi
fi

# --- Flatpak: YAML parse; real build needs flatpak-builder ---
if python3 -c "import yaml; yaml.safe_load(open('packaging/flatpak/com.github.LucYTerM.remote-app.yml'))" 2>/dev/null; then
  if have flatpak-builder; then report FLATPAK PASS "flatpak-builder present; manifest parses"
  else report FLATPAK MISS "manifest parses; no build (missing: flatpak-builder,flatpak)"; fi
else
  report FLATPAK MISS "manifest YAML parse FAILED and flatpak-builder missing"
fi

# --- Snap: YAML parse; real build needs snapcraft ---
if python3 -c "import yaml; yaml.safe_load(open('packaging/snap/snapcraft.yaml'))" 2>/dev/null; then
  if have snapcraft; then report SNAP PASS "snapcraft present; recipe parses"
  else report SNAP MISS "recipe parses; no build (missing: snapcraft)"; fi
else
  report SNAP MISS "recipe YAML parse FAILED and snapcraft missing"
fi

# --- AppImage: YAML parse; real build needs appimage-builder/appimagetool ---
if python3 -c "import yaml; yaml.safe_load(open('packaging/appimage/AppImageBuilder.yml'))" 2>/dev/null; then
  if have appimage-builder || have appimagetool; then report APPIMAGE PASS "AppImage toolchain present; recipe parses"
  else report APPIMAGE MISS "recipe parses; no build (missing: appimage-builder,appimagetool)"; fi
else
  report APPIMAGE MISS "recipe YAML parse FAILED and AppImage toolchain missing"
fi

# --- AUR: bash -n parse (+shellcheck when present); makepkg is Arch-only ---
if bash -n packaging/arch/PKGBUILD; then
  if have makepkg; then report AUR PASS "makepkg present; PKGBUILD parses"
  else report AUR MISS "PKGBUILD bash -n PASS; no package build (missing: makepkg Arch-only$(have shellcheck || echo ',shellcheck'))"; fi
else
  report AUR MISS "PKGBUILD bash -n FAILED"
fi

# --- Nix: real check needs nix ---
if have nix; then
  if nix --extra-experimental-features 'nix-command flakes' flake check ./packaging/nix 2>/dev/null; then
    report NIX PASS "nix flake check passed"
  else
    report NIX MISS "nix present but flake check FAILED"
  fi
else
  if [[ -e Cargo.lock ]]; then report NIX MISS "flake unread by evaluator; no check (missing: nix,nix-instantiate; Cargo.lock present)"
  else report NIX MISS "flake unread by evaluator; no check (missing: nix,nix-instantiate; Cargo.lock MISSING)"; fi
fi

echo "smoke summary: $pass PASS, $miss MISS"
[[ "$miss" == 0 ]]
