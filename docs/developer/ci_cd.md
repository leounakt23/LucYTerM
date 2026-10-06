# CI/CD and Release Automation

## Pipeline

Pull requests run independent workflows so failures are attributable and slow
jobs can be retried separately:

| Workflow | Purpose | Expected limit |
| --- | --- | --- |
| `ci.yml` | Build and test on Ubuntu 22.04/24.04, Fedora 40, and Arch | 15 minutes |
| `lint.yml` | rustfmt, Clippy with warnings denied, rustdoc, and mdBook | 15 minutes |
| `security.yml` | RustSec audit, cargo-deny policy, unsafe-code inventory, SARIF | 20 minutes |
| `coverage.yml` | Workspace LCOV and Codecov regression gate | 20 minutes |
| `bench.yml` | Criterion comparison and PR report | 20 minutes |
| `fuzz.yml` | Five-minute parallel campaigns per target; one hour nightly | 70 minutes |
| `e2e.yml` | Disposable OpenSSH/SFTP and forwarding tests | 15 minutes |

`Swatinem/rust-cache` keys caches from `Cargo.lock`. The lint job also uses
`sccache`; release container builds use BuildKit caches. Workflows have
concurrency groups so a newer push cancels superseded work.

Documentation is built from `docs/` and deployed to GitHub Pages on pushes to
`main`. Set the `PAGES_CUSTOM_DOMAIN` repository variable after DNS has been
configured; the workflow writes the deployed `CNAME` file.

## Branches and Pull Requests

`main` is stable and must remain releasable. Use short-lived `feature/*` and
`hotfix/*` branches; `develop` is available when a larger integration effort
needs it, but normal work follows GitHub Flow.

Configure the `main` ruleset in GitHub to require:

1. A pull request and at least one CODEOWNER approval.
2. Successful CI, Lint, Security, Coverage, and End-to-end checks.
3. Conversation resolution and a branch up to date with `main`.
4. No force pushes or branch deletion.
5. Signed commits or vigilant mode where organizational policy requires it.

Path labels, change size labels, CODEOWNERS, and release notes are automated.
Dependabot opens grouped weekly Cargo, Actions, and Docker updates. Patch
updates are approved and set to auto-merge only after required checks pass;
GitHub's "Allow GitHub Actions to create and approve pull requests" setting
must be enabled for that optional behavior.

## Versioning

Versions use SemVer, including tags such as `v1.4.0-alpha.1`. Prepare a release
from a clean `main` checkout:

```sh
cargo install cargo-release
# Move Unreleased notes into a dated version and update versioned docs first.
cargo release 1.4.0 --execute
git push origin main --follow-tags
```

`release.toml` updates the workspace and internal dependency versions and
creates `vX.Y.Z`. The maintainer must update `CHANGELOG.md` and any versioned
documentation in the same release commit. The tag workflow rejects a tag that
does not exactly match the root package version.

## Release Pipeline

The tag workflow runs the complete test suite before building GNU and musl
binaries for x86-64 and ARM64 through `cross`. Musl archives are the static
distribution. It also creates DEB, RPM, AppImage, and tar archives, generates
CycloneDX SBOMs with `cargo-sbom`, signs every artifact and `SHA256SUMS`, and
publishes a GitHub Release and GHCR image. Pre-release tags use `staging` and
stable tags use `production`.

Configure the `production` GitHub Environment with required reviewers. This
is the manual approval boundary before stable publication. Staging may allow
maintainers to deploy pre-releases without that approval. Package-manager
updates are enabled only when their repository variables are set. Library
crate publication is enabled with `PUBLISH_CRATES=true`.

Required secrets:

| Secret | Access |
| --- | --- |
| `GPG_PRIVATE_KEY`, `GPG_PASSPHRASE` | Artifact signing only |
| `CRATES_IO_TOKEN` | Publish only the `mbxt-*` crates |
| `CODECOV_TOKEN` | Upload coverage |
| `HOMEBREW_TAP_TOKEN` | Dispatch only to the tap repository |
| `FLATHUB_TOKEN` | Dispatch only to the Flathub packaging repository |
| `AUR_SSH_KEY` | Push only to the Remote App AUR repository |
| `AUR_KNOWN_HOSTS` | Reviewed, pinned AUR SSH host keys |
| `RELEASE_WEBHOOK_URL` | Post release notifications |

Optional repository variables are `PUBLISH_CRATES`,
`HOMEBREW_TAP_REPOSITORY`, `FLATHUB_REPOSITORY`, `AUR_REPOSITORY`,
`RELEASE_NOTIFICATIONS`, `RELEASE_DISCUSSION_CATEGORY_ID`, and
`PAGES_CUSTOM_DOMAIN`. `APPIMAGETOOL_SHA256` must contain the reviewed digest
of the pinned release builder downloaded by the workflow.

Use environment-scoped secrets, least-privilege fine-grained tokens, and
annual rotation. Record owners and expiry dates outside the repository.
Security alert email routing is configured in the GitHub organization, not in
a workflow, to avoid exposing distribution lists.

## Post-release Verification

1. Install and launch the DEB, RPM, AppImage, tarball, and static archive.
2. Verify signatures and checksums from a clean machine.
3. Confirm the GHCR version and `latest` tags; pre-releases must not move `latest`.
4. Confirm configured Homebrew, Flathub, and AUR update jobs completed.
5. Check GitHub release notes, Discussions, Pages, and the release webhook.
6. Update website download links if they are not generated from GitHub Releases.
