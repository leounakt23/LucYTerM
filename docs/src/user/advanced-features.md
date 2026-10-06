---
title: Advanced Features
---

# Advanced Features

## Port forwarding

SSH sessions can define local (`-L`), remote (`-R`), and dynamic SOCKS
forwarding rules. Configure the bind address, listen port, destination host,
and destination port in the forwarding panel. Bind to loopback unless another
machine must reach the listener.

## X11 forwarding

Enable X11 forwarding per SSH session. The local display must be available and
the remote host must have an X11 application and authentication setup. X11
exposes a broad client surface, so enable it only for trusted hosts.

## Macros

Press `Ctrl+Shift+R` to begin recording terminal input and press it again to
stop. Macros preserve key events and timing, can use variables, and can be
bound to `F5` through `F8`. Review a macro before replaying it against a
production host.

## Multi-exec

Press `Ctrl+Shift+M`, select target sessions, and type into the broadcast
terminal. Target indicators show which sessions are armed. Disable broadcast
mode after use to prevent accidental fan-out.
