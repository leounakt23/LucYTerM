# Final Report: Remote App v1.0

> Template — fill each section with measured values before any release claim.
> Do not ship v1.0 with placeholders. Current repository version is `0.1.0`;
> this report becomes factual only when every `[FILL]` is replaced with
> evidence and linked artifacts.

## Project Summary

- Report date: [FILL: YYYY-MM-DD]
- Release tag: [FILL: e.g. v1.0.0 — must exist]
- Total development time: [FILL: X weeks/months]
- Lines of code: [FILL: `tokei` or equivalent output + commit hash]
- Number of files: [FILL]
- Dependencies: [FILL: `cargo tree` summary + `cargo deny` result]

## Features Delivered

[FILL: list implemented features with one-line descriptions and links to docs
or tests. Only features merged behind their Cargo gates count.]

## Performance Metrics

[FILL: measured vs target table. Example: cold start median of 5 runs on
named hardware; idle RSS with named measurement method; FPS method and
result; transfer throughput with iperf-class setup. Unmeasured rows stay
marked UNMEASURED — never copy targets into results.]

## Security Posture

[FILL: crypto implementation (AES-256-GCM, Argon2id parameters), host-key
policy, permission checks, log audit, `cargo audit`/`cargo deny` outputs,
threat-model deltas, and any accepted exceptions with tracking issues.]

## Test Coverage

[FILL: line-coverage percentage with tool and command, test counts by tier
(unit/integration/E2E/fuzz), live-gated suites with server configuration, and
platforms actually exercised.]

## Known Limitations

The following are true at the time of writing and must not be edited into
successes without evidence:

- Version is `0.1.0`; no v1.0 release has been cut.
- Single maintainer (bus factor 1); backup seats vacant.
- Private GitHub vulnerability reporting returned 404 during review; enabling
  it (or a verified private contact) is a release blocker.
- CI covers Ubuntu 22.04/24.04, Fedora 40, and Arch; Debian and
  current/previous Fedora need release smoke tests until permanent jobs exist.
- Local full-test linking is constrained on Windows-GNU hosts without
  `dlltool.exe`; CI is authoritative for the full suite.
- No official chat, social, blog, Weblate, funding, or meeting cadence exists.
- RDP is an external-process integration, not a native binding; VNC, serial,
  and some protocol paths require live counterparts to verify.
- Performance numbers are targets until measured per the checklist.

[FILL: append any further limitations found during verification.]

## Future Work

See [`ROADMAP.md`](../ROADMAP.md) (Now/Next/Later/Won't-Do) and
[`docs/roadmap_details.md`](roadmap_details.md). Only milestone-assigned work
with an owner is committed scope.

## Acknowledgments

[FILL: contributors from `CONTRIBUTORS.md` with consent, plus inspirations.
Never list anyone without explicit consent.]

## License

MIT OR Apache-2.0. See `Cargo.toml` and the dependency policy for
third-party license handling.
