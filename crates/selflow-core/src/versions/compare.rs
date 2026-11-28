use std::collections::HashSet;

use jiff::{
    SignedDuration,
    civil::{Time, Weekday},
};

use super::types::{Change, ChangeKind, Scope, UnitChange, agent_field};
use crate::agent::folders_cover;
use crate::config::{Action, AgentStep, Challenges, Config, DAYS, Exception, Rule, RuleEnd};

type Args = Vec<(&'static str, String)>;
type Field = (&'static str, Option<Change>);

#[derive(Clone, Copy)]
struct Side<'r> {
    rule: &'r Rule,
    day_starts_at: Time,
}

pub(super) fn rules(old: &Config, new: &Config) -> Vec<UnitChange> {
    DAYS.iter()
        .flat_map(|(_, day)| on_day(old, new, *day))
        .collect()
}

pub(super) fn settings(old: &Config, new: &Config) -> Vec<UnitChange> {
    let fields: [Field; 4] = [
        (
            "day_starts_at",
            (old.day_starts_at != new.day_starts_at).then(|| {
                let args = old_new(old.day_starts_at, new.day_starts_at, clock);
                change("change-day-start", ChangeKind::Loosen, args)
            }),
        ),
        (
            "loosening_delay",
            (old.loosening_delay != new.loosening_delay).then(|| {
                moved(
                    new.loosening_delay > old.loosening_delay,
                    ("change-delay-longer", "change-delay-shorter"),
                    old_new(old.loosening_delay, new.loosening_delay, friendly),
                )
            }),
        ),
        (
            "language",
            (old.language != new.language).then(|| {
                let args = old_new(&old.language, &new.language, String::clone);
                change("change-language", ChangeKind::Neutral, args)
            }),
        ),
        (
            "agent_allowed_commands",
            allowed_commands(&old.agent_allowed_commands, &new.agent_allowed_commands),
        ),
    ];
    units(Scope::Setting, fields).collect()
}

pub(super) fn exceptions(old: &Config, new: &Config) -> Vec<UnitChange> {
    let kept_or_removed = old.exceptions.iter().map(|previous| {
        let change = match new.exception_on(previous.date) {
            Some(next) => coverage(previous, next),
            None => Some(change(
                "change-exception-removed",
                ChangeKind::Tighten,
                exception_args(previous),
            )),
        };
        (previous.date, change)
    });
    let added = new
        .exceptions
        .iter()
        .filter(|next| old.exception_on(next.date).is_none())
        .map(|next| {
            let added = change(
                "change-exception-added",
                ChangeKind::Loosen,
                exception_args(next),
            );
            (next.date, Some(added))
        });
    kept_or_removed
        .chain(added)
        .flat_map(|(date, change)| units(Scope::Exception(date), [("exception", change)]))
        .collect()
}

fn on_day(old: &Config, new: &Config, day: Weekday) -> Vec<UnitChange> {
    let (old_rules, new_rules) = (old.rules_on(day), new.rules_on(day));
    let scope = |rule: &Rule| Scope::Rule {
        day,
        rule: rule.id.clone(),
    };
    let mut changes = Vec::new();
    for rule in old_rules {
        let fields = match new_rules.iter().find(|next| next.id == rule.id) {
            Some(next) => rule_fields(
                Side {
                    rule,
                    day_starts_at: old.day_starts_at,
                },
                Side {
                    rule: next,
                    day_starts_at: new.day_starts_at,
                },
            ),
            None => vec![presence("change-rule-removed", ChangeKind::Loosen, rule)],
        };
        changes.extend(units(scope(rule), fields));
    }
    for rule in new_rules
        .iter()
        .filter(|next| !old_rules.iter().any(|previous| previous.id == next.id))
    {
        let added = presence("change-rule-added", ChangeKind::Tighten, rule);
        changes.extend(units(scope(rule), [added]));
    }
    changes
}

fn rule_fields(old: Side, new: Side) -> Vec<(String, Option<Change>)> {
    let (previous, next) = (old.rule, new.rule);
    let enforcing = previous.enforces() || next.enforces();
    let judge = |change: Option<Change>| change.map(|change| judged(enforcing, change));
    let [first_timing, second_timing] = timing(old, new);
    let [first_action, second_action] = action(previous.action, next.action);
    let challenges = challenge_fields(&previous.challenges, &next.challenges)
        .map(|(field, change)| (field, judge(change)));
    let agent = agent_fields(&previous.agent, &next.agent)
        .into_iter()
        .map(|(field, change)| (field, judge(change)));
    let named: Vec<Field> = [
        ("timing", judge(first_timing)),
        ("timing", judge(second_timing)),
        ("action", first_action),
        ("action", second_action),
        ("notices", notices(previous, next)),
        (
            "message",
            (previous.message != next.message)
                .then(|| change("change-message", ChangeKind::Neutral, Vec::new())),
        ),
        (
            "label",
            (previous.label != next.label).then(|| {
                let args = old_new(&previous.label, &next.label, String::clone);
                change("change-label", ChangeKind::Neutral, args)
            }),
        ),
    ]
    .into_iter()
    .chain(challenges)
    .collect();
    named
        .into_iter()
        .map(|(field, change)| (field.to_owned(), change))
        .chain(agent)
        .collect()
}

fn agent_fields(old: &[AgentStep], new: &[AgentStep]) -> Vec<(String, Option<Change>)> {
    let kept_or_removed = old.iter().map(|previous| {
        let change = match new.iter().find(|next| same_step(previous, next)) {
            Some(next) => paired_step(previous, next),
            None => Some(change(
                "change-agent-step-removed",
                ChangeKind::Loosen,
                vec![
                    ("step", step_key(previous)),
                    ("old", step_summary(previous)),
                ],
            )),
        };
        (agent_field(previous), change)
    });
    let added = new
        .iter()
        .filter(|next| !old.iter().any(|previous| same_step(previous, next)))
        .map(|next| {
            let added = change(
                "change-agent-step-added",
                ChangeKind::Tighten,
                vec![("step", step_key(next)), ("new", step_summary(next))],
            );
            (agent_field(next), Some(added))
        });
    kept_or_removed.chain(added).collect()
}

fn same_step(old: &AgentStep, new: &AgentStep) -> bool {
    (old.before, old.level) == (new.before, new.level)
}

fn paired_step(old: &AgentStep, new: &AgentStep) -> Option<Change> {
    let tightens = [
        folders_cover(&new.folders, &old.folders) && !folders_cover(&old.folders, &new.folders),
        !old.unattended && new.unattended,
    ]
    .contains(&true);
    let loosens = [
        !folders_cover(&new.folders, &old.folders),
        old.unattended && !new.unattended,
    ]
    .contains(&true);
    let args = vec![
        ("step", step_key(new)),
        ("old", step_summary(old)),
        ("new", step_summary(new)),
    ];
    match (tightens, loosens) {
        (true, false) => Some(change(
            "change-agent-step-firmer",
            ChangeKind::Tighten,
            args,
        )),
        (false, true) => Some(change("change-agent-step-looser", ChangeKind::Loosen, args)),
        (true, true) => Some(change(
            "change-agent-step-changed",
            ChangeKind::Loosen,
            args,
        )),
        (false, false) => (old.message != new.message).then(|| {
            change(
                "change-agent-message",
                ChangeKind::Neutral,
                args.into_iter().take(1).collect(),
            )
        }),
    }
}

fn step_key(step: &AgentStep) -> String {
    format!("{}, {}", friendly(step.before), step.level.name())
}

fn step_summary(step: &AgentStep) -> String {
    let folders = (!step.folders.is_empty()).then(|| step.folders.join(" "));
    let unattended = step.unattended.then(|| "unattended".to_owned());
    [Some(step_key(step)), folders, unattended]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
}

fn allowed_commands(old: &[String], new: &[String]) -> Option<Change> {
    let (old_set, new_set): (HashSet<&String>, HashSet<&String>) =
        (old.iter().collect(), new.iter().collect());
    let args = old_new(old, new, |commands| commands.join(", "));
    match (new_set.is_subset(&old_set), old_set.is_subset(&new_set)) {
        (true, true) => None,
        (true, false) => Some(change(
            "change-agent-commands-fewer",
            ChangeKind::Tighten,
            args,
        )),
        (false, true) => Some(change(
            "change-agent-commands-more",
            ChangeKind::Loosen,
            args,
        )),
        (false, false) => Some(change(
            "change-agent-commands-changed",
            ChangeKind::Loosen,
            args,
        )),
    }
}

fn presence(id: &'static str, kind: ChangeKind, rule: &Rule) -> (String, Option<Change>) {
    let change = judged(rule.enforces(), change(id, kind, Vec::new()));
    ("presence".to_owned(), Some(change))
}

fn timing(old: Side, new: Side) -> [Option<Change>; 2] {
    let reference = old.day_starts_at;
    let moved = [
        start(old.rule, new.rule, reference),
        end(old, new, reference),
    ];
    if moved.iter().any(Option::is_some) || !rewritten(old.rule, new.rule) {
        return moved;
    }
    [
        Some(change("change-rule-timing", ChangeKind::Loosen, Vec::new())),
        None,
    ]
}

fn rewritten(old: &Rule, new: &Rule) -> bool {
    (old.start, old.start_jitter, old.end) != (new.start, new.start_jitter, new.end)
}

fn start(old: &Rule, new: &Rule, reference: Time) -> Option<Change> {
    let (old_bounds, new_bounds) = (start_bounds(old, reference), start_bounds(new, reference));
    let [old_earliest, old_latest] = old_bounds;
    let [new_earliest, new_latest] = new_bounds;
    let args = old_new(old_bounds, new_bounds, |bounds| window(reference, bounds));
    if new_earliest > old_earliest || new_latest > old_latest {
        return Some(change("change-rule-starts-later", ChangeKind::Loosen, args));
    }
    (new_earliest < old_earliest || new_latest < old_latest)
        .then(|| change("change-rule-starts-earlier", ChangeKind::Tighten, args))
}

fn end(old: Side, new: Side, reference: Time) -> Option<Change> {
    let (old_bounds, new_bounds) = (end_bounds(old, reference), end_bounds(new, reference));
    let [old_earliest, old_latest] = old_bounds;
    let [new_earliest, new_latest] = new_bounds;
    let args = old_new(old_bounds, new_bounds, |bounds| window(reference, bounds));
    if new_earliest < old_earliest || new_latest < old_latest {
        return Some(change("change-rule-ends-earlier", ChangeKind::Loosen, args));
    }
    (new_earliest > old_earliest || new_latest > old_latest)
        .then(|| change("change-rule-ends-later", ChangeKind::Tighten, args))
}

fn start_bounds(rule: &Rule, reference: Time) -> [SignedDuration; 2] {
    let earliest = after(reference, rule.start);
    [earliest, earliest.saturating_add(rule.start_jitter)]
}

fn end_bounds(side: Side, reference: Time) -> [SignedDuration; 2] {
    let rule = side.rule;
    let [earliest, latest] = start_bounds(rule, reference);
    match rule.end {
        RuleEnd::At(end) => [earliest + after(rule.start, end); 2],
        RuleEnd::After(length) => [
            earliest.saturating_add(length),
            latest.saturating_add(length),
        ],
        RuleEnd::DayEnd => {
            [SignedDuration::from_hours(24) + side.day_starts_at.duration_since(reference); 2]
        }
    }
}

fn action(old: Action, new: Action) -> [Option<Change>; 2] {
    match (old, new) {
        (
            Action::Lock {
                shutdown_after: old,
            },
            Action::Lock {
                shutdown_after: new,
            },
        ) => [shutdown(old, new), None],
        (Action::ShutDown { repeat: old }, Action::ShutDown { repeat: new }) => [
            (old != new).then(|| {
                moved(
                    new,
                    ("change-shutdown-repeats", "change-shutdown-once"),
                    Vec::new(),
                )
            }),
            None,
        ],
        _ => [
            step_ids(old).map(|(_, removed)| change(removed, ChangeKind::Loosen, Vec::new())),
            step_ids(new).map(|(added, _)| change(added, ChangeKind::Tighten, Vec::new())),
        ],
    }
}

fn step_ids(action: Action) -> Option<(&'static str, &'static str)> {
    match action {
        Action::Nothing => None,
        Action::Lock { .. } => Some(("change-lock-added", "change-lock-removed")),
        Action::ShutDown { .. } => Some((
            "change-shutdown-at-start-added",
            "change-shutdown-at-start-removed",
        )),
    }
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

fn notices(old: &Rule, new: &Rule) -> Option<Change> {
    (old.notices != new.notices).then(|| {
        let args = old_new(old, new, |rule| {
            rule.notice_offsets()
                .into_iter()
                .map(friendly)
                .collect::<Vec<_>>()
                .join(", ")
        });
        change("change-notices", ChangeKind::Neutral, args)
    })
}

fn challenge_fields(old: &Challenges, new: &Challenges) -> [Field; 5] {
    [
        (
            "wait",
            (old.wait != new.wait).then(|| {
                moved(
                    new.wait > old.wait,
                    ("change-wait-longer", "change-wait-shorter"),
                    old_new(old.wait, new.wait, friendly),
                )
            }),
        ),
        (
            "reason",
            (old.reason != new.reason).then(|| {
                moved(
                    new.reason,
                    ("change-reason-required", "change-reason-dropped"),
                    Vec::new(),
                )
            }),
        ),
        ("phrase", phrase(old.phrase.as_ref(), new.phrase.as_ref())),
        (
            "extensions",
            (old.extensions != new.extensions).then(|| {
                moved(
                    new.extensions < old.extensions,
                    ("change-fewer-extensions", "change-more-extensions"),
                    old_new(old.extensions, new.extensions, |count| count.to_string()),
                )
            }),
        ),
        (
            "extension_length",
            (old.extension_length != new.extension_length).then(|| {
                moved(
                    new.extension_length < old.extension_length,
                    ("change-shorter-extensions", "change-longer-extensions"),
                    old_new(old.extension_length, new.extension_length, friendly),
                )
            }),
        ),
    ]
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

fn coverage(old: &Exception, new: &Exception) -> Option<Change> {
    let (id, kind) = match (includes(new, old), includes(old, new)) {
        (true, true) => return None,
        (true, false) => ("change-exception-wider", ChangeKind::Loosen),
        (false, true) => ("change-exception-narrower", ChangeKind::Tighten),
        (false, false) => ("change-exception-changed", ChangeKind::Loosen),
    };
    Some(change(id, kind, exception_args(new)))
}

fn includes(outer: &Exception, inner: &Exception) -> bool {
    outer.rules.is_empty()
        || (!inner.rules.is_empty() && inner.rules.iter().all(|rule| outer.covers(rule)))
}

fn exception_args(exception: &Exception) -> Args {
    vec![
        ("date", exception.date.to_string()),
        ("rules", rules_arg(&exception.rules)),
    ]
}

fn rules_arg(rules: &[String]) -> String {
    if rules.is_empty() {
        return "all".to_owned();
    }
    rules.join(", ")
}

fn units<F: Into<String>>(
    scope: Scope,
    fields: impl IntoIterator<Item = (F, Option<Change>)>,
) -> impl Iterator<Item = UnitChange> {
    fields.into_iter().filter_map(move |(field, change)| {
        change.map(|change| UnitChange {
            scope: scope.clone(),
            field: field.into(),
            id: change.id,
            args: change.args,
            kind: change.kind,
        })
    })
}

fn judged(enforcing: bool, change: Change) -> Change {
    if enforcing {
        return change;
    }
    Change {
        kind: ChangeKind::Neutral,
        ..change
    }
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

fn window(reference: Time, [earliest, latest]: [SignedDuration; 2]) -> String {
    let at = |position| clock(reference.wrapping_add(position));
    if earliest == latest {
        return at(earliest);
    }
    format!("{}–{}", at(earliest), at(latest))
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
