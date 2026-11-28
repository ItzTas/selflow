mod support;

use std::fs::{Permissions, set_permissions};
use std::os::unix::fs::PermissionsExt;

use std::sync::{Arc, Mutex};

use jiff::SignedDuration;
use jiff::civil::date;
use selflow_i18n::language::Language;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_protocol::ipc::{ExtensionSource, MessageView, ReportEntry, Request, Response};
use selflowd::enforcer::PowerMode;
use selflowd::ipc::handle;
use selflowd::store::{EventKind, Store};
use serde_json::{Value, json};
use support::{
    Call, Calls, LIGHTS_OUT, REMINDER, TestClock, at, brt, engine_on, engine_on_boot, extension,
    no_jitter, save_at, save_text, second, setup, setup_on, setup_over, shared_over,
    start_engine_with, steps,
};

#[test]
fn keeps_enforcing_when_the_store_cannot_record() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/selflow.db");
    let (mut engine, calls, clock, store) = setup_on(Store::open(&path).unwrap(), at(19, 23, 30));
    let state = path.parent().unwrap();
    set_permissions(state, Permissions::from_mode(0o500)).unwrap();
    let mut ticks = vec![engine.tick()];
    for moment in [at(19, 23, 31), at(20, 7, 0)] {
        clock.set(moment);
        ticks.push(engine.tick());
    }
    set_permissions(state, Permissions::from_mode(0o700)).unwrap();
    assert!(ticks.iter().all(Result::is_ok));
    assert_eq!(
        calls.take(),
        vec![
            Call::Show("bedtime".into(), 1),
            Call::Show("bedtime".into(), 1),
            Call::ShutDown,
            Call::Hide
        ]
    );
    assert_eq!(
        store
            .lock()
            .unwrap()
            .events_for(date(2026, 11, 19))
            .unwrap(),
        vec![]
    );
}

#[test]
fn locks_at_bedtime_and_records_the_start_once() {
    let (mut engine, calls, _clock, store) = setup(at(19, 23, 0));
    engine.tick().unwrap();
    engine.tick().unwrap();
    assert_eq!(
        calls.take(),
        vec![
            Call::Show("bedtime".into(), 1),
            Call::Show("bedtime".into(), 1)
        ]
    );
    assert_eq!(
        store
            .lock()
            .unwrap()
            .events_for(date(2026, 11, 19))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn sends_a_warning_once() {
    let (mut engine, calls, _clock, _store) = setup(at(19, 22, 45));
    engine.tick().unwrap();
    engine.tick().unwrap();
    assert_eq!(
        calls
            .take()
            .iter()
            .filter(|c| matches!(c, Call::Warn(_)))
            .count(),
        1
    );
}

#[test]
fn locks_after_the_wall_clock_jumps_past_bedtime() {
    let (mut engine, calls, clock, _store) = setup(at(19, 22, 30));
    assert_eq!(engine.tick().unwrap(), at(19, 22, 31).timestamp());
    clock.set(at(19, 23, 30));
    engine.tick().unwrap();
    clock.set(at(19, 23, 31));
    engine.tick().unwrap();
    let calls = calls.take();
    assert!(!calls.iter().any(|c| matches!(c, Call::Warn(_))));
    assert_eq!(
        calls[1..],
        [
            Call::Show("bedtime".into(), 1),
            Call::Show("bedtime".into(), 1),
            Call::ShutDown
        ]
    );
}

#[test]
fn a_restart_keeps_the_spent_budget_and_one_start_event() {
    let (mut first, _calls, clock, store) = setup(at(19, 23, 30));
    store
        .lock()
        .unwrap()
        .record_extension(&extension((23, 5), (23, 25)))
        .unwrap();
    first.tick().unwrap();
    drop(first);
    let (mut second, calls) = engine_on(store.clone(), clock.clone());
    second.tick().unwrap();
    assert_eq!(calls.take(), vec![Call::Show("bedtime".into(), 0)]);
    let events = store
        .lock()
        .unwrap()
        .events_for(date(2026, 11, 19))
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == EventKind::LockStarted)
            .count(),
        1
    );
}

