mod support;

use std::path::Path;

use jiff::{SignedDuration, Timestamp, ZonedDateTime, civil::date};
use selflow_core::agent::governing;
use selflow_core::config::{Action, AgentLevel, Challenges, Config, parse};
use selflow_core::enforce::{
    DayLog, Enforcement, GrantedException, GrantedExtension, NoticeKind, ShutdownDone, evaluate,
    started_rule,
};
use selflow_core::plan::{RuleWindow, windows_for};
use selflow_core::timing::Seed;
use support::{LIGHTS_OUT, REMINDER, at, brt, no_jitter, one_extension, two_locks};

fn started_on_the_19th() -> DayLog {
    DayLog {
        started: windows_for(
            &parse(&no_jitter()).unwrap(),
            Seed(7),
            date(2026, 11, 19),
            &brt(),
        ),
        ..DayLog::default()
    }
}

fn with_save_only_step(extra: &str) -> String {
    format!(
        "{}\n[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"save-only\"\nmessage = \"Stop now.\"\n{extra}",
        no_jitter()
    )
}

fn started_with_step(extra: &str) -> DayLog {
    DayLog {
        started: windows_for(
            &parse(&with_save_only_step(extra)).unwrap(),
            Seed(7),
            date(2026, 11, 19),
            &brt(),
        ),
        ..DayLog::default()
    }
}

fn governed(log: &DayLog, extra: &str, cwd: &str, attended: bool) -> bool {
    let config = parse(&with_save_only_step(extra)).unwrap();
    let agent = evaluate(&at(19, 22, 30), &config, Seed(7), log).agent;
    assert_eq!(agent.len(), 1);
    assert_eq!(agent[0].message.as_deref(), Some("Stop now."));
    governing(&agent, Path::new(cwd), Some(Path::new("/home/u")), attended).is_some()
}

fn locked(now: &ZonedDateTime, config: &Config, log: &DayLog) -> bool {
    matches!(
        evaluate(now, config, Seed(7), log).enforcement,
        Enforcement::Locked(_)
    )
}

#[test]
fn locks_from_bedtime_until_wake() {
    let c = parse(&no_jitter()).unwrap();
    assert!(!locked(&at(19, 22, 59), &c, &DayLog::default()));
    assert!(!locked(&at(20, 7, 0), &c, &DayLog::default()));
    let Enforcement::Locked(lock) =
        evaluate(&at(19, 23, 0), &c, Seed(7), &DayLog::default()).enforcement
    else {
        panic!("expected a lock")
    };
    assert_eq!(lock.extensions_left, 1);
    assert_eq!(lock.extension_available_at, at(19, 23, 2).timestamp());
    assert_eq!(lock.shutdown_at, Some(at(19, 23, 30).timestamp()));
    assert_eq!(lock.window.ends_at, at(20, 7, 0));
}

#[test]
fn notifies_fifteen_five_and_one_minute_before_a_lock() {
    let c = parse(&no_jitter()).unwrap();
    let evaluation = evaluate(&at(19, 22, 0), &c, Seed(7), &DayLog::default());
    let times: Vec<_> = evaluation.notices.iter().map(|n| n.at).collect();
    assert_eq!(
        times,
        [(22, 45), (22, 55), (22, 59)]
            .map(|(h, m)| at(19, h, m).timestamp())
            .to_vec()
    );
    assert!(evaluation.notices.iter().all(|notice| {
        notice.kind == NoticeKind::Lock
            && notice.key == "bedtime"
            && notice.moment == at(19, 23, 0).timestamp()
            && notice.message.is_none()
    }));
    assert_eq!(evaluation.next_change_at, Some(at(19, 22, 45).timestamp()));
}

