mod support;

use jiff::{ZonedDateTime, civil::date};
use selflow_core::challenge::{ExtensionAnswer, Refusal, request_extension};
use selflow_core::config::{Config, parse};
use selflow_core::enforce::{ActiveLock, Enforcement, NightLog, evaluate};
use selflow_core::timing::Seed;
use support::{at, no_jitter, one_extension};

fn lock_at(now: ZonedDateTime, config: &Config, log: &NightLog) -> ActiveLock {
    match evaluate(&now, config, Seed(7), log).enforcement {
        Enforcement::Locked(lock) => lock,
        Enforcement::Free => panic!("expected a lock"),
    }
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
        &NightLog::default(),
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
        &NightLog::default(),
    );
    assert_eq!(
        request_extension(at(19, 23, 3).timestamp(), &lock, &answer("   ok    ", "")),
        Err(Refusal::ReasonTooShort { min_chars: 10 })
    );
}

#[test]
fn grants_the_extension_length_but_never_past_the_end_of_the_lock() {
    let config = parse(&no_jitter()).unwrap();
    let evening = lock_at(at(19, 23, 3), &config, &NightLog::default());
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
    let morning = lock_at(at(20, 6, 50), &config, &NightLog::default());
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
    let lock = lock_at(at(19, 23, 3), &parse(&text).unwrap(), &NightLog::default());
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
