mod types;

use jiff::{
    ZonedDateTime,
    civil::{self, Time},
    tz::TimeZone,
};

use crate::config::{Action, Config, RuleEnd};
use crate::timing::{Seed, instant_on, jitter, logical_date, next_after};

pub use types::LockWindow;

pub fn windows_for(
    config: &Config,
    seed: Seed,
    date: civil::Date,
    tz: &TimeZone,
) -> Vec<LockWindow> {
    let Some(set) = config.rule_set_for(date.weekday()) else {
        return Vec::new();
    };
    let exception = config.exception_on(date);
    set.rules
        .iter()
        .filter(|rule| !exception.is_some_and(|exception| exception.covers(&rule.id)))
        .filter_map(|rule| match rule.action {
            Action::Lock { shutdown_after } => Some((rule, shutdown_after)),
            _ => None,
        })
        .map(|(rule, shutdown_after)| {
            let key = format!("{}/{}", set.name, rule.id);
            let offset = jitter(seed, &key, date, rule.start_jitter);
            let starts_at = instant_on(date, rule.start, config.day_starts_at, tz)
                .checked_add(offset)
                .expect("jittered start is within the supported range");
            let ends_at = ends_at(config, date, &starts_at, rule.end);
            LockWindow {
                rule_set: set.name.clone(),
                rule_id: rule.id.clone(),
                label: rule.label.clone(),
                logical_date: date,
                starts_at,
                ends_at,
                warnings: rule.notice_offsets(),
                shutdown_after,
                challenges: rule.challenges.clone(),
                test: false,
            }
        })
        .collect()
}

fn ends_at(
    config: &Config,
    date: civil::Date,
    starts_at: &ZonedDateTime,
    end: RuleEnd,
) -> ZonedDateTime {
    match end {
        RuleEnd::At(end) => next_after(starts_at, end),
        RuleEnd::After(length) => starts_at.saturating_add(length),
        RuleEnd::DayEnd => {
            let next = date.tomorrow().expect("date is within the supported range");
            instant_on(
                next,
                config.day_starts_at,
                Time::midnight(),
                starts_at.time_zone(),
            )
        }
    }
}

pub fn windows_around(config: &Config, seed: Seed, now: &ZonedDateTime) -> Vec<LockWindow> {
    let today = logical_date(now, config.day_starts_at);
    let tz = now.time_zone();
    let mut windows: Vec<LockWindow> = today
        .yesterday()
        .into_iter()
        .chain(Some(today))
        .flat_map(|date| windows_for(config, seed, date, tz))
        .collect();
    windows.sort_by_key(|window| window.starts_at.timestamp());
    windows
}
