mod support;

use std::collections::BTreeSet;

use jiff::{
    SignedDuration,
    civil::{date, time},
    tz::TimeZone,
};
use selflow_core::timing::{
    Seed, TimingError, fnv1a64, instant_on, jitter, logical_date, next_after, parse_clock,
    parse_duration,
};
use support::{at, brt};

#[test]
fn parses_durations_in_the_friendly_format() {
    assert_eq!(
        parse_duration("15m").unwrap(),
        SignedDuration::from_mins(15)
    );
    assert_eq!(
        parse_duration("2h 30m").unwrap(),
        SignedDuration::from_mins(150)
    );
    assert!(matches!(
        parse_duration("0m"),
        Err(TimingError::NotPositive(_))
    ));
    assert!(matches!(
        parse_duration("1 day"),
        Err(TimingError::Invalid(_))
    ));
    assert!(matches!(
        parse_duration("soon"),
        Err(TimingError::Invalid(_))
    ));
}

#[test]
fn parses_clock_times() {
    assert_eq!(parse_clock("23:00").unwrap(), time(23, 0, 0, 0));
    assert!(parse_clock("25:00").is_err());
}

#[test]
fn early_hours_belong_to_the_previous_logical_day() {
    let starts = time(5, 0, 0, 0);
    assert_eq!(logical_date(&at(20, 1, 30), starts), date(2026, 11, 19));
    assert_eq!(logical_date(&at(19, 23, 0), starts), date(2026, 11, 19));
}

#[test]
fn places_times_before_the_day_start_on_the_next_civil_date() {
    let instant = instant_on(
        date(2026, 11, 19),
        time(0, 30, 0, 0),
        time(5, 0, 0, 0),
        &brt(),
    );
    assert_eq!(instant, at(20, 0, 30));
}

#[test]
fn jitter_is_stable_within_a_day_and_stays_in_the_window() {
    let window = SignedDuration::from_mins(15);
    let thursday = jitter(Seed(7), "weeknight/bedtime", date(2026, 11, 19), window);
    assert_eq!(
        thursday,
        jitter(Seed(7), "weeknight/bedtime", date(2026, 11, 19), window)
    );
    assert!(thursday >= SignedDuration::ZERO && thursday <= window);
    assert_eq!(thursday.as_secs() % 60, 0);
    let month: BTreeSet<_> = (1..=28)
        .map(|day| jitter(Seed(7), "weeknight/bedtime", date(2026, 2, day), window))
        .collect();
    assert!(month.len() > 1);
}

#[test]
fn fnv1a_matches_the_reference_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf29ce484222325);
    assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
}

#[test]
fn next_after_picks_the_same_day_when_the_time_is_later() {
    assert_eq!(next_after(&at(19, 10, 0), time(23, 0, 0, 0)), at(19, 23, 0));
}

#[test]
fn next_after_picks_the_next_day_when_the_time_is_earlier() {
    assert_eq!(next_after(&at(19, 10, 0), time(8, 0, 0, 0)), at(20, 8, 0));
}

#[test]
fn next_after_is_strictly_after() {
    assert_eq!(next_after(&at(19, 23, 0), time(23, 0, 0, 0)), at(20, 23, 0));
}

fn new_york() -> TimeZone {
    TimeZone::posix("EST5EDT,M3.2.0,M11.1.0").unwrap()
}

#[test]
fn instant_on_moves_a_time_in_a_spring_forward_gap_past_it() {
    let instant = instant_on(
        date(2026, 3, 8),
        time(2, 30, 0, 0),
        time(0, 0, 0, 0),
        &new_york(),
    );
    assert_eq!(instant.to_string(), "2026-03-08T03:30:00-04:00[-04:00]");
}

#[test]
fn next_after_skips_a_day_whose_time_falls_in_the_gap() {
    let after = date(2026, 3, 7)
        .at(12, 0, 0, 0)
        .to_zoned_datetime(new_york())
        .unwrap();
    let next = next_after(&after, time(2, 30, 0, 0));
    assert_eq!(next.date(), date(2026, 3, 9));
    assert_eq!(next.time(), time(2, 30, 0, 0));
}
