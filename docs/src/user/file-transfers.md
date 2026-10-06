---
title: File Transfers
---

# File Transfers

The SFTP browser lists remote files, supports sorting/filtering, and exposes
upload/download actions. Drag files onto the browser to enqueue transfers.

Transfers are chunked and bounded in memory. Existing destination prefixes are
used as resume offsets when the source length is known. A `.part` path and
progress events protect interrupted work; successful transfers finalize the
destination. The transfer manager supports pause, cancel, retry backoff, and
bandwidth limits.

The SFTP pipeline depth and throttle are configurable under network settings.
Higher depth can improve high-latency links but consumes more outstanding
requests; measure before increasing it.

## Resume and overwrite policy

A transfer resumes only when the destination holds a strict prefix of a
known-length source (`destination < source`): both ends seek to the
existing length and continue. In every other case — unknown source
length, destination longer than the source, or equal lengths — the
destination is truncated and rewritten from byte 0, so unverifiable
partials are never appended to. Downloads stage through a `.part` path
and rename into place only on success; an interrupted transfer never
leaves a half-written file at the final name.
