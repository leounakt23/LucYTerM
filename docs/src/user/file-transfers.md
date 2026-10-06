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
