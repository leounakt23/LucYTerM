---
title: Terminal
---

# Terminal

## Shortcuts

The complete application shortcut list is in [Keyboard Shortcuts](shortcuts.md).
The most common actions are `Ctrl+T` for a tab, `Ctrl+W` to close it, `Ctrl+B`

## Copy, paste, and search

Type directly into the focused terminal tab. `Ctrl+Shift+A` selects the
visible screen, `Ctrl+Shift+C` copies the selection to the clipboard, and
`Ctrl+Shift+V` pastes (bracketed-paste wrapped when the remote end enabled
it). Mouse-drag selection and click-to-cell mouse reporting land with the
interactive terminal widget; until then the encoder honors the remote
mouse mode for programmatic input. Search
matches screen and scrollback content. Avoid pasting secrets into a broadcast
terminal or an untrusted host.

## Customization

Settings control font family, font size, scrollback limit, cursor behavior,
copy-on-select, dark mode, and display scale. Large scrollback limits retain
more history but consume memory; use the performance guide when handling long
running output.
