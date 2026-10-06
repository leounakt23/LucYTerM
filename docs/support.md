# Support Policy

Support is best-effort and community-driven. Public channels are appropriate
for non-sensitive questions only. Never post credentials, private keys,
session exports, host details, terminal content, or unreviewed diagnostics.

## Channels

- **Documentation:** search the versioned user guide and FAQ first.
- **GitHub Discussions:** questions, feature proposals, roadmap voting,
  workflows, and community help.
- **GitHub Issues:** reproducible bugs and accepted implementation work. New
  feature ideas start in Discussions.
- **Matrix/Discord:** real-time community support only after an official room
  is linked from the repository. Unlisted rooms are unofficial.
- **Security:** private GitHub Security Advisory, as described in
  `docs/security_policy.md`.
- **Email:** `security@<project-domain>` and `contact@<project-domain>` are
  reserved aliases, not active contacts until a verified domain and the exact
  addresses appear in `SECURITY.md` and on the documentation site.

GitHub Discussions is currently the primary support forum. Chat is not a bug
tracker; outcomes that require maintainer work should be summarized in an
issue. There is no guaranteed private product-support channel unless separately
contracted.

## Response Targets

These are operating goals, not an SLA or guarantee:

| Request                  | Target first response     |
| ------------------------ | ------------------------- |
| Critical security report | 24 hours                  |
| Regular bug              | Triaged within one week   |
| Feature request          | Reviewed within two weeks |
| Pull request             | One week                  |

Maintainers may prioritize security, data loss, releases, and incidents over
normal response targets. A response may request information or explain that no
maintainer currently has capacity; it does not promise implementation.

## Bug Triage

Weekly asynchronous triage checks reproducibility, supported versions, impact,
regression status, duplicates, and privacy. Actionable reports receive a
category, priority, and milestone when scheduled. Common labels include
`confirmed`, `needs-info`, `duplicate`, `wontfix`, and `good-first-issue`.

Issues with no activity for 83 days receive a seven-day warning and may close
at 90 days. Confirmed bugs, security work, P0 issues, roadmap items, pinned
issues, and LTS work are exempt. Closure is housekeeping, not a rejection;
users can comment with current reproduction details to request reopening.

## Scope

The project supports the platform matrix in `docs/maintenance.md`. Reports for
other distributions, unsupported server implementations, custom patches, or
end-of-life versions are welcome but may depend entirely on community help.
Maintainers cannot provide emergency access recovery, inspect production
credentials, or guarantee compatibility with proprietary infrastructure.

Paid enterprise support may be offered separately in the future. It will not
paywall core features or reduce public security disclosure obligations.
