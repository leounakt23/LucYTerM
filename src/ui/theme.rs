//! Theme definitions (prompt 1.4) plus user-supplied palettes.
//!
//! Built-ins (Light/Dark/Solarized) always exist. Custom themes load from
//! `<config_dir>/themes/*.ron` at startup; the toolbar theme button cycles
//! built-ins then customs. Selection persists in
//! `settings.appearance.theme` (empty = follow the legacy `dark_theme`
//! flag, so old configs keep working).

use crate::utils::config::AppConfig;

/// Application theme variants.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum AppTheme {
    Light,
    #[default]
    Dark,
    SolarizedLight,
    SolarizedDark,
    /// User-supplied palette loaded from a theme file.
    Custom {
        name: String,
        palette: iced::theme::Palette,
    },
}

impl AppTheme {
    /// Theme implied by the current settings plus loaded customs.
    /// Unknown names fall back to Dark (documented, never panics).
    pub fn from_settings(settings: &AppConfig, customs: &[AppTheme]) -> Self {
        let name = settings.appearance.theme.trim();
        if name.is_empty() {
            return if settings.appearance.dark_theme {
                Self::Dark
            } else {
                Self::Light
            };
        }
        match name.to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            "solarized-light" => Self::SolarizedLight,
            "solarized-dark" => Self::SolarizedDark,
            other => customs
                .iter()
                .find(|theme| theme.name().eq_ignore_ascii_case(other))
                .cloned()
                .unwrap_or(Self::Dark),
        }
    }

    /// Cycle Light → Dark → Solarized Dark → Solarized Light → back.
    /// (Bound to a toolbar button/shortcut in a later prompt; kept total.)
    pub fn cycle(&self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::SolarizedDark,
            Self::SolarizedDark => Self::SolarizedLight,
            Self::SolarizedLight => Self::Light,
            Self::Custom { .. } => Self::Light,
        }
    }

    /// Cycle through built-ins then `customs` (toolbar theme button).
    /// Unknown current themes restart at Dark.
    pub fn cycle_with(&self, customs: &[AppTheme]) -> Self {
        let mut order = vec![
            Self::Light,
            Self::Dark,
            Self::SolarizedDark,
            Self::SolarizedLight,
        ];
        order.extend(customs.iter().cloned());
        let current = order.iter().position(|theme| theme.name() == self.name());
        match current {
            Some(index) => order[(index + 1) % order.len()].clone(),
            None => Self::Dark,
        }
    }

    /// Canonical settings name (`theme` field round-trips through this).
    pub fn canonical_name(&self) -> String {
        match self {
            Self::Light => "light".to_string(),
            Self::Dark => "dark".to_string(),
            Self::SolarizedLight => "solarized-light".to_string(),
            Self::SolarizedDark => "solarized-dark".to_string(),
            Self::Custom { name, .. } => name.clone(),
        }
    }

    /// Toggle between the two "appearance" modes driven by settings.
    pub fn toggle(&self) -> Self {
        match self {
            Self::Light => Self::Dark,
            _ => Self::Light,
        }
    }

    /// Human-readable name (also used as the iced custom theme name).
    pub fn name(&self) -> String {
        match self {
            Self::Light => "Light".into(),
            Self::Dark => "Dark".into(),
            Self::SolarizedLight => "Solarized Light".into(),
            Self::SolarizedDark => "Solarized Dark".into(),
            Self::Custom { name, .. } => name.clone(),
        }
    }

    /// Map to the iced theme used by the runtime. Built-ins pass through;
    /// the rest become named custom themes with our palettes.
    pub fn to_iced(&self) -> iced::Theme {
        match self {
            Self::Light => iced::Theme::Light,
            Self::Dark => iced::Theme::Dark,
            other => iced::Theme::custom(other.name(), other.palette()),
        }
    }

    /// Palette for custom variants (Solarized Ethan Schoonover values).
    pub fn palette(&self) -> iced::theme::Palette {
        if let Self::Custom { palette, .. } = self {
            return *palette;
        }
        let (background, text, primary, success, danger) = match self {
            Self::SolarizedLight => (
                iced::Color::from_rgb8(0xFD, 0xF6, 0xE3), // base3
                iced::Color::from_rgb8(0x65, 0x7B, 0x83), // base00
                iced::Color::from_rgb8(0x26, 0x8B, 0xD2), // blue
                iced::Color::from_rgb8(0x85, 0x99, 0x00), // green
                iced::Color::from_rgb8(0xDC, 0x32, 0x2F), // red
            ),
            // Solarized Dark (and Light/Dark, unused — they map to
            // iced built-ins in to_iced) default to the dark palette.
            _ => (
                iced::Color::from_rgb8(0x00, 0x2B, 0x36), // base03
                iced::Color::from_rgb8(0x93, 0xA1, 0xA1), // base1
                iced::Color::from_rgb8(0x26, 0x8B, 0xD2), // blue
                iced::Color::from_rgb8(0x85, 0x99, 0x00), // green
                iced::Color::from_rgb8(0xCB, 0x4B, 0x16), // orange/red
            ),
        };
        iced::theme::Palette {
            background,
            text,
            primary,
            success,
            danger,
        }
    }

    /// Whether the theme reads as dark. Built-ins are known; custom
    /// palettes are measured by background luminance. Keeps the legacy
    /// `dark_theme` flag meaningful when a custom palette takes over.
    /// (`palette()` is only meaningful for custom variants — Light and
    /// Dark map to iced built-ins in `to_iced` — so this does not read
    /// through it for them.)
    pub fn is_dark(&self) -> bool {
        match self {
            Self::Dark | Self::SolarizedDark => true,
            Self::Light | Self::SolarizedLight => false,
            Self::Custom { palette, .. } => {
                let background = palette.background;
                0.2126 * background.r + 0.7152 * background.g + 0.0722 * background.b < 0.5
            },
        }
    }
}

