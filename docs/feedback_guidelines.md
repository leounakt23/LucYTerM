# Feedback Guidelines

## Before Reporting

Search existing issues and test the newest build in your selected channel.
Separate unrelated problems. Security vulnerabilities must be reported
privately according to `SECURITY.md`.

## Useful Reports

Include:

1. A short, specific title.
2. Reproduction steps using disposable hosts and credentials.
3. Expected and actual behavior.
4. Version, channel, distribution, display server, and installation format.
5. Whether the issue is a regression and the last known working version.
6. Frequency and impact, including whether data may be lost.
7. A reviewed feedback preview if it adds useful evidence.

Choose bug report, feature request, general feedback, or crash report in the
in-app form. For UX feedback, describe the goal and where the workflow became
unclear. For requests, explain the use case before proposing an implementation.

## Privacy Review

The application does not send feedback automatically. Optional logs are
limited and conservatively redacted; breadcrumbs contain action categories,
not parameters. Automated redaction is not a guarantee. Before sending:

- Remove session names, hostnames, addresses, usernames, and organization names.
- Remove terminal, command, clipboard, and file content.
- Remove passwords, tokens, cookies, private keys, and authentication headers.
- Crop or redact screenshots and window titles.
- Do not attach encrypted stores or configuration directories.

Use the generated `preview.md` as the authoritative review surface. You may
submit it through GitHub, use the displayed email fallback, or provide no
diagnostics at all.

## Triage

Reports receive `beta` plus a category (`bug`, `ux`, `feature-request`, `docs`,
or `performance`) and priority (`P0`, `P1`, or `P2`). Maintainers target an
initial response within 48 hours and review the public Beta Project weekly.
Constructive follow-up and confirmation on fixed builds are especially useful.
