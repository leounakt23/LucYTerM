---
title: Extending Remote App
---

# Extending Remote App

## New protocol

Add protocol-neutral types to `crates/core`, implement the `Connection` trait
in `crates/connections`, register the factory match arm, map transport events
into the existing event bus, and add an integration fixture. Keep credentials
and native handles inside the connection layer.

## New network tool

Add a module under `src/tools`, expose typed parameters/events in
`src/tools/mod.rs`, use explicit process arguments for local commands, and
provide parser tests plus a remote execution path where appropriate. Never
interpolate untrusted input into a local shell.

## New theme

Add immutable palette data to `src/ui/theme.rs`, map it through settings, and
test text/background contrast. Share theme data rather than cloning it per
widget.

## New localization string

Add a stable key to `locales/en-US/app.ftl`, use the localization lookup in the
UI, and add a fallback English value. Do not use translated text as a protocol
value, command, or security decision.