#[test]
fn a_lock_notice_at_its_start_stays_due_while_the_lock_runs() {
    let at_start =
        parse(&no_jitter().replace(r#"notices = ["5m", "15m", "1m"]"#, r#"notices = ["0m"]"#))
            .unwrap();
    let just_after = |hour, minute| {
        at(19, hour, minute)
            .checked_add(SignedDuration::from_millis(3))
            .unwrap()
    };
    let due = |now: ZonedDateTime, config: &Config, log: &DayLog| {
        let evaluation = evaluate(&now, config, Seed(7), log);
        assert!(matches!(evaluation.enforcement, Enforcement::Locked(_)));
        evaluation
            .notices
            .iter()
            .map(|notice| notice.at)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        due(just_after(23, 0), &at_start, &DayLog::default()),
        vec![at(19, 23, 0).timestamp()]
    );
    assert_eq!(
        due(just_after(23, 25), &at_start, &one_extension()),
        vec![at(19, 23, 25).timestamp()]
    );
    let before_start = parse(&no_jitter()).unwrap();
    assert_eq!(
        due(just_after(23, 0), &before_start, &DayLog::default()),
        vec![]
    );
}

#[test]
fn a_rule_without_a_lock_notifies_at_its_start_and_never_locks() {
    let c = parse(REMINDER).unwrap();
    let before = evaluate(&at(19, 14, 0), &c, Seed(7), &DayLog::default());
    assert_eq!(
        before.notices.iter().map(|n| n.at).collect::<Vec<_>>(),
        vec![at(19, 15, 0).timestamp()]
    );
    assert_eq!(before.notices[0].kind, NoticeKind::Reminder);
    assert_eq!(before.next_change_at, Some(at(19, 15, 0).timestamp()));
    assert!(matches!(
        evaluate(&at(19, 15, 0), &c, Seed(7), &DayLog::default()).enforcement,
        Enforcement::Free
    ));
}

#[test]
fn a_shutdown_rule_is_due_from_its_start_until_its_end() {
    let c = parse(LIGHTS_OUT).unwrap();
    let due = |now: ZonedDateTime| {
        let evaluation = evaluate(&now, &c, Seed(7), &DayLog::default());
        assert!(matches!(evaluation.enforcement, Enforcement::Free));
        evaluation.shutdown.map(|window| {
            (
                window.rule_id,
                window.logical_date,
                window.starts_at,
                window.ends_at,
            )
        })
    };
    let lights_out = Some((
        "lights-out".to_owned(),
        date(2026, 11, 19),
        at(20, 2, 0),
        at(20, 4, 0),
    ));
    assert_eq!(due(at(20, 1, 59)), None);
    assert_eq!(due(at(20, 2, 0)), lights_out.clone());
    assert_eq!(due(at(20, 3, 59)), lights_out);
    assert_eq!(due(at(20, 4, 0)), None);
}

#[test]
fn once_is_not_due_after_it_happened() {
    let log = DayLog {
        shutdowns: vec![ShutdownDone {
            rule_id: "lights-out".to_owned(),
            logical_date: date(2026, 11, 19),
            at: at(20, 2, 1).timestamp(),
        }],
        ..DayLog::default()
    };
    let due_on = |text: &str, now: ZonedDateTime| {
        evaluate(&now, &parse(text).unwrap(), Seed(7), &log)
            .shutdown
            .map(|window| window.logical_date)
    };
    assert_eq!(due_on(LIGHTS_OUT, at(20, 2, 30)), None);
    assert_eq!(due_on(LIGHTS_OUT, at(21, 2, 30)), Some(date(2026, 11, 20)));
    let repeat = LIGHTS_OUT.replace(r#"shutdown = "once""#, r#"shutdown = "repeat""#);
    assert_eq!(due_on(&repeat, at(20, 2, 30)), Some(date(2026, 11, 19)));
}

#[test]
fn a_custom_message_picks_one_text_per_day() {
    let c = parse(REMINDER).unwrap();
    let message_at = |now: ZonedDateTime| {
        evaluate(&now, &c, Seed(7), &DayLog::default()).notices[0]
            .message
            .clone()
            .unwrap()
    };
    let chosen = message_at(at(19, 14, 0));
    assert!(["Stand up for a minute.", "Time to stretch."].contains(&chosen.as_str()));
    assert_eq!(message_at(at(19, 14, 30)), chosen);
}

#[test]
fn an_extension_frees_the_screen_and_the_lock_comes_back_after_it() {
    let c = parse(&no_jitter()).unwrap();
    let during = evaluate(&at(19, 23, 10), &c, Seed(7), &one_extension());
    assert!(matches!(during.enforcement, Enforcement::Free));
    let times: Vec<_> = during.notices.iter().map(|n| n.at).collect();
    assert_eq!(
        times,
        [(23, 10), (23, 20), (23, 24)]
            .map(|(h, m)| at(19, h, m).timestamp())
            .to_vec()
    );
    assert!(
        during
            .notices
            .iter()
            .all(|notice| notice.kind == NoticeKind::Lock)
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
fn each_rule_has_its_own_extension_budget() {
    let c = parse(&two_locks()).unwrap();
    let log = DayLog {
        extensions: vec![GrantedExtension {
            rule_set: "weeknight".to_owned(),
            rule_id: "nap".to_owned(),
            logical_date: date(2026, 11, 19),
            granted_at: at(19, 14, 5).timestamp(),
            until: at(19, 14, 25).timestamp(),
            reason: "finishing a call".to_owned(),
        }],
        ..DayLog::default()
    };
    let Enforcement::Locked(nap) = evaluate(&at(19, 14, 25), &c, Seed(7), &log).enforcement else {
        panic!("expected the nap lock")
    };
    assert_eq!(nap.extensions_left, 0);
    let Enforcement::Locked(bedtime) = evaluate(&at(19, 23, 0), &c, Seed(7), &log).enforcement
    else {
        panic!("expected the bedtime lock")
    };
    assert_eq!(bedtime.window.rule_id, "bedtime");
    assert_eq!(bedtime.extensions_left, 1);
}

#[test]
fn no_shutdown_when_the_lock_ends_first() {
    let short = parse(&no_jitter().replace(r#"wake = "07:00""#, r#"wake = "23:20""#)).unwrap();
    let Enforcement::Locked(lock) =
        evaluate(&at(19, 23, 0), &short, Seed(7), &DayLog::default()).enforcement
    else {
        panic!("expected a lock")
    };
    assert_eq!(lock.shutdown_at, None);
}

#[test]
fn an_exception_day_skips_its_rules() {
    let c = parse(&format!(
        "{}\n[[exception]]\ndate = \"2026-11-19\"\nrules = [\"bedtime\"]\n",
        no_jitter()
    ))
    .unwrap();
    assert!(!locked(&at(19, 23, 30), &c, &DayLog::default()));
    assert!(locked(&at(18, 23, 30), &c, &DayLog::default()));
}

#[test]
fn a_granted_exception_drops_a_started_window() {
    let c = parse(&no_jitter()).unwrap();
    let log = DayLog {
        exceptions: vec![GrantedException {
            rule_id: "bedtime".to_owned(),
            logical_date: date(2026, 11, 19),
            granted_at: at(19, 22, 50).timestamp(),
            reason: "on call tonight".to_owned(),
        }],
        ..started_on_the_19th()
    };
    let evaluation = evaluate(&at(19, 23, 30), &c, Seed(7), &log);
    assert!(matches!(evaluation.enforcement, Enforcement::Free));
    assert!(evaluation.notices.is_empty());
    assert!(locked(&at(18, 23, 30), &c, &log));
}

#[test]
fn a_loosening_after_the_first_notice_does_not_move_the_coming_lock() {
    let log = started_on_the_19th();
    let later =
        parse(&no_jitter().replace(r#"bedtime = "23:00""#, r#"bedtime = "01:00""#)).unwrap();
    let Enforcement::Locked(lock) = evaluate(&at(19, 23, 0), &later, Seed(7), &log).enforcement
    else {
        panic!("expected a lock")
    };
    assert_eq!(lock.window.starts_at, at(19, 23, 0));
    assert_eq!(lock.window.ends_at, at(20, 7, 0));
    assert!(locked(&at(19, 23, 10), &Config::empty(), &log));
}

#[test]
fn a_tightening_after_the_first_notice_brings_the_lock_earlier() {
    let log = started_on_the_19th();
    let earlier =
        parse(&no_jitter().replace(r#"bedtime = "23:00""#, r#"bedtime = "22:50""#)).unwrap();
    assert!(!locked(&at(19, 22, 49), &earlier, &log));
    assert!(locked(&at(19, 22, 50), &earlier, &log));
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
        evaluate(&at(19, 23, 0), &patient, Seed(7), &DayLog::default()).enforcement
    else {
        panic!("expected a lock")
    };
    assert_eq!(lock.extension_available_at, Timestamp::MAX);
    assert_eq!(lock.shutdown_at, None);
    let early = parse(&no_jitter().replace(
        r#"notices = ["5m", "15m", "1m"]"#,
        r#"notices = ["200000000h"]"#,
    ))
    .unwrap();
    assert!(!locked(&at(19, 22, 0), &early, &DayLog::default()));
}

#[test]
fn a_started_lock_follows_a_later_wake_and_keeps_its_end_over_an_earlier_one() {
    let log = started_on_the_19th();
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
    let log = started_on_the_19th();
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
            .replace(
                r#"id = "bedtime""#,
                "id = \"bedtime\"\nlabel = \"sleep\"\nmessage = \"Good night.\"",
            )
            .replace(r#"notices = ["5m", "15m", "1m"]"#, r#"notices = ["10m"]"#)
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
        stricter.window.action,
        Action::Lock {
            shutdown_after: Some(SignedDuration::from_mins(10))
        }
    );
    assert_eq!(stricter.window.label, "sleep");
    assert_eq!(stricter.window.message, ["Good night."]);
    assert_eq!(stricter.window.notices, [SignedDuration::from_mins(10)]);
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
        looser.window.action,
        Action::Lock {
            shutdown_after: Some(SignedDuration::from_mins(30))
        }
    );
}

#[test]
fn a_test_window_stays_apart_from_the_real_window_of_its_rule() {
    let c = parse(&no_jitter()).unwrap();
    let real = windows_for(&c, Seed(7), date(2026, 11, 19), &brt()).remove(0);
    let test = RuleWindow {
        rule_id: "bedtime#test-1".to_owned(),
        starts_at: at(19, 20, 0),
        ends_at: at(19, 20, 30),
        action: Action::Lock {
            shutdown_after: None,
        },
        test: true,
        ..real.clone()
    };
    let log = DayLog {
        started: vec![test.clone()],
        extensions: vec![GrantedExtension {
            rule_set: "weeknight".to_owned(),
            rule_id: test.rule_id.clone(),
            logical_date: date(2026, 11, 19),
            granted_at: at(19, 20, 2).timestamp(),
            until: at(19, 20, 22).timestamp(),
            reason: "checking the lock screen".to_owned(),
        }],
        ..DayLog::default()
    };
    let during = evaluate(&at(19, 20, 25), &c, Seed(7), &log);
    let Enforcement::Locked(lock) = during.enforcement else {
        panic!("expected the test lock")
    };
    assert_eq!(lock.window, test);
    assert_eq!(lock.extensions_left, 0);
    assert!(
        during.notices.iter().any(|notice| {
            notice.key == "bedtime" && notice.moment == at(19, 23, 0).timestamp()
        })
    );
    let Enforcement::Locked(lock) = evaluate(&at(19, 23, 0), &c, Seed(7), &log).enforcement else {
        panic!("expected the real lock")
    };
    assert_eq!(lock.window, real);
    assert!(!lock.window.test);
    assert_eq!(lock.extensions_left, 1);
}

#[test]
fn widening_the_folders_of_a_started_step_covers_the_new_folder_at_once() {
    let log = started_with_step("folders = [\"~/work\"]\n");
    let widened = "folders = [\"~/work\", \"~/notes\"]\n";
    assert!(governed(&log, widened, "/home/u/notes", true));
    assert!(governed(&log, widened, "/home/u/work/a", true));
    assert!(!governed(&log, widened, "/home/u/music", true));
}

#[test]
fn a_started_step_without_folders_on_either_side_covers_every_session() {
    let narrow = started_with_step("folders = [\"~/work\"]\n");
    assert!(governed(&narrow, "", "/home/u/notes", true));
    let everywhere = started_with_step("");
    assert!(governed(
        &everywhere,
        "folders = [\"~/work\"]\n",
        "/home/u/notes",
        true
    ));
}

#[test]
fn a_started_step_turning_unattended_applies_at_once() {
    let log = started_with_step("");
    assert!(governed(&log, "unattended = true\n", "/home/u/x", false));
    let unattended = started_with_step("unattended = true\n");
    assert!(governed(&unattended, "", "/home/u/x", false));
}

#[test]
fn narrowing_the_folders_of_a_started_step_keeps_the_started_folders() {
    let log = started_with_step("folders = [\"~/work\", \"~/notes\"]\n");
    assert!(governed(
        &log,
        "folders = [\"~/work\"]\n",
        "/home/u/notes",
        true
    ));
}

#[test]
fn a_started_window_keeps_its_action_and_steps_in_the_rule() {
    let locking = parse(&with_save_only_step("")).unwrap();
    let started = windows_for(&locking, Seed(7), date(2026, 11, 19), &brt()).remove(0);
    let plain = parse(&no_jitter().replace("lock = true\nshutdown_after = \"30m\"\n", "")).unwrap();
    let rule = plain.rule_sets[0].rules[0].clone();
    assert!(!rule.enforces());
    let judged = started_rule(&rule, Some(&started));
    assert!(judged.enforces());
    assert_eq!(judged.action, started.action);
    assert_eq!(judged.agent.len(), 1);
    assert_eq!(judged.agent[0].level, AgentLevel::SaveOnly);
    assert_eq!(started_rule(&rule, None), rule);
}

#[test]
fn a_started_window_without_an_action_takes_the_action_of_the_rule() {
    let plain = parse(&no_jitter().replace("lock = true\nshutdown_after = \"30m\"\n", "")).unwrap();
    let started = windows_for(&plain, Seed(7), date(2026, 11, 19), &brt()).remove(0);
    assert_eq!(started.action, Action::Nothing);
    let locking = parse(&no_jitter()).unwrap();
    let rule = locking.rule_sets[0].rules[0].clone();
    let judged = started_rule(&rule, Some(&started));
    assert!(judged.enforces());
    assert_eq!(judged.action, rule.action);
}
