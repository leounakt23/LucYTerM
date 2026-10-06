---
title: API Documentation
---

# API Documentation

Rust API documentation is generated from public workspace crates:

```text
cargo doc --workspace --no-deps --open
```

Public types should have rustdoc examples or an explanation of their error and
cancellation behavior. The application crate is the composition layer; stable
library APIs belong in `mbxt-core`, `mbxt-terminal`, `mbxt-connections`, or
`mbxt-storage` when they are useful outside the GUI.

The release process can publish generated documentation to the project's
documentation host. The repository does not currently claim an external
docs.rs publication until the package metadata and public API surface are
stable.
