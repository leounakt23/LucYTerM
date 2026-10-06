# Development Setup

## Toolchain

Use the pinned toolchain in `rust-toolchain.toml` (currently 1.85.1 with
rustfmt and clippy). The repository's local validation for this line uses
`stable-x86_64-pc-windows-gnu` only as a fallback when MSVC linkers are
missing; Linux builds and CI remain authoritative.

```text
rustup show
cargo fetch --locked
cargo build --locked
```

Useful extras: `mdbook` for `docs/`, `cargo-fuzz` with a nightly toolchain for
`fuzz/`, and `cargo-llvm-cov` for coverage. Install them only when you need
them.

## System Dependencies

Install distribution packages from `docs/installation.md` first: Rust, Cargo,
pkg-config, DBus and udev development headers, and a working graphics stack.
Container builds for DEB/RPM families are described in
`packaging/scripts/build-all.sh`.

## Daily Commands

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Run `cargo xtask docs` after installing mdBook to verify the book builds. Run
`cargo xtask i18n` after touching `locales/en-US/app.ftl`.

## Editor and Workflow

Any LSP-capable editor works. Keep `main` releasable with short-lived
`feature/*` and `hotfix/*` branches. Never commit secrets, session exports,
`target/` output, or sanitized-fixture originals.
