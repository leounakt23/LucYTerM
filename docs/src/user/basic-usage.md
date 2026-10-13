---
title: Basic Usage
---

# Basic Usage

## SSH and terminal sessions

Select a saved session or create a new one. Once connected, the terminal owns
the input focus. Output is parsed into the terminal grid and rendered through
the native renderer. Disconnecting a tab stops its session actor and associated
forwarding/transfer tasks.

Authentication can use a password, private key, SSH agent, or keyboard
interactive prompts. Passwords are not written to logs.

## Terminal input

Type normally into the focused terminal tab. `Ctrl+Shift+A` selects the
visible screen, `Ctrl+Shift+C` copies the selection to the clipboard, and
`Ctrl+Shift+V` pastes (bracketed-paste wrapped when the remote end enabled
it). Mouse-drag selection and click-to-cell mouse reporting land with the
interactive terminal widget. Multi-exec mode can broadcast input to selected
sessions; keep it disabled when typing commands intended for one host.

## Headless session operations

The headless binary operates on the same encrypted store:

```text
remote-app-headless list-sessions
remote-app-headless export-sessions backup.enc
remote-app-headless import-sessions backup.enc
remote-app-headless completions bash
remote-app-headless reset-config
```

Commands requiring the master password prompt without echo. The `connect`
subcommand is currently a CLI routing placeholder; use the GUI for interactive
connections.
