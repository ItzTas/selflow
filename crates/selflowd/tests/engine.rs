mod support;

use std::fs::{Permissions, set_permissions};
use std::os::unix::fs::PermissionsExt;

use jiff::SignedDuration;
use jiff::civil::date;
use selflow_i18n::language::Language;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_protocol::ipc::{Request, Response};
use selflowd::enforcer::PowerMode;
use selflowd::ipc::handle;
use selflowd::store::{EventKind, Store};
use serde_json::{Value, json};
use support::{
    Call, Calls, REMINDER, at, brt, engine_on, extension, no_jitter, save_at, save_text, setup,
    setup_on, setup_over, shared_over,
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
