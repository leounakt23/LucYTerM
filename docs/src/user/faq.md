---
title: FAQ
---

# FAQ

## Where are passwords stored?

Passwords are held in memory during use. Saved session data is encrypted; the
desktop keyring is optional and disabled by default.

## Does Remote App accept unknown SSH keys automatically?

No. The SSH handler requires known-host verification. First-use and mismatch
decisions require an explicit user action.

## Can I use the same sessions in the CLI?

Yes. `remote-app-headless` reads the same encrypted session store and supports
list, import, export, reset, and shell completion commands.

## Is RDP available on every build?

Protocol support is feature-gated. A package or build must include the relevant
feature and native runtime dependencies.

## How do I report a vulnerability?

Follow `SECURITY.md`; do not publish undisclosed vulnerabilities in an issue.
