use jiff::tz::TimeZone;
use jiff::{Timestamp, ZonedDateTime};
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_protocol::ipc::{
    ChangeKindView, ConfigStatusView, MessageView, PositionedMessage, ReportEntry, ReportView,
};

const WEEKDAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

pub fn message_text(localizer: &Localizer, message: &MessageView) -> String {
    let args: Vec<(&str, Arg)> = message
        .args
        .iter()
        .map(|(name, value)| (name.as_str(), Arg::Text(value.clone())))
        .collect();
    localizer.text(&message.id, &args)
}

pub fn render_errors(localizer: &Localizer, errors: &[PositionedMessage]) -> String {
    errors
        .iter()
        .map(|error| {
            localizer.text(
                "cli-error-line",
                &[
                    ("line", Arg::Number(error.line as i64)),
                    ("column", Arg::Number(error.column as i64)),
                    ("text", Arg::Text(message_text(localizer, &error.message))),
                ],
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn render_saved(
    localizer: &Localizer,
    kind: ChangeKindView,
    effective_at: Timestamp,
    changes: &[MessageView],
    tz: &TimeZone,
) -> String {
    let when = format_moment(localizer, effective_at, tz);
    let headline = match kind {
        ChangeKindView::Tighten => localizer.text("cli-saved-tighten", &[]),
        ChangeKindView::Loosen => localizer.text("cli-saved-loosen", &[("when", Arg::Text(when))]),
        ChangeKindView::Neutral => localizer.text("cli-saved-neutral", &[]),
    };
    with_bullets(localizer, headline, changes)
}

pub fn render_config_status(
    localizer: &Localizer,
    status: &ConfigStatusView,
    tz: &TimeZone,
) -> String {
    let in_force = match status.in_force_saved_at {
        Some(at) => localizer.text(
            "cli-status-in-force",
            &[("when", Arg::Text(format_moment(localizer, at, tz)))],
        ),
        None => localizer.text("cli-status-default", &[]),
    };
    let Some(pending) = &status.pending else {
        return format!(
            "{in_force}\n{}",
            localizer.text("cli-status-no-pending", &[])
        );
    };
    let headline = localizer.text(
        "cli-status-pending",
        &[(
            "when",
            Arg::Text(format_moment(localizer, pending.effective_at, tz)),
        )],
    );
    format!(
        "{in_force}\n{}",
        with_bullets(localizer, headline, &pending.changes)
    )
}

pub fn render_report(localizer: &Localizer, report: &ReportView, tz: &TimeZone) -> String {
    let heading = localizer.text(
        "cli-report-heading",
        &[("date", Arg::Text(report.logical_date.to_string()))],
    );
    if report.entries.is_empty() {
        return format!("{heading}\n{}", localizer.text("cli-report-empty", &[]));
    }
    let lines = report
        .entries
        .iter()
        .map(|entry| report_line(localizer, entry, tz));
    std::iter::once(heading)
        .chain(lines)
        .collect::<Vec<_>>()
        .join("\n")
}

fn report_line(localizer: &Localizer, entry: &ReportEntry, tz: &TimeZone) -> String {
    if entry.message.id == "report-pending" {
        let when = format_moment(localizer, entry.at, tz);
        return localizer.text("report-pending", &[("when", Arg::Text(when))]);
    }
    let at = entry.at.to_zoned_datetime(tz.clone());
    format!(
        "{:02}:{:02}  {}",
        at.hour(),
        at.minute(),
        message_text(localizer, &entry.message)
    )
}

fn with_bullets(localizer: &Localizer, headline: String, changes: &[MessageView]) -> String {
    changes
        .iter()
        .map(|change| format!("- {}", message_text(localizer, change)))
        .fold(headline, |text, line| format!("{text}\n{line}"))
}

fn format_moment(localizer: &Localizer, at: Timestamp, tz: &TimeZone) -> String {
    let zoned: ZonedDateTime = at.to_zoned_datetime(tz.clone());
    let weekday = WEEKDAYS[zoned.weekday().to_monday_zero_offset() as usize];
    localizer.text(
        "cli-moment",
        &[
            (
                "weekday",
                Arg::Text(localizer.text(&format!("weekday-short-{weekday}"), &[])),
            ),
            ("day", Arg::Number(i64::from(zoned.day()))),
            (
                "month",
                Arg::Text(localizer.text(&format!("month-short-{}", zoned.month()), &[])),
            ),
            (
                "time",
                Arg::Text(format!("{:02}:{:02}", zoned.hour(), zoned.minute())),
            ),
        ],
    )
}
