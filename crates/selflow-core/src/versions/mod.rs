mod assemble;
mod clock;
mod compare;
mod constants;
mod types;

use jiff::{SignedDuration, Timestamp};

use crate::config::{Config, day_tag};

pub use assemble::config_at;
pub use clock::{plan_save, rule_since, stored_changes, waiting};
pub use types::{
    ChangeKind, History, SavedConfig, Scope, ShownChange, StoredChange, StoredVersion, TimedChange,
    UnitChange,
};

pub fn diff(old: &Config, new: &Config) -> Vec<UnitChange> {
    compare::rules(old, new)
        .into_iter()
        .chain(compare::settings(old, new))
        .chain(compare::exceptions(old, new))
        .collect()
}

pub fn shown(changes: &[TimedChange]) -> Vec<ShownChange> {
    let mut merged: Vec<(Option<&str>, ShownChange, Vec<&str>)> = Vec::new();
    for timed in changes {
        let entry = ShownChange {
            id: timed.change.id,
            args: timed.change.args.clone(),
            effective_at: timed.effective_at,
        };
        let Scope::Rule { day, rule } = &timed.change.scope else {
            merged.push((None, entry, Vec::new()));
            continue;
        };
        match merged
            .iter_mut()
            .find(|(seen_rule, seen, _)| *seen_rule == Some(rule.as_str()) && *seen == entry)
        {
            Some((_, _, days)) if days.contains(&day_tag(*day)) => {}
            Some((_, _, days)) => days.push(day_tag(*day)),
            None => merged.push((Some(rule), entry, vec![day_tag(*day)])),
        }
    }
    let mut shown: Vec<ShownChange> = merged
        .into_iter()
        .map(|(rule, entry, days)| match rule {
            Some(rule) => with_days(rule, entry, &days),
            None => entry,
        })
        .collect();
    shown.sort_by_key(|entry| entry.effective_at);
    shown
}

fn with_days(rule: &str, entry: ShownChange, days: &[&str]) -> ShownChange {
    let args = [("rule", rule.to_owned()), ("days", days.join(", "))]
        .into_iter()
        .chain(entry.args)
        .collect();
    ShownChange { args, ..entry }
}

fn just_before(time: Timestamp) -> Timestamp {
    time.checked_sub(SignedDuration::from_nanos(1))
        .unwrap_or(Timestamp::MIN)
}
