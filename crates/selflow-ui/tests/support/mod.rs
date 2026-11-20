#![allow(dead_code)]

use jiff::ZonedDateTime;
use jiff::civil::date;
use jiff::tz::{self, TimeZone};
use selflow_protocol::ipc::LockView;

pub fn brt() -> TimeZone {
    TimeZone::fixed(tz::offset(-3))
}

pub fn at(day: i8, hour: i8, minute: i8) -> ZonedDateTime {
    date(2026, 11, day)
        .at(hour, minute, 0, 0)
        .to_zoned_datetime(brt())
        .unwrap()
}

pub fn view() -> LockView {
    LockView {
        label: "bedtime".into(),
        ends_at: at(20, 7, 0).timestamp(),
        extension_available_at: at(19, 23, 2).timestamp(),
        extensions_left: 1,
        extension_minutes: 20,
        reason_required: true,
        phrase: None,
        shutdown_at: Some(at(19, 23, 30).timestamp()),
        language: "en".into(),
    }
}
