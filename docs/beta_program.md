# Remote App Beta Program

## Purpose and Principles

The beta program validates reliability, usability, packaging, and protocol
compatibility on real systems before a general release. Participation never
requires telemetry. Feedback attachments are previewed before submission, and
testers may remove any field or use email instead.

## Phases

### Alpha

Alpha is limited to maintainers and trusted contributors. Builds deploy
continuously from `main` through the nightly workflow and should be stable
enough for daily non-critical use. Alpha users are expected to recover their
configuration from backups and provide detailed diagnostics.

### Closed Beta

Closed beta targets 50 to 200 invited Linux users across distributions,
desktop environments, graphics drivers, and protocol combinations. Candidates
apply through the pinned Beta Applications GitHub Discussion. Maintainers
select a balanced cohort and retain only the GitHub handle, test matrix, and
contact preference needed to operate the program.

Closed-beta releases are signed pre-releases distributed as AppImage and
Flatpak bundles. Invitations are not transferable and do not unlock telemetry
or crash reporting.

### Open Beta

Open beta is public and requires no invitation. Signed builds are available
through every supported distribution channel. Announcements may be posted to
GitHub, relevant Reddit communities, and Hacker News, with known limitations
linked prominently rather than hidden in marketing copy.

## Release Channels

- `stable`: generally available, manually approved production releases.
- `beta`: pre-release tags and builds promoted from the `beta` branch.
- `nightly`: automated snapshots from `main`; useful for verification but not supported for critical work.

Select a channel in Settings or launch with `--channel stable|beta|nightly`.
Changing channels changes only the signed update stream. It does not enable
analytics or crash reporting. Use `--show-telemetry` to inspect the exact
current payload without sending it.

## Feedback and Support

Use the in-app **Send Feedback** form or the beta issue template. The form
supports bug reports, feature requests, general feedback, and crash reports.
System metadata is allow-listed. Redacted logs, content-free breadcrumbs, and
a screenshot each require separate consent and are shown in `preview.md`
before submission.

Testers can join the project-operated Matrix or Discord beta room listed in
the pinned Beta Program Discussion. A public status update and changelog are
posted weekly. Email updates are sent only to testers who separately opt in.

## Triage Commitment

Maintainers triage beta feedback weekly and aim to reply within 48 hours:

- `P0`: data loss, credential exposure, security defect, or release blocker.
- `P1`: major regression or common workflow failure that should be fixed.
- `P2`: limited-impact issue or improvement that may follow general release.

Issues also receive one of `bug`, `ux`, `feature-request`, `docs`, or
`performance`, plus `beta`. Public work is tracked on the Beta GitHub Project;
security reports follow `SECURITY.md` and never enter the public board.

## Success Measures

The public dashboard reports aggregate cohort-level metrics only:

- Crash-free session rate, target greater than 99.5%.
- Aggregate feature adoption counts, never feature content.
- Bug report rate by severity.
- Median and 90th-percentile report-to-fix time.
- An optional, anonymous NPS survey every four weeks.

Small cohorts are suppressed to avoid identifying individuals. Metrics are
generated from GitHub issue metadata and explicitly opted-in Matomo/Plausible
events, and displayed through a simple static report or self-hosted Grafana.

## Recognition

Testers receive early access, roadmap voting in Discussions, and an optional
in-app beta badge. Contributors who consent are thanked in release notes and
`CONTRIBUTORS.md`. Material rewards or swag, when available, never depend on
enabling telemetry or reporting private data.
