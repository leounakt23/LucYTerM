# Post-Launch Maintenance

This plan describes the intended operating model after v1.0. Remote App is a
community-maintained project with limited capacity. Cadence and response times
are targets, not contractual guarantees, unless a separate written support
agreement says otherwise.

## Release Cadence

- Patch releases (`v1.0.x`) are cut as needed for security defects, data loss,
  and critical regressions. The target is a tested release within 48 hours of
  confirming a critical issue, subject to maintainer availability.
- Minor releases (`v1.x.0`) are planned every six to eight weeks for backward-
  compatible features and improvements. A release may be delayed rather than
  shipping known unsafe or incomplete work.
- Major releases (`vX.0.0`) are expected every 12 to 18 months when justified
  by breaking changes or substantial architecture work.
- An LTS line is selected approximately every two years and receives security
  and critical-correctness fixes for three years. LTS starts only when at least
  two maintainers can support it; otherwise the project will announce the
  capacity shortfall instead of implying unsupported coverage.

Stable releases are signed and produced by the protected release workflow.
Pre-releases and nightly builds do not reduce the support period of stable or
LTS releases. Supported branches and end-of-life dates are listed in release
notes; `SECURITY.md` records the currently supported lines.

## Supported Platforms

The compatibility target requested for the v1 series is:

- The two latest Ubuntu LTS releases, currently 22.04 and 24.04.
- Current Fedora and its immediately previous release.
- Debian stable and oldstable.
- Arch Linux rolling at the time of each release.
- X11 and Wayland sessions.

CI currently covers the published Ubuntu 22.04/24.04 baseline, Fedora 40, and
Arch; current/previous Fedora and Debian stable/oldstable require release smoke
testing until permanent jobs are added. A release must not claim a platform was
tested when that smoke test did not run. "Supported" means maintainers accept
actionable reports and make a best-effort correction. It does not guarantee
compatibility with every kernel, compositor, GPU driver, remote server, or
downstream package. Other distributions are community-supported.

## Routine Work

Weekly async triage reviews new issues, regressions, security signals, and pull
requests. Confirmed work is assigned a release milestone. Maintainers use
`confirmed`, `needs-info`, `duplicate`, `wontfix`, and `good-first-issue` in
addition to area, severity, beta, and `tech-debt` labels.

Each planning cycle reserves roughly 20 percent of available capacity for
refactoring, tests, dependency work, performance, and documentation. Once per
year the project schedules a feature-free technical-debt week. Debt that
cannot be handled immediately is recorded publicly with the `tech-debt` label,
except security-sensitive details.

## Dependencies and Toolchain

- Dependabot groups Cargo, Actions, and container updates every week.
- Maintainers review the dependency tree monthly, including duplicate versions,
  default features, native libraries, and supply-chain exposure.
- Every quarter, maintainership and release activity of critical crates is
  checked. An unmaintained dependency is replaced, isolated, or forked only
  after ownership, security, and long-term cost are reviewed.
- The minimum Rust version is reviewed annually and aligned with a stable Rust
  release. Emergency compiler upgrades may happen sooner for security or
  platform compatibility.
- `cargo audit`, `cargo deny`, SBOM generation, pinned actions, and license
  review remain release gates.

### Security Fixes Blocked by the Stack Pins

An advisory sweep on 2026-10-10 recorded what each open advisory needs,
straight from crates.io version metadata:

| Crate (locked)    | Fixed in            | Minimum rustc | Blocker                                          |
| ----------------- | ------------------- | ------------- | ------------------------------------------------ |
| `time` 0.3.41     | 0.3.47              | 1.88          | toolchain pin (1.85.1)                           |
| `hickory` 0.24.4  | 0.26.2 / 0.26.1     | 1.88          | toolchain pin + 0.24 → 0.26 API bump             |
| `russh` 0.45.0    | 0.61.1              | 1.85          | **DONE 2026-10-15** (migration; rekey fixed)     |
| `quinn-proto` 0.11.14 | 0.11.18         | 1.85          | **DONE 2026-10-20** (MSRV bump; lock-only)        |
| `russh` 0.61.2    | 0.63.1              | 1.85          | **DONE 2026-10-20** (2 GitHub GHSAs)             |
| `russh` 0.63.1    | 0.63.2 / 0.64.1     | 1.89          | remaining fixes; toolchain pin                   |
| `lru` 0.12.5      | 0.16.3              | —             | `iced_glyphon` 0.6 requires `lru` 0.12           |

The ordered migration when maintainership chooses to spend it:

