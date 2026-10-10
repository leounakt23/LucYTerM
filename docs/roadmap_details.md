# Roadmap Details

This document explains why roadmap candidates matter and what must be true
before they ship. Items remain proposals until assigned to a milestone. Every
feature must include tests, documentation, accessibility where applicable,
threat analysis, and a maintenance owner.

## Evaluation Framework

Each proposal is assessed publicly against six criteria:

| Criterion        | Evidence                                                              |
| ---------------- | --------------------------------------------------------------------- |
| User demand      | Unique use cases, GitHub reactions, beta reports, Discussion poll     |
| Vision alignment | Improves secure Linux remote computing without unrelated scope        |
| Complexity       | Design size, dependencies, platform variance, migration work          |
| Maintenance      | Upstream stability, support surface, test infrastructure, owner       |
| Security/privacy | Credentials, trust boundaries, content exposure, audit needs          |
| Performance      | Startup, memory, rendering, latency, binary size, battery/network use |

Scores guide discussion rather than replacing judgment. Reaction counts can be
gamed and naturally favor common workflows, so maintainers also consider
accessibility, underserved users, security, and maintenance capacity. A request
with more than 50 thumbs-up reactions is guaranteed review within the next
quarterly planning round, not implementation.

## Community Planning Cycle

Feature Request Discussions are the canonical place to propose and react to
ideas. Major tradeoffs use time-bounded Discussion polls. Once per quarter,
maintainers also publish an optional survey covering roadmap priorities,
satisfaction, missing workflows, and respondent context; no answer requires
telemetry enrollment. An anonymized result summary and the resulting priority
changes are posted publicly.

The GitHub Projects roadmap board mirrors candidates with `Now`, `Next`,
`Later`, `Declined`, and `Shipped` views. Each card links its Discussion, demand
evidence, owner if any, latest decision, and target only when assigned. The
board is a view of repository metadata, not a second backlog. Maintainers review
requests over 50 thumbs-up reactions each quarter and explain deferrals or
rejections against the evaluation framework.

## v1.1: Migration and Polish

### MobaXterm XML Import

**Value:** lowers the largest switching cost for users with established session
libraries. **Scope:** import documented session names, folders, protocols,
addresses, ports, and safe preferences; never import plaintext credentials.
Unknown fields are reported rather than guessed. Sanitized, user-contributed
fixtures define compatibility.

### PuTTY and KiTTY Import

**Value:** helps Linux users migrate from Windows-managed estates. **Scope:**
support exported registry files and documented KiTTY extensions without reading
a live Windows registry. Host-key caches and saved secrets require a separate
security design and are excluded initially.

### OpenSSH Configuration

**Value:** avoids duplicating hosts, aliases, identities, jumps, and common
options already maintained in `~/.ssh/config`. **Constraints:** preserve OpenSSH
precedence and `Include` behavior, identify unsupported directives, prevent
command execution during preview, and require confirmation before materializing
sessions. `ProxyCommand` execution belongs to the later connectivity review.

### Tab Reordering and Pinning

**Value:** makes long-running multi-session work predictable. Pinned state needs
a small, migration-safe persisted schema; keyboard and pointer reordering must
remain accessible and avoid accidental session closure.

### Regex Terminal Search

**Value:** supports incident response and structured output navigation.
Patterns operate on bounded scrollback snapshots, run away from rendering, and
have time/size limits to prevent denial of service. Search text stays local and
is neither logged nor sent as telemetry.

### Shortcut Customization

**Value:** supports terminal habits, non-US layouts, accessibility tools, and
desktop conflicts. The editor needs conflict detection, reset/export, reserved-
shortcut warnings, and keyboard-only operation. Imported mappings are treated
as data, never commands.

### Security Stack Migration

**Value:** retires the open advisories against the pinned SSH and DNS stack
instead of documenting them indefinitely. **Scope:** stage 1 migrates
`russh` 0.45 to 0.61.1 on the current toolchain, clearing every
high-severity SSH advisory, validated with the full live-server suite;
stage 2 raises the Rust toolchain pin to at least 1.89 and finishes
`russh` 0.63.2, `time` 0.3.47, and the `hickory` 0.24 → 0.26 bump. Exact
blockers and ordering live in `docs/maintenance.md`. Stage 2 amends a
MASTER_PROMPT pin and needs an explicit decision.

## v1.2: Connectivity

### SSH Certificates

Adds OpenSSH certificate/key pairs, expiry display, principal diagnostics, and
agent-backed use. Private keys keep the existing secret-storage guarantees.

### PKCS#11 and Smart Cards

Enables hardware-backed enterprise identities. Delivery depends on a maintained
provider abstraction, PIN handling that never logs or caches by default, token
removal tests, and documented Linux middleware compatibility.

### FIDO2/U2F Keys

Supports modern phishing-resistant SSH credentials. The design must preserve
user-presence/user-verification semantics and provide clear errors for remote
or sandboxed device access.

### SSH Proxies

Implements `ProxyJump` first. Arbitrary `ProxyCommand` is higher risk because it
executes a local command; imported commands remain disabled until explicitly
reviewed and enabled per host.

### AWS SSM Session Manager

