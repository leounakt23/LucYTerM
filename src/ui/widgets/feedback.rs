//! Explicit-consent feedback form and preview action.

use iced::widget::{button, checkbox, column, pick_list, scrollable, text, text_input};

use crate::app::messages::{Message, UiMsg};
use crate::app::state::AppState;
use crate::feedback::FeedbackKind;

const KINDS: [FeedbackKind; 4] = [
    FeedbackKind::BugReport,
    FeedbackKind::FeatureRequest,
    FeedbackKind::GeneralFeedback,
    FeedbackKind::CrashReport,
];

pub fn view(app: &AppState) -> iced::Element<'_, Message> {
    let feedback = &app.feedback;
    let mut form = column![
        text("Send Feedback").size(24),
        text("Nothing is sent automatically. Review the generated preview before choosing GitHub, self-hosted Sentry, or email."),
        pick_list(KINDS, Some(feedback.kind), |kind| {
            Message::Ui(UiMsg::FeedbackKindChanged(kind))
        }),
        text_input("Short summary", &feedback.title)
            .on_input(|value| Message::Ui(UiMsg::FeedbackTitleChanged(value))),
        text_input("What happened, what did you expect, or what would help?", &feedback.description)
            .on_input(|value| Message::Ui(UiMsg::FeedbackDescriptionChanged(value))),
        checkbox("Include recent redacted logs", feedback.consent.include_redacted_logs)
            .on_toggle(|enabled| Message::Ui(UiMsg::FeedbackLogsToggled(enabled))),
        checkbox("Include the last 30 content-free action categories", feedback.consent.include_breadcrumbs)
            .on_toggle(|enabled| Message::Ui(UiMsg::FeedbackBreadcrumbsToggled(enabled))),
        checkbox("Include screenshot from the path below", feedback.consent.include_screenshot)
            .on_toggle(|enabled| Message::Ui(UiMsg::FeedbackScreenshotToggled(enabled))),
        text_input("Screenshot path (optional)", &feedback.screenshot_path)
            .on_input(|value| Message::Ui(UiMsg::FeedbackScreenshotPathChanged(value))),
        text("Never included: session or terminal content, credentials, hostnames, clipboard data, or file contents.").size(12),
        button("Generate private preview")
            .on_press(Message::Ui(UiMsg::ReportIssue))
            .padding(8),
    ]
    .spacing(10);
    if !feedback.preview.is_empty() {
        form = form.push(text("Preview (review before sending)").size(18));
        form = form.push(scrollable(text(&feedback.preview).size(12)).height(240));
    }
    form.padding(20).max_width(760).into()
}
