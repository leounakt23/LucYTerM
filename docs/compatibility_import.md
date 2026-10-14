# Session Import Compatibility Matrix (v1.1)

One-way import: MobaXterm XML, PuTTY/KiTTY `.reg`, OpenSSH `ssh_config` →
new sessions in the encrypted store. Native `.enc` export/import
round-trips separately and is always supported.

Dispositions: **I** = imported · **W** = ignored + warning (in the preview
report) · **R** = rejected/dropped + warning (row kept in the report) ·
**D** = dropped password-like material + warning (never silent) ·
**—** = not applicable.

Merge rule (all sources): **merge-by-name, no overwrite**. Collisions skip
with a rename suggestion (`<name> (imported)`, `<name> (imported 2)`, …).
`ProxyCommand` and auto-commands are parsed as opaque text and stay
**disabled by default** — never executed during preview or import.

## Session-level fields

| Field | MobaXterm XML | PuTTY/KiTTY `.reg` | OpenSSH `ssh_config` |
| --- | --- | --- | --- |
| Session name | I (missing → R) | I from key last component (missing → R) | I from `Host` alias (multi-pattern → I + W) |
| Protocol | I + map; unknown → R | I (`Protocol`; absent → ssh + W) | Always SSH (config is SSH-only) |
| Host/address | I; empty → R | I (`HostName`); empty → R | I (`HostName`, else alias if host-shaped, else R) |
| Port | I (bad → default + W) | I (`PortNumber` dword; bad → default + W) | I (bad → 22 + W; first wins) |
| Username | I | I (`UserName`) | I (first wins) |
| Auth (safe subset) | I (agent/key-path/none; passwords → D) | I (same; `AgentFwd` → forward off + W) | I (`IdentityFile` first → `KeyFile`; extras → W) |
| Tags/folders | I (folder path → tags) | I (documented group hints) | I (adjacent `# group:` comments, best-effort + W) |
| Notes | I (description fields) | I (comment fields) | I (provenance line + proxy-disabled notice) |
| X11 forwarding | I when explicit bool | W (mapped when present; else off) | I (`ForwardX11[Trusted]` yes → on) |
| Port forwards | I (well-formed `-L`/`-D` shapes; bad → W-drop) | W (version-skewed → reported) | I (well-formed; malformed → W-drop) |
| Serial params | I (device + baud; invalid → R) | I (protocol mapped; line params unset — configure device after import) | — |
| `ProxyJump` | I as data, disabled + W | W (proxy values reported per key, not mapped — no silent guess) | I as data, disabled + W |
| `ProxyCommand`/auto/Remote/Local commands | R-value (disabled flag + notes hint) + W | R-value (same) + W | R-value (same) + W — never executed |
| Passwords/saved secrets/host-key caches | D + W, never stored | D + W; host-key cache out of scope | D + W |
| Unknown attributes/directives/values | W per key (capped, remainder counted) | W per value (same cap) | W per directive (same cap) |
| Duplicate names in one import | R-later (first kept) | R-later (first kept) | R-later (first kept) |

## OpenSSH directive detail (first wins; later duplicates → W)

| Directive | Disposition |
| --- | --- |
| `Host`, `HostName`, `User`, `Port`, `IdentityFile` (first), `ForwardX11[Trusted]` | I |
| `Include` (relative, inside import root) | I (followed, depth ≤ 8) |
| `Include` absolute / outside root / glob miss | W (import continues) |
| `Match` (all forms incl. `exec`) | W (block not materialized) |
| `ProxyJump` | I as data, disabled + W |
| `ProxyCommand` | R-value + quarantine + W |
| `LocalForward` / `RemoteForward` / `DynamicForward` | I when well-formed, else W-drop |
| `ForwardAgent` | Parsed → forced off + W |
| `LocalCommand`, `PermitLocalCommand`, `RemoteCommand`, side effects | R-value + W; preview never runs them |
| Tuning prefs (`Compression`, `ServerAlive*`, `ConnectTimeout`, …) | W (recorded, mapped only where a field exists) |
| Any other directive | W with file:line |

## Verification

Per-source fixtures under `tests/fixtures/import/` with integration tests
in `crates/storage/tests/import_*.rs` assert this matrix row by row,
including: precedence + `Include`, merge collisions (no overwrite),
password-drop per source, proxy-quarantine no-exec (a
`touch /tmp/mbxt-import-must-not-run`-shaped command asserts the file is
absent), and cap/oversize behavior. Fixture hygiene: RFC 5737 TEST-NET-1
hosts only; `grep -rIinE 'password|passwd|secret|BEGIN.*PRIVATE'`
`tests/fixtures/import/` hits only `*_drop*`/`*_secret*` fixtures whose
values are `REDACTED`/`example`.
