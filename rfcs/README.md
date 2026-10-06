# RFCs

Architectural and other significant changes are decided through public Request
for Comments documents. Small bug fixes, docs-only changes, and work already
covered by an accepted tracking issue do not need an RFC.

Existing proposals under `docs/feature_proposals/` remain valid and follow this
same process. New RFCs should be opened here under `rfcs/`.

## When an RFC Is Required

- New protocols, transports, or authentication mechanisms.
- Changes crossing trust boundaries (credentials, secrets, sandboxing).
- New dependencies, services, or hosted components.
- Persisted, public API, protocol, or configuration-format changes.
- Work likely to span more than one release cycle.

## Process

1. Start with a Feature Request Discussion. After a maintainer confirms an RFC
   is useful, copy `rfcs/0000-template.md` to `rfcs/NNNN-short-name.md`
   (`NNNN` assigned during review) and open a pull request.
2. **Public discussion period:** at least two weeks from the `Review` announcement.
3. **Final comment period:** at least one week after maintainers post a proposed
   decision. Material design changes restart the final comment period.
4. Maintainers record the decision, rationale, and dissent in the RFC and close
   the Discussion. Community votes are evidence, not the sole mechanism.
5. Accepted RFCs name an implementation owner, tracking issue, milestone
   candidate, and review date. Acceptance does not reserve capacity or promise
   a release.

## Status

| RFC                      | Title                       | Status   | Decision |
| ------------------------ | --------------------------- | -------- | -------- |
| [0000](0000-template.md) | Template (do not implement) | Template | —        |

Statuses: `Draft`, `Review`, `Accepted`, `Rejected`, `Withdrawn`, `Superseded`.
Security-sensitive details belong in a private GitHub Security Advisory, never
in an RFC. Experimental follow-ups use `--enable-experimental-*` flags per the
roadmap policy.
