---
title: Building from Source
---

# Building from Source

The pinned toolchain is in `rust-toolchain.toml`. Linux builds need Rust,
Cargo, pkg-config, DBus development headers, udev development headers, and a
working graphics stack. Distribution-specific package requirements are listed
in `packaging/`.

```text
rustup show
cargo fetch --locked
cargo build --locked
cargo build --locked --release
```

The GUI binary is `remote-app`; the headless binary is
`remote-app-headless`. Protocol support is controlled by Cargo features. Use
`--all-features` for the broadest development check, but package only the
features and native libraries you intend to support.
