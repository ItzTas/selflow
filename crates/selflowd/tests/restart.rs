mod support;

use std::fs::{Permissions, set_permissions};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};

use jiff::civil::date;
use selflow_protocol::ipc::LockView;
use selflowd::engine::{Enforcer, Engine};
use selflowd::store::{Announcement, EventKind, Store};
use support::{
    BOOT, LIGHTS_OUT, TestClock, at, engine_on, engine_on_boot, save_text, second, setup,
    setup_over, start_engine_with, steps,
};

#[test]
fn a_restart_keeps_the_announced_shutdown_time() {
    let (mut engine, calls, clock, store) = start_engine_with(LIGHTS_OUT, at(20, 2, 0));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["warn"]);
    drop(engine);
    clock.set(second(20, 2, 0, 30));
    let (mut restarted, calls) = engine_on(store, clock.clone());
    assert_eq!(restarted.tick().unwrap(), at(20, 2, 1).timestamp());
    assert_eq!(steps(&calls), Vec::<&str>::new());
    clock.set(at(20, 2, 1));
    restarted.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn restarting_in_a_loop_never_postpones_the_shutdown() {
    let (mut engine, calls, clock, store) = start_engine_with(LIGHTS_OUT, at(20, 2, 0));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["warn"]);
    drop(engine);
    for moment in [second(20, 2, 0, 20), second(20, 2, 0, 40)] {
        clock.set(moment);
        let (mut restarted, calls) = engine_on(store.clone(), clock.clone());
        restarted.tick().unwrap();
        assert_eq!(steps(&calls), Vec::<&str>::new());
    }
    clock.set(at(20, 2, 1));
    let (mut restarted, calls) = engine_on(store, clock);
    restarted.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn a_restart_keeps_the_fresh_minute_a_resume_gave() {
    let defaults = LIGHTS_OUT.replace("notices = []\n", "");
    let (mut engine, calls, clock, store) = start_engine_with(&defaults, at(20, 1, 45));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["warn"]);
    clock.set(at(20, 2, 20));
    assert_eq!(engine.tick().unwrap(), at(20, 2, 21).timestamp());
    assert_eq!(steps(&calls), ["warn"]);
    drop(engine);
    clock.set(second(20, 2, 20, 30));
    let (mut restarted, calls) = engine_on(store, clock.clone());
    assert_eq!(restarted.tick().unwrap(), at(20, 2, 21).timestamp());
    assert_eq!(steps(&calls), Vec::<&str>::new());
    clock.set(at(20, 2, 21));
    restarted.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn a_restart_keeps_the_shutdown_time_on_the_lock_screen() {
    let (mut engine, calls, clock, store) = setup(at(19, 23, 0));
    engine.tick().unwrap();
    assert_eq!(
        calls.last_view().and_then(|view| view.shutdown_at),
        Some(at(19, 23, 30).timestamp())
    );
    drop(engine);
    clock.set(second(19, 23, 29, 30));
    let (mut restarted, calls) = engine_on(store, clock.clone());
    restarted.tick().unwrap();
    assert_eq!(
        calls.last_view().and_then(|view| view.shutdown_at),
        Some(at(19, 23, 30).timestamp())
    );
    clock.set(at(19, 23, 30));
    restarted.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

#[test]
fn a_restart_after_a_shutdown_does_not_shut_down_again() {
    let repeat = LIGHTS_OUT.replace(r#"shutdown = "once""#, r#"shutdown = "repeat""#);
    let (mut engine, calls, clock, store) = start_engine_with(&repeat, at(20, 2, 0));
    engine.tick().unwrap();
    clock.set(at(20, 2, 1));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["warn", "shut down"]);
    drop(engine);
    clock.set(second(20, 2, 1, 30));
    let (mut restarted, calls) = engine_on(store, clock);
    restarted.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
}

#[test]
fn a_restart_after_a_lock_shut_down_does_not_shut_down_again() {
    let (mut engine, calls, clock, store) = setup(at(19, 23, 0));
    engine.tick().unwrap();
    clock.set(at(19, 23, 30));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
    drop(engine);
    clock.set(second(19, 23, 30, 30));
    let (mut restarted, calls) = engine_on(store, clock);
    restarted.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
}

#[test]
fn a_restart_does_not_repeat_a_notice() {
    let (mut engine, calls, clock, store) = setup(at(19, 22, 45));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["warn"]);
    drop(engine);
    clock.set(at(19, 22, 46));
    let (mut restarted, calls) = engine_on(store, clock);
    restarted.tick().unwrap();
    assert_eq!(steps(&calls), Vec::<&str>::new());
}

