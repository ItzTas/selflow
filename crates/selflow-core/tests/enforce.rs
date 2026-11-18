mod support;

use jiff::{SignedDuration, Timestamp, ZonedDateTime, civil::date};
use selflow_core::config::{Challenges, Config, parse};
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

#[test]
fn huge_durations_saturate_instead_of_overflowing() {
    let patient = parse(
        &no_jitter()
            .replace(r#"wait = "2m""#, r#"wait = "90000000h""#)
            .replace(
                r#"shutdown_after = "30m""#,
                r#"shutdown_after = "90000000h""#,
            ),
    )
    .unwrap();
    let Enforcement::Locked(lock) =
        evaluate(&at(19, 23, 0), &patient, Seed(7), &NightLog::default()).enforcement
    else {
        panic!("expected a lock")
    };
    assert_eq!(lock.extension_available_at, Timestamp::MAX);
    assert_eq!(lock.shutdown_at, None);
    let early = parse(&no_jitter().replace(
        r#"warnings = ["5m", "15m", "1m"]"#,
        r#"warnings = ["200000000h"]"#,
    ))
    .unwrap();
    assert!(matches!(
        evaluate(&at(19, 22, 0), &early, Seed(7), &NightLog::default()).enforcement,
        Enforcement::Free
    ));
}

#[test]
fn the_extension_budget_covers_the_whole_night() {
    let two_locks = parse(&format!(
        "{}\n[[rule_set.lock]]\nid = \"dawn\"\nstart = \"04:30\"\nend = \"04:50\"\n",
        no_jitter().replace(r#"wake = "07:00""#, r#"wake = "04:00""#)
    ))
    .unwrap();
    let Enforcement::Locked(lock) =
        evaluate(&at(20, 4, 40), &two_locks, Seed(7), &one_extension()).enforcement
    else {
        panic!("expected a lock")
    };
    assert_eq!(lock.window.rule_id, "dawn");
    assert_eq!(lock.window.logical_date, date(2026, 11, 19));
    assert_eq!(lock.extensions_left, 0);
}

#[test]
fn a_started_lock_follows_a_later_wake_and_keeps_its_end_over_an_earlier_one() {
    let c = parse(&no_jitter()).unwrap();
    let log = NightLog {
        started: windows_for(&c, Seed(7), date(2026, 11, 19), &brt()),
        ..NightLog::default()
    };
    let lock_with_wake = |wake: &str, now: ZonedDateTime| {
        let config =
            parse(&no_jitter().replace(r#"wake = "07:00""#, &format!(r#"wake = "{wake}""#)))
                .unwrap();
        let Enforcement::Locked(lock) = evaluate(&now, &config, Seed(7), &log).enforcement else {
            panic!("expected a lock")
        };
        lock
    };
    assert_eq!(
        lock_with_wake("08:00", at(20, 7, 30)).window.ends_at,
        at(20, 8, 0)
    );
    assert_eq!(
        lock_with_wake("06:00", at(20, 6, 30)).window.ends_at,
        at(20, 7, 0)
    );
}

#[test]
fn a_started_lock_takes_only_the_stricter_settings_of_a_new_configuration() {
    let c = parse(&no_jitter()).unwrap();
    let log = NightLog {
        started: windows_for(&c, Seed(7), date(2026, 11, 19), &brt()),
        ..NightLog::default()
    };
    let lock_under = |text: String| {
        let Enforcement::Locked(lock) =
            evaluate(&at(19, 23, 10), &parse(&text).unwrap(), Seed(7), &log).enforcement
        else {
            panic!("expected a lock")
        };
        lock
    };
    let stricter = lock_under(
        no_jitter()
            .replace(
                r#"wait = "2m""#,
                "wait = \"5m\"\nphrase = \"I choose to rest now\"",
            )
            .replace("extensions = 1", "extensions = 0")
            .replace(r#"extension_length = "20m""#, r#"extension_length = "10m""#)
            .replace(r#"id = "bedtime""#, "id = \"bedtime\"\nlabel = \"sleep\"")
            .replace(r#"warnings = ["5m", "15m", "1m"]"#, r#"warnings = ["10m"]"#)
            .replace(r#"shutdown_after = "30m""#, r#"shutdown_after = "10m""#),
    );
    assert_eq!(
        stricter.window.challenges,
        Challenges {
            wait: SignedDuration::from_mins(5),
            reason: true,
            phrase: Some("I choose to rest now".to_owned()),
            extensions: 0,
            extension_length: SignedDuration::from_mins(10),
        }
    );
    assert_eq!(
        stricter.window.shutdown_after,
        Some(SignedDuration::from_mins(10))
    );
    assert_eq!(stricter.window.label, "sleep");
    assert_eq!(stricter.window.warnings, [SignedDuration::from_mins(10)]);
    assert_eq!(stricter.window.starts_at, at(19, 23, 0));
    let looser = lock_under(
        no_jitter()
            .replace(r#"wait = "2m""#, r#"wait = "1m""#)
            .replace("reason = true", "reason = false")
            .replace("extensions = 1", "extensions = 3")
            .replace(r#"extension_length = "20m""#, r#"extension_length = "40m""#)
            .replace("shutdown_after = \"30m\"\n", ""),
    );
    assert_eq!(looser.window.challenges, log.started[0].challenges);
    assert_eq!(
        looser.window.shutdown_after,
        Some(SignedDuration::from_mins(30))
    );
}
