---
title: Theming
---

# Theming

The application ships five built-in themes — Light, Dark, Solarized Light,
Solarized Dark, and Rainbow Dark (vibrant high-contrast: near-black
background, saturated cyan/green/pink accents) — plus any custom
palettes found at startup. UI theme code
lives in `src/ui/theme.rs`; terminal colors are modeled separately in the
terminal grid and renderer. The toolbar `theme` button cycles built-ins
then customs; the choice persists in settings and restores on next launch.

## Custom theme files

Drop one `.ron` file per theme into `<config_dir>/themes/` (next to
`config.ron`):

```ron
(name: "Harbor",
 background: "#101018", text: "#e8e8f0", primary: "#8080ff",
 success: "#80ff80", danger: "#ff8080")
```

Colors are `#rgb` or `#rrggbb`. Files that fail to read or parse are
skipped with a startup warning — they never block the launch, and an
unknown selected name falls back to Dark. Do not place credentials or
host-specific values in a theme file.
