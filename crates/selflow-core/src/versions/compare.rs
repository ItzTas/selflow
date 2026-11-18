use jiff::{
    SignedDuration,
    civil::{Time, Weekday},
};

use super::types::{Change, ChangeKind};
use crate::config::{Challenges, Config, LockRule, RuleSet};

type Args = Vec<(&'static str, String)>;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Finding {
    rule: String,
    change: Change,
}

impl Finding {
    pub(super) fn on(self, days: &[&str]) -> Change {
        let args = [("rule", self.rule), ("days", days.join(", "))]
            .into_iter()
            .chain(self.change.args)
            .collect();
        Change {
            id: self.change.id,
            args,
            kind: self.change.kind,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Side<'c> {
    day_starts_at: Time,
    set: Option<&'c RuleSet>,
}

impl<'c> Side<'c> {
    pub(super) fn of(config: &'c Config, day: Weekday) -> Side<'c> {
        Side {
            day_starts_at: config.day_starts_at,
            set: config.rule_set_for(day),
        }
    }

    fn set_with_locks(&self) -> Option<&'c RuleSet> {
        self.set.filter(|set| !set.locks.is_empty())
    }

    fn locks(&self) -> &'c [LockRule] {
        self.set.map_or(&[], |set| &set.locks)
    }

    fn start(&self, lock: &LockRule) -> SignedDuration {
        after(self.day_starts_at, latest_start(lock))
    }

    fn end(&self, lock: &LockRule) -> SignedDuration {
        self.start(lock) + after(latest_start(lock), lock.end)
    }
}

pub(super) fn day(current: Side, proposed: Side) -> Vec<Finding> {
    let (old_locks, new_locks) = (current.locks(), proposed.locks());
    let mut findings = Vec::new();
    for old in old_locks {
        let changes = match new_locks.iter().find(|new| new.id == old.id) {
            Some(new) => lock_changes(old, new, current, proposed),
            None => vec![change(
                "change-lock-removed",
                ChangeKind::Loosen,
                Vec::new(),
            )],
        };
        findings.extend(about(&old.id, changes));
    }
    for new in new_locks
        .iter()
        .filter(|new| !old_locks.iter().any(|old| old.id == new.id))
    {
        let added = change("change-lock-added", ChangeKind::Tighten, Vec::new());
        findings.extend(about(&new.id, vec![added]));
    }
    findings.extend(challenge_findings(current, proposed));
    findings
}

pub(super) fn settings(old: &Config, new: &Config) -> Vec<Change> {
    [
        (old.day_starts_at != new.day_starts_at).then(|| {
            let args = old_new(old.day_starts_at, new.day_starts_at, clock);
            change("change-day-start", ChangeKind::Loosen, args)
        }),
        (old.loosening_delay != new.loosening_delay).then(|| {
            moved(
                new.loosening_delay > old.loosening_delay,
                ("change-delay-longer", "change-delay-shorter"),
                old_new(old.loosening_delay, new.loosening_delay, friendly),
            )
        }),
        (old.language != new.language).then(|| {
            let args = old_new(&old.language, &new.language, String::clone);
            change("change-language", ChangeKind::Neutral, args)
        }),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn challenge_findings(current: Side, proposed: Side) -> Vec<Finding> {
    let (Some(old), Some(new)) = (current.set_with_locks(), proposed.set_with_locks()) else {
        return Vec::new();
    };
    about(
        &new.name,
        challenge_changes(&old.challenges, &new.challenges),
    )
    .collect()
}

fn lock_changes(old: &LockRule, new: &LockRule, current: Side, proposed: Side) -> Vec<Change> {
    let (old_start, new_start) = (latest_start(old), latest_start(new));
    [
        (old_start != new_start).then(|| {
            moved(
                proposed.start(new) < current.start(old),
                ("change-lock-starts-earlier", "change-lock-starts-later"),
                old_new(old_start, new_start, clock),
            )
        }),
        (old.end != new.end).then(|| {
            moved(
                proposed.end(new) > current.end(old),
                ("change-lock-ends-later", "change-lock-ends-earlier"),
                old_new(old.end, new.end, clock),
            )
        }),
        shutdown(old.shutdown_after, new.shutdown_after),
        (old.warnings != new.warnings).then(|| {
            let args = old_new(&old.warnings, &new.warnings, |warnings| {
                warnings
                    .iter()
                    .map(|warning| friendly(*warning))
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            change("change-warnings", ChangeKind::Neutral, args)
        }),
        (old.label != new.label).then(|| {
            let args = old_new(&old.label, &new.label, String::clone);
            change("change-label", ChangeKind::Neutral, args)
        }),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn challenge_changes(old: &Challenges, new: &Challenges) -> Vec<Change> {
    [
        (old.wait != new.wait).then(|| {
            moved(
                new.wait > old.wait,
                ("change-wait-longer", "change-wait-shorter"),
                old_new(old.wait, new.wait, friendly),
            )
        }),
        (old.reason != new.reason).then(|| {
            moved(
                new.reason,
                ("change-reason-required", "change-reason-dropped"),
                Vec::new(),
            )
        }),
        phrase(old.phrase.as_ref(), new.phrase.as_ref()),
        (old.extensions != new.extensions).then(|| {
            moved(
                new.extensions < old.extensions,
                ("change-fewer-extensions", "change-more-extensions"),
                old_new(old.extensions, new.extensions, |count| count.to_string()),
            )
        }),
        (old.extension_length != new.extension_length).then(|| {
            moved(
                new.extension_length < old.extension_length,
                ("change-shorter-extensions", "change-longer-extensions"),
                old_new(old.extension_length, new.extension_length, friendly),
            )
        }),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn shutdown(old: Option<SignedDuration>, new: Option<SignedDuration>) -> Option<Change> {
    match (old, new) {
        (None, Some(new)) => Some(change(
            "change-shutdown-added",
            ChangeKind::Tighten,
            vec![("new", friendly(new))],
        )),
        (Some(old), None) => Some(change(
            "change-shutdown-removed",
            ChangeKind::Loosen,
            vec![("old", friendly(old))],
        )),
        (Some(old), Some(new)) if old != new => Some(moved(
            new < old,
            ("change-shutdown-sooner", "change-shutdown-later"),
            old_new(old, new, friendly),
        )),
        _ => None,
    }
}

fn phrase(old: Option<&String>, new: Option<&String>) -> Option<Change> {
    match (old, new) {
        (None, Some(new)) => Some(change(
            "change-phrase-added",
            ChangeKind::Tighten,
            vec![("new", new.clone())],
        )),
        (Some(old), None) => Some(change(
            "change-phrase-dropped",
            ChangeKind::Loosen,
            vec![("old", old.clone())],
        )),
        (Some(old), Some(new)) if old != new => Some(change(
            "change-phrase-changed",
            ChangeKind::Loosen,
            old_new(old, new, String::clone),
        )),
        _ => None,
    }
}

fn about(rule: &str, changes: Vec<Change>) -> impl Iterator<Item = Finding> + '_ {
    changes.into_iter().map(move |change| Finding {
        rule: rule.to_owned(),
        change,
    })
}

fn moved(tighter: bool, (tighten, loosen): (&'static str, &'static str), args: Args) -> Change {
    if tighter {
        return change(tighten, ChangeKind::Tighten, args);
    }
    change(loosen, ChangeKind::Loosen, args)
}

fn change(id: &'static str, kind: ChangeKind, args: Args) -> Change {
    Change { id, args, kind }
}

fn old_new<T>(old: T, new: T, show: impl Fn(T) -> String) -> Args {
    vec![("old", show(old)), ("new", show(new))]
}

fn latest_start(lock: &LockRule) -> Time {
    lock.start.wrapping_add(lock.start_jitter)
}

fn after(from: Time, to: Time) -> SignedDuration {
    SignedDuration::from_mins(to.duration_since(from).as_mins().rem_euclid(24 * 60))
}

fn clock(time: Time) -> String {
    time.strftime("%H:%M").to_string()
}

fn friendly(duration: SignedDuration) -> String {
    format!("{duration:#}")
}
