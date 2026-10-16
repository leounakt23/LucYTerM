# Changelog

All notable changes to Remote App are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
Semantic Versioning.

## [Unreleased]

## [1.0.0] - 2026-10-16

### Added

- User, developer, packaging, security, and reproducible-build documentation.
- Cross-distribution packaging recipes and release automation.
- Multi-distribution CI, coverage, benchmark, fuzz, E2E, signed release, and dependency automation.
- Privacy-preserving beta feedback, release channels, opt-in crash reporting and telemetry, and nightly builds.
- Sustainable post-launch maintenance, support, deprecation, security-response, and maintainer continuity policies.
- Public Now/Next/Later roadmap, long-term vision, feature voting form, and RFC process.
- Community building: expanded contributing guide, full Code of Conduct, all-contributors acknowledgment, BDFN governance, onboarding, coding style, triage, release checklist, community channels, discussion templates, event plans, and contributors-table check.
- Final integration: master rebuild prompt, verification checklist, final report template, and developer rebuild guide.

### Security

- Process hardening, secret wrappers, host-key policy, and secure file writes.
- Migrated `russh` 0.45 → 0.61.1, clearing the high-severity SSH advisory set and retiring its `cargo audit` / `cargo deny` ignores.
- Cleared RustSec advisories published during release prep: `rustls` 0.23.45 → 0.23.46 (RUSTSEC-2026-0346 … -0349); five lock-only `quinn-proto` advisories are ignore-listed with reasons (crate never compiled; fix requires rustc ≥1.85).

### Fixed

- Large SFTP downloads aborted when the server initiated a mid-session SSH rekey (`StrictKeyExchangeViolation`); transfers now complete across rekeys.

### Performance

- Parallel-handle SFTP download pipeline with monotonic progress: 512 MiB download 37 → 517 MB/s; 1 GiB download no longer stalls (672 MB/s, checksum-verified).
- Ten-session 8 h soak: peak VmHWM 14.8 MiB, VmRSS slope 177.8 KiB/h, 0 reconnects, 2.75 TB transferred.

## [0.1.0] - 2026-01-01

### Added

- Initial native remote application workspace and session model.
- SSH, terminal, SFTP, forwarding, macro, and multi-exec foundations.

[Unreleased]: https://github.com/leounakt23/LucYTerM/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/leounakt23/LucYTerM/releases/tag/v1.0.0
