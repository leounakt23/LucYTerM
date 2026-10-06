# Maintainers

## Current Team

| Maintainer | Role | Status |
| --- | --- | --- |
| [@LucYTerM](https://github.com/LucYTerM) | Project owner and release maintainer | Active |
| Vacant | Backup maintainer | Recruitment target |
| Vacant | Backup maintainer | Recruitment target |

The project does not currently meet its bus-factor target. Until at least one
backup is appointed, releases and private security response may be delayed.
Vacancies are stated explicitly rather than listing contributors without their
agreement.

## Responsibilities

Maintainers share responsibility for:

- Reviewing changes for correctness, security, privacy, compatibility, tests,
  documentation, and sustainable scope.
- Triaging issues and pull requests according to `docs/support.md`.
- Coordinating private vulnerability reports and supported-version patches.
- Protecting release, package, domain, signing, and service credentials.
- Cutting reproducible releases and verifying downstream packages.
- Maintaining CI, dependencies, documentation, community standards, and the
  published roadmap.
- Recusing themselves from decisions with a material conflict of interest.

No individual is expected to be continuously available. Security and release
duties should have a primary and backup before an LTS line is announced.

## Access Levels

Repository write access is separate from production and security access.
Privileges are granted incrementally and reviewed twice yearly:

1. Triage access after consistent, accurate issue work.
2. Write/review access after sustained code or documentation contributions.
3. Release access after shadowing at least two releases.
4. Security-advisory and signing access only after response-process training.

Use individual accounts, hardware-backed MFA where available, least-privilege
tokens, protected environments, and auditable automation. Shared passwords and
private keys are prohibited.

## Appointment and Departure

New maintainers are nominated in a public governance discussion and approved
by the existing maintainers by consensus. Selection considers technical
judgment, respectful collaboration, review quality, reliability over time, and
security/privacy awareness. Employment or sponsorship is neither required nor
sufficient.

A maintainer may step down at any time. Inactive maintainers are contacted
privately before access is reduced. On departure, owned issues and services are
transferred, repository/service access is removed, and relevant secrets are
rotated. Emergency removal for account compromise or abuse may happen before
public discussion.

## Continuity

A private emergency record should contain current maintainer contact methods,
service owners, domain and package accounts, recovery codes, key-rotation
instructions, and an incident escalation order. Until a backup is appointed,
the project owner keeps encrypted offline recovery copies and documents the
single-person risk. Once appointed, at least two maintainers must be able to
recover the services. The record is checked every six months and after any team
or service change; private contact details never belong in this repository.
