mod support;

use jiff::{ZonedDateTime, civil::date};
use selflow_core::challenge::{ExtensionAnswer, Refusal, request_exception, request_extension};
use selflow_core::config::{Config, Rule, parse};
use selflow_core::enforce::{ActiveLock, DayLog, Enforcement, GrantedException, evaluate};
use selflow_core::timing::Seed;
use support::{REMINDER, at, no_jitter, one_extension};

fn lock_at(now: ZonedDateTime, config: &Config, log: &DayLog) -> ActiveLock {
    match evaluate(&now, config, Seed(7), log).enforcement {
        Enforcement::Locked(lock) => lock,
        Enforcement::Free => panic!("expected a lock"),
    }
}

fn rule_on_the_19th(text: &str) -> Rule {
    parse(text).unwrap().rules_on(date(2026, 11, 19).weekday())[0].clone()
}

fn answer(reason: &str, phrase: &str) -> ExtensionAnswer {
    ExtensionAnswer {
        reason: reason.into(),
        phrase: phrase.into(),
    }
}

#[test]
fn refuses_before_the_wait_is_over() {
    let lock = lock_at(
        at(19, 23, 1),
        &parse(&no_jitter()).unwrap(),
        &DayLog::default(),
    );
    assert_eq!(
        request_extension(
            at(19, 23, 1).timestamp(),
            &lock,
            &answer("talking to my sister", "")
        ),
        Err(Refusal::TooEarly {
            available_at: at(19, 23, 2).timestamp()
        })
    );
}

#[test]
fn refuses_a_reason_that_is_too_short() {
    let lock = lock_at(
        at(19, 23, 3),
        &parse(&no_jitter()).unwrap(),
        &DayLog::default(),
    );
    assert_eq!(
        request_extension(at(19, 23, 3).timestamp(), &lock, &answer("   ok    ", "")),
        Err(Refusal::ReasonTooShort { min_chars: 10 })
    );
}

#[test]
fn grants_the_extension_length_but_never_past_the_end_of_the_lock() {
    let config = parse(&no_jitter()).unwrap();
    let evening = lock_at(at(19, 23, 3), &config, &DayLog::default());
    let granted = request_extension(
        at(19, 23, 3).timestamp(),
        &evening,
        &answer("talking to my sister", ""),
    )
    .unwrap();
    assert_eq!(granted.until, at(19, 23, 23).timestamp());
    assert_eq!(
        (granted.rule_set.as_str(), granted.rule_id.as_str()),
        ("weeknight", "bedtime")
    );
    assert_eq!(granted.logical_date, date(2026, 11, 19));
    let morning = lock_at(at(20, 6, 50), &config, &DayLog::default());
    let granted = request_extension(
        at(20, 6, 50).timestamp(),
        &morning,
        &answer("talking to my sister", ""),
    )
    .unwrap();
    assert_eq!(granted.until, at(20, 7, 0).timestamp());
}

#[test]
fn refuses_when_the_night_budget_is_spent() {
    let lock = lock_at(
        at(19, 23, 30),
        &parse(&no_jitter()).unwrap(),
        &one_extension(),
    );
    assert_eq!(
        request_extension(
            at(19, 23, 30).timestamp(),
            &lock,
            &answer("talking to my sister", "")
        ),
        Err(Refusal::NoBudget)
    );
}

#[test]
fn accepts_the_phrase_whatever_the_case_and_spacing() {
    let text = no_jitter().replace(
        "extensions = 1",
        "extensions = 1\nphrase = \"I choose to stay up\"",
    );
    let lock = lock_at(at(19, 23, 3), &parse(&text).unwrap(), &DayLog::default());
    let now = at(19, 23, 3).timestamp();
    assert!(
        request_extension(
            now,
            &lock,
            &answer("talking to my sister", " i  choose to STAY up ")
        )
        .is_ok()
    );
    assert_eq!(
        request_extension(now, &lock, &answer("talking to my sister", "i choose")),
        Err(Refusal::PhraseMismatch)
    );
}

#[test]
fn an_exception_waits_from_its_request() {
    let bedtime = rule_on_the_19th(&no_jitter());
    let requested_at = at(19, 20, 0).timestamp();
    let ask = |now: ZonedDateTime| {
        request_exception(
            now.timestamp(),
            requested_at,
            &bedtime,
            date(2026, 11, 19),
            1,
            &answer("  on call tonight  ", ""),
        )
    };
    assert_eq!(
        ask(at(19, 20, 1)),
        Err(Refusal::TooEarly {
            available_at: at(19, 20, 2).timestamp()
        })
    );
    assert_eq!(
        ask(at(19, 20, 2)),
        Ok(GrantedException {
            rule_id: "bedtime".to_owned(),
            logical_date: date(2026, 11, 19),
            granted_at: at(19, 20, 2).timestamp(),
            reason: "on call tonight".to_owned(),
        })
    );
}

#[test]
fn an_exception_needs_an_extension_left() {
    let bedtime = rule_on_the_19th(&no_jitter());
    let requested_at = at(19, 20, 0).timestamp();
    let ask = |now: ZonedDateTime, extensions_left, reason| {
        request_exception(
            now.timestamp(),
            requested_at,
            &bedtime,
            date(2026, 11, 19),
            extensions_left,
            &answer(reason, ""),
        )
    };
    assert_eq!(
        ask(at(19, 20, 1), 0, "on call tonight"),
        Err(Refusal::TooEarly {
            available_at: at(19, 20, 2).timestamp()
        })
    );
    assert_eq!(ask(at(19, 20, 2), 0, "ok"), Err(Refusal::NoBudget));
    assert_eq!(
        ask(at(19, 20, 2), 1, "ok"),
        Err(Refusal::ReasonTooShort { min_chars: 10 })
    );
}

#[test]
fn a_rule_with_only_notices_is_skipped_without_challenges() {
    let stretch = rule_on_the_19th(REMINDER);
    let now = at(19, 14, 0).timestamp();
    assert_eq!(
        request_exception(now, now, &stretch, date(2026, 11, 19), 0, &answer("", "")),
        Ok(GrantedException {
            rule_id: "stretch".to_owned(),
            logical_date: date(2026, 11, 19),
            granted_at: now,
            reason: String::new(),
        })
    );
}
