# Remote App Roadmap

This roadmap communicates direction, not a delivery contract. Only issues in a
GitHub release milestone with an assigned maintainer are committed scope.
Version themes and dates may change after design, security, performance, or
maintenance review. Stability and security work can displace feature work at
any time.

This is the planning authority for post-v1 features. The strict parity boundary
in `doc/feature_requirements.md` remains the historical v1 delivery baseline;
it does not commit or prohibit later candidates.

Detailed value propositions and risks are in
[`docs/roadmap_details.md`](https://github.com/LucYTerM/mbxt/blob/main/docs/roadmap_details.md). Public requests and votes
belong in [GitHub Discussions](https://github.com/LucYTerM/mbxt/discussions)
and the repository's public Projects view.

## Now

### v1.1: Migration and Daily-Use Polish

The active cycle focuses on reducing migration friction and improving common
terminal workflows. The candidate pool is deliberately larger than one small
team should promise for a single six-to-eight-week cycle; milestone planning
will select the best-supported items and move the rest to the next minor.

- Import sessions from MobaXterm XML exports.
- Import sessions from PuTTY and KiTTY.
- Parse commonly used `~/.ssh/config` hosts and options.
- Reorder and pin tabs.
- Add regex mode to terminal search with explicit complexity limits.
- Improve keyboard shortcut customization and conflict detection.

Before the v1.1 scope freezes, each importer needs sanitized fixtures and a
published compatibility matrix. Parser inputs must never execute commands or
silently overwrite existing sessions.

## Next

### v1.2: Connectivity

- SSH certificate authentication.
- Smart card and PKCS#11 support.
- FIDO2/U2F-backed SSH keys.
- SSH `ProxyCommand`/`ProxyJump` behavior imported from trusted config.
- AWS Systems Manager Session Manager integration.
- Kubernetes port-forward integration.

### v1.3: Productivity

- Command palette (`Ctrl+Shift+P`).
- Session groups with inherited defaults and visible overrides.
- Reusable snippets library.
- Remote file editing with conflict-aware synchronization.
- SSH agent-forwarding controls with prominent risk warnings.
- Bookmark folders in the SFTP browser.

"Next" means candidate scope for the next one or two releases. It does not mean
every item will ship in v1.2 or v1.3.

## Later

### v1.4: Additional Protocol Workflows

- Mosh support.
- Kubernetes `exec` through an installed `kubectl` or a reviewed native client.
- Docker/OCI container `exec`.
- Serial connections over Bluetooth.
- A documented WebSocket terminal protocol.

### v1.5: Team Workflows

- Shareable session configuration through Git with secret-free schemas.
- Team snippet libraries with review and provenance.
- Role-based access controls for optional self-hosted team services.
- Tamper-evident administrative audit logging.
- OIDC and SAML integration for optional self-hosted services.

Team services must use open formats and protocols. Paid support or hosting may
fund them, but core client features and interoperability will not become
proprietary.

### v2.0 Candidates

- Horizontal and vertical split terminal panes.
- Detached windows and multi-window lifecycle support.
- Consent-driven terminal sharing for pair programming.
- Session recording in asciinema-compatible format.
- Opt-in command suggestions using a local model only.
- A capability-restricted WASM protocol plugin system.
- A view-only iOS/Android companion application.
- A self-hosted web access gateway.
- A serverless connection mode using SSH over a documented WebSocket gateway.
- A signed, curated plugin registry; not an unreviewed marketplace.

### Integrations and Ecosystem

- VS Code and JetBrains actions that open Remote App sessions externally.
- GNOME/KDE jump lists, notifications, portals, and desktop integration.
- Ansible/Terraform workflows based on documented, non-secret schemas.
- GitLab and Jenkins examples for signed headless packages.
- A stable scripting CLI, provisionally named `remote-app-cli`.
- Reusable connection, terminal, and SFTP library APIs.

The repository already publishes or prepares `mbxt-core`, `mbxt-terminal`, and
other `mbxt-*` crates. Renaming them to `remote-app-core`,
`remote-app-terminal`, and `remote-app-sftp` would break consumers and therefore
requires an RFC, transition packages, and a major-version window. API stability
comes before branding.

## Won't Do

- **First-party Windows or macOS native clients:** the small team is focused on
  Linux. WSL, remote Linux environments, community ports, or other clients are
  better options unless sustainable platform maintainers volunteer.
- **Closed or proprietary core features:** core remote-computing functionality
  and file formats remain open. Enterprise funding may purchase support or
  hosting, not exclusive protocol capabilities.
- **Advertising:** no ad-supported monetization, sponsored command results, or
  tracking pixels.
- **Telemetry without explicit consent:** no forced analytics, dark patterns,
  hidden experiment enrollment, or reduced functionality for opting out.
- **Cloud-provider lock-in:** AWS, GCP, Azure, and Kubernetes integrations must
  remain modular; local and standards-based workflows stay first-class.
- **Unrestricted native plugins:** third-party extensions must use a sandboxed,
  capability-based interface. Arbitrary in-process code loading is outside the
  security model.
- **Cloud AI command/content upload:** command suggestions, if pursued, use an
  explicitly enabled local model and never transmit terminal or session content.

## How Priorities Change

Features are scored on visible user demand, alignment with the Linux remote-
computing vision, implementation complexity, ongoing maintenance burden,
security/privacy implications, and performance impact. A request with more than
50 GitHub thumbs-up reactions receives maintainer triage, not automatic
acceptance. Major decisions use Discussion polls and a quarterly community
survey; maintainers publish the rationale when votes cannot be followed.

Large or security-sensitive work starts with an RFC in
`docs/feature_proposals/`. Prototypes live on the `experiments` branch or a
short-lived `experiment/*` branch and behind an explicit
`--enable-experimental-*` flag. Experiments do not silently enroll users or
collect data. Unsuccessful experiments are removed with an export/migration
path for any persisted data and a short public retrospective. Experiment notes
are published on the project blog when available and mirrored in Discussions.
