---
title: Importing Sessions
---

# Importing Sessions

Third-party session files import **one way**: foreign formats become new
sessions in the encrypted store. Imports never overwrite, never execute,
and never store passwords.

## Supported sources

| Source | File | Command |
| --- | --- | --- |
| MobaXterm XML export | `.xml` | `remote-app-headless import --from mobaxterm --file sessions.xml` |
| PuTTY / KiTTY registry export | `.reg` | `remote-app-headless import --from putty --file sessions.reg` |
| OpenSSH config | `ssh_config` | `remote-app-headless import --from ssh-config --file ~/.ssh/config` |

Preview is the default. Nothing is stored until `--commit`:

```text
remote-app-headless import --from ssh-config --file ~/.ssh/config
remote-app-headless import --from ssh-config --file ~/.ssh/config --commit
```

## Rules that always hold

- **Preview first**: the report lists drafted sessions, rejected rows with
  reasons, and every unknown field. Materializing needs `--commit`.
- **No overwrite**: name collisions skip with a rename suggestion
  (`name (imported)`, `name (imported 2)`, …). Nothing is renamed or
  replaced without explicit confirmation.
- **No passwords**: password-shaped values are dropped with a warning and
  never reach the store, logs, or the report.
- **No execution**: `ProxyCommand`, auto-commands, and `Match exec` arrive
  as disabled-by-default notes. They never run during preview or import.
- **Unknown fields are reported, never guessed**: an unknown protocol
  rejects the session with a reason instead of coercing it to SSH.

Every imported session carries `imported` + source tags and a provenance
note naming the source file (basename only).

## Compatibility matrix

The per-source, per-field disposition table lives in
[Import Compatibility](../../compatibility_import.md).
