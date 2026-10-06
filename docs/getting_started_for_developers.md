# Getting Started for Developers (Rebuild with the Master Prompt)

This guide explains how to use `MASTER_PROMPT.md` to rebuild the project from
scratch, phase by phase, without inheriting unverified claims.

## Prerequisites

- Linux with Rust 1.85.1 (via `rust-toolchain.toml`), pkg-config, DBus/udev
  development headers, and a graphics stack (`docs/installation.md`).
- Optional: `mdbook`, `cargo-fuzz` (nightly), Docker for live-server tests.
- On Windows-GNU hosts without MSVC linkers, set
  `RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu` for local iteration; CI on
  Linux remains authoritative.

## Rebuild Loop (Per Phase)

1. Read the phase in `MASTER_PROMPT.md` and its map to `src/`/`crates/`.
2. Implement the smallest slice that compiles.
3. Run the phase gate from the master prompt, then the shared gates:
   `cargo fmt --all -- --check`,
   `cargo check --workspace --all-targets --all-features`,
   `cargo test --workspace`,
   `cargo clippy --workspace --all-targets -- -D warnings`.
4. Add or update tests, docs, and `CHANGELOG.md` in the same change.
5. Commit with a Conventional Commit message. Do not advance with red gates.

## Verification and Reporting

- Execute `docs/verification_checklist.md` top to bottom for release
  candidates; record every measurement in `docs/final_report.md`.
- Live-gated suites (`vnc_integration`, `x11_integration`, SSH/SFTP live
  cases) need disposable containers and documented `MBXT_*_TEST_*` variables.
  Never point them at production.
- Coverage, audit, deny, mdBook, and actionlint are release gates, not
  advisories.

## When Stuck

- Check `docs/src/developer/architecture.md`, `doc/tech_stack.md`, and the
  phase's listed modules before adding new abstractions.
- Ask in GitHub Discussions with version, environment, repro steps, and
  sanitized logs. Security-sensitive problems follow `SECURITY.md` privately.
- Prefer splitting a stuck phase into smaller reviewable PRs over skipping
  gates.

## Honesty Rules

- Measured values only in reports; targets are not results.
- No invented maintainers, contacts, platforms, or metrics.
- Single-maintainer capacity applies: stability and security preempt features.
