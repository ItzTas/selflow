use iced::widget::{Column, Text, button, column, container, text, text_input};
use iced::{Alignment, Element, Length};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_protocol::ipc::RefusalView;

use super::model::{ExtensionArea, FormError, LockEvent, LockModel, Screen};
use super::style;

pub fn lock_screen<'a>(
    model: &LockModel,
    localizer: &Localizer,
    tz: &TimeZone,
) -> Element<'a, LockEvent> {
    let content: Element<'a, LockEvent> = match model.screen() {
        Screen::Connecting => text(localizer.text("lock-brand", &[]))
            .size(style::BRAND_SIZE)
            .color(style::MUTED)
            .into(),
        Screen::Locked {
            label,
            until,
            shutdown_at,
            extension,
        } => column![
            heading(label, until, shutdown_at, localizer, tz),
            extension_area(extension, model, localizer, tz),
        ]
        .spacing(style::SECTION_SPACING)
        .align_x(Alignment::Center)
        .into(),
    };
    container(content).center(Length::Fill).into()
}

fn heading<'a>(
    label: String,
    until: Timestamp,
    shutdown_at: Option<Timestamp>,
    localizer: &Localizer,
    tz: &TimeZone,
) -> Column<'a, LockEvent> {
    let heading = column![
        text(label).size(style::LABEL_SIZE),
        muted(localizer.text("lock-until-caption", &[])),
        text(clock(until, tz)).size(style::TIME_SIZE),
    ]
    .spacing(style::SPACING)
    .align_x(Alignment::Center);
    let Some(shutdown_at) = shutdown_at else {
        return heading;
    };
    heading.push(muted(localizer.text(
        "lock-shutdown",
        &[("time", Arg::Text(clock(shutdown_at, tz)))],
    )))
}

fn extension_area<'a>(
    area: ExtensionArea,
    model: &LockModel,
    localizer: &Localizer,
    tz: &TimeZone,
) -> Element<'a, LockEvent> {
    match area {
        ExtensionArea::NoneLeft => muted(localizer.text("lock-none-left", &[])).into(),
        ExtensionArea::WaitUntil(at) => muted(localizer.text(
            "lock-wait",
            &[("countdown", Arg::Text(countdown(model.now(), at)))],
        ))
        .into(),
        ExtensionArea::Form {
            minutes,
            reason_required,
            phrase,
            error,
            sending,
        } => {
            let mut form = column![muted(
                localizer.text("lock-form-offer", &[("minutes", Arg::Number(minutes))])
            )]
            .spacing(style::SPACING)
            .width(style::FORM_WIDTH)
            .align_x(Alignment::Center);
            if reason_required {
                form = form.push(field(
                    localizer.text("lock-reason-label", &[]),
                    localizer.text("lock-reason-placeholder", &[]),
                    model.reason(),
                    LockEvent::ReasonEdited,
                ));
            }
            if let Some(phrase) = phrase {
                form = form.push(field(
                    localizer.text("lock-phrase-label", &[("phrase", Arg::Text(phrase))]),
                    localizer.text("lock-phrase-placeholder", &[]),
                    model.phrase(),
                    LockEvent::PhraseEdited,
                ));
            }
            if let Some(error) = error {
                form = form.push(muted(form_error_text(&error, localizer, tz)));
            }
            let (label, on_press) = if sending {
                ("lock-asking", None)
            } else {
                ("lock-ask", Some(LockEvent::Submit))
            };
            form.push(
                button(text(localizer.text(label, &[])).size(style::BODY_SIZE))
                    .padding(style::BUTTON_PADDING)
                    .on_press_maybe(on_press),
            )
            .into()
        }
    }
}

fn field<'a>(
    label: String,
    placeholder: String,
    value: &str,
    on_input: fn(String) -> LockEvent,
) -> Column<'a, LockEvent> {
    column![
        text(label).size(style::CAPTION_SIZE),
        text_input(&placeholder, value)
            .on_input(on_input)
            .size(style::BODY_SIZE)
            .padding(style::FIELD_PADDING),
    ]
    .spacing(style::SPACING)
}

fn muted<'a>(line: String) -> Text<'a> {
    text(line)
        .size(style::BODY_SIZE)
        .color(style::MUTED)
        .center()
}

fn form_error_text(error: &FormError, localizer: &Localizer, tz: &TimeZone) -> String {
    let FormError::Refused(refusal) = error else {
        return localizer.text("lock-request-failed", &[]);
    };
    match refusal {
        RefusalView::TooEarly { available_at } => localizer.text(
            "lock-refused-too-early",
            &[("time", Arg::Text(clock(*available_at, tz)))],
        ),
        RefusalView::NoBudget => localizer.text("lock-refused-no-budget", &[]),
        RefusalView::ReasonTooShort { min_chars } => localizer.text(
            "lock-refused-reason-too-short",
            &[("min_chars", Arg::Number(*min_chars as i64))],
        ),
        RefusalView::PhraseMismatch => localizer.text("lock-refused-phrase-mismatch", &[]),
    }
}

fn clock(at: Timestamp, tz: &TimeZone) -> String {
    let zoned = at.to_zoned_datetime(tz.clone());
    format!("{:02}:{:02}", zoned.hour(), zoned.minute())
}

fn countdown(now: Timestamp, at: Timestamp) -> String {
    let left = at - now;
    let seconds = left.as_secs() + i64::from(left.subsec_nanos() > 0);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
