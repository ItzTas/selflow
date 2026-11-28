mod types;

use jiff::{
    ZonedDateTime,
    civil::{self, Time},
    tz::TimeZone,
};

use crate::config::{Config, RuleEnd};
use crate::timing::{Seed, instant_on, jitter, logical_date, next_after};

pub use types::RuleWindow;

pub fn windows_for(
    config: &Config,
    seed: Seed,
    date: civil::Date,
    tz: &TimeZone,
) -> Vec<RuleWindow> {
    let Some(set) = config.rule_set_for(date.weekday()) else {
        return Vec::new();
    };
    let exception = config.exception_on(date);
    set.rules
        .iter()
        .filter(|rule| !exception.is_some_and(|exception| exception.covers(&rule.id)))
        .map(|rule| {
            let offset = jitter(seed, &rule.id, date, rule.start_jitter);
            let starts_at = instant_on(date, rule.start, config.day_starts_at, tz)
                .checked_add(offset)
                .expect("jittered start is within the supported range");
            let ends_at = ends_at(config, date, &starts_at, rule.end);
            RuleWindow {
                set: set.name.clone(),
                rule_id: rule.id.clone(),
                label: rule.label.clone(),
                logical_date: date,
                starts_at,
                ends_at,
                notices: rule.notice_offsets(),
                message: rule.message.clone(),
                action: rule.action,
                challenges: rule.challenges.clone(),
                test: false,
                agent: rule.agent.clone(),
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

pub fn windows_around(config: &Config, seed: Seed, now: &ZonedDateTime) -> Vec<RuleWindow> {
    let today = logical_date(now, config.day_starts_at);
    let tz = now.time_zone();
    let mut windows: Vec<RuleWindow> = today
        .yesterday()
        .into_iter()
        .chain(Some(today))
        .flat_map(|date| windows_for(config, seed, date, tz))
        .collect();
    windows.sort_by_key(|window| window.starts_at.timestamp());
    windows
}
