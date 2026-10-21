# Threat Model

This document uses STRIDE against the desktop application boundary. It is a
security design record, not a claim that all residual risk is eliminated.

## Assets

- Master passwords, SSH passwords, private-key passphrases, tokens, and keys.
- Encrypted session data, host names, usernames, ports, and forwarding rules.
- Terminal content, clipboard contents, transfer contents, and logs.
- Host-key trust decisions and the local application configuration.

## STRIDE Analysis

| Category | Threat | Mitigation | Residual risk |
| --- | --- | --- | --- |
| Spoofing | A server impersonates a saved host | russh checks `known_hosts`; unknown and changed keys are rejected unless the user explicitly confirms; optional per-host pins are represented by `security::host_key` | A user can still approve a malicious first-use key; out-of-band fingerprint verification is required |
| Spoofing | Local attacker replaces config or session store | Config directory is owner-only, files are `0600`, writes use `O_NOFOLLOW`, ownership is checked, and writes take an exclusive lock | A privileged/root attacker can bypass these controls |
| Tampering | Session database or encrypted blob is modified | AES-256-GCM authentication tag covers the ciphertext and magic header as AAD; corruption fails closed | Password compromise defeats confidentiality and permits a valid re-encryption |
| Tampering | Dependency or build artifact is compromised | `Cargo.lock`, cargo-audit, cargo-deny, CI checks, and periodic dependency review | A compromised trusted registry, maintainer, or CI runner remains possible |
| Repudiation | Sensitive actions cannot be reconstructed | Structured audit events and security posture report avoid secret values while recording control state | Logs intentionally omit terminal/session data, limiting forensic detail |
| Information disclosure | Passwords, keys, or plaintext sessions leak from memory | `secrecy`, `zeroize`, `Zeroizing`, process core-dump disabling, best-effort memory locking, and redacted `Debug` implementations | Rust allocator copies and OS/kernel pages cannot be completely controlled; memory locking may be denied |
| Information disclosure | Logs expose credentials or terminal escape sequences | Redacted auth debug output, no credential fields in tracing, owner-only log directory | Future log statements must be reviewed; terminal content can still be sensitive |
| Denial of service | Argon2, malformed protocol data, or a malicious server consumes resources | Bounded parsing, async I/O, transfer limits, timeouts, and Argon2 parameters selected for interactive unlock | A hostile peer can consume network/UI resources within configured limits |
| Elevation of privilege | Tool input becomes shell injection | New tools must use `Command::new` with explicit arguments; shell builders are restricted to remote systems and quote arguments | Existing legacy shell paths require continued review; remote shell execution is inherently privileged |

## Cryptographic Choices

Argon2id uses 64 MiB memory, 3 iterations, and parallelism 4. This raises the
cost of offline password guessing while remaining usable for an interactive
desktop unlock on contemporary systems. AES-256-GCM uses a fresh 96-bit OS
random nonce per encryption operation; the nonce is stored with the sealed
record and the authentication tag is part of the ciphertext returned by the
AEAD implementation. Secrets are never generated with a general-purpose RNG.

## Audit Boundaries

The SSH transport owns host-key verification because russh invokes the handler
during handshake. The application security modules expose the user decision,
pinning, idle lock, and posture report without silently overriding transport
policy. Any future relaxed host-key mode must be opt-in, temporary, and
visible in the audit report.

## Known Dependency Advisories

The RustSec database mirrored by `cargo audit` covers only part of the
advisory landscape for the pinned stack. GitHub's advisory service listed,
inventory date 2026-10-10, 21 advisories (6 high, 12 medium, 3 low) against
locked versions:

- `russh` (SSH transport): **migrated 2026-10-15** from 0.45.0 to
  0.61.1, which carried 16 advisories (RUSTSEC-2026-0154 and its
  `russh-cryptovec` twin were RustSec-mirrored); both ignores were
  removed from `.cargo/audit.toml` / `deny.toml`. **0.61.2 → 0.63.1 on
  2026-10-20** cleared two GitHub-only advisories that the RustSec
  mirror does not carry (so `cargo audit` could not see them):
  GHSA-47hw-gvq5-r2gm / CVE-2026-102823 (high 7.5: client channel-scoped
  `Handler` callbacks fire for channel IDs the client never opened) and
  GHSA-p8qx-h547-fjw9 / CVE-2026-102822 (low 3.7: a MAC-requiring block
  cipher negotiated with `mac=none` panics the session task), both
  affecting ≤0.63.0 and patched in 0.63.1. Exposure here was bounded:
  the `Handler` implements no per-channel data/close callbacks (the
  defaults are no-ops), and every client config leaves the MAC
  preference list at russh's default, which never offers `none`.
  0.63.1 is current on the 1.88 toolchain; 0.63.2/0.64.1 need rustc
  ≥1.89 (tracked in `docs/maintenance.md`).
