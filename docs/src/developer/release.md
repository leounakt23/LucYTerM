---
title: Release Process
---

# Release Process

1. Update `CHANGELOG.md` and version metadata.
2. Run formatting, workspace checks, tests, clippy, cargo audit, and cargo deny.
3. Build and inspect the four supported Linux targets with
   `packaging/scripts/build-all.sh` or the release workflow.
4. Test DEB/RPM installation in clean distro containers and smoke-test the
   Flatpak, Snap, AppImage, Arch, and Nix recipes where their toolchains are available.
5. Set `SOURCE_DATE_EPOCH`, generate checksums, and compare reference builds with diffoscope.
6. Create an annotated `vX.Y.Z` tag and push it. `.github/workflows/release.yml`
   builds artifacts and publishes the GitHub release.
7. If configured, import the release signing key in CI and publish the detached
   signature beside `SHA256SUMS`.

Package-manager submissions to Flathub, Snapcraft, AUR, PPA, and COPR require
their platform-specific review or maintainer credentials and are not silently
performed by CI.
