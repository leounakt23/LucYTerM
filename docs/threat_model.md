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

- `russh` 0.61.1 (SSH transport): **migrated 2026-10-15** from 0.45.0,
  which carried 16 advisories (RUSTSEC-2026-0154 and its
  `russh-cryptovec` twin were RustSec-mirrored). 0.61.1 clears the
  high-severity set on the pinned toolchain and both ignores were removed
  from `.cargo/audit.toml` / `deny.toml`; a 0.63.2 follow-up (rustc
  ≥1.89) would clear the remainder.
- `hickory-resolver` 0.24.4: the lookup APIs obscure DNSSEC validation
  failures (GHSA-5j98-2g5x-46v6). Only explicitly configured resolvers are
  queried and DNSSEC is not enabled.
- `hickory-proto` 0.24.4: O(n^2) name compression (mirrored as
  RUSTSEC-2026-0119, ignored with reason).
- `time` 0.3.41: stack-exhaustion denial of service during parsing (mirrored
  as RUSTSEC-2026-0009, ignored with reason); those parsing entry points are
  not fed attacker-controlled input.
- `lru` 0.12.5: two unsoundness advisories that `cargo audit` surfaces as
  warnings; transitive through `iced_glyphon` in the pinned GUI stack, not a
  network-facing path.
- `quinn-proto` 0.11.14: five advisories published 2026-06-22 … 2026-09-14
  (RUSTSEC-2026-0185, -0338, -0339, -0340, -0342; minimum fix 0.11.18).
  The crate is recorded in `Cargo.lock` only — reqwest's `rustls-tls`
  feature chain references `quinn?/ring`, which resolves the optional
  dependency without activating it (`cargo tree --all-features -i
  quinn-proto` prints nothing; the release builds never compile it) — and
  the minimum fix requires rustc ≥ 1.85, above the workspace MSRV 1.80
  that the MSRV-aware resolver enforces. All five are ignore-listed with
  that reason in `.cargo/audit.toml`.
- `rustls` 0.23.45: four advisories published 2026-10-14 during release
  prep (RUSTSEC-2026-0346 … -0349: `Debug` TLS-secret exposure, ECH
  application-data resend, ServerHello downgrade, Brotli decompression
  allocation) — cleared by the 0.23.45 → 0.23.46 bump before tagging.

Exploitability in this application: every reachable advisory requires a
malicious or compromised SSH peer, or an accepted host-key mismatch that the
default policy rejects. The impact class is client-side denial of service —
memory exhaustion or panics inside the SSH session task — not remote code
execution or credential disclosure. Strict host-key verification and the
opt-in, audit-visible relaxation mode remain the primary mitigations. Exact
fix availability per crate and the ordered migration are tracked in
`docs/maintenance.md`.
