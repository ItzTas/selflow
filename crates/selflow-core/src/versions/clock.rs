use std::mem::discriminant;

use jiff::{SignedDuration, Timestamp, civil::Weekday};

use super::assemble::config_at;
use super::diff;
use super::types::{ChangeKind, History, Scope, StoredChange, TimedChange, UnitChange, rule_unit};
use crate::config::{Action, Config};

pub fn plan_save(history: &History, proposed: &Config, saved_at: Timestamp) -> Vec<TimedChange> {
    let Some(latest) = history.saved.last() else {
        return diff(&Config::empty(), proposed)
            .into_iter()
            .map(|change| TimedChange {
                change,
                effective_at: saved_at,
            })
            .collect();
    };
    let now = config_at(history, saved_at);
    let judged = diff(&now, proposed);
    let delayed = saved_at
        .checked_add(now.loosening_delay)
        .unwrap_or(Timestamp::MAX);
    changed_units(&latest.config, proposed)
        .into_iter()
        .flat_map(|(unit, first)| {
            let mut entries: Vec<UnitChange> = judged
                .iter()
                .filter(|entry| entry.unit() == unit)
                .cloned()
                .collect();
            if entries.is_empty() {
                entries.push(canceled(&first));
            }
            let effective_at = match loosest(&entries) {
                ChangeKind::Loosen => earliest(history, &unit, proposed, saved_at, delayed),
                ChangeKind::Tighten | ChangeKind::Neutral => saved_at,
            };
            entries.into_iter().map(move |change| TimedChange {
                change,
                effective_at,
            })
        })
        .collect()
}

pub fn waiting(history: &History, at: Timestamp) -> Vec<TimedChange> {
    let now = config_at(history, at);
    history
        .changes
        .iter()
        .filter(|stored| stored.effective_at > at && !overridden(history, stored))
        .flat_map(|stored| {
            let entries = history
                .saved
                .iter()
                .find(|version| version.id == stored.version_id)
                .map(|version| diff(&now, &version.config))
                .unwrap_or_default();
            entries
                .into_iter()
                .filter(|entry| entry.unit() == stored.unit)
                .map(|change| TimedChange {
                    change,
                    effective_at: stored.effective_at,
                })
        })
        .collect()
}

pub fn rule_since(
    history: &History,
    at: Timestamp,
    day: Weekday,
    rule_id: &str,
) -> Option<Timestamp> {
    let units = ["presence", "action"].map(|field| rule_unit(day, rule_id, field));
    let mut times: Vec<Timestamp> = history
        .changes
        .iter()
        .filter(|stored| stored.effective_at <= at && units.contains(&stored.unit))
        .map(|stored| stored.effective_at)
        .collect();
    times.sort_unstable();
    times.dedup();
    times
        .into_iter()
        .rev()
        .find(|time| came_into_force(history, *time, day, rule_id))
}

fn changed_units(latest: &Config, proposed: &Config) -> Vec<(String, UnitChange)> {
    let mut units: Vec<(String, UnitChange)> = Vec::new();
    for entry in diff(latest, proposed) {
        let unit = entry.unit();
        if units.iter().any(|(seen, _)| *seen == unit) {
            continue;
        }
        units.push((unit, entry));
    }
    units
}

fn canceled(first: &UnitChange) -> UnitChange {
    let (id, args) = match &first.scope {
        Scope::Setting => (
            "change-setting-canceled",
            vec![("setting", first.field.to_owned())],
        ),
        Scope::Rule { .. } => ("change-canceled", Vec::new()),
        Scope::Exception(date) => (
            "change-exception-canceled",
            vec![("date", date.to_string())],
        ),
    };
    UnitChange {
        scope: first.scope.clone(),
        field: first.field,
        id,
        args,
        kind: ChangeKind::Neutral,
    }
}

fn loosest(entries: &[UnitChange]) -> ChangeKind {
    [ChangeKind::Loosen, ChangeKind::Neutral]
        .into_iter()
        .find(|kind| entries.iter().any(|entry| entry.kind == *kind))
        .unwrap_or(ChangeKind::Tighten)
}

fn earliest(
    history: &History,
    unit: &str,
    proposed: &Config,
    saved_at: Timestamp,
    delayed: Timestamp,
) -> Timestamp {
    history
        .changes
        .iter()
        .filter(|stored| stored.unit == unit && stored.effective_at > saved_at)
        .filter(|stored| {
            history
                .saved
                .iter()
                .find(|version| version.id == stored.version_id)
                .is_some_and(|version| {
                    !diff(&version.config, proposed)
                        .iter()
                        .any(|entry| entry.unit() == unit && entry.kind == ChangeKind::Loosen)
                })
        })
        .map(|stored| stored.effective_at)
        .fold(delayed, Timestamp::min)
}

fn overridden(history: &History, stored: &StoredChange) -> bool {
    history.changes.iter().any(|later| {
        later.unit == stored.unit
            && later.version_id > stored.version_id
            && later.effective_at <= stored.effective_at
    })
}

fn came_into_force(history: &History, time: Timestamp, day: Weekday, rule_id: &str) -> bool {
    let action_at = |moment| {
        config_at(history, moment)
            .rules_on(day)
            .iter()
            .find(|rule| rule.id == rule_id)
            .map(|rule| rule.action)
    };
    let Some(after) = action_at(time).filter(|action| *action != Action::Nothing) else {
        return false;
    };
    let before = time
        .checked_sub(SignedDuration::from_nanos(1))
        .unwrap_or(Timestamp::MIN);
    action_at(before).is_none_or(|action| discriminant(&action) != discriminant(&after))
}
