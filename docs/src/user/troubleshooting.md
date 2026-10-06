---
title: Troubleshooting
---

# Troubleshooting

## SSH host-key warning

Compare the displayed fingerprint with a trusted inventory or the host
administrator. Do not delete `known_hosts` as a first response; investigate a
changed key for host replacement or reinstallation.

## Authentication fails

Check username, key path, agent availability, and server authentication policy.
Use `RUST_LOG=remote_app=debug` for diagnostic flow without enabling payload
logging. Never attach logs containing terminal output or credentials.

## Serial access is denied

On Linux, verify the user is in the group that owns the serial device (often
`dialout`) and reconnect after changing group membership.

## Transfers stall

Check network latency and remote SFTP limits. Lower pipeline depth, verify the
remote path, and retry; resumable transfers preserve a valid prefix.

## Display or Wayland problems

Confirm the platform display/socket permissions and graphics driver. For
packaged builds, test the matching Flatpak portal or Snap plug permissions.

## Reset local configuration

Export sessions first, then use `remote-app-headless reset-config`. This removes
the local config and encrypted store; keyring cleanup is also attempted.
