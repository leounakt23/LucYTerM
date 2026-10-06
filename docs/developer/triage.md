# Issue Triage

Triage is asynchronous and best-effort. Goals, not SLAs: critical security
reports within 24 hours, regular bugs within one week, feature requests within
two weeks, PRs within one week. Security and data-loss work preempts targets.

## Labels

- Kind: `bug`, `ux`, `feature-request`, `docs`, `performance`, `beta`,
  `tech-debt`, `enhancement`.
- Priority: `P0` (data loss, credential exposure, release blocker), `P1`
  (major regression), `P2` (limited impact).
- State: `confirmed`, `needs-info`, `duplicate`, `wontfix`, `good-first-issue`,
  `help-wanted`, `in-progress`, `roadmap`, `pinned`, `lts`, `security`.
- Stale automation warns at 83 days and closes at 90 days; `confirmed`,
  `security`, `P0`, `pinned`, `roadmap`, `lts`, and `in-progress` issues and
  all PRs are exempt.

## Steps

1. Reproduce or request a minimal repro: version, distribution, install format,
   protocol, steps, sanitized logs. Strip credentials and terminal content.
2. Search for duplicates before labeling `confirmed`. Link the canonical issue.
3. Assign kind plus priority; add a milestone only when scheduled. New feature
   ideas belong in Discussions until accepted.
4. Route security content to `SECURITY.md` privately; never triage exploits in
   public.
5. Close housekeeping issues kindly: explain the reason, link the policy, and
   describe how to reopen with current details.

## Good Triage Comments

State what was checked, what is still needed, and who owns the next step.
Newcomers confirming bugs should say which version they tested and what they
observed; guesses without testing are labeled as such.
