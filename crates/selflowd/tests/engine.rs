mod support;

use jiff::civil::date;
use selflowd::store::EventKind;
use support::{Call, at, engine_on, extension, setup};

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
