# Beta Testing Checklist

Record the application version, release channel, distribution, desktop,
display server, installation format, and relevant protocols. Never use
production credentials or systems containing irreplaceable data.

## Installation and Startup

- [ ] Verify the signature and `SHA256SUMS` before installation.
- [ ] Install on a fresh supported system using AppImage or Flatpak.
- [ ] Confirm first launch creates only documented configuration files.
- [ ] Restart, upgrade, downgrade where supported, and uninstall cleanly.
- [ ] Switch among stable, beta, and nightly update channels.
- [ ] Run `remote-app --show-telemetry` and verify no identifying fields appear.

## Sessions and Protocols

- [ ] Create, edit, connect, disconnect, and delete SSH sessions.
- [ ] Exercise every other compiled protocol available to you: Telnet, serial, VNC, and RDP.
- [ ] Verify host-key prompts, failed authentication, reconnect, and timeout behavior.
- [ ] Confirm session names, hosts, usernames, and terminal content never appear in feedback previews.

## File Transfer

- [ ] Upload and download a small text file and verify its checksum.
- [ ] Transfer a multi-gigabyte test file and verify progress and checksum.
- [ ] Pause/cancel transfers and disconnect during transfer.
- [ ] Exercise overwrite, permissions, missing paths, and low-disk-space failures.

## Forwarding and X11

- [ ] Test local (`-L`), remote (`-R`), and dynamic SOCKS (`-D`) forwarding.
- [ ] Confirm forwards stop when their session disconnects.
- [ ] Test X11 forwarding with a harmless GUI application such as `xclock`.
- [ ] Verify failure messaging when X11/Wayland display access is unavailable.

## Productivity Features

- [ ] Record, edit, save, replay, cancel, import, and export a macro.
- [ ] Verify secret macro variables are never displayed or logged.
- [ ] Run multi-exec against at least five disposable sessions.
- [ ] Confirm disconnected targets and destructive command confirmation behave safely.
- [ ] Switch light/dark themes and restart to verify persistence.

## Security and Resilience

- [ ] Create a master password, restart, unlock, change it, and test a wrong password.
- [ ] Verify encrypted stores contain no plaintext credentials or session details.
- [ ] Test clipboard clearing and idle lock behavior.
- [ ] Simulate latency, loss, duplication, and reordering with a disposable network namespace:

```sh
sudo tc qdisc add dev eth0 root netem delay 250ms 100ms loss 5% duplicate 1% reorder 2%
# Run reconnect, terminal, forwarding, and transfer checks.
sudo tc qdisc del dev eth0 root
```

- [ ] Generate a feedback preview with each optional attachment combination.
- [ ] Review every line before submission and verify turning consent off removes the attachment.
- [ ] If opted into crash reporting, use only the documented test-crash procedure on disposable data.

## Report

Submit one issue per defect using the beta template. Include expected and
actual behavior, reproducible steps, severity, regression status, and the
reviewed preview. Attach screenshots only after checking every visible window,
notification, title, and terminal region for sensitive data.
