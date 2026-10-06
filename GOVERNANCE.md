# Governance

Remote App uses a Benevolent Dictator For Now (BDFN) model. The project owner
sets technical direction, cuts releases, and enforces community standards,
subject to the rules in this file. All decisions are public by default; only
security reports, conduct reports, and private credentials are handled
confidentially.

## Roles

- **Project owner (BDFN):** [@LucYTerM](https://github.com/LucYTerM). Final
  reviewer for architecture, releases, and governance changes while the team is
  small. Bound by the Code of Conduct like every contributor.
- **Maintainers:** appointed per `MAINTAINERS.md` after sustained, high-quality
  contributions. Two backup-maintainer seats are currently vacant.
- **Moderators:** no separately trained moderator team exists yet (minimum
  target is three before a standing team is claimed). Moderation rests with the
  project owner until then.
- **Contributors:** everyone who improves code, docs, design, tests,
  translations, or community support.

## Decision-Making

- Day-to-day work is decided through reviewed pull requests. Security-sensitive
  changes require explicit maintainer review and updated threat-model
  documentation.
- Significant or architectural changes require an RFC under `rfcs/README.md`.
  Accepted RFCs are binding for the scoped change; implementation still needs
  reviewed PRs and does not reserve release capacity.
- The BDFN may break a deadlock with a written rationale. Any contributor may
  appeal by opening a governance Discussion; appeals are decided in public with
  reasons recorded.
- Changes to this governance file require a public Discussion of at least two
  weeks and are recorded here with the date and rationale.

## Transition to a Steering Committee

When the project has more than five regular contributors (sustained reviews or
merges over at least three months), the owner opens a public transition
Discussion to form a steering committee with written seats, terms, and quorum.
Until that transition is complete, "committee" or "board" language must not be
used to imply shared authority that does not exist.

## Meetings

- **Monthly community call:** proposed, not yet scheduled. When it starts, the
  agenda is posted in advance, the session is recorded, and notes are published.
- **Decision log:** material decisions are recorded in Discussion closings,
  release notes, or `CHANGELOG.md`, not only in call recordings.

## Transparency

Roadmap, RFCs, triage labels, release gates, and funding (when any exists) are
public. Funding does not buy technical decisions, vulnerability suppression, or
access to core features. Employment or sponsorship is neither required nor
sufficient for maintainership.
