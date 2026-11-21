use std::mem::discriminant;

use jiff::{Timestamp, civil::Weekday};

use super::assemble::config_at;
use super::constants::RULE_FIELDS;
use super::types::{ChangeKind, History, Scope, StoredChange, TimedChange, UnitChange, rule_unit};
use super::{diff, just_before};
use crate::config::{Action, Config, DAYS};

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
    let waiting_units: Vec<&str> = still_waiting(history, saved_at)
        .map(|stored| stored.unit.as_str())
        .collect();
    changed_units(&latest.config, proposed, &judged, &waiting_units)
        .into_iter()
        .flat_map(|(unit, scope, field)| {
            let mut entries: Vec<UnitChange> = judged
                .iter()
                .filter(|entry| entry.unit() == unit)
                .cloned()
                .collect();
            if entries.is_empty() {
                entries.push(canceled(scope, field));
            }
            let effective_at = match loosest(entries.iter().map(|entry| entry.kind)) {
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
    still_waiting(history, at)
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

pub fn stored_changes(planned: &[TimedChange]) -> Vec<StoredChange> {
    let mut stored: Vec<StoredChange> = Vec::new();
    for timed in planned {
        let unit = timed.change.unit();
        match stored.iter_mut().find(|seen| seen.unit == unit) {
            Some(seen) => seen.kind = loosest([seen.kind, timed.change.kind].into_iter()),
            None => stored.push(StoredChange {
                version_id: 0,
                unit,
                kind: timed.change.kind,
                effective_at: timed.effective_at,
            }),
        }
    }
    stored
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

fn changed_units(
    latest: &Config,
    proposed: &Config,
    judged: &[UnitChange],
    waiting: &[&str],
) -> Vec<(String, Scope, &'static str)> {
    let returning = DAYS
        .iter()
        .flat_map(|(_, day)| {
            proposed
                .rules_on(*day)
                .iter()
                .filter(|rule| !latest.rules_on(*day).iter().any(|kept| kept.id == rule.id))
                .flat_map(move |rule| {
                    RULE_FIELDS.iter().map(move |(field, _)| {
                        let scope = Scope::Rule {
                            day: *day,
                            rule: rule.id.clone(),
                        };
                        (rule_unit(*day, &rule.id, field), scope, *field)
                    })
                })
        })
        .filter(|(unit, _, _)| {
            waiting.contains(&unit.as_str()) || judged.iter().any(|entry| entry.unit() == *unit)
        });
    let mut units: Vec<(String, Scope, &'static str)> = Vec::new();
    for (unit, scope, field) in diff(latest, proposed)
        .into_iter()
        .map(|entry| (entry.unit(), entry.scope, entry.field))
        .chain(returning)
    {
        if units.iter().any(|(seen, _, _)| *seen == unit) {
            continue;
        }
        units.push((unit, scope, field));
    }
    units
}

fn canceled(scope: Scope, field: &'static str) -> UnitChange {
    let (id, args) = match &scope {
        Scope::Setting => (
            "change-setting-canceled",
            vec![("setting", field.to_owned())],
        ),
        Scope::Rule { .. } => ("change-canceled", Vec::new()),
        Scope::Exception(date) => (
            "change-exception-canceled",
            vec![("date", date.to_string())],
        ),
    };
    UnitChange {
        scope,
        field,
        id,
        args,
        kind: ChangeKind::Neutral,
    }
}

fn loosest(kinds: impl Iterator<Item = ChangeKind> + Clone) -> ChangeKind {
    [ChangeKind::Loosen, ChangeKind::Neutral]
        .into_iter()
        .find(|kind| kinds.clone().any(|seen| seen == *kind))
        .unwrap_or(ChangeKind::Tighten)
}

fn earliest(
    history: &History,
    unit: &str,
    proposed: &Config,
    saved_at: Timestamp,
    delayed: Timestamp,
) -> Timestamp {
    still_waiting(history, saved_at)
        .filter(|stored| stored.unit == unit)
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

fn still_waiting(history: &History, at: Timestamp) -> impl Iterator<Item = &StoredChange> {
    history
        .changes
        .iter()
        .filter(move |stored| stored.effective_at > at && !overridden(history, stored))
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
    action_at(just_before(time)).is_none_or(|action| discriminant(&action) != discriminant(&after))
}
