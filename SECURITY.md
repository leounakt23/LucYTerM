# Security Policy

## Reporting a Vulnerability

Do not open a public issue for an undisclosed vulnerability. Send a report to
the project maintainers through the repository security advisory channel (or
the private contact listed by the project owner) with:

- affected version and operating system;
- a minimal reproduction or proof of concept;
- impact and required privileges;
- whether credentials or private data may be exposed.

Critical reports are targeted for acknowledgment within 24 hours. This is a
best-effort community target, not a contractual guarantee. Reports are
investigated privately and credited unless the reporter requests anonymity.
The full response and 90-day coordinated-disclosure process is documented in
[`docs/security_policy.md`](docs/security_policy.md).

## Supported Versions

Security fixes target the latest stable release, supported LTS lines, and the
default branch. Users should upgrade before reporting behavior already fixed in
a newer release.

| Version | Supported | Notes |
| --- | --- | --- |
| `1.0.x` | Yes | Current stable line; update this row when a later minor ships |
| `< 1.0` | No | Pre-release builds receive no backports |
| LTS | None designated | The first LTS needs two active maintainers |

## Security Expectations

Never include passwords, private keys, session exports, or unredacted logs in
reports. Host-key warnings must not be bypassed without verifying the
fingerprint through an independent channel.
