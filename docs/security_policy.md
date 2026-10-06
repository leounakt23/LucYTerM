# Security Response Policy

## Private Reporting

Do not disclose an unpatched vulnerability in a public issue, discussion, or
chat room. Use the repository's **Report a vulnerability** button to open a
private GitHub Security Advisory:

`https://github.com/LucYTerM/mbxt/security/advisories/new`

Private vulnerability reporting must be enabled in repository settings before
v1.0 is declared operational. If the link returns 404, no working private
channel is configured: do not send vulnerability details publicly. Open a
detail-free support discussion asking the owner to establish a private channel.
This is a release blocker, not a reporter responsibility.

Include affected versions, platform, prerequisites, impact, a minimal proof of
concept, and whether credentials or private content may be exposed. Use
disposable credentials and redact logs. The project credits reporters unless
they request anonymity.

The project intends to provide `security@<project-domain>` and
`contact@<project-domain>` aliases once a verified domain and at least two
recipients are configured. Until those exact addresses are published in
`SECURITY.md`, they must not be assumed operational. GitHub Security Advisories
is the authoritative private channel.

## Response Targets

Critical security reports are targeted for acknowledgment within 24 hours. An
initial response confirms receipt, not validity or a fix deadline. Maintainers
then assess severity, affected versions, exploitability, data exposure, and
whether credentials should be rotated. Capacity or emergencies may delay a
response; reporters may request a status update through the private advisory.

## Coordinated Disclosure

The default disclosure window is 90 days from acknowledgment. Maintainers and
the reporter may shorten it after a fix is broadly available or extend it when
users need more deployment time and active exploitation is not occurring.
Active exploitation, public disclosure, or imminent user harm may require an
accelerated advisory.

Investigation and remediation happen in the advisory's private fork. Access is
limited to people needed for the response. Maintainers avoid vulnerability
details in public commits, CI logs, chat, project boards, and issue titles
until coordinated publication.

## Severity and Remediation

- Critical: practical credential disclosure, remote code execution, signing or
  update compromise, or widespread unrecoverable data loss.
- High: significant confidentiality, integrity, authentication, or sandbox
  bypass with realistic prerequisites.
- Medium/Low: limited impact, strong prerequisites, defense-in-depth, or
  information exposure without sensitive content.

Fixes are prepared for the latest stable line and every supported LTS line
affected. Other supported versions receive a patch when technically safe; if a
backport is too risky, the advisory states the required upgrade. Critical
confirmed issues target a tested patch release within 48 hours, but safety and
verification take priority over an artificial deadline.

## Advisory and CVE

Before publication, maintainers prepare affected-version ranges, CVSS
rationale, workarounds, upgrade instructions, credits, and package status.
GitHub Security Advisories is used to request a CVE when the issue affects a
released version and meets CVE criteria. Duplicate CVEs are avoided by
coordinating with affected upstream projects and distributions.

The release process builds signed artifacts, checksums, and SBOMs from the
patched tag. Maintainers notify relevant package channels without sharing
embargoed details beyond trusted contacts. After availability is confirmed,
the advisory and CVE are published together with the changelog entry.

## Follow-up

Critical incidents receive a public, blameless post-mortem after users have had
reasonable time to update. It covers timeline, impact, contributing technical
and process factors, detection, remediation, and tracked prevention work.
Secrets, exploit details that still endanger supported users, and reporter
identity are omitted unless disclosure is appropriate and consented.

Security response data is retained only as needed for coordination, legal
obligations, and lessons learned. Repository access, signing keys, service
tokens, and the private emergency contact list are reviewed after an incident.
