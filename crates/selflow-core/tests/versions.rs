mod support;

use jiff::ZonedDateTime;
use selflow_core::config::{Config, parse};
use selflow_core::versions::{
    ChangeKind, StoredVersion, classify, effective_at, in_force, pending,
};
use support::{at, no_jitter};

#[test]
fn moving_bedtime_later_loosens() {
    let current = parse(&no_jitter()).unwrap();
    let with = |from: &str, to: &str| parse(&no_jitter().replace(from, to)).unwrap();
    let result = classify(
        &current,
        &with(r#"bedtime = "23:00""#, r#"bedtime = "01:00""#),
    );
    assert_eq!(result.kind, ChangeKind::Loosen);
    assert_eq!(result.changes[0].id, "change-lock-starts-later");
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
        (ChangeKind::Loosen, "change-lock-removed")
    );
    assert_eq!(result.changes[0].args[1], ("days", "sun".into()));
}

#[test]
fn rewording_labels_and_warnings_is_neutral() {
    let current = parse(&no_jitter()).unwrap();
    let proposed = parse(
        &no_jitter()
            .replace("id = \"bedtime\"", "id = \"bedtime\"\nlabel = \"Sleep\"")
            .replace(r#"warnings = ["5m", "15m", "1m"]"#, r#"warnings = ["10m"]"#),
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
    assert_eq!(result.changes[0].id, "change-lock-starts-later");
    assert_eq!(
        result.changes[0].args[2..],
        [("old", "22:00–23:00".into()), ("new", "22:50".into())]
    );
}

#[test]
fn classifies_each_kind_of_change() {
    let config = |edits: &[(&str, &str)]| {
        let text = edits.iter().fold(no_jitter(), |text, (from, to)| {
            assert!(text.contains(from), "the fixture has no {from}");
            text.replace(from, to)
        });
        parse(&text).unwrap()
    };
    type Edits = &'static [(&'static str, &'static str)];
    const PHRASE: (&str, &str) = (
        "reason = true",
        "reason = true\nphrase = \"Rest is enough\"",
    );
    let cases: &[(Edits, Edits, &str, ChangeKind)] = &[
        (
            &[],
            &[(r#"wake = "07:00""#, r#"wake = "08:00""#)],
            "change-lock-ends-later",
            ChangeKind::Tighten,
        ),
        (
            &[],
            &[(r#"wake = "07:00""#, r#"wake = "06:00""#)],
            "change-lock-ends-earlier",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[(r#"bedtime = "23:00""#, r#"bedtime = "05:30""#)],
            "change-lock-ends-earlier",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[(
                r#"start = "bedtime""#,
                "start = \"22:30\"\nstart_jitter = \"60m\"",
            )],
            "change-lock-starts-later",
            ChangeKind::Loosen,
        ),
        (
            &[],
            &[(
                r#"start = "bedtime""#,
                "start = \"22:00\"\nstart_jitter = \"60m\"",
            )],
            "change-lock-starts-earlier",
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
            "change-lock-added",
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
            &[(r#"warnings = ["5m", "15m", "1m"]"#, r#"warnings = ["10m"]"#)],
            "change-warnings",
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
