# Contributing

Remote App is a community-maintained project with limited reviewer capacity.
All contribution paths below are real, but review speed depends on maintainer
availability. Security and stability work can delay feature review at any time.

## Code of Conduct

Participation is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). The
rules apply equally to maintainers, contributors, and users. Report conduct
concerns through the confidential channel described there, never in a public
issue.

## Ways to Contribute

- **Code:** bug fixes, features with an accepted tracking issue, refactoring
  with `tech-debt` scope. Keep changes small, tested, and documented.
- **Documentation:** fix errors, write guides, improve examples. Docs-only PRs
  follow the normal review process and are welcome.
- **Design:** icons, themes, UI/UX proposals. Open a Discussion first with
  mockups; implementation needs an accepted issue.
- **Testing:** reproduce reported bugs, add tests, run beta checklists, and
  write fuzz harnesses for parsers. See `docs/src/developer/testing.md`.
- **Translation:** Fluent strings live in `locales/en-US/app.ftl`. No Weblate
  instance exists yet; translation PRs are manual and need a native-speaker
  reviewer before merge.
- **Community support:** answer questions in Discussions, confirm reproductions,
  and triage with accurate labels. See `docs/developer/triage.md`.
- **Advocacy:** blog posts, talks, and videos using your own environment. Do
  not present experimental builds as stable releases.
- **Funding:** no GitHub Sponsors or OpenCollective account exists yet. Do not
  send money to anyone claiming to collect for this project until funding
  channels are announced in this file.

## Starting Out

1. Read `docs/onboarding.md` for environment setup and your first build.
2. Look for `good-first-issue` (small, mentored entry tasks) and `help-wanted`
   (tasks where contributor help is genuinely needed). Both labels are
   best-effort; an issue may already be claimed or waiting on design.
3. For a first PR, keep the scope to one behavior change. First-time
   contributors get extra review context, not lower standards.
4. A maintainer or experienced contributor may volunteer as an informal mentor
   when capacity allows. There is no standing mentor roster or guaranteed
   weekly office hours yet; see `docs/community.md` for current status.

## Feature Proposals

New feature ideas start in GitHub Discussions so demand, alternatives, and
tradeoffs stay visible. Do not open an implementation pull request until the
work has an accepted tracking issue. Cross-cutting, security-sensitive, or
long-running changes use the RFC process in `rfcs/README.md` (existing
proposals under `docs/feature_proposals/` remain valid). RFC acceptance does
not guarantee a release or reserve maintainer capacity.

## Setup

Install the pinned Rust toolchain from `rust-toolchain.toml`, Linux development
packages listed in `docs/installation.md`, and optional mdBook/cargo-fuzz tools.
Clone the repository, run `cargo fetch --locked`, and build the workspace.

## Standards

- Format with rustfmt and keep `cargo check --workspace --all-targets` clean.
- Use async I/O; isolate unavoidable blocking work with `spawn_blocking`.
- Keep secrets out of logs, tests, screenshots, fixtures, and commit messages.
- Add focused tests for behavior changes and update the user/developer docs.
- Explain unsafe blocks with a safety comment and keep them platform-isolated.
- Follow `docs/developer/coding_style.md` for Rust style and
  `docs/developer/triage.md` when touching issues.

## Pull Requests

Use a clear Conventional Commit title, describe behavior and security impact,
list validation commands, and include before/after performance data for hot
paths. PRs should be small enough to review and must not include build
artifacts, credentials, or generated `target/` files. First-time PR authors
should complete every item in `.github/PULL_REQUEST_TEMPLATE.md`; maintainers
will explain any unfamiliar step rather than closing the PR for process reasons.

## Commits

Use `feat:`, `fix:`, `docs:`, `perf:`, `security:`, `build:`, `test:`, or
`chore:` prefixes. Keep commits buildable when practical.

## Recognition

Contributors are acknowledged in `CONTRIBUTORS.md` (all-contributors format),
in release notes with consent, and via GitHub's built-in contributor graphs.
There is currently no "Contributor of the Month" program, badge automation, or
swag inventory; those start only when announced here. Recognition never depends
on enabling telemetry.
