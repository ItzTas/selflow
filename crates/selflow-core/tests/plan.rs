mod support;

use jiff::{SignedDuration, civil::date};
use selflow_core::config::parse;
use selflow_core::plan::{windows_around, windows_for};
use selflow_core::timing::{Seed, jitter};
use support::{VALID, at, brt};

#[test]
fn a_window_runs_from_the_jittered_start_to_the_next_wake() {
    let windows = windows_for(&parse(VALID).unwrap(), Seed(7), date(2026, 11, 19), &brt());
    let offset = jitter(
        Seed(7),
        "weeknight/bedtime",
        date(2026, 11, 19),
        SignedDuration::from_mins(15),
    );
    assert_eq!(windows.len(), 1);
    assert_eq!(
        windows[0].starts_at,
        at(19, 23, 0).checked_add(offset).unwrap()
    );
    assert_eq!(windows[0].ends_at, at(20, 7, 0));
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
