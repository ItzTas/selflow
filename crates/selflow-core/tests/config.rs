mod support;

use jiff::{
    SignedDuration,
    civil::{Weekday, time},
};
use selflow_core::config::{Config, parse};
use selflow_i18n::{catalog::default_config, language::Language};
use support::VALID;

#[test]
fn parses_a_valid_file_and_resolves_anchors() {
    let config = parse(VALID).unwrap();
    let set = config.rule_set_for(Weekday::Monday).unwrap();
    let lock = &set.locks[0];
    assert_eq!(
        (lock.start, lock.end),
        (time(23, 0, 0, 0), time(7, 0, 0, 0))
    );
    assert_eq!(lock.label, "bedtime");
    assert_eq!(
        lock.warnings,
        [15, 5, 1].map(SignedDuration::from_mins).to_vec()
    );
    assert_eq!(set.challenges.phrase, None);
    assert!(config.rule_set_for(Weekday::Friday).is_none());
}

#[test]
fn reports_every_error_with_its_line() {
    let text = VALID
        .replace(r#"wait = "2m""#, r#"wait = "soon""#)
        .replace(r#"start = "bedtime""#, r#"start = "bedtme""#);
    let found: Vec<_> = parse(&text)
        .unwrap_err()
        .iter()
        .map(|e| (e.id, e.line))
        .collect();
    assert_eq!(
        found,
        vec![("config-bad-duration", 15), ("config-unknown-anchor", 22)]
    );
}

#[test]
fn rejects_unknown_keys() {
    let text = VALID.replace(
        "language = \"pt-BR\"",
        "language = \"pt-BR\"\ncolour = \"red\"",
    );
    let errors = parse(&text).unwrap_err();
    assert_eq!((errors[0].id, errors[0].line), ("config-syntax", 3));
}

#[test]
fn rejects_a_day_claimed_by_two_rule_sets() {
    let text = format!("{VALID}\n[[rule_set]]\nname = \"weekend\"\ndays = [\"mon\", \"fri\"]\n");
    let errors = parse(&text).unwrap_err();
    assert_eq!(errors[0].id, "config-day-in-two-sets");
    assert_eq!(
        errors[0].args,
        vec![
            ("day", "mon".into()),
            ("first", "weeknight".into()),
            ("second", "weekend".into())
        ]
    );
}

#[test]
fn rejects_overlapping_locks_in_one_rule_set() {
    let text =
        format!("{VALID}\n[[rule_set.lock]]\nid = \"late\"\nstart = \"01:00\"\nend = \"06:00\"\n");
    assert_eq!(parse(&text).unwrap_err()[0].id, "config-locks-overlap");
}

#[test]
fn rejects_jitter_as_long_as_the_window() {
    let text = VALID.replace(r#"wake = "07:00""#, r#"wake = "23:15""#);
    assert_eq!(parse(&text).unwrap_err()[0].id, "config-jitter-too-long");
}

#[test]
fn both_default_files_describe_the_same_rules() {
    let without_labels = |mut config: Config| {
        config
            .rule_sets
            .iter_mut()
            .flat_map(|set| &mut set.locks)
            .for_each(|lock| lock.label.clear());
        config.rule_sets
    };
    let en = parse(default_config(Language::En)).unwrap();
    let pt = parse(default_config(Language::PtBr)).unwrap();
    assert_eq!(
        (en.language.as_str(), pt.language.as_str()),
        ("en", "pt-BR")
    );
    assert_eq!(
        en.rule_sets[0].locks[0].shutdown_after,
        Some(SignedDuration::from_mins(30))
    );
    assert_eq!(without_labels(en), without_labels(pt));
}

fn with_morning_lock(text: &str) -> String {
    format!("{text}\n[[rule_set.lock]]\nid = \"morning\"\nstart = \"06:00\"\nend = \"08:00\"\n")
}

fn error_ids(text: &str) -> Vec<&'static str> {
    parse(text).unwrap_err().iter().map(|e| e.id).collect()
}

#[test]
fn reports_an_empty_window_next_to_a_bad_jitter() {
    let text = VALID
        .replace(r#"start_jitter = "15m""#, r#"start_jitter = "soon""#)
        .replace(r#"end = "wake""#, r#"end = "bedtime""#);
    assert_eq!(
        error_ids(&text),
        ["config-bad-duration", "config-empty-window"]
    );
}

#[test]
fn reports_an_empty_window_when_the_day_start_is_bad() {
    let text = VALID
        .replace(r#"day_starts_at = "05:00""#, r#"day_starts_at = "5am""#)
        .replace(r#"end = "wake""#, r#"end = "bedtime""#);
    assert_eq!(error_ids(&text), ["config-bad-time", "config-empty-window"]);
}

#[test]
fn rejects_a_lock_that_overlaps_the_night_lock_after_the_day_start() {
    let errors = parse(&with_morning_lock(VALID)).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].id, "config-locks-overlap");
    assert_eq!(
        errors[0].args,
        vec![
            ("first", "bedtime".into()),
            ("second", "morning".into()),
            ("set", "weeknight".into())
        ]
    );
}

#[test]
fn compares_locks_across_the_day_start_only_on_consecutive_days() {
    let text = VALID.replace(
        r#"days = ["sun", "mon", "tue", "wed", "thu"]"#,
        r#"days = ["mon", "wed"]"#,
    );
    assert!(parse(&with_morning_lock(&text)).is_ok());
}
