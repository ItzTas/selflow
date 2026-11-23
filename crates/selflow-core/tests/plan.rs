mod support;

use jiff::{SignedDuration, civil::date};
use selflow_core::config::{Action, parse};
use selflow_core::plan::{windows_around, windows_for};
use selflow_core::timing::{Seed, jitter};
use support::{REMINDER, VALID, at, brt};

#[test]
fn a_window_runs_from_the_jittered_start_to_the_next_wake() {
    let windows = windows_for(&parse(VALID).unwrap(), Seed(7), date(2026, 11, 19), &brt());
    let offset = jitter(
        Seed(7),
        "bedtime",
        date(2026, 11, 19),
        SignedDuration::from_mins(15),
    );
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].key(), "bedtime");
    assert_eq!(windows[0].set, "weeknight");
    assert_eq!(
        windows[0].starts_at,
        at(19, 23, 0).checked_add(offset).unwrap()
    );
    assert_eq!(windows[0].ends_at, at(20, 7, 0));
}

#[test]
fn windows_end_at_their_length_or_the_day_boundary() {
    let day_end = windows_for(
        &parse(REMINDER).unwrap(),
        Seed(7),
        date(2026, 11, 19),
        &brt(),
    );
    assert_eq!(day_end.len(), 1);
    assert_eq!(day_end[0].action, Action::Nothing);
    assert_eq!(day_end[0].notices, [SignedDuration::ZERO]);
    assert_eq!(day_end[0].starts_at, at(19, 15, 0));
    assert_eq!(day_end[0].ends_at, at(20, 5, 0));
    let with_length = REMINDER.replace(
        "start = \"15:00\"\n",
        "start = \"15:00\"\nlength = \"10m\"\n",
    );
    let length = windows_for(
        &parse(&with_length).unwrap(),
        Seed(7),
        date(2026, 11, 19),
        &brt(),
    );
    assert_eq!(length[0].ends_at, at(19, 15, 10));
}

#[test]
fn renaming_a_rule_set_keeps_the_start() {
    let starts_on_the_19th = |text: &str| {
        windows_for(&parse(text).unwrap(), Seed(7), date(2026, 11, 19), &brt())[0]
            .starts_at
            .clone()
    };
    let renamed = VALID.replace(r#"name = "weeknight""#, r#"name = "nights""#);
    assert_eq!(starts_on_the_19th(&renamed), starts_on_the_19th(VALID));
}

#[test]
fn a_day_without_a_rule_set_has_no_window() {
    assert!(windows_for(&parse(VALID).unwrap(), Seed(7), date(2026, 11, 20), &brt()).is_empty());
}

#[test]
fn the_morning_after_still_sees_the_previous_night() {
    let windows = windows_around(&parse(VALID).unwrap(), Seed(7), &at(20, 6, 0));
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].logical_date, date(2026, 11, 19));
}