#[test]
fn a_lock_that_ended_while_the_machine_was_off_gets_its_end() {
    let (mut engine, _calls, clock, store) = setup(at(19, 23, 10));
    engine.tick().unwrap();
    drop(engine);
    clock.set(at(20, 7, 30));
    let (mut rebooted, _calls) = engine_on_boot(store.clone(), clock, "boot-2");
    rebooted.tick().unwrap();
    let ends = store
        .lock()
        .unwrap()
        .events_for(date(2026, 11, 19))
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == EventKind::LockEnded)
        .count();
    assert_eq!(ends, 1);
}

#[test]
fn a_reboot_gives_a_lock_with_a_shutdown_a_fresh_minute() {
    let (mut engine, calls, clock, store) = setup(at(19, 23, 0));
    engine.tick().unwrap();
    clock.set(at(19, 23, 30));
    engine.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
    drop(engine);
    clock.set(at(19, 23, 35));
    let (mut rebooted, calls) = engine_on_boot(store, clock.clone(), "boot-2");
    rebooted.tick().unwrap();
    assert_eq!(
        calls.last_view().and_then(|view| view.shutdown_at),
        Some(at(19, 23, 36).timestamp())
    );
    clock.set(at(19, 23, 36));
    rebooted.tick().unwrap();
    assert_eq!(steps(&calls), ["shut down"]);
}

struct ProbingEnforcer {
    store: Arc<Mutex<Store>>,
    saw: Arc<Mutex<Vec<bool>>>,
}

impl Enforcer for ProbingEnforcer {
    fn warn(&mut self, _title: String, _body: String) {
        let announced = self
            .store
            .lock()
            .unwrap()
            .boot_state(BOOT, date(2026, 11, 18))
            .unwrap()
            .announced;
        let lights_out = Announcement {
            rule_key: "lights-out".to_owned(),
            starts_at: at(20, 2, 0).timestamp(),
            announced_at: at(20, 2, 0).timestamp(),
        };
        self.saw
            .lock()
            .unwrap()
            .push(announced.contains(&lights_out));
    }

    fn show_lock(&mut self, _view: LockView) {}

    fn hide_lock(&mut self) {}

    fn shut_down(&mut self) {}
}

#[test]
fn the_announcement_is_stored_before_the_notice_goes_out() {
    let store = Store::open_in_memory().unwrap();
    save_text(&store, LIGHTS_OUT);
    let store = Arc::new(Mutex::new(store));
    let saw = Arc::new(Mutex::new(Vec::new()));
    let probe = ProbingEnforcer {
        store: store.clone(),
        saw: saw.clone(),
    };
    let clock = Arc::new(TestClock::new(at(20, 2, 0)));
    let mut engine = Engine::new(clock, probe, store, BOOT.to_owned()).unwrap();
    engine.tick().unwrap();
    assert_eq!(*saw.lock().unwrap(), [true]);
}

#[test]
fn a_store_that_cannot_record_still_shuts_down_on_time() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/selflow.db");
    let store = Store::open(&path).unwrap();
    save_text(&store, LIGHTS_OUT);
    store.seed().unwrap();
    let state = path.parent().unwrap();
    set_permissions(state, Permissions::from_mode(0o500)).unwrap();
    let (mut engine, calls, clock, _store) = setup_over(store, at(20, 2, 0));
    let mut ticks = vec![engine.tick()];
    let at_the_start = steps(&calls);
    clock.set(at(20, 2, 1));
    ticks.push(engine.tick());
    let a_minute_later = steps(&calls);
    set_permissions(state, Permissions::from_mode(0o700)).unwrap();
    assert!(ticks.iter().all(Result::is_ok));
    assert_eq!(at_the_start, ["warn"]);
    assert_eq!(a_minute_later, ["shut down"]);
}
