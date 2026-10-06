---
title: Architecture
---

# Architecture

Remote App is an Iced MVU application around a Tokio runtime. The UI maps
messages to state transitions and tasks; session actors own transport handles;
storage and system integrations are behind crate/module boundaries.

## Workspace

- `crates/core`: protocol-independent domain types and IDs.
- `crates/terminal`: VTE-facing terminal model, grid, atlas, and renderer.
- `crates/connections`: transport trait, SSH/Telnet/serial/VNC/RDP paths, SFTP, and forwarding.
- `crates/storage`: persistence support.
- `crates/system`: platform integration.
- `src/app`: composition root, messages, state, subscriptions, and update logic.
- `src/ui`: Iced views and widgets.
- `src/tools`: integrated network tools.
- `src/utils`: configuration, logging, crypto, secure storage, and profiling.

The UI must not depend directly on russh, filesystem protocols, or platform
handles. Async I/O stays off the UI thread. Bounded channels and session actors
provide backpressure and cancellation. See `doc/architecture.md` for sequence
diagrams and the full design record.
