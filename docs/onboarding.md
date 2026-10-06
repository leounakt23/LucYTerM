# Onboarding

This guide takes a new contributor from zero to a first reviewed pull request.
It assumes a Linux machine; other platforms are community-supported only.

## 1. Set Up

1. Install the pinned Rust toolchain from `rust-toolchain.toml` and the Linux
   packages in `docs/installation.md`.
2. Clone the repository and fetch dependencies:
   `cargo fetch --locked`.
3. Build the workspace: `cargo build --locked`.
4. Run the fast gates before changing anything:
   `cargo fmt --all -- --check`,
   `cargo check --workspace --all-targets --all-features`,
   `cargo test --workspace`.

If a step fails on a supported distribution, open a bug with the OS version,
toolchain output, and full log. Do not paste secrets.

## 2. Learn the Map

- `CONTRIBUTING.md` for contribution paths and standards.
- `docs/src/developer/architecture.md` for the workspace layout.
- `docs/developer/dev_setup.md` for editor, container, and native-library help.
- `docs/developer/coding_style.md` for Rust conventions.
- `docs/developer/testing.md` (in-book) for test tiers.
- `docs/support.md` and `docs/developer/triage.md` before commenting on issues.

## 3. Pick a First Task

- `good-first-issue`: small, well-scoped entry tasks. A mentor helps when
  capacity allows, but mentoring is not guaranteed for every issue.
- `help-wanted`: tasks where contributor help is genuinely needed.
- Docs-only fixes are valid first PRs and follow the same template.

Claim an issue with a short comment, ask one focused question if the scope is
unclear, and wait for a maintainer acknowledgement before large refactors.

## 4. Open Your First PR

1. Create a short-lived `feature/*` branch from `main`.
2. Make one behavior change with tests and docs in the same PR.
3. Complete every item in `.github/PULL_REQUEST_TEMPLATE.md`, including
   validation commands and security/compatibility notes.
4. Use a Conventional Commit title (`feat:`, `fix:`, `docs:`, …).
5. Respond to review as a conversation; maintainers explain unfamiliar steps
   rather than closing first PRs for process reasons.

## 5. After Merge

- Confirm your entry in `CONTRIBUTORS.md` only with your explicit consent.
- Stay for a second issue if you can: first-time contributor retention is a
  tracked community-health metric, but there is no obligation.
- Informal mentoring after a first merge depends on volunteer capacity; ask in
  the issue or Discussion rather than assuming a standing program.