Provides an SSH-like transport through the user's installed/authenticated AWS
tooling or a narrowly scoped SDK. Remote App must not become an AWS credential
store, and AWS support cannot become required for normal SSH.

### Kubernetes Port Forwarding

Surfaces namespace/context selection, forwarding state, and cancellation using
`kubectl` initially. Commands are generated from structured fields, contexts are
shown before execution, and no cluster credentials enter application logs.

## v1.3: Productivity

### Command Palette

Makes existing actions discoverable and keyboard-driven. Results contain action
names only; terminal history and content are not indexed.

### Session Groups and Inheritance

Reduces repeated configuration while showing where each effective value came
from. Cycles are rejected, secret fields cannot be inherited accidentally, and
group edits have a preview.

### Snippets

Stores reusable commands with descriptions, variables, confirmation policy,
and provenance. Snippets are never executed merely by selecting or importing
them. Team sharing is a later layer over the same open format.

### Remote File Editing

Downloads to a restricted temporary area, launches an explicitly configured
editor, and uploads only after local change and remote-conflict checks. Cleanup,
symlinks, permissions, reconnects, and sensitive temporary files are release
blockers for this feature.

### Agent Forwarding UI

Provides per-session consent, destination warnings, active-state visibility,
and automatic shutdown. It is off by default because a compromised remote host
can use forwarded agent credentials while the connection is active.

### SFTP Bookmark Folders

Organizes frequently used paths without probing them at startup. Bookmark names
and paths remain encrypted with session data and excluded from telemetry.

## v1.4: Protocol Workflows

- **Mosh:** resilient roaming for unstable networks, subject to maintained Rust
  protocol support or safe external-process integration and UDP diagnostics.
- **Kubernetes exec:** structured pod/container selection and terminal resize,
  with visible context and namespace on every connection.
- **Docker/OCI exec:** local or remote container shells through explicit socket
  permissions; Remote App will not request broad daemon access silently.
- **Bluetooth serial:** Linux BlueZ discovery plus a normal serial transport,
  with pairing delegated to the desktop and no device tracking.
- **WebSocket terminal:** a documented, authenticated, encrypted protocol for
  gateways, with origin, replay, and proxy threat analysis.

## v1.5: Team Workflows

Shared sessions and snippets use reviewable Git repositories and schemas that
cannot contain credentials. Merge conflicts and provenance must be visible.
Role-based access, audit logs, OIDC, and SAML apply only to optional open-source
self-hosted coordination services; they do not gate local client features.
Audit events describe administrative actions without recording terminal,
command, clipboard, credential, or file content.

## v2.0 Architecture Candidates

Split panes and detached windows require a new ownership model for sessions,
renderers, focus, clipboard, and shutdown. They are grouped into v2 because
retrofitting them may break persisted window state and UI APIs.

Terminal sharing and a web gateway introduce multi-party authorization,
end-to-end encryption, consent, abuse controls, and hosted-service obligations.
They require independent RFCs and security review. Session recording uses the
open asciinema format, defaults off, displays a persistent recording indicator,
and requires a redaction/export design.

The WASM plugin system is capability-based: no ambient filesystem, network,
credential, clipboard, or process access. Permissions are declared, reviewed,
and granted per plugin. A registry, if built, is signed and curated; sideloading
remains possible with warnings. "Marketplace" does not imply paid lock-in or
unreviewed executable code.

AI command suggestions are considered only for an explicitly enabled local
model. No prompt, terminal output, session metadata, command history, or file
content is sent to a cloud model. The feature must remain removable and cannot
become necessary for ordinary operation.

Mobile is view-only initially and the web gateway is self-hosted. Both depend
on the same audited sharing protocol rather than separate proprietary services.

## Integration Strategy

IDE integrations should invoke a stable CLI/deep-link interface rather than
embed credentials. GNOME/KDE work uses standard portals and notification APIs.
Ansible and Terraform integrations exchange declarative, secret-free session
metadata; they are not remote-execution backdoors. CI examples verify downloads
and signatures and favor the headless binary.

The existing crates are `mbxt-core`, `mbxt-terminal`, `mbxt-connections`,
`mbxt-storage`, `mbxt-system`, and `mbxt-cli`. Before promising stable library
APIs, the project will document public surfaces, remove application-only types,
define SemVer boundaries, and publish API compatibility tests. A separately
usable SFTP crate may be extracted only when it has consumers and an owner.

## Experiments

Prototype work uses the dedicated `experiments` branch or short-lived
`experiment/*` branches. Users opt in with a narrowly named
`--enable-experimental-*` flag; builds visibly identify experimental behavior.
There is no hidden assignment or telemetry-based A/B test. A comparison may ask
volunteers to select a visible variant such as
`--enable-experimental-example=variant-a`; it never assigns a cohort silently.
If an experiment needs measurement, it follows the normal explicit telemetry
consent and exposes the exact added fields through `--show-telemetry` before
collection.

Every experiment defines success, security stop conditions, a maximum review
date, and cleanup ownership before coding starts. Results are summarized in a
Discussion or blog post. Unsuccessful experiments are removed cleanly, with a
migration or export path for persisted data and no indefinite compatibility
promise beyond the experimental policy.
