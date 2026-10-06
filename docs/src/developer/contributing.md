---
title: Contributing
---

# Contributing

Read `CONTRIBUTING.md` for the short policy. Before opening a pull request:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Keep changes layered and testable. Add a focused unit test for parser,
storage, security, and transfer changes. Do not log secrets or include
generated build output in commits. Use Conventional Commit prefixes such as
`feat:`, `fix:`, `docs:`, `perf:`, and `security:`.