/// One user-supplied palette file (`<config_dir>/themes/*.ron`):
/// `(name: "...", background: "#rrggbb", text: "#rrggbb",
/// primary: "#rrggbb", success: "#rrggbb", danger: "#rrggbb")`.
/// `#rgb` shorthand is accepted. No secrets or host data belong here.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomThemeFile {
    pub name: String,
    #[serde(default)]
    pub background: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub primary: String,
    #[serde(default)]
    pub success: String,
    #[serde(default)]
    pub danger: String,
}

/// Parse `#rgb` / `#rrggbb` into an iced color.
pub fn parse_hex_color(text: &str) -> Result<iced::Color, String> {
    let hex = text
        .trim()
        .strip_prefix('#')
        .ok_or_else(|| format!("color {text:?} must look like #rgb or #rrggbb"))?;
    let full: String = if hex.len() == 3 {
        hex.chars().flat_map(|digit| [digit, digit]).collect()
    } else {
        hex.to_string()
    };
    if full.len() != 6 || !full.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("color {text:?} must look like #rgb or #rrggbb"));
    }
    let channel = |index: usize| {
        u8::from_str_radix(&full[index..index + 2], 16)
            .map_err(|_| format!("color {text:?} must look like #rgb or #rrggbb"))
    };
    Ok(iced::Color::from_rgb8(
        channel(0)?,
        channel(2)?,
        channel(4)?,
    ))
}

impl CustomThemeFile {
    /// Validate into a theme (empty names and bad colors are errors,
    /// reported per file by the loader — never a panic).
    pub fn into_theme(self) -> Result<AppTheme, String> {
        if self.name.trim().is_empty() {
            return Err("custom theme needs a non-empty name".to_string());
        }
        Ok(AppTheme::Custom {
            name: self.name.trim().to_string(),
            palette: iced::theme::Palette {
                background: parse_hex_color(&self.background)?,
                text: parse_hex_color(&self.text)?,
                primary: parse_hex_color(&self.primary)?,
                success: parse_hex_color(&self.success)?,
                danger: parse_hex_color(&self.danger)?,
            },
        })
    }
}