- `hickory-resolver` 0.26.3 / `hickory-proto` 0.26.3: **cleared
  2026-10-21** by the 0.24 → 0.26.3 bump. The resolver advisory
  (GHSA-5j98-2g5x-46v6) affects versions `< 0.26.2`; the O(n²) name
  compression advisory (RUSTSEC-2026-0119) no longer matches the lock.
  Both ignore entries were removed from `.cargo/audit.toml`/`deny.toml`.
- `time` 0.3.47: **cleared 2026-10-21**; the `RUSTSEC-2026-0009`
  stack-exhaustion fix is in this release (requires rustc ≥1.88). The
  ignore was removed.
- `webpki-roots` 1.0.10 / `rustls-webpki` 0.103.16: **cleared
  2026-10-21**. RUSTSEC-2026-0381/-0382 allowed certificate paths
  through distrusted emSign roots/intermediates whose private keys are
  well known; the patched releases remove/reject them. No ignore was
  added. These crates sit on the opt-in crash-reporting/network path.
- `lru` 0.12.5: two unsoundness advisories that `cargo audit` surfaces as
  warnings; transitive through `iced_glyphon` in the pinned GUI stack, not a
  network-facing path.
- `quinn-proto` 0.11.18 (was 0.11.14): five advisories published 2026-06-22 … 2026-09-14
  (RUSTSEC-2026-0185, -0338, -0339, -0340, -0342; minimum fix 0.11.18).
  The crate is recorded in `Cargo.lock` only — reqwest's `rustls-tls`
  feature chain references `quinn?/ring`, which resolves the optional
  dependency without activating it (`cargo tree --all-features -i
  quinn-proto` prints nothing; the release builds never compile it).
  **Cleared 2026-10-20:** the workspace MSRV moved from 1.80 to 1.85
  (the fix's minimum), `Cargo.lock` now carries quinn-proto 0.11.18, and
  the five ignore entries were removed from `.cargo/audit.toml`.
- `rustls` 0.23.45: four advisories published 2026-10-14 during release
  prep (RUSTSEC-2026-0346 … -0349: `Debug` TLS-secret exposure, ECH
  application-data resend, ServerHello downgrade, Brotli decompression
  allocation) — cleared by the 0.23.45 → 0.23.46 bump before tagging.
- `zip` 7.2.0: three extraction advisories published 2026-10-16
  (RUSTSEC-2026-0367, -0368, -0369; one rated 9.1 critical). The crate
  enters only through the opt-in `crash-reporting` feature
  (`symbolic-debuginfo` 12.18.3 pins `zip ^7.2.0`), and the vulnerable
  `extract()` APIs are never called: symbolic only reads archives from
  memory and writes source bundles, so no attacker-controlled archive
  path is materialised on disk. No stable symbolic release allows
  zip ≥ 9 (symbolic 12.18.3 and 13.9.0 still pin `zip ^7.2.0`; only
  14.0.0-alpha pre-releases use a newer zip), so the fix is
  upstream-gated.
- `zbus` 4.x stack (`zbus_names`, `zvariant`, `zvariant_derive`):
  **cleared 2026-10-21** by the `iced` default-features trim, which
  removed `dark-light` 1.1.1 → `zbus` 4.4 from the lock; the three
  unmaintained ignores (RUSTSEC-2026-0370 … -0372) were removed.

Exploitability in this application: the remaining SSH-path advisory
(`rsa`) requires a malicious or compromised SSH peer, or an accepted
host-key mismatch that the default policy rejects; the `zip` advisories
are on the opt-in crash-reporting stack and never call `extract()`; the
`lru` warnings are GUI-local. No remaining advisory gives remote code
execution or credential disclosure. Strict host-key verification and the
opt-in, audit-visible relaxation mode remain the primary mitigations. Exact
fix availability per crate and the ordered migration are tracked in
`docs/maintenance.md`.
