use jiff::tz::TimeZone;
use jiff::{Timestamp, ZonedDateTime};
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_protocol::ipc::{
    ChangeView, ConfigStatusView, MessageView, PositionedMessage, ReportEntry, ReportView,
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
    saved_at: Timestamp,
    changes: &[ChangeView],
    tz: &TimeZone,
) -> String {
    let saved = localizer.text("cli-saved", &[]);
    if changes.is_empty() {
        return format!("{saved}\n{}", localizer.text("cli-saved-nothing", &[]));
    }
    let (now, later): (Vec<&ChangeView>, Vec<&ChangeView>) = changes
        .iter()
        .partition(|change| change.effective_at <= saved_at);
    let applied = (!now.is_empty())
        .then(|| with_bullets(localizer, localizer.text("cli-saved-now", &[]), &now));
    std::iter::once(saved)
        .chain(applied)
        .chain(by_time(localizer, "cli-saved-later", &later, tz))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn render_config_status(
    localizer: &Localizer,
    status: &ConfigStatusView,
    tz: &TimeZone,
) -> String {
    let saved = match status.last_saved_at {
        Some(at) => localizer.text(
            "cli-status-last-saved",
            &[("when", Arg::Text(format_moment(localizer, at, tz)))],
        ),
        None => localizer.text("cli-status-default", &[]),
    };
    if status.pending.is_empty() {
        return format!("{saved}\n{}", localizer.text("cli-status-no-pending", &[]));
    }
    let pending: Vec<&ChangeView> = status.pending.iter().collect();
    std::iter::once(saved)
        .chain(by_time(localizer, "cli-status-pending", &pending, tz))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn render_report(localizer: &Localizer, report: &ReportView, tz: &TimeZone) -> String {
    let mut lines = vec![localizer.text(
        "cli-report-heading",
        &[("date", Arg::Text(report.logical_date.to_string()))],
    )];
    if report.entries.is_empty() {
        lines.push(localizer.text("cli-report-empty", &[]));
    }
    lines.extend(
        report
            .entries
            .iter()
            .map(|entry| report_line(localizer, entry, tz)),
    );
    if report.pending.is_empty() {
        return lines.join("\n");
    }
    lines.push(localizer.text("cli-report-waiting", &[]));
    lines.extend(report.pending.iter().map(|change| {
        localizer.text(
            "cli-report-waiting-line",
            &[
                (
                    "when",
                    Arg::Text(format_moment(localizer, change.effective_at, tz)),
                ),
                ("text", Arg::Text(message_text(localizer, &change.message))),
            ],
        )
    }));
    lines.join("\n")
}

fn report_line(localizer: &Localizer, entry: &ReportEntry, tz: &TimeZone) -> String {
    let at = entry.at.to_zoned_datetime(tz.clone());
    format!(
        "{:02}:{:02}  {}",
        at.hour(),
        at.minute(),
        message_text(localizer, &entry.message)
    )
}

fn by_time(
    localizer: &Localizer,
    heading: &str,
    changes: &[&ChangeView],
    tz: &TimeZone,
) -> Vec<String> {
    let mut times: Vec<Timestamp> = changes.iter().map(|change| change.effective_at).collect();
    times.sort();
    times.dedup();
    times
        .into_iter()
        .map(|time| {
            let when = Arg::Text(format_moment(localizer, time, tz));
            let group: Vec<&ChangeView> = changes
                .iter()
                .copied()
                .filter(|change| change.effective_at == time)
                .collect();
            with_bullets(
                localizer,
                localizer.text(heading, &[("when", when)]),
                &group,
            )
        })
        .collect()
}

fn with_bullets(localizer: &Localizer, headline: String, changes: &[&ChangeView]) -> String {
    changes
        .iter()
        .map(|change| format!("- {}", message_text(localizer, &change.message)))
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
