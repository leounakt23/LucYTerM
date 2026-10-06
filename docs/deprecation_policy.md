# Deprecation Policy

Remote App avoids breaking stable workflows without notice. A deprecation is
used when an interface, configuration field, protocol option, package, or
behavior needs replacement for maintainability, security, or correctness.

## Notice Period

Normal deprecations are announced at least two minor releases before removal.
The first release adds a runtime or build warning where practical, release
notes, documentation, and a migration guide. The next minor release retains
the old behavior and repeats the warning. Removal may occur in the following
major release after the two-minor notice period.

An announcement identifies the deprecated behavior, reason, replacement,
first deprecated version, earliest removal version, compatibility effects, and
an issue where users can report migration blockers.

## Compatibility

Deprecated configuration remains readable and is migrated without destroying
the original when possible. Deprecated CLI options continue to work with a
clear stderr warning. Persisted formats need explicit migration and rollback
tests. Automation-friendly warnings must not contaminate machine-readable
stdout.

Migration guides include before-and-after examples, package-specific steps,
rollback limitations, and any privacy or security changes. LTS releases retain
deprecated behavior for their documented support lifetime unless keeping it
would expose users to a serious vulnerability.

## Exceptions

Maintainers may remove or disable behavior sooner when it creates an actively
exploited vulnerability, risks credential or data loss, violates legal
requirements, or depends on an upstream component that can no longer be
safely distributed. The security advisory or release notes must explain the
shortened timeline, affected versions, mitigations, and available migration.

Experimental features explicitly marked unstable may change without the full
notice period, but their data formats still receive a safe migration or an
explicit export path. Nightly behavior is not a compatibility commitment.

## Process

Deprecations require a tracking issue, release milestone, documentation owner,
and removal checklist. Removal PRs link the original announcement and verify
that the replacement has been available for two minor releases. Usage metrics
may inform timing only when collected under the opt-in privacy policy; lack of
telemetry is never treated as proof that nobody uses a feature.