#[test]
fn a_rule_that_just_came_into_force_waits_its_full_shutdown_time() {
    let store = Store::open_in_memory().unwrap();
    save_at(&store, &no_jitter(), at(19, 23, 40));
    let (mut engine, calls, clock, store) = setup_over(store, at(19, 23, 40));
    engine.tick().unwrap();
    clock.set(at(19, 23, 42));
    engine.tick().unwrap();
    assert_eq!(
        calls.take(),
        vec![
            Call::Show("bedtime".into(), 1),
            Call::Show("bedtime".into(), 1)
        ]
    );
    assert_eq!(
        calls.last_view().and_then(|view| view.shutdown_at),
        Some(at(20, 0, 10).timestamp())
    );
    clock.set(
        date(2026, 11, 20)
            .at(0, 9, 30, 0)
            .to_zoned_datetime(brt())
            .unwrap(),
    );
    assert_eq!(engine.tick().unwrap(), at(20, 0, 10).timestamp());
    clock.set(at(20, 0, 10));
    engine.tick().unwrap();
    assert_eq!(
        calls.take(),
        vec![
            Call::Show("bedtime".into(), 1),
            Call::Show("bedtime".into(), 1),
            Call::ShutDown
        ]
    );
    let shutdowns: Vec<_> = store
        .lock()
        .unwrap()
        .events_for(date(2026, 11, 19))
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == EventKind::Shutdown)
        .map(|event| event.at)
        .collect();
    assert_eq!(shutdowns, vec![at(20, 0, 10).timestamp()]);
}

#[test]
fn a_neutral_save_during_a_lock_does_not_postpone_its_shutdown() {
    let (mut engine, calls, clock, store) = setup(at(19, 23, 30));
    let relabeled = no_jitter().replace(
        "id = \"bedtime\"\n",
        "id = \"bedtime\"\nlabel = \"sleep\"\n",
    );
    save_at(&store.lock().unwrap(), &relabeled, at(19, 23, 20));
    engine.tick().unwrap();
    clock.set(at(19, 23, 31));
    engine.tick().unwrap();
    assert_eq!(
        calls.take(),
        vec![
            Call::Show("sleep".into(), 1),
            Call::Show("sleep".into(), 1),
            Call::ShutDown
        ]
    );
}

