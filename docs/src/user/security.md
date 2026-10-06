---
title: Security
---

# Security

Remote App uses an encrypted session store with Argon2id key derivation and
AES-256-GCM encryption. Argon2id uses 64 MiB memory, 3 iterations, and
parallelism 4. Each encryption operation receives a fresh OS-generated 96-bit
nonce.

SSH host keys are checked against `known_hosts`. Unknown and changed keys must
be explicitly reviewed. Agent forwarding is a per-session decision and should
remain disabled unless required.

The desktop keyring is opt-in. The master password is not logged, and secret
buffers are zeroized where ownership permits. Core dumps are disabled during
startup when the platform supports it. Idle locking and clipboard clearing are
available through security settings; configure a non-zero idle timeout on
shared workstations.

Read `docs/threat_model.md` and `SECURITY.md` before deploying the application
in a privileged or multi-user environment.
