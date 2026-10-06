---
title: Session Management
---

# Session Management

Sessions have a display name, protocol, host, port, username, authentication
method, tags, notes, forwarding rules, and optional X11 settings. Use tags to
group the session sidebar; the search field matches names and tags.

Saved sessions are serialized into `sessions.enc`, authenticated and encrypted
with AES-256-GCM. Export and import use encrypted files as well. Import merges
by session name and does not replace an existing same-named session.

To rotate the master password, unlock the store with the old password and use
the password-change action exposed by the storage layer. Rotation creates a
new salt and re-encrypts the records.
