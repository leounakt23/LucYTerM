//! Privacy and update-channel settings.

use iced::widget::{button, checkbox, column, pick_list, text};

use crate::app::messages::{Message, UiMsg};
use crate::app::state::AppState;
use crate::utils::config::ReleaseChannel;

const CHANNELS: [ReleaseChannel; 3] = [
    ReleaseChannel::Stable,
    ReleaseChannel::Beta,
    ReleaseChannel::Nightly,
];

pub fn view(app: &AppState) -> iced::Element<'_, Message> {
    column![
        text("Settings").size(24),
        text("Update channel"),
        pick_list(
            CHANNELS,
            Some(app.settings.general.release_channel),
            |channel| Message::Ui(UiMsg::ReleaseChannelChanged(channel)),
        ),
        text("Changing channels only changes which signed updates are offered. It never enables data collection.").size(12),
        button("Check selected channel for updates")
            .on_press(Message::Ui(UiMsg::CheckForUpdates)),
        checkbox(
            "Share anonymous launch, feature-count, and performance metrics",
            app.settings.general.telemetry_enabled,
        )
        .on_toggle(|enabled| Message::Ui(UiMsg::TelemetryToggled(enabled))),
        checkbox(
            "Send crash reports to the self-hosted Sentry service",
            app.settings.general.crash_reporting_enabled,
        )
        .on_toggle(|enabled| Message::Ui(UiMsg::CrashReportingToggled(enabled))),
        text("Both options are off by default. Feedback attachments always require a separate preview and consent.").size(12),
    ]
    .spacing(12)
    .padding(20)
    .max_width(760)
    .into()
}
