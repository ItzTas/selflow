#![allow(dead_code)]

use jiff::{
    ZonedDateTime,
    civil::date,
    tz::{self, TimeZone},
};
use selflow_core::enforce::{GrantedExtension, NightLog};

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

[[rule_set.rule]]
id = "bedtime"
start = "bedtime"
start_jitter = "15m"
end = "wake"
notices = ["5m", "15m", "1m"]
lock = true
shutdown_after = "30m"
"#;

pub fn no_jitter() -> String {
    VALID.replace("start_jitter = \"15m\"\n", "")
}

pub fn one_extension() -> NightLog {
    NightLog {
        extensions: vec![GrantedExtension {
            rule_set: "weeknight".to_owned(),
            rule_id: "bedtime".to_owned(),
            logical_date: date(2026, 11, 19),
            granted_at: at(19, 23, 5).timestamp(),
            until: at(19, 23, 25).timestamp(),
            reason: "talking to my sister".to_owned(),
        }],
        ..NightLog::default()
    }
}
