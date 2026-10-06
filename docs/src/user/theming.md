---
title: Theming
---

# Theming

The application has built-in dark and light theme settings and a terminal color
palette. UI theme code lives in `src/ui/theme.rs`; terminal colors are modeled
separately in the terminal grid and renderer.

To create a custom theme during development, add a named palette in the theme
module, use it from the settings mapping, and add a unit test that verifies
contrast-sensitive colors. Do not place credentials or host-specific values in
a theme file. Theme and font resources should remain immutable and shared by
the UI rather than copied per session.