#[test]
fn a_loosening_waits_while_a_tightening_from_the_same_save_applies() {
    let shown = |calls: &Calls| -> Vec<Call> {
        calls
            .take()
            .into_iter()
            .filter(|call| !matches!(call, Call::Warn(_)))
            .collect()
    };
    let (mut engine, calls, clock, store) = setup(at(18, 22, 0));
    let text = no_jitter()
        .replace(r#"bedtime = "23:00""#, r#"bedtime = "22:00""#)
        .replace("extensions = 1", "extensions = 3");
    save_at(&store.lock().unwrap(), &text, at(18, 12, 0));
    engine.tick().unwrap();
    assert_eq!(shown(&calls), [Call::Show("bedtime".into(), 1)]);
    clock.set(at(19, 22, 0));
    engine.tick().unwrap();
    assert_eq!(shown(&calls), [Call::Show("bedtime".into(), 3)]);
}

#[test]
fn a_shutdown_that_would_come_after_the_lock_ends_is_dropped() {
    let store = Store::open_in_memory().unwrap();
    save_at(&store, &no_jitter(), at(20, 6, 40));
    let (mut engine, calls, clock, _store) = setup_over(store, at(20, 6, 40));
    engine.tick().unwrap();
    assert_eq!(calls.last_view().map(|view| view.shutdown_at), Some(None));
    for minute in [41, 59] {
        clock.set(at(20, 6, minute));
        engine.tick().unwrap();
    }
    clock.set(at(20, 7, 0));
    engine.tick().unwrap();
    assert_eq!(
        calls.take(),
        vec![
            Call::Show("bedtime".into(), 1),
            Call::Show("bedtime".into(), 1),
            Call::Show("bedtime".into(), 1),
            Call::Hide
        ]
    );
}

#[test]
fn a_test_lock_shows_marks_its_events_and_never_shuts_down() {
    let (mut engine, calls, clock, store) = setup(at(19, 20, 0));
    let shared = shared_over(store.clone(), clock.clone(), PowerMode::LogOnly);
    let request = Request::TestLock {
        rule: None,
        duration: SignedDuration::from_mins(40),
    };
    assert!(matches!(
        handle(request, &shared),
        Response::TestLockStarted { .. }
    ));
    engine.tick().unwrap();
    for minute in [30, 40] {
        clock.set(at(19, 20, minute));
        engine.tick().unwrap();
    }
    assert_eq!(
        calls.take(),
        vec![
            Call::Show("bedtime".into(), 1),
            Call::Show("bedtime".into(), 1),
            Call::Hide
        ]
    );
    let events: Vec<(EventKind, Value)> = store
        .lock()
        .unwrap()
        .events_for(date(2026, 11, 19))
        .unwrap()
        .into_iter()
        .map(|event| {
            let detail = serde_json::from_str(event.detail.as_deref().unwrap()).unwrap();
            (event.kind, detail)
        })
        .collect();
    let marked = json!({ "label": "bedtime", "test": true });
    assert_eq!(
        events,
        vec![
            (EventKind::LockStarted, marked.clone()),
            (EventKind::LockEnded, marked)
        ]
    );
}

fn warnings(calls: &Calls) -> Vec<String> {
    calls
        .take()
        .into_iter()
        .filter_map(|call| match call {
            Call::Warn(body) => Some(body),
            _ => None,
        })
        .collect()
}

#[test]
fn a_reminder_rule_shows_its_own_text_at_its_start() {
    let store = Store::open_in_memory().unwrap();
    save_text(&store, REMINDER);
    let (mut engine, calls, _clock, _store) = setup_over(store, at(19, 15, 0));
    engine.tick().unwrap();
    let warned = warnings(&calls);
    assert_eq!(warned.len(), 1);
    assert!(["Stand up for a minute.", "Time to stretch."].contains(&warned[0].as_str()));
}

#[test]
fn the_first_notice_records_the_window() {
    let (mut engine, calls, _clock, store) = setup(at(19, 22, 45));
    engine.tick().unwrap();
    assert_eq!(warnings(&calls).len(), 1);
    let started = store
        .lock()
        .unwrap()
        .night_log(&[date(2026, 11, 19)])
        .unwrap()
        .started;
    assert_eq!(
        started
            .iter()
            .map(|window| (window.rule_id.as_str(), window.starts_at.clone()))
            .collect::<Vec<_>>(),
        vec![("bedtime", at(19, 23, 0))]
    );
}

#[test]
fn a_shutdown_notice_with_its_own_text_still_says_it_shuts_down() {
    let store = Store::open_in_memory().unwrap();
    save_text(
        &store,
        r#"
language = "en"

[[rule_set]]
name = "always"
days = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]

[[rule_set.rule]]
id = "lights-out"
start = "02:00"
end = "04:00"
notices = ["10m"]
shutdown = "once"
message = "Time to stop."
"#,
    );
    let (mut engine, calls, _clock, _store) = setup_over(store, at(20, 1, 50));
    engine.tick().unwrap();
    assert_eq!(warnings(&calls), ["Time to stop."]);
    let title = Localizer::new(Language::En).text(
        "notice-shutdown-title",
        &[
            ("label", Arg::Text("lights-out".to_owned())),
            ("time", Arg::Text("02:00".to_owned())),
        ],
    );
    assert_eq!(calls.last_title(), Some(title));
}

#[test]
fn a_tightening_after_the_first_notice_keeps_the_earlier_start_through_the_lock() {
    let (mut engine, calls, clock, store) = setup(at(19, 22, 45));
    engine.tick().unwrap();
    let earlier = no_jitter().replace(r#"bedtime = "23:00""#, r#"bedtime = "22:50""#);
    save_at(&store.lock().unwrap(), &earlier, at(19, 22, 46));
    for minute in [(22, 50), (23, 1)] {
        clock.set(at(19, minute.0, minute.1));
        engine.tick().unwrap();
    }
    assert_eq!(
        calls.last_view().and_then(|view| view.shutdown_at),
        Some(at(19, 23, 20).timestamp())
    );
    let store = store.lock().unwrap();
    let started = store.night_log(&[date(2026, 11, 19)]).unwrap().started;
    assert_eq!(
        started
            .iter()
            .map(|window| window.starts_at.clone())
            .collect::<Vec<_>>(),
        vec![at(19, 22, 50)]
    );
    let lock_starts = store
        .events_for(date(2026, 11, 19))
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == EventKind::LockStarted)
        .count();
    assert_eq!(lock_starts, 1);
}

