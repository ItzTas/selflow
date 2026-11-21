#![allow(dead_code)]

use jiff::{
    ZonedDateTime,
    civil::date,
    tz::{self, TimeZone},
};
use selflow_core::config::parse;
use selflow_core::enforce::{GrantedExtension, NightLog};
use selflow_core::versions::{ChangeKind, History, SavedConfig, StoredChange, plan_save};

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

pub fn history_of(saves: &[(ZonedDateTime, &str)]) -> History {
    let mut history = History::default();
    for (id, (when, text)) in (1..).zip(saves) {
        let config = parse(text).unwrap();
        let saved_at = when.timestamp();
        let mut changes: Vec<StoredChange> = Vec::new();
        for timed in plan_save(&history, &config, saved_at) {
            let unit = timed.change.unit();
            match changes.iter_mut().find(|stored| stored.unit == unit) {
                Some(stored) => stored.kind = looser(stored.kind, timed.change.kind),
                None => changes.push(StoredChange {
                    version_id: id,
                    unit,
                    kind: timed.change.kind,
                    effective_at: timed.effective_at,
                }),
            }
        }
        history.changes.extend(changes);
        history.saved.push(SavedConfig {
            id,
            saved_at,
            config,
        });
    }
    history
}

fn looser(first: ChangeKind, second: ChangeKind) -> ChangeKind {
    [ChangeKind::Loosen, ChangeKind::Neutral]
        .into_iter()
        .find(|kind| [first, second].contains(kind))
        .unwrap_or(ChangeKind::Tighten)
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