/// Load every `*.ron` palette in `dir`, sorted by name. A missing dir is
/// normal (first run) and yields no themes and no errors; unreadable or
/// invalid files are skipped with one message each.
pub fn load_custom_themes(dir: &std::path::Path) -> (Vec<AppTheme>, Vec<String>) {
    let mut themes = Vec::new();
    let mut errors = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return (themes, errors),
    };
    let mut files: Vec<std::path::PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "ron"))
        .collect();
    files.sort();
    for path in files {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                errors.push(format!("theme {name}: cannot read ({error})"));
                continue;
            },
        };
        let file: CustomThemeFile = match ron::from_str(&text) {
            Ok(file) => file,
            Err(error) => {
                errors.push(format!("theme {name}: invalid RON ({error})"));
                continue;
            },
        };
        match file.into_theme() {
            Ok(theme) => themes.push(theme),
            Err(error) => errors.push(format!("theme {name}: {error}")),
        }
    }
    themes.sort_by_key(|theme| theme.name());
    (themes, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_settings_follows_dark_flag() {
        let mut settings = AppConfig::default();
        assert_eq!(AppTheme::from_settings(&settings, &[]), AppTheme::Dark);
        settings.appearance.dark_theme = false;
        assert_eq!(AppTheme::from_settings(&settings, &[]), AppTheme::Light);
    }

    #[test]
    fn from_settings_resolves_names_and_falls_back() {
        let customs = vec![AppTheme::Custom {
            name: "Harbor".into(),
            palette: iced::theme::Palette {
                background: iced::Color::from_rgb8(0x10, 0x10, 0x20),
                text: iced::Color::WHITE,
                primary: iced::Color::from_rgb8(0x80, 0x80, 0xFF),
                success: iced::Color::from_rgb8(0x80, 0xFF, 0x80),
                danger: iced::Color::from_rgb8(0xFF, 0x80, 0x80),
            },
        }];
        let mut settings = AppConfig::default();
        settings.appearance.theme = "solarized-dark".into();
        assert_eq!(
            AppTheme::from_settings(&settings, &customs),
            AppTheme::SolarizedDark
        );
        settings.appearance.theme = "HARBOR".into();
        assert_eq!(AppTheme::from_settings(&settings, &customs), customs[0]);
        settings.appearance.theme = "no-such-theme".into();
        assert_eq!(AppTheme::from_settings(&settings, &customs), AppTheme::Dark);
    }

    #[test]
    fn cycle_with_visits_customs_in_order() {
        let harbor = AppTheme::Custom {
            name: "Harbor".into(),
            palette: AppTheme::Dark.palette(),
        };
        let customs = vec![harbor.clone()];
        assert_eq!(AppTheme::Light.cycle_with(&customs), AppTheme::Dark);
        // ... Dark -> SolarizedDark -> SolarizedLight -> Harbor -> Light.
        let mut theme = AppTheme::Light;
        for _ in 0..5 {
            theme = theme.cycle_with(&customs);
        }
        assert_eq!(theme, AppTheme::Light);
        assert_eq!(harbor.cycle_with(&customs), AppTheme::Light);
        assert_eq!(harbor.canonical_name(), "Harbor");
    }

    #[test]
    fn hex_colors_parse_strictly() {
        assert_eq!(
            parse_hex_color("#ff0000").unwrap(),
            iced::Color::from_rgb8(0xFF, 0, 0)
        );
        assert_eq!(
            parse_hex_color("#f00").unwrap(),
            iced::Color::from_rgb8(0xFF, 0, 0)
        );
        assert!(parse_hex_color("ff0000").is_err());
        assert!(parse_hex_color("#ff00").is_err());
        assert!(parse_hex_color("#gggggg").is_err());
        assert!(parse_hex_color("").is_err());
    }

    #[test]
    fn loader_skips_bad_files_and_sorts() {
        let dir = std::env::temp_dir().join(format!(
            "mbxt-theme-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("b.ron"),
            "(name: \"B\", background: \"#111111\", text: \"#eeeeee\", primary: \"#2222ff\", success: \"#22ff22\", danger: \"#ff2222\")",
        )
        .expect("write");
        std::fs::write(
            dir.join("a.ron"),
            "(name: \"A\", background: \"#111111\", text: \"#eeeeee\", primary: \"#2222ff\", success: \"#22ff22\", danger: \"#ff2222\")",
        )
        .expect("write");
        std::fs::write(dir.join("broken.ron"), "(name: ").expect("write");
        std::fs::write(
            dir.join("badcolor.ron"),
            "(name: \"Bad\", background: \"red\", text: \"#eeeeee\", primary: \"#2222ff\", success: \"#22ff22\", danger: \"#ff2222\")",
        )
        .expect("write");
        std::fs::write(dir.join("notes.txt"), "ignored").expect("write");
        let (themes, errors) = load_custom_themes(&dir);
        assert_eq!(
            themes.iter().map(|theme| theme.name()).collect::<Vec<_>>(),
            vec!["A".to_string(), "B".to_string()]
        );
        assert_eq!(errors.len(), 2, "broken + badcolor: {errors:?}");
        std::fs::remove_dir_all(&dir).ok();
        // Missing dir is first-run normal: silent and empty.
        let (themes, errors) = load_custom_themes(&dir.join("does-not-exist"));
        assert!(themes.is_empty() && errors.is_empty());
    }

    #[test]
    fn dark_detection_follows_background() {
        assert!(AppTheme::Dark.is_dark());
        assert!(!AppTheme::Light.is_dark());
        assert!(AppTheme::SolarizedDark.is_dark());
        assert!(!AppTheme::SolarizedLight.is_dark());
    }

    #[test]
    fn toggle_matches_settings_flag() {
        assert_eq!(AppTheme::Dark.toggle(), AppTheme::Light);
        assert_eq!(AppTheme::Light.toggle(), AppTheme::Dark);
        assert_eq!(AppTheme::SolarizedDark.toggle(), AppTheme::Light);
    }

    #[test]
    fn cycle_visits_all_variants() {
        let mut theme = AppTheme::Light;
        let mut seen = vec![theme.clone()];
        for _ in 0..4 {
            theme = theme.cycle();
            seen.push(theme.clone());
        }
        assert!(seen.contains(&AppTheme::SolarizedDark));
        assert_eq!(theme, AppTheme::Light);
    }

    #[test]
    fn palettes_differ_between_variants() {
        assert_ne!(AppTheme::Dark.palette(), AppTheme::SolarizedLight.palette());
    }
}