fn rule_shutdowns(store: &Mutex<Store>) -> Vec<(String, Value)> {
    store
        .lock()
        .unwrap()
        .events_for(date(2026, 11, 19))
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == EventKind::RuleShutdown)
        .map(|event| {
            let detail = serde_json::from_str(event.detail.as_deref().unwrap()).unwrap();
            (event.key, detail)
        })
        .collect()
}

fn shutdown_now_text(time: &str) -> String {
    Localizer::new(Language::En).text(
        "notice-shutdown-now",
        &[("time", Arg::Text(time.to_owned()))],
    )
}

fn after_the_first_shutdown(text: &str) -> (Arc<Mutex<Store>>, Arc<TestClock>) {
    let (mut engine, calls, clock, store) = start_engine_with(text, at(20, 2, 0));
    engine.tick().unwrap();
    clock.set(at(20, 2, 1));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["warn", "shut down"]);
    (store, clock)
}

#[test]
fn a_shutdown_without_a_lock_gives_a_minute_of_notice() {
    let (mut engine, calls, clock, _store) = start_engine_with(LIGHTS_OUT, at(20, 2, 0));
    assert_eq!(engine.tick().unwrap(), at(20, 2, 1).timestamp());
    let en = Localizer::new(Language::En);
    let time = || ("time", Arg::Text("02:01".to_owned()));
    assert_eq!(
        calls.take(),
        [Call::Hide, Call::Warn(shutdown_now_text("02:01"))]
    );
    assert_eq!(
        calls.last_title(),
        Some(en.text(
            "notice-shutdown-title",
            &[("label", Arg::Text("lights-out".to_owned())), time()]
        ))
    );
    clock.set(second(20, 2, 0, 59));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
    clock.set(at(20, 2, 1));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn once_stays_up_after_a_reboot() {
    let (store, clock) = after_the_first_shutdown(LIGHTS_OUT);
    clock.set(at(20, 2, 30));
    let (mut engine, calls) = engine_on_boot(store.clone(), clock.clone(), "boot-2");
    engine.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
    assert_eq!(
        rule_shutdowns(&store),
        [(
            "lights-out/2026-11-19".to_owned(),
            json!({ "label": "lights-out", "rule": "lights-out", "repeat": false })
        )]
    );
    let shared = shared_over(store, clock, PowerMode::LogOnly);
    let request = Request::Report {
        logical_date: Some(date(2026, 11, 19)),
    };
    let Response::Report(report) = handle(request, &shared) else {
        panic!("expected Report")
    };
    assert_eq!(
        report.entries,
        [ReportEntry {
            at: at(20, 2, 1).timestamp(),
            message: MessageView {
                id: "report-rule-shutdown".to_owned(),
                args: vec![("label".to_owned(), "lights-out".to_owned())],
            },
        }]
    );
}

#[test]
fn repeat_shuts_down_again_after_each_reboot_until_the_rule_ends() {
    let repeat = LIGHTS_OUT.replace(r#"shutdown = "once""#, r#"shutdown = "repeat""#);
    let (store, clock) = after_the_first_shutdown(&repeat);
    clock.set(at(20, 2, 30));
    let (mut second_run, calls) = engine_on_boot(store.clone(), clock.clone(), "boot-2");
    assert_eq!(second_run.tick().unwrap(), at(20, 2, 31).timestamp());
    assert_eq!(steps(&calls), ["warn"]);
    clock.set(at(20, 2, 31));
    second_run.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
    clock.set(at(20, 4, 0));
    let (mut third_run, calls) = engine_on_boot(store.clone(), clock, "boot-3");
    third_run.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
    let keys: Vec<String> = rule_shutdowns(&store)
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(
        keys,
        [(2, 1), (2, 31)].map(|(hour, minute)| format!(
            "lights-out/2026-11-19/{}",
            at(20, hour, minute).timestamp()
        ))
    );
}

#[test]
fn configured_notices_count_as_the_minute() {
    let ten = LIGHTS_OUT.replace("notices = []", r#"notices = ["10m"]"#);
    let (mut engine, calls, clock, _store) = start_engine_with(&ten, at(20, 1, 50));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["warn"]);
    clock.set(at(20, 2, 0));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

fn shutdown_notice(time: &str, minutes: i64) -> (String, Vec<String>) {
    let en = Localizer::new(Language::En);
    let args = [
        ("label", Arg::Text("lights-out".to_owned())),
        ("time", Arg::Text(time.to_owned())),
        ("minutes", Arg::Number(minutes)),
    ];
    let bodies = (1..=3)
        .map(|variant| en.text(&format!("notice-shutdown-body-{variant}"), &args))
        .collect();
    (en.text("notice-shutdown-title", &args), bodies)
}

#[test]
fn a_notice_at_the_start_of_a_shutdown_names_the_minute_it_waits() {
    let zero = LIGHTS_OUT.replace("notices = []", r#"notices = ["0m"]"#);
    let (mut engine, calls, clock, _store) = start_engine_with(&zero, at(20, 2, 0));
    assert_eq!(engine.tick().unwrap(), at(20, 2, 1).timestamp());
    let (title, bodies) = shutdown_notice("02:01", 1);
    assert_eq!(calls.last_title(), Some(title));
    let warned = warnings(&calls);
    assert_eq!(warned.len(), 1);
    assert!(bodies.contains(&warned[0]), "{}", warned[0]);
    clock.set(at(20, 2, 1));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn a_notice_at_the_start_after_an_earlier_one_names_the_start() {
    let both = LIGHTS_OUT.replace("notices = []", r#"notices = ["15m", "0m"]"#);
    let (mut engine, calls, clock, _store) = start_engine_with(&both, at(20, 1, 45));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["warn"]);
    clock.set(at(20, 2, 0));
    engine.tick().unwrap();
    let (title, bodies) = shutdown_notice("02:00", 0);
    assert_eq!(calls.last_title(), Some(title));
    let taken = calls.take();
    let Some(Call::Warn(body)) = taken.first() else {
        panic!("expected a notice first: {taken:?}")
    };
    assert!(bodies.contains(body), "{body}");
    assert_eq!(taken.last(), Some(&Call::ShutDown));
}

#[test]
fn a_resume_after_the_start_still_gets_its_one_shutdown() {
    let (mut engine, calls, clock, _store) = start_engine_with(LIGHTS_OUT, at(20, 1, 0));
    engine.tick().unwrap();
    clock.set(at(20, 2, 20));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["warn"]);
    for (minute, expected) in [(21, vec!["shut down"]), (30, vec![])] {
        clock.set(at(20, 2, minute));
        engine.tick().unwrap();
        assert_eq!(steps(&calls), expected);
    }
}

#[test]
fn the_minute_counts_from_when_the_notice_went_out() {
    let one = LIGHTS_OUT.replace("notices = []", r#"notices = ["1m"]"#);
    let (mut engine, calls, clock, _store) = start_engine_with(&one, second(20, 2, 0, 30));
    assert_eq!(engine.tick().unwrap(), second(20, 2, 1, 30).timestamp());
    assert_eq!(steps(&calls), ["warn"]);
    clock.set(second(20, 2, 1, 29));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
    clock.set(second(20, 2, 1, 30));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn a_shutdown_rule_during_a_lock_shows_on_the_lock_screen() {
    let rule = &LIGHTS_OUT[LIGHTS_OUT.find("[[rule_set.rule]]").unwrap()..];
    let text = format!(
        "{}\n{rule}",
        no_jitter().replace("shutdown_after = \"30m\"\n", "")
    );
    let (mut engine, calls, clock, store) = start_engine_with(&text, at(19, 23, 0));
    engine.tick().unwrap();
    assert_eq!(calls.last_view().map(|view| view.shutdown_at), Some(None));
    clock.set(at(20, 2, 0));
    assert_eq!(engine.tick().unwrap(), at(20, 2, 1).timestamp());
    assert_eq!(
        calls.last_view().and_then(|view| view.shutdown_at),
        Some(at(20, 2, 1).timestamp())
    );
    clock.set(second(20, 2, 0, 59));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
    clock.set(at(20, 2, 1));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
    let kinds: Vec<EventKind> = store
        .lock()
        .unwrap()
        .events_for(date(2026, 11, 19))
        .unwrap()
        .into_iter()
        .map(|event| event.kind)
        .collect();
    assert_eq!(kinds, [EventKind::LockStarted, EventKind::RuleShutdown]);
}

#[test]
fn a_save_during_a_lock_that_brings_the_shutdown_forward_still_gives_a_minute() {
    let (mut engine, calls, clock, store) = setup(at(19, 23, 0));
    engine.tick().unwrap();
    assert_eq!(
        calls.last_view().and_then(|view| view.shutdown_at),
        Some(at(19, 23, 30).timestamp())
    );
    let sooner = no_jitter().replace(r#"shutdown_after = "30m""#, r#"shutdown_after = "5m""#);
    save_at(&store.lock().unwrap(), &sooner, at(19, 23, 10));
    clock.set(at(19, 23, 10));
    assert_eq!(engine.tick().unwrap(), at(19, 23, 11).timestamp());
    assert_eq!(
        calls.last_view().and_then(|view| view.shutdown_at),
        Some(at(19, 23, 11).timestamp())
    );
    clock.set(second(19, 23, 10, 59));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
    clock.set(at(19, 23, 11));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn a_started_rule_that_gains_a_shutdown_announces_it_and_shuts_down_a_minute_later() {
    let quiet = LIGHTS_OUT.replace("shutdown = \"once\"\n", "");
    let (mut engine, calls, clock, store) = start_engine_with(&quiet, at(20, 2, 10));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
    save_at(&store.lock().unwrap(), LIGHTS_OUT, at(20, 2, 20));
    clock.set(at(20, 2, 20));
    assert_eq!(engine.tick().unwrap(), at(20, 2, 21).timestamp());
    assert_eq!(
        calls.take(),
        [Call::Hide, Call::Warn(shutdown_now_text("02:21"))]
    );
    clock.set(second(20, 2, 20, 59));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
    clock.set(at(20, 2, 21));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn a_resume_after_an_earlier_notice_gets_a_fresh_minute() {
    let defaults = LIGHTS_OUT.replace("notices = []\n", "");
    let (mut engine, calls, clock, _store) = start_engine_with(&defaults, at(20, 1, 45));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["warn"]);
    clock.set(at(20, 2, 20));
    assert_eq!(engine.tick().unwrap(), at(20, 2, 21).timestamp());
    assert_eq!(
        calls.take(),
        [Call::Hide, Call::Warn(shutdown_now_text("02:21"))]
    );
    clock.set(second(20, 2, 20, 59));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
    clock.set(at(20, 2, 21));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn a_lock_resumed_after_its_shutdown_time_gets_a_fresh_minute() {
    let (mut engine, calls, clock, _store) = setup(at(19, 23, 0));
    engine.tick().unwrap();
    assert_eq!(
        calls.last_view().and_then(|view| view.shutdown_at),
        Some(at(19, 23, 30).timestamp())
    );
    clock.set(at(19, 23, 40));
    assert_eq!(engine.tick().unwrap(), at(19, 23, 41).timestamp());
    assert_eq!(
        calls.last_view().and_then(|view| view.shutdown_at),
        Some(at(19, 23, 41).timestamp())
    );
    clock.set(second(19, 23, 40, 59));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
    clock.set(at(19, 23, 41));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn a_boot_less_than_a_minute_before_a_shutdown_rule_ends_stays_quiet() {
    let (mut engine, calls, clock, _store) = start_engine_with(LIGHTS_OUT, second(20, 3, 59, 30));
    engine.tick().unwrap();
    clock.set(second(20, 3, 59, 59));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
}

fn with_late_lock(text: &str, start: &str, end: &str, extra: &str) -> String {
    format!(
        "{text}\n[[rule_set.rule]]\nid = \"late\"\nstart = \"{start}\"\nend = \"{end}\"\nlock = true\n{extra}"
    )
}

#[test]
fn a_shutdown_shown_on_the_lock_screen_keeps_its_time_when_the_lock_lifts() {
    let text = with_late_lock(LIGHTS_OUT, "01:50", "03:00", "");
    let (mut engine, calls, clock, store) = start_engine_with(&text, at(20, 2, 0));
    engine.tick().unwrap();
    assert_eq!(
        calls.last_view().and_then(|view| view.shutdown_at),
        Some(at(20, 2, 1).timestamp())
    );
    clock.set(second(20, 2, 0, 30));
    let shared = shared_over(store, clock.clone(), PowerMode::LogOnly);
    let ask = Request::RequestExtension {
        reason: "talking to my sister".into(),
        phrase: String::new(),
        source: ExtensionSource::Terminal,
    };
    assert!(matches!(
        handle(ask, &shared),
        Response::ExtensionGranted { .. }
    ));
    calls.take();
    engine.tick().unwrap();
    assert_eq!(calls.take(), [Call::Hide]);
    clock.set(at(20, 2, 1));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn a_shutdown_notice_under_a_lock_does_not_count_as_its_announcement() {
    let ten = LIGHTS_OUT.replace("notices = []", r#"notices = ["10m"]"#);
    let text = with_late_lock(&ten, "01:30", "01:58", "");
    let (mut engine, calls, clock, _store) = start_engine_with(&text, at(20, 1, 50));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["warn"]);
    clock.set(at(20, 1, 58));
    engine.tick().unwrap();
    assert_eq!(calls.take(), [Call::Hide]);
    clock.set(at(20, 2, 0));
    assert_eq!(engine.tick().unwrap(), at(20, 2, 1).timestamp());
    assert_eq!(
        calls.take(),
        [Call::Hide, Call::Warn(shutdown_now_text("02:01"))]
    );
    clock.set(at(20, 2, 1));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn a_rule_shutdown_that_would_reach_the_lock_end_is_left_off_the_lock_screen() {
    let text = with_late_lock(LIGHTS_OUT, "01:30", "02:01", "");
    let (mut engine, calls, clock, _store) = start_engine_with(&text, at(20, 2, 0));
    engine.tick().unwrap();
    assert_eq!(calls.last_view().map(|view| view.shutdown_at), Some(None));
    clock.set(second(20, 2, 0, 59));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
    clock.set(at(20, 2, 1));
    assert_eq!(engine.tick().unwrap(), at(20, 2, 2).timestamp());
    assert_eq!(
        calls.take(),
        [Call::Hide, Call::Warn(shutdown_now_text("02:02"))]
    );
    clock.set(at(20, 2, 2));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn a_lock_shutdown_at_a_once_rules_start_counts_for_the_rule() {
    let text = with_late_lock(LIGHTS_OUT, "01:50", "02:30", "shutdown_after = \"10m\"\n");
    let (mut engine, calls, clock, store) = start_engine_with(&text, at(20, 1, 50));
    engine.tick().unwrap();
    calls.take();
    clock.set(at(20, 2, 0));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
    clock.set(at(20, 2, 35));
    let (mut after_boot, calls) = engine_on(store.clone(), clock);
    after_boot.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
    assert_eq!(
        rule_shutdowns(&store),
        [(
            "lights-out/2026-11-19".to_owned(),
            json!({ "label": "lights-out", "rule": "lights-out", "repeat": false })
        )]
    );
}

const WRAP_UP: &str = r#"
language = "en"
day_starts_at = "05:00"

[[rule_set]]
name = "everyday"
days = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]

[[rule_set.rule]]
id = "wrap-up"
start = "22:00"
end = "23:00"

[[rule_set.rule.agent]]
before = "30m"
level = "no-new-work"
"#;

fn published_keys(calls: &Calls) -> Vec<String> {
    calls
        .last_directives()
        .unwrap()
        .steps
        .into_iter()
        .map(|step| step.key)
        .collect()
}

#[test]
fn the_engine_publishes_active_agent_steps() {
    let text = format!(
        "{}\n[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"light\"\n",
        no_jitter()
    );
    let (mut engine, calls, clock, _store) = start_engine_with(&text, at(19, 21, 59));
    engine.tick().unwrap();
    assert_eq!(published_keys(&calls), Vec::<String>::new());
    clock.set(at(19, 22, 0));
    engine.tick().unwrap();
    assert_eq!(published_keys(&calls), ["bedtime/2026-11-19/60m/light"]);
    let directives = calls.last_directives().unwrap();
    assert_eq!(directives.language, Language::PtBr);
    assert_eq!(
        directives.allowed_commands,
        [
            "git status",
            "git diff",
            "git add",
            "git commit",
            "git stash push"
        ]
    );
}

#[test]
fn an_agent_step_records_its_window_as_started() {
    let (mut engine, _calls, clock, store) = start_engine_with(WRAP_UP, at(19, 21, 29));
    let started = |store: &Mutex<Store>| {
        store
            .lock()
            .unwrap()
            .night_log(&[date(2026, 11, 19)])
            .unwrap()
            .started
            .into_iter()
            .map(|window| (window.rule_id, window.starts_at))
            .collect::<Vec<_>>()
    };
    engine.tick().unwrap();
    assert_eq!(started(&store), []);
    clock.set(at(19, 21, 30));
    engine.tick().unwrap();
    assert_eq!(started(&store), [("wrap-up".to_owned(), at(19, 22, 0))]);
}
