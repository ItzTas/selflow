mod support;

use jiff::{ZonedDateTime, civil::Weekday};
use selflow_core::config::{Config, parse};
use selflow_core::versions::{
    ChangeKind, Scope, StoredVersion, UnitChange, classify, diff, effective_at, in_force, pending,
};
use support::{at, no_jitter};

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
fn moving_bedtime_later_loosens() {
    let current = parse(&no_jitter()).unwrap();
    let with = |from: &str, to: &str| parse(&no_jitter().replace(from, to)).unwrap();
    let result = classify(
        &current,
        &with(r#"bedtime = "23:00""#, r#"bedtime = "01:00""#),
    );
    assert_eq!(result.kind, ChangeKind::Loosen);
    assert_eq!(result.changes[0].id, "change-rule-starts-later");
    assert_eq!(
        result.changes[0].args,
        vec![
            ("rule", "bedtime".into()),
            ("days", "mon, tue, wed, thu, sun".into()),
            ("old", "23:00".into()),
            ("new", "01:00".into()),
        ]
    );
}

#[test]
fn moving_bedtime_earlier_tightens_and_applies_at_once() {
    let current = parse(&no_jitter()).unwrap();
    let with = |from: &str, to: &str| parse(&no_jitter().replace(from, to)).unwrap();
    let result = classify(
        &current,
        &with(r#"bedtime = "23:00""#, r#"bedtime = "22:00""#),
    );
    assert_eq!(result.kind, ChangeKind::Tighten);
    let saved = at(18, 10, 0).timestamp();
    assert_eq!(effective_at(result.kind, saved, &current), saved);
}

#[test]
fn a_loosening_waits_the_delay_in_force_even_when_the_file_shortens_it() {
    let current = parse(&no_jitter()).unwrap();
    let proposed = parse(
        &no_jitter()
            .replace(r#"bedtime = "23:00""#, r#"bedtime = "01:00""#)
            .replace(r#"loosening_delay = "24h""#, r#"loosening_delay = "1h""#),
    )
    .unwrap();
    let result = classify(&current, &proposed);
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.id == "change-delay-shorter")
    );
    assert_eq!(
        effective_at(result.kind, at(18, 10, 0).timestamp(), &current),
        at(19, 10, 0).timestamp()
    );
}

#[test]
fn a_mixed_change_counts_as_loosening() {
    let current = parse(&no_jitter()).unwrap();
    let proposed = parse(
        &no_jitter()
            .replace(r#"bedtime = "23:00""#, r#"bedtime = "22:00""#)
            .replace("extensions = 1", "extensions = 3"),
    )
    .unwrap();
    assert_eq!(classify(&current, &proposed).kind, ChangeKind::Loosen);
}

#[test]
fn removing_a_day_loosens() {
    let current = parse(&no_jitter()).unwrap();
    let with = |from: &str, to: &str| parse(&no_jitter().replace(from, to)).unwrap();
    let result = classify(
        &current,
        &with(
            r#"days = ["sun", "mon", "tue", "wed", "thu"]"#,
            r#"days = ["mon", "tue", "wed", "thu"]"#,
        ),
    );
    assert_eq!(
        (result.kind, result.changes[0].id),
        (ChangeKind::Loosen, "change-rule-removed")
    );
    assert_eq!(result.changes[0].args[1], ("days", "sun".into()));
}

#[test]
fn rewording_labels_and_notices_is_neutral() {
    let current = parse(&no_jitter()).unwrap();
    let proposed = parse(
        &no_jitter()
            .replace("id = \"bedtime\"", "id = \"bedtime\"\nlabel = \"Sleep\"")
            .replace(r#"notices = ["5m", "15m", "1m"]"#, r#"notices = ["10m"]"#),
    )
    .unwrap();
    assert_eq!(classify(&current, &proposed).kind, ChangeKind::Neutral);
}

#[test]
fn the_first_configuration_applies_at_once() {
    let current = parse(&no_jitter()).unwrap();
    assert_eq!(
        classify(&Config::empty(), &current).kind,
        ChangeKind::Tighten
    );
}

#[test]
fn finds_the_version_in_force_and_the_pending_one() {
    let version = |id, effective: ZonedDateTime| StoredVersion {
        id,
        saved_at: at(18, 10, 0).timestamp(),
        effective_at: effective.timestamp(),
        text: String::new(),
    };
    let versions = vec![version(1, at(18, 10, 0)), version(2, at(19, 10, 0))];
    assert_eq!(
        in_force(&versions, at(18, 12, 0).timestamp()).map(|v| v.id),
        Some(1)
    );
    assert_eq!(
        pending(&versions, at(18, 12, 0).timestamp()).map(|v| v.id),
        Some(2)
    );
    assert_eq!(
        in_force(&versions, at(19, 10, 0).timestamp()).map(|v| v.id),
        Some(2)
    );
    assert_eq!(
        pending(&versions, at(19, 10, 0).timestamp()).map(|v| v.id),
        None
    );
}

#[test]
fn a_start_window_compares_both_bounds() {
    let starting =
        |start: &str| parse(&no_jitter().replace(r#"start = "bedtime""#, start)).unwrap();
    let result = classify(
        &starting("start = \"22:00\"\nstart_jitter = \"60m\""),
        &starting(r#"start = "22:50""#),
    );
    assert_eq!(result.kind, ChangeKind::Loosen);
    assert_eq!(result.changes[0].id, "change-rule-starts-later");
    assert_eq!(
        result.changes[0].args[2..],
        [("old", "22:00–23:00".into()), ("new", "22:50".into())]
    );
}

#[test]
fn classifies_each_kind_of_change() {
    let cases: &[(Edits, Edits, &str, ChangeKind)] = &[
        (
            &[],
            &[(r#"wake = "07:00""#, r#"wake = "08:00""#)],
            "change-rule-ends-later",
            ChangeKind::Tighten,
        ),
        (
            &[],
            &[(r#"wake = "07:00""#, r#"wake = "06:00""#)],
            "change-rule-ends-earlier",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[(r#"bedtime = "23:00""#, r#"bedtime = "05:30""#)],
            "change-rule-ends-earlier",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[(
                r#"start = "bedtime""#,
                "start = \"22:30\"\nstart_jitter = \"60m\"",
            )],
            "change-rule-starts-later",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[(
                r#"start = "bedtime""#,
                "start = \"22:00\"\nstart_jitter = \"60m\"",
            )],
            "change-rule-starts-earlier",
            ChangeKind::Tighten,
        ),
        (
            &[(r#"shutdown_after = "30m""#, "")],
            &[],
            "change-shutdown-added",
            ChangeKind::Tighten,
        ),
        (
            &[],
            &[(r#"shutdown_after = "30m""#, "")],
            "change-shutdown-removed",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[(r#"shutdown_after = "30m""#, r#"shutdown_after = "10m""#)],
            "change-shutdown-sooner",
            ChangeKind::Tighten,
        ),
        (
            &[],
            &[(r#"shutdown_after = "30m""#, r#"shutdown_after = "1h""#)],
            "change-shutdown-later",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[(r#"wait = "2m""#, r#"wait = "5m""#)],
            "change-wait-longer",
            ChangeKind::Tighten,
        ),
        (
            &[],
            &[(r#"wait = "2m""#, r#"wait = "1m""#)],
            "change-wait-shorter",
            ChangeKind::Loosen,
        ),
        (
            &[("reason = true", "reason = false")],
            &[],
            "change-reason-required",
            ChangeKind::Tighten,
        ),
        (
            &[],
            &[("reason = true", "reason = false")],
            "change-reason-dropped",
            ChangeKind::Loosen,
        ),
        (&[], &[PHRASE], "change-phrase-added", ChangeKind::Tighten),
        (&[PHRASE], &[], "change-phrase-dropped", ChangeKind::Loosen),
        (
            &[PHRASE],
            &[(
                "reason = true",
                "reason = true\nphrase = \"Tomorrow starts tonight\"",
            )],
            "change-phrase-changed",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[("extensions = 1", "extensions = 0")],
            "change-fewer-extensions",
            ChangeKind::Tighten,
        ),
        (
            &[],
            &[("extensions = 1", "extensions = 2")],
            "change-more-extensions",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[(r#"extension_length = "20m""#, r#"extension_length = "10m""#)],
            "change-shorter-extensions",
            ChangeKind::Tighten,
        ),
        (
            &[],
            &[(r#"extension_length = "20m""#, r#"extension_length = "30m""#)],
            "change-longer-extensions",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[(r#"loosening_delay = "24h""#, r#"loosening_delay = "48h""#)],
            "change-delay-longer",
            ChangeKind::Tighten,
        ),
        (
            &[],
            &[(r#"loosening_delay = "24h""#, r#"loosening_delay = "12h""#)],
            "change-delay-shorter",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[(r#"day_starts_at = "05:00""#, r#"day_starts_at = "04:00""#)],
            "change-day-start",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[(r#""thu"]"#, r#""thu", "fri"]"#)],
            "change-rule-added",
            ChangeKind::Tighten,
        ),
        (
            &[],
            &[(r#"id = "bedtime""#, "id = \"bedtime\"\nlabel = \"Sleep\"")],
            "change-label",
            ChangeKind::Neutral,
        ),
        (
            &[],
            &[(r#"notices = ["5m", "15m", "1m"]"#, r#"notices = ["10m"]"#)],
            "change-notices",
            ChangeKind::Neutral,
        ),
        (
            &[],
            &[(r#"language = "pt-BR""#, r#"language = "en""#)],
            "change-language",
            ChangeKind::Neutral,
        ),
    ];
    for &(current, proposed, id, kind) in cases {
        let result = classify(&config(current), &config(proposed));
        assert!(
            result
                .changes
                .iter()
                .any(|change| (change.id, change.kind) == (id, kind)),
            "{id}: {:?}",
            result.changes
        );
        assert_eq!(result.kind, kind, "{id}");
    }
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
