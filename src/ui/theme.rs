//! Theme definitions (prompt 1.4).
//!
//! `AppTheme` extends iced's built-in themes with Solarized palettes and a
//! custom slot. Selection: `settings.appearance.dark_theme` picks Light/Dark
//! today; full palette selection via settings UI lands with the settings
//! view (feature matrix #43).

use crate::utils::config::AppConfig;

/// Application theme variants.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AppTheme {
    Light,
    #[default]
    Dark,
    SolarizedLight,
    SolarizedDark,
    /// User-defined palette (settings-driven; placeholder in v1).
    Custom(String),
}

impl AppTheme {
    /// Theme implied by the current settings (prompt 1.2 config).
    pub fn from_settings(settings: &AppConfig) -> Self {
        if settings.appearance.dark_theme {
            Self::Dark
        } else {
            Self::Light
        }
    }

    /// Cycle Light → Dark → Solarized Dark → Solarized Light → back.
    /// (Bound to a toolbar button/shortcut in a later prompt; kept total.)
    pub fn cycle(&self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::SolarizedDark,
            Self::SolarizedDark => Self::SolarizedLight,
            Self::SolarizedLight | Self::Custom(_) => Self::Light,
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
            Self::Custom(name) => name.clone(),
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
        let (background, text, primary, success, danger) = match self {
            Self::SolarizedLight => (
                iced::Color::from_rgb8(0xFD, 0xF6, 0xE3), // base3
                iced::Color::from_rgb8(0x65, 0x7B, 0x83), // base00
                iced::Color::from_rgb8(0x26, 0x8B, 0xD2), // blue
                iced::Color::from_rgb8(0x85, 0x99, 0x00), // green
                iced::Color::from_rgb8(0xDC, 0x32, 0x2F), // red
            ),
            // Solarized Dark and any Custom default to the dark palette.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_settings_follows_dark_flag() {
        let mut settings = AppConfig::default();
        assert_eq!(AppTheme::from_settings(&settings), AppTheme::Dark);
        settings.appearance.dark_theme = false;
        assert_eq!(AppTheme::from_settings(&settings), AppTheme::Light);
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
