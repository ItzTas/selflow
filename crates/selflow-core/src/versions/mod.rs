mod compare;
mod types;

use jiff::Timestamp;

use crate::config::{Config, day_tag};

pub use types::{Change, ChangeKind, Classification, Scope, StoredVersion, UnitChange};

pub fn diff(old: &Config, new: &Config) -> Vec<UnitChange> {
    compare::rules(old, new)
        .into_iter()
        .chain(compare::settings(old, new))
        .chain(compare::exceptions(old, new))
        .collect()
}

pub fn classify(current: &Config, proposed: &Config) -> Classification {
    let mut merged: Vec<(String, Change, Vec<&str>)> = Vec::new();
    let mut others = Vec::new();
    for entry in diff(current, proposed) {
        let change = Change {
            id: entry.id,
            args: entry.args,
            kind: entry.kind,
        };
        let Scope::Rule { day, rule } = entry.scope else {
            others.push(change);
            continue;
        };
        match merged
            .iter_mut()
            .find(|(seen_rule, seen, _)| *seen_rule == rule && *seen == change)
        {
            Some((_, _, days)) => days.push(day_tag(day)),
            None => merged.push((rule, change, vec![day_tag(day)])),
        }
    }
    let changes: Vec<Change> = merged
        .into_iter()
        .map(|(rule, change, days)| on_days(rule, change, &days))
        .chain(others)
        .collect();
    let kind = [ChangeKind::Loosen, ChangeKind::Tighten]
        .into_iter()
        .find(|kind| changes.iter().any(|change| change.kind == *kind))
        .unwrap_or(ChangeKind::Neutral);
    Classification { kind, changes }
}

pub fn effective_at(kind: ChangeKind, saved_at: Timestamp, current: &Config) -> Timestamp {
    if kind != ChangeKind::Loosen {
        return saved_at;
    }
    saved_at
        .checked_add(current.loosening_delay)
        .unwrap_or(Timestamp::MAX)
}

pub fn in_force(versions: &[StoredVersion], now: Timestamp) -> Option<&StoredVersion> {
    versions
        .iter()
        .filter(|version| version.effective_at <= now)
        .max_by_key(|version| (version.effective_at, version.id))
}

pub fn pending(versions: &[StoredVersion], now: Timestamp) -> Option<&StoredVersion> {
    versions
        .iter()
        .filter(|version| version.effective_at > now)
        .max_by_key(|version| version.id)
}

fn on_days(rule: String, change: Change, days: &[&str]) -> Change {
    let args = [("rule", rule), ("days", days.join(", "))]
        .into_iter()
        .chain(change.args)
        .collect();
    Change { args, ..change }
}
