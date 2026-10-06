---
title: Getting Started
---

# Getting Started

## Install

Use the package for your distribution or build from source. The package and
reproducible-build instructions are in the repository's [installation guide](../../installation.md).

## First launch

Remote App creates its configuration directory on first launch, initializes
owner-only permissions, and starts the structured log writer. The default
configuration does not automatically connect to a host.

The default files are stored under `~/.config/remote-app/` on Linux:

- `config.ron`: non-secret preferences;
- `sessions.enc`: encrypted session records;
- `logs/`: rotating application logs.

## Create a session

1. Press `Ctrl+T` or choose `+ tab`.
2. Open the new-session dialog from the session list.
3. Select SSH, enter the host, port, and username, and choose an authentication method.
4. Save the session only if you want it in the encrypted session store.
5. Connect and verify the host fingerprint before approving a first-use key.

The initial host-key decision is security-sensitive. Never approve a changed
key without checking it through a trusted independent channel.
