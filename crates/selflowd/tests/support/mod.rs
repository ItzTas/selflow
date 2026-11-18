#![allow(dead_code)]

use jiff::{
    ZonedDateTime,
    civil::date,
    tz::{self, TimeZone},
};
use selflow_core::enforce::GrantedExtension;

pub fn brt() -> TimeZone {
    TimeZone::fixed(tz::offset(-3))
}

pub fn at(day: i8, hour: i8, minute: i8) -> ZonedDateTime {
    date(2026, 11, day)
        .at(hour, minute, 0, 0)
        .to_zoned_datetime(brt())
        .unwrap()
}

pub const VALID: &str = r#"
language = "pt-BR"
day_starts_at = "05:00"
loosening_delay = "24h"

[anchors]
bedtime = "23:00"
wake = "07:00"

[[rule_set]]
name = "weeknight"
days = ["sun", "mon", "tue", "wed", "thu"]

[rule_set.challenges]
wait = "2m"
reason = true
extensions = 1
extension_length = "20m"

[[rule_set.lock]]
id = "bedtime"
start = "bedtime"
start_jitter = "15m"
end = "wake"
warnings = ["5m", "15m", "1m"]
shutdown_after = "30m"
"#;

pub fn no_jitter() -> String {
    VALID.replace("start_jitter = \"15m\"\n", "")
}

pub fn extension(from: (i8, i8), until: (i8, i8)) -> GrantedExtension {
    GrantedExtension {
        rule_set: "weeknight".to_owned(),
        rule_id: "bedtime".to_owned(),
        logical_date: date(2026, 11, 19),
        granted_at: at(19, from.0, from.1).timestamp(),
        until: at(19, until.0, until.1).timestamp(),
        reason: "talking to my sister".to_owned(),
    }
}