1. **DONE 2026-10-15:** migrated `russh` 0.45 to 0.61.1 on the pinned
   toolchain (breaking handler/client API: `#[async_trait]` dropped,
   `AuthResult::success()`, `PrivateKeyWithHashAlg`, agent
   `AgentIdentity`). Workspace tests, live suites (`ssh_auth_live`,
   `sftp_integration`, `forward_integration`, `x11_integration`), and a
   `RekeyLimit 16M` repro (62 rekeys) pass; the two russh ignores were
   retired from `.cargo/audit.toml` and `deny.toml`.
2. Raise the toolchain pin to at least 1.89 (MASTER_PROMPT amendment, MSRV
   update, CI images, edition-2024 dependencies), then `cargo update -p
   time` (semver-compatible) and the `hickory` 0.24 → 0.26 bump; finish the
   russh move at 0.63.2/0.64.1 (the two 2026-10-20 GHSAs are already
   cleared by 0.63.1).
   - **DONE 2026-10-20 (partial step 2):** MSRV raised 1.80 → 1.85
     (`Cargo.toml`, `.clippy.toml`, `.cargo/config.toml` comment; CI
     keeps the 1.85.1 pin). This cleared the five lock-only
     `quinn-proto` advisories (`Cargo.lock` 0.11.14 → 0.11.18; the crate
     is still never compiled) and the yanked `inout` 0.2.3 (→ 0.2.5,
     non-yanked); both ignore sets were removed. The 1.89 target is
     still required for `time`/`hickory` and the russh 0.63.2 move.
    - **DONE 2026-10-20 (russh security stop):** migrated `russh`
      0.61.2 → 0.63.1, the last release compatible with 1.85, for
      GHSA-47hw-gvq5-r2gm (CVE-2026-102823, high 7.5) and
      GHSA-p8qx-h547-fjw9 (CVE-2026-102822, low 3.7) — GitHub-only
      advisories absent from the RustSec mirror. API migration:
      `check_server_key` now receives `PublicKeyOrCertificate`
      (certificates are rejected fail-closed; no CA trust store exists)
      and server-initiated channel opens use the explicit
      `ChannelOpenHandle` accept/reject API. fmt, clippy, 36 workspace
      test suites, `cargo audit`, and `cargo deny` pass; live suites
      remain CI-E2E-gated. 0.63.2/0.64.1 need rustc ≥1.89 and stay on
      the toolchain track.
3. `lru` follows a future `iced` bump; until then it stays recorded in
   `docs/threat_model.md`.

The remaining RustSec-mirrored advisories stay listed with reasons in
`.cargo/audit.toml` / `deny.toml`, and the full GitHub inventory lives in
`docs/threat_model.md`.

## Documentation

Behavior, configuration, compatibility, privacy, and migration documentation
must change in the same pull request as code. Documentation-only contributions
are welcome and follow the normal review process. Maintainers audit the full
manual annually and before an LTS designation.

Each stable release publishes a versioned mdBook snapshot. The default site
points to the latest stable release and clearly labels older, beta, and nightly
documentation. Fixes may be backported to a supported version when incorrect
instructions could cause data loss or a security problem.

## Production Signals

Only explicitly opted-in, anonymous metrics described in `docs/privacy.md` are
used. Cohort-level dashboards may show crash-free sessions by version, release
adoption, broad feature counts, performance aggregates, and normalized error
categories. They never contain session, host, command, file, terminal, or user
content. Small cohorts are suppressed. Product decisions must also consider
issues, discussions, surveys, and users who choose not to send metrics.

## Sustainability

The project aims for two or three trusted co-maintainers and a bus factor above
one. Promotion requires sustained contributions, sound review judgment,
security awareness, and agreement to the responsibilities in `MAINTAINERS.md`.
Emergency contacts and recovery details are stored privately and reviewed
twice a year. The current single-maintainer limitation is recorded in
`MAINTAINERS.md`; two-person recovery becomes mandatory when a backup joins.

Optional funding may include GitHub Sponsors, OpenCollective, corporate
sponsorship, and separately contracted enterprise support. Funding does not
buy technical decisions, vulnerability suppression, or access to core
features. The core application remains available under its existing license.

## Legal and Compliance

Automated license policy is supplemented by release-time review. Maintainers
track cryptography-related export obligations applicable to where releases and
services are operated; this plan is not legal advice. Any hosted feedback,
analytics, update, or crash service needs documented retention, deletion,
access control, subprocessors, and a GDPR lawful basis where applicable. A
hosted service must publish appropriate Terms of Service and privacy contacts
before collecting user data.
