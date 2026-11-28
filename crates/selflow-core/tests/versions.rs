mod support;

use jiff::civil::Weekday;
use selflow_core::config::{Config, parse};
use selflow_core::versions::{ChangeKind, Scope, UnitChange, diff};
use support::no_jitter;

type Edits = &'static [(&'static str, &'static str)];

fn config(edits: &[(&str, &str)]) -> Config {
    let text = edits.iter().fold(no_jitter(), |text, (from, to)| {
        assert!(text.contains(from), "the fixture has no {from}");
        text.replace(from, to)
    });
    parse(&text).unwrap()
}

const PHRASE: (&str, &str) = (
    "reason = true",
    "reason = true\nphrase = \"Rest is enough\"",
);
const NO_LOCK: (&str, &str) = ("lock = true\nshutdown_after = \"30m\"\n", "");
const ONCE: (&str, &str) = (
    "lock = true\nshutdown_after = \"30m\"\n",
    "shutdown = \"once\"\n",
);
const REPEAT: (&str, &str) = (
    "lock = true\nshutdown_after = \"30m\"\n",
    "shutdown = \"repeat\"\n",
);
const NO_THURSDAY: (&str, &str) = (r#""wed", "thu"]"#, r#""wed"]"#);
const EVENING: (&str, &str) = (
    "[[rule_set.rule]]",
    "[[rule_set.rule]]\nid = \"evening\"\nstart = \"21:00\"\n\n[[rule_set.rule]]",
);
const JITTER: (&str, &str) = (
    r#"start = "bedtime""#,
    "start = \"bedtime\"\nstart_jitter = \"15m\"",
);
const LENGTH: (&str, &str) = (r#"end = "wake""#, r#"length = "8h""#);
const SHORTER_WAIT: (&str, &str) = (r#"wait = "2m""#, r#"wait = "1m""#);
const RULE_WAIT: (&str, &str) = (
    "shutdown_after = \"30m\"\n",
    "shutdown_after = \"30m\"\n\n[rule_set.rule.challenges]\nwait = \"5m\"\n",
);
const EVERY_RULE_OFF: (&str, &str) = (
    "[anchors]",
    "[[exception]]\ndate = \"2026-11-19\"\n\n[anchors]",
);
const BEDTIME_OFF: (&str, &str) = (
    "[anchors]",
    "[[exception]]\ndate = \"2026-11-19\"\nrules = [\"bedtime\"]\n\n[anchors]",
);
const EVENING_OFF: (&str, &str) = (
    "[anchors]",
    "[[exception]]\ndate = \"2026-11-19\"\nrules = [\"evening\"]\n\n[anchors]",
);

#[test]
fn a_start_window_compares_both_bounds() {
    let starting =
        |start: &str| parse(&no_jitter().replace(r#"start = "bedtime""#, start)).unwrap();
    let changes = diff(
        &starting("start = \"22:00\"\nstart_jitter = \"60m\""),
        &starting(r#"start = "22:50""#),
    );
    assert_eq!(
        (changes[0].id, changes[0].kind),
        ("change-rule-starts-later", ChangeKind::Loosen)
    );
    assert_eq!(
        changes[0].args,
        [("old", "22:00–23:00".into()), ("new", "22:50".into())]
    );
}

#[test]
fn diff_judges_every_field() {
    use ChangeKind::{Loosen, Neutral, Tighten};
    const LATER: (&str, &str) = (r#"bedtime = "23:00""#, r#"bedtime = "01:00""#);
    const WAKE_LATER: (&str, &str) = (r#"wake = "07:00""#, r#"wake = "08:00""#);
    let cases: &[(Edits, Edits, &str, &str, ChangeKind)] = &[
        (
            &[NO_THURSDAY],
            &[],
            "thu/bedtime/presence",
            "change-rule-added",
            Tighten,
        ),
        (
            &[],
            &[NO_THURSDAY],
            "thu/bedtime/presence",
            "change-rule-removed",
            Loosen,
        ),
        (
            &[],
            &[(r#""thu"]"#, r#""thu", "fri"]"#)],
            "fri/bedtime/presence",
            "change-rule-added",
            Tighten,
        ),
        (
            &[],
            &[EVENING],
            "thu/evening/presence",
            "change-rule-added",
            Neutral,
        ),
        (
            &[EVENING],
            &[],
            "thu/evening/presence",
            "change-rule-removed",
            Neutral,
        ),
        (
            &[],
            &[(r#"bedtime = "23:00""#, r#"bedtime = "22:00""#)],
            "thu/bedtime/timing",
            "change-rule-starts-earlier",
            Tighten,
        ),
        (
            &[],
            &[LATER],
            "thu/bedtime/timing",
            "change-rule-starts-later",
            Loosen,
        ),
        (
            &[NO_LOCK],
            &[NO_LOCK, LATER],
            "thu/bedtime/timing",
            "change-rule-starts-later",
            Neutral,
        ),
        (
            &[],
            &[WAKE_LATER],
            "thu/bedtime/timing",
            "change-rule-ends-later",
            Tighten,
        ),
        (
            &[],
            &[(r#"wake = "07:00""#, r#"wake = "06:00""#)],
            "thu/bedtime/timing",
            "change-rule-ends-earlier",
            Loosen,
        ),
        (
            &[],
            &[(r#"bedtime = "23:00""#, r#"bedtime = "05:30""#)],
            "thu/bedtime/timing",
            "change-rule-ends-earlier",
            Loosen,
        ),
        (
            &[],
            &[(
                r#"start = "bedtime""#,
                "start = \"22:30\"\nstart_jitter = \"60m\"",
            )],
            "thu/bedtime/timing",
            "change-rule-starts-later",
            Loosen,
        ),
        (
            &[],
            &[(
                r#"start = "bedtime""#,
                "start = \"22:00\"\nstart_jitter = \"60m\"",
            )],
            "thu/bedtime/timing",
            "change-rule-starts-earlier",
            Tighten,
        ),
        (
            &[],
            &[(r#"end = "wake""#, r#"length = "9h""#)],
            "thu/bedtime/timing",
            "change-rule-ends-later",
            Tighten,
        ),
        (
            &[REPEAT],
            &[REPEAT, ("end = \"wake\"\n", "")],
            "thu/bedtime/timing",
            "change-rule-ends-earlier",
            Loosen,
        ),
        (
            &[NO_LOCK],
            &[NO_LOCK, WAKE_LATER],
            "thu/bedtime/timing",
            "change-rule-ends-later",
            Neutral,
        ),
        (
            &[JITTER],
            &[JITTER, LENGTH],
            "thu/bedtime/timing",
            "change-rule-ends-later",
            Tighten,
        ),
        (
            &[JITTER, LENGTH],
            &[JITTER],
            "thu/bedtime/timing",
            "change-rule-ends-earlier",
            Loosen,
        ),
        (
            &[REPEAT, (r#"end = "wake""#, r#"end = "05:00""#)],
            &[
                REPEAT,
                ("end = \"wake\"\n", ""),
                (r#"day_starts_at = "05:00""#, r#"day_starts_at = "06:00""#),
            ],
            "thu/bedtime/timing",
            "change-rule-ends-later",
            Tighten,
        ),
        (
            &[],
            &[LENGTH],
            "thu/bedtime/timing",
            "change-rule-timing",
            Loosen,
        ),
        (
            &[NO_LOCK],
            &[NO_LOCK, LENGTH],
            "thu/bedtime/timing",
            "change-rule-timing",
            Neutral,
        ),
        (
            &[NO_LOCK],
            &[],
            "thu/bedtime/action",
            "change-lock-added",
            Tighten,
        ),
        (
            &[],
            &[NO_LOCK],
            "thu/bedtime/action",
            "change-lock-removed",
            Loosen,
        ),
        (
            &[(r#"shutdown_after = "30m""#, "")],
            &[],
            "thu/bedtime/action",
            "change-shutdown-added",
            Tighten,
        ),
        (
            &[],
            &[(r#"shutdown_after = "30m""#, "")],
            "thu/bedtime/action",
            "change-shutdown-removed",
            Loosen,
        ),
        (
            &[],
            &[(r#"shutdown_after = "30m""#, r#"shutdown_after = "10m""#)],
            "thu/bedtime/action",
            "change-shutdown-sooner",
            Tighten,
        ),
        (
            &[],
            &[(r#"shutdown_after = "30m""#, r#"shutdown_after = "1h""#)],
            "thu/bedtime/action",
            "change-shutdown-later",
            Loosen,
        ),
        (
            &[NO_LOCK],
            &[ONCE],
            "thu/bedtime/action",
            "change-shutdown-at-start-added",
            Tighten,
        ),
        (
            &[ONCE],
            &[NO_LOCK],
            "thu/bedtime/action",
            "change-shutdown-at-start-removed",
            Loosen,
        ),
        (
            &[ONCE],
            &[REPEAT],
            "thu/bedtime/action",
            "change-shutdown-repeats",
            Tighten,
        ),
        (
            &[REPEAT],
            &[ONCE],
            "thu/bedtime/action",
            "change-shutdown-once",
            Loosen,
        ),
        (
            &[],
            &[ONCE],
            "thu/bedtime/action",
            "change-lock-removed",
            Loosen,
        ),
        (
            &[],
            &[ONCE],
            "thu/bedtime/action",
            "change-shutdown-at-start-added",
            Tighten,
        ),
        (
            &[ONCE],
            &[],
            "thu/bedtime/action",
            "change-shutdown-at-start-removed",
            Loosen,
        ),
        (
            &[ONCE],
            &[],
            "thu/bedtime/action",
            "change-lock-added",
            Tighten,
        ),
        (
            &[],
            &[(r#"notices = ["5m", "15m", "1m"]"#, r#"notices = ["10m"]"#)],
            "thu/bedtime/notices",
            "change-notices",
            Neutral,
        ),
        (
            &[],
            &[("lock = true", "message = \"Time to rest\"\nlock = true")],
            "thu/bedtime/message",
            "change-message",
            Neutral,
        ),
        (
            &[],
            &[(r#"id = "bedtime""#, "id = \"bedtime\"\nlabel = \"Sleep\"")],
            "thu/bedtime/label",
            "change-label",
            Neutral,
        ),
        (
            &[],
            &[RULE_WAIT],
            "thu/bedtime/wait",
            "change-wait-longer",
            Tighten,
        ),
        (
            &[],
            &[(r#"wait = "2m""#, r#"wait = "1m""#)],
            "thu/bedtime/wait",
            "change-wait-shorter",
            Loosen,
        ),
        (
            &[NO_LOCK],
            &[NO_LOCK, SHORTER_WAIT],
            "thu/bedtime/wait",
            "change-wait-shorter",
            Neutral,
        ),
        (
            &[NO_LOCK],
            &[SHORTER_WAIT],
            "thu/bedtime/wait",
            "change-wait-shorter",
            Loosen,
        ),
        (
            &[("reason = true", "reason = false")],
            &[],
            "thu/bedtime/reason",
            "change-reason-required",
            Tighten,
        ),
        (
            &[],
            &[("reason = true", "reason = false")],
            "thu/bedtime/reason",
            "change-reason-dropped",
            Loosen,
        ),
        (
            &[],
            &[PHRASE],
            "thu/bedtime/phrase",
            "change-phrase-added",
            Tighten,
        ),
        (
            &[PHRASE],
            &[],
            "thu/bedtime/phrase",
            "change-phrase-dropped",
            Loosen,
        ),
        (
            &[PHRASE],
            &[(
                "reason = true",
                "reason = true\nphrase = \"Tomorrow starts tonight\"",
            )],
            "thu/bedtime/phrase",
            "change-phrase-changed",
            Loosen,
        ),
        (
            &[],
            &[("extensions = 1", "extensions = 0")],
            "thu/bedtime/extensions",
            "change-fewer-extensions",
            Tighten,
        ),
        (
            &[],
            &[("extensions = 1", "extensions = 2")],
            "thu/bedtime/extensions",
            "change-more-extensions",
            Loosen,
        ),
        (
            &[],
            &[(r#"extension_length = "20m""#, r#"extension_length = "10m""#)],
            "thu/bedtime/extension_length",
            "change-shorter-extensions",
            Tighten,
        ),
        (
            &[],
            &[(r#"extension_length = "20m""#, r#"extension_length = "30m""#)],
            "thu/bedtime/extension_length",
            "change-longer-extensions",
            Loosen,
        ),
        (
            &[],
            &[(r#"day_starts_at = "05:00""#, r#"day_starts_at = "04:00""#)],
            "day_starts_at",
            "change-day-start",
            Loosen,
        ),
        (
            &[],
            &[(r#"loosening_delay = "24h""#, r#"loosening_delay = "48h""#)],
            "loosening_delay",
            "change-delay-longer",
            Tighten,
        ),
        (
            &[],
            &[(r#"loosening_delay = "24h""#, r#"loosening_delay = "12h""#)],
            "loosening_delay",
            "change-delay-shorter",
            Loosen,
        ),
        (
            &[],
            &[(r#"language = "pt-BR""#, r#"language = "en""#)],
            "language",
            "change-language",
            Neutral,
        ),
        (
            &[],
            &[EVERY_RULE_OFF],
            "exception/2026-11-19",
            "change-exception-added",
            Loosen,
        ),
        (
            &[EVERY_RULE_OFF],
            &[],
            "exception/2026-11-19",
            "change-exception-removed",
            Tighten,
        ),
        (
            &[BEDTIME_OFF],
            &[EVERY_RULE_OFF],
            "exception/2026-11-19",
            "change-exception-wider",
            Loosen,
        ),
        (
            &[EVERY_RULE_OFF],
            &[BEDTIME_OFF],
            "exception/2026-11-19",
            "change-exception-narrower",
            Tighten,
        ),
        (
            &[EVENING, BEDTIME_OFF],
            &[EVENING, EVENING_OFF],
            "exception/2026-11-19",
            "change-exception-changed",
            Loosen,
        ),
    ];
    for &(current, proposed, unit, id, kind) in cases {
        let changes = diff(&config(current), &config(proposed));
        assert!(
            changes
                .iter()
                .any(|change| (change.unit().as_str(), change.id, change.kind) == (unit, id, kind)),
            "{unit} {id}: {changes:?}"
        );
    }
}

#[test]
fn diff_keeps_each_weekday_apart() {
    let later = config(&[(r#"bedtime = "23:00""#, r#"bedtime = "01:00""#)]);
    let changes = diff(&config(&[]), &later);
    let units: Vec<String> = changes.iter().map(UnitChange::unit).collect();
    assert_eq!(
        units,
        [
            "mon/bedtime/timing",
            "tue/bedtime/timing",
            "wed/bedtime/timing",
            "thu/bedtime/timing",
            "sun/bedtime/timing"
        ]
    );
    assert_eq!(
        changes[3],
        UnitChange {
            scope: Scope::Rule {
                day: Weekday::Thursday,
                rule: "bedtime".to_owned(),
            },
            field: "timing",
            id: "change-rule-starts-later",
            args: vec![("old", "23:00".into()), ("new", "01:00".into())],
            kind: ChangeKind::Loosen,
        }
    );
}

#[test]
fn diff_measures_rule_times_from_the_old_day_start() {
    let earlier_day = (r#"day_starts_at = "05:00""#, r#"day_starts_at = "04:00""#);
    let timing = |proposed: &[(&str, &str)]| -> Vec<(&'static str, ChangeKind)> {
        diff(&config(&[]), &config(proposed))
            .into_iter()
            .filter(|change| change.unit() == "thu/bedtime/timing")
            .map(|change| (change.id, change.kind))
            .collect()
    };
    assert!(timing(&[earlier_day]).is_empty());
    assert_eq!(
        timing(&[
            earlier_day,
            (r#"bedtime = "23:00""#, r#"bedtime = "22:00""#)
        ]),
        [("change-rule-starts-earlier", ChangeKind::Tighten)]
    );
}

#[test]
fn a_day_boundary_end_shows_each_sides_own_day_start() {
    let changes = diff(
        &config(&[REPEAT, (r#"end = "wake""#, r#"end = "05:00""#)]),
        &config(&[
            REPEAT,
            ("end = \"wake\"\n", ""),
            (r#"day_starts_at = "05:00""#, r#"day_starts_at = "06:00""#),
        ]),
    );
    let end = changes
        .iter()
        .find(|change| change.unit() == "thu/bedtime/timing")
        .unwrap();
    assert_eq!(
        end.args,
        vec![("old", "05:00".into()), ("new", "06:00".into())]
    );
}

#[test]
fn an_omitted_notices_key_stays_omitted_when_the_action_changes() {
    let without_notices = ("notices = [\"5m\", \"15m\", \"1m\"]\n", "");
    let changes = diff(
        &config(&[without_notices]),
        &config(&[without_notices, NO_LOCK]),
    );
    assert!(changes.iter().all(|change| change.field != "notices"));
}

#[test]
fn an_exception_entry_names_its_date_and_rules() {
    let args_of = |current: &[(&str, &str)], proposed: &[(&str, &str)]| {
        diff(&config(current), &config(proposed)).remove(0).args
    };
    assert_eq!(
        args_of(&[], &[EVERY_RULE_OFF]),
        vec![("date", "2026-11-19".into()), ("rules", "all".into())]
    );
    assert_eq!(
        args_of(&[EVERY_RULE_OFF], &[BEDTIME_OFF]),
        vec![("date", "2026-11-19".into()), ("rules", "bedtime".into())]
    );
}

fn stepped(step: Option<&str>) -> Config {
    let tables = step.map_or(String::new(), |body| {
        format!("\n[[rule_set.rule.agent]]\n{body}\n")
    });
    parse(&format!("{}{tables}", no_jitter())).unwrap()
}

#[test]
fn agent_step_changes_are_judged() {
    use ChangeKind::{Loosen, Neutral, Tighten};
    const LIGHT: &str = "before = \"60m\"\nlevel = \"light\"";
    let cases: &[(Option<&str>, Option<&str>, &str, ChangeKind)] = &[
        (None, Some(LIGHT), "change-agent-step-added", Tighten),
        (Some(LIGHT), None, "change-agent-step-removed", Loosen),
        (
            Some(LIGHT),
            Some("before = \"30m\"\nlevel = \"light\""),
            "change-agent-step-looser",
            Loosen,
        ),
        (
            Some("before = \"30m\"\nlevel = \"light\""),
            Some(LIGHT),
            "change-agent-step-firmer",
            Tighten,
        ),
        (
            Some(LIGHT),
            Some("before = \"60m\"\nlevel = \"save-only\""),
            "change-agent-step-firmer",
            Tighten,
        ),
        (
            Some("before = \"60m\"\nlevel = \"save-only\""),
            Some("before = \"60m\"\nlevel = \"no-new-work\""),
            "change-agent-step-looser",
            Loosen,
        ),
        (
            Some(LIGHT),
            Some("before = \"60m\"\nlevel = \"light\"\nfolders = [\"~/a\"]"),
            "change-agent-step-looser",
            Loosen,
        ),
        (
            Some("before = \"60m\"\nlevel = \"light\"\nfolders = [\"~/a\"]"),
            Some(LIGHT),
            "change-agent-step-firmer",
            Tighten,
        ),
        (
            Some("before = \"60m\"\nlevel = \"light\"\nfolders = [\"~/a\"]"),
            Some("before = \"60m\"\nlevel = \"light\"\nfolders = [\"~/b\"]"),
            "change-agent-step-looser",
            Loosen,
        ),
        (
            Some("before = \"60m\"\nlevel = \"light\"\nunattended = true"),
            Some(LIGHT),
            "change-agent-step-looser",
            Loosen,
        ),
        (
            Some(LIGHT),
            Some("before = \"60m\"\nlevel = \"light\"\nunattended = true"),
            "change-agent-step-firmer",
            Tighten,
        ),
        (
            Some(LIGHT),
            Some("before = \"30m\"\nlevel = \"save-only\""),
            "change-agent-step-changed",
            Loosen,
        ),
        (
            Some(LIGHT),
            Some("before = \"60m\"\nlevel = \"light\"\nmessage = \"Wrap up\""),
            "change-agent-message",
            Neutral,
        ),
    ];
    for &(current, proposed, id, kind) in cases {
        let changes = diff(&stepped(current), &stepped(proposed));
        assert!(
            changes
                .iter()
                .any(|change| (change.unit().as_str(), change.id, change.kind)
                    == ("thu/bedtime/agent", id, kind)),
            "{id}: {changes:?}"
        );
    }
}

#[test]
fn an_agent_step_change_names_its_position_and_both_sides() {
    let changes = diff(
        &stepped(Some("before = \"60m\"\nlevel = \"light\"")),
        &stepped(Some(
            "before = \"60m\"\nlevel = \"light\"\nfolders = [\"~/a\", \"~/b\"]\nunattended = true",
        )),
    );
    let change = changes
        .iter()
        .find(|change| change.unit() == "thu/bedtime/agent")
        .unwrap();
    assert_eq!(
        change.args,
        [
            ("step", "1".into()),
            ("old", "1h, light".into()),
            ("new", "1h, light, ~/a ~/b, unattended".into())
        ]
    );
}

#[test]
fn a_second_step_is_judged_against_the_second_step() {
    let two = |second: &str| {
        let text = format!(
            "{}\n[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"light\"\n\n[[rule_set.rule.agent]]\n{second}\n",
            no_jitter()
        );
        parse(&text).unwrap()
    };
    let changes = diff(
        &two("before = \"15m\"\nlevel = \"no-new-work\""),
        &two("before = \"15m\"\nlevel = \"save-only\""),
    );
    let change = changes
        .iter()
        .find(|change| change.unit() == "thu/bedtime/agent")
        .unwrap();
    assert_eq!(
        (change.id, change.args[0].1.as_str()),
        ("change-agent-step-firmer", "2")
    );
}

#[test]
fn removing_a_rule_with_only_steps_loosens() {
    const WITH_STEPS: (&str, &str) = (
        "[[rule_set.rule]]",
        "[[rule_set.rule]]\nid = \"evening\"\nstart = \"21:00\"\n\n[[rule_set.rule.agent]]\nbefore = \"0m\"\nlevel = \"message\"\n\n[[rule_set.rule]]",
    );
    let changes = diff(&config(&[WITH_STEPS]), &config(&[]));
    assert!(changes.iter().any(|change| {
        (change.unit().as_str(), change.id, change.kind)
            == (
                "thu/evening/presence",
                "change-rule-removed",
                ChangeKind::Loosen,
            )
    }));
}

#[test]
fn allowed_command_changes_are_judged() {
    use ChangeKind::{Loosen, Tighten};
    let listed = |commands: &str| {
        parse(&format!(
            "{}\n[agent]\nallowed_commands = {commands}\n",
            no_jitter()
        ))
        .unwrap()
    };
    let default = r#"["git status", "git diff", "git add", "git commit", "git stash push"]"#;
    let cases: &[(&str, &str, ChangeKind)] = &[
        (
            r#"["git status", "git diff", "git add", "git commit"]"#,
            "change-agent-commands-fewer",
            Tighten,
        ),
        (
            r#"["git status", "git diff", "git add", "git commit", "git stash push", "git push"]"#,
            "change-agent-commands-more",
            Loosen,
        ),
        (
            r#"["git status", "git diff", "git add", "git commit", "git push"]"#,
            "change-agent-commands-changed",
            Loosen,
        ),
    ];
    for &(proposed, id, kind) in cases {
        let changes = diff(&listed(default), &listed(proposed));
        assert!(
            changes
                .iter()
                .any(|change| (change.unit().as_str(), change.id, change.kind)
                    == ("agent_allowed_commands", id, kind)),
            "{id}: {changes:?}"
        );
    }
}

#[test]
fn the_same_allowed_commands_in_another_order_change_nothing() {
    let listed = |commands: &str| {
        parse(&format!(
            "{}\n[agent]\nallowed_commands = {commands}\n",
            no_jitter()
        ))
        .unwrap()
    };
    let changes = diff(
        &listed(r#"["git status", "git diff"]"#),
        &listed(r#"["git diff", "git status"]"#),
    );
    assert!(changes.is_empty());
}
