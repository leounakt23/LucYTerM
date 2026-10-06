# Release Process

This is the canonical contributor-facing release checklist. The short in-book
summary lives at `docs/src/developer/release.md`.

## Prepare

1. Update `CHANGELOG.md`, version metadata, and supported-version notes in
   `SECURITY.md`.
2. Run formatting, workspace checks, tests, strict Clippy, `cargo audit`, and
   `cargo deny`.
3. Build Linux targets with `packaging/scripts/build-all.sh` or
   `.github/workflows/release.yml`.
4. Smoke-test DEB/RPM installs in clean distro containers; exercise Flatpak,
   Snap, AppImage, Arch, and Nix recipes where toolchains exist. Never claim an
   untested platform was tested.

## Sign and Tag

5. Set `SOURCE_DATE_EPOCH`, generate checksums, and compare reference builds
   with diffoscope.
6. Create an annotated `vX.Y.Z` tag and push it. The protected release workflow
   builds artifacts and publishes the GitHub release.
7. Publish the detached signature beside `SHA256SUMS` when the signing key is
   configured.

## Publish and Follow Up

8. Post release notes mentioning contributors with consent; file downstream
   packaging PRs (Flathub, Snapcraft, AUR, PPA, COPR) under their own review.
9. Monitor opt-in crash and feedback dashboards plus issue triage; cut a patch
   release for security, data-loss, or critical-regression fixes.
10. Package-manager submissions need platform credentials and are never
    performed silently by CI.
