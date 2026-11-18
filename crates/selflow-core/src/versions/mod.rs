mod compare;
mod types;

use jiff::Timestamp;

use crate::config::{Config, DAYS};
use compare::{Finding, Side};

pub use types::{Change, ChangeKind, Classification, StoredVersion};

pub fn classify(current: &Config, proposed: &Config) -> Classification {
    let mut merged: Vec<(Finding, Vec<&str>)> = Vec::new();
    for (tag, day) in DAYS {
        for finding in compare::day(Side::of(current, day), Side::of(proposed, day)) {
            match merged.iter_mut().find(|(seen, _)| *seen == finding) {
                Some((_, days)) => days.push(tag),
                None => merged.push((finding, vec![tag])),
            }
        }
    }
    let changes: Vec<Change> = merged
        .into_iter()
        .map(|(finding, days)| finding.on(&days))
        .chain(compare::settings(current, proposed))
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
