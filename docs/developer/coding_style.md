# Coding Style

## Rust Baseline

- rustfmt is required; Clippy runs with `-D warnings`.
- Target the pinned toolchain; do not use features from newer compilers
  without a toolchain bump.
- Async I/O by default; blocking work goes through `spawn_blocking`.
- No `unwrap`/`expect` on untrusted input or I/O paths; return typed errors.

## Structure

- UI code must not depend directly on transports, filesystems, or platform
  handles. Session actors own transport state behind crate boundaries.
- Keep functions small and total behavior observable: one behavior change per
  PR, with a focused test at the lowest tier that can prove it.
- Parser, crypto, storage, and transfer changes need unit or property tests;
  update `docs/` in the same PR.

## Safety and Secrets

- `unsafe` blocks need a safety comment and stay platform-isolated.
- Never log secrets, host details, terminal content, or fuzz inputs.
- Imported configuration is data, never commands; preview before executing or
  materializing anything.

## Commits and Reviews

- Conventional Commit prefixes: `feat:`, `fix:`, `docs:`, `perf:`,
  `security:`, `build:`, `test:`, `chore:`.
- Describe behavior, security/privacy impact, protocol and packaging effects,
  validation commands, and hot-path performance data where relevant.
- Prefer boring, reviewable code over clever abstractions. Refactors that mix
  behavior changes are sent back for splitting.
