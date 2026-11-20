mod types;

use jiff::{ZonedDateTime, civil, tz::TimeZone};

use crate::config::Config;
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
    set.locks
        .iter()
        .map(|rule| {
            let key = format!("{}/{}", set.name, rule.id);
            let offset = jitter(seed, &key, date, rule.start_jitter);
            let starts_at = instant_on(date, rule.start, config.day_starts_at, tz)
                .checked_add(offset)
                .expect("jittered start is within the supported range");
            let ends_at = next_after(&starts_at, rule.end);
            LockWindow {
                rule_set: set.name.clone(),
                rule_id: rule.id.clone(),
                label: rule.label.clone(),
                logical_date: date,
                starts_at,
                ends_at,
                warnings: rule.warnings.clone(),
                shutdown_after: rule.shutdown_after,
                challenges: set.challenges.clone(),
                test: false,
            }
        })
        .collect()
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
