//! Combined multi-exec output view (Prompt 5.1): interleaved per-session
//! lines with name prefixes, newest at the bottom.
//!
//! Pure view over `AppState::multi_exec_log` — rendering caps at the newest
//! 200 lines so a flooding session cannot stall the UI; the full 500-line
//! ring stays available to the next repaint.

use iced::widget::{column, container, scrollable, text};

use crate::app::messages::Message;
use crate::app::state::AppState;

/// Rows rendered per repaint (the ring holds more).
pub const RENDER_LINES: usize = 200;

/// Combined output pane.
pub fn view(app: &AppState) -> iced::Element<'_, Message> {
    let total = app.multi_exec_log.len();
    let start = total.saturating_sub(RENDER_LINES);

    let mut list = column![].spacing(1);
    if app.multi_exec_mode.enabled {
        list = list.push(text("⚠ multi-exec armed — keystrokes fan out to every target").size(12));
    }
    if total == 0 {
        list = list.push(text("no broadcast output yet — type in a targeted terminal").size(12));
    }
    for (session, line) in app.multi_exec_log.iter().skip(start) {
        let name = app
            .session(*session)
            .map(|s| s.spec.name.as_str())
            .unwrap_or("?");
        list = list.push(
            text(format!("[{name}] {line}"))
                .size(12)
                .font(iced::Font::MONOSPACE),
        );
    }

    let body = column![
        text(format!("Multi-exec output ({total} lines)")).size(16),
        scrollable(list).height(iced::Fill),
    ]
    .spacing(6);

    container(body)
        .width(iced::Fill)
        .height(iced::Fill)
        .padding(12)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> AppState {
        let base = std::env::temp_dir().join(format!("mbxt-mev-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        AppState::new(crate::utils::config::AppConfig::default(), paths).0
    }

    #[test]
    fn empty_and_full_views_build() {
        let app = test_app();
        let _ = view(&app);

        let mut full = test_app();
        full.multi_exec_mode.enabled = true;
        for i in 0..300 {
            full.multi_exec_log.push_back((1, format!("line {i}")));
        }
        let _ = view(&full);
    }
}
