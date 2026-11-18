mod support;

use jiff::{ZonedDateTime, civil::date};
use selflow_core::config::{Config, parse};
use selflow_core::enforce::{Enforcement, NightLog, evaluate};
use selflow_core::plan::windows_for;
use selflow_core::timing::Seed;
use support::{at, brt, no_jitter, one_extension};

#[test]
fn locks_from_bedtime_until_wake() {
    let c = parse(&no_jitter()).unwrap();
    let free = |now: ZonedDateTime| {
        matches!(
            evaluate(&now, &c, Seed(7), &NightLog::default()).enforcement,
            Enforcement::Free
        )
    };
    assert!(free(at(19, 22, 59)));
    assert!(free(at(20, 7, 0)));
    let Enforcement::Locked(lock) =
        evaluate(&at(19, 23, 0), &c, Seed(7), &NightLog::default()).enforcement
    else {
        panic!("expected a lock")
    };
    assert_eq!(lock.extensions_left, 1);
    assert_eq!(lock.extension_available_at, at(19, 23, 2).timestamp());
    assert_eq!(lock.shutdown_at, Some(at(19, 23, 30).timestamp()));
    assert_eq!(lock.window.ends_at, at(20, 7, 0));
}

#[test]
fn warns_fifteen_five_and_one_minute_before() {
    let c = parse(&no_jitter()).unwrap();
    let evaluation = evaluate(&at(19, 22, 0), &c, Seed(7), &NightLog::default());
    let times: Vec<_> = evaluation.warnings.iter().map(|w| w.at).collect();
    assert_eq!(
        times,
        [(22, 45), (22, 55), (22, 59)]
            .map(|(h, m)| at(19, h, m).timestamp())
            .to_vec()
    );
    assert_eq!(evaluation.next_change_at, Some(at(19, 22, 45).timestamp()));
}

#[test]
fn an_extension_frees_the_screen_and_the_lock_comes_back_after_it() {
    let c = parse(&no_jitter()).unwrap();
    let during = evaluate(&at(19, 23, 10), &c, Seed(7), &one_extension());
    assert!(matches!(during.enforcement, Enforcement::Free));
    let times: Vec<_> = during.warnings.iter().map(|w| w.at).collect();
    assert_eq!(
        times,
        [(23, 10), (23, 20), (23, 24)]
            .map(|(h, m)| at(19, h, m).timestamp())
            .to_vec()
    );
    let Enforcement::Locked(lock) =
        evaluate(&at(19, 23, 25), &c, Seed(7), &one_extension()).enforcement
    else {
        panic!("expected a lock")
    };
    assert_eq!(lock.locked_since, at(19, 23, 25).timestamp());
    assert_eq!(lock.extensions_left, 0);
    assert_eq!(lock.shutdown_at, Some(at(19, 23, 55).timestamp()));
}

#[test]
fn no_shutdown_when_the_lock_ends_first() {
    let short = parse(&no_jitter().replace(r#"wake = "07:00""#, r#"wake = "23:20""#)).unwrap();
    let Enforcement::Locked(lock) =
        evaluate(&at(19, 23, 0), &short, Seed(7), &NightLog::default()).enforcement
    else {
        panic!("expected a lock")
    };
    assert_eq!(lock.shutdown_at, None);
}

#[test]
fn a_started_lock_keeps_its_times_when_the_configuration_changes() {
    let c = parse(&no_jitter()).unwrap();
    let started = windows_for(&c, Seed(7), date(2026, 11, 19), &brt());
    let log = NightLog {
        started,
        ..NightLog::default()
    };
    let later =
        parse(&no_jitter().replace(r#"bedtime = "23:00""#, r#"bedtime = "01:00""#)).unwrap();
    let Enforcement::Locked(lock) = evaluate(&at(19, 23, 10), &later, Seed(7), &log).enforcement
    else {
        panic!("expected a lock")
    };
    assert_eq!(lock.window.ends_at, at(20, 7, 0));
    assert!(matches!(
        evaluate(&at(19, 23, 10), &Config::empty(), Seed(7), &log).enforcement,
        Enforcement::Locked(_)
    ));
}
