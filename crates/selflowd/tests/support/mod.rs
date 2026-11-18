#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use jiff::{
    ZonedDateTime,
    civil::date,
    tz::{self, TimeZone},
};
use selflow_core::enforce::GrantedExtension;
use selflow_core::versions::ChangeKind;
use selflow_protocol::ipc::LockView;
use selflowd::clock::Clock;
use selflowd::engine::{Enforcer, Engine};
use selflowd::store::Store;

pub struct TestClock(Mutex<ZonedDateTime>);

impl TestClock {
    pub fn new(now: ZonedDateTime) -> TestClock {
        TestClock(Mutex::new(now))
    }

    pub fn set(&self, now: ZonedDateTime) {
        *self.0.lock().unwrap() = now;
    }
}

impl Clock for TestClock {
    fn now(&self) -> ZonedDateTime {
        self.0.lock().unwrap().clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Warn(String),
    Show(String, u32),
    Hide,
    ShutDown,
}

#[derive(Clone, Default)]
pub struct Calls(Arc<Mutex<Vec<Call>>>);

impl Calls {
    pub fn take(&self) -> Vec<Call> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }

    fn push(&self, call: Call) {
        self.0.lock().unwrap().push(call);
    }
}

pub struct FakeEnforcer(Calls);

impl Enforcer for FakeEnforcer {
    fn warn(&mut self, _title: String, body: String) {
        self.0.push(Call::Warn(body));
    }

    fn show_lock(&mut self, view: LockView) {
        self.0.push(Call::Show(view.label, view.extensions_left));
    }

    fn hide_lock(&mut self) {
        self.0.push(Call::Hide);
    }

    fn shut_down(&mut self) {
        self.0.push(Call::ShutDown);
    }
}

pub type TestEngine = Engine<TestClock, FakeEnforcer>;

pub fn setup(now: ZonedDateTime) -> (TestEngine, Calls, Arc<TestClock>, Arc<Mutex<Store>>) {
    setup_on(Store::open_in_memory().unwrap(), now)
}

pub fn setup_on(
    store: Store,
    now: ZonedDateTime,
) -> (TestEngine, Calls, Arc<TestClock>, Arc<Mutex<Store>>) {
    let saved_at = at(18, 10, 0).timestamp();
    store
        .save_version(&no_jitter(), saved_at, saved_at, ChangeKind::Tighten)
        .unwrap();
    let store = Arc::new(Mutex::new(store));
    let clock = Arc::new(TestClock::new(now));
    let (engine, calls) = engine_on(store.clone(), clock.clone());
    (engine, calls, clock, store)
}

pub fn engine_on(store: Arc<Mutex<Store>>, clock: Arc<TestClock>) -> (TestEngine, Calls) {
    let calls = Calls::default();
    let engine = Engine::new(clock, FakeEnforcer(calls.clone()), store).unwrap();
    (engine, calls)
}

pub fn brt() -> TimeZone {
    TimeZone::fixed(tz::offset(-3))
}

pub fn at(day: i8, hour: i8, minute: i8) -> ZonedDateTime {
    date(2026, 11, day)
        .at(hour, minute, 0, 0)
        .to_zoned_datetime(brt())
        .unwrap()
}

pub const VALID: &str = r#"
language = "pt-BR"
day_starts_at = "05:00"
loosening_delay = "24h"

[anchors]
bedtime = "23:00"
wake = "07:00"

[[rule_set]]
name = "weeknight"
days = ["sun", "mon", "tue", "wed", "thu"]

[rule_set.challenges]
wait = "2m"
reason = true
extensions = 1
extension_length = "20m"

[[rule_set.lock]]
id = "bedtime"
start = "bedtime"
start_jitter = "15m"
end = "wake"
warnings = ["5m", "15m", "1m"]
shutdown_after = "30m"
"#;

pub fn no_jitter() -> String {
    VALID.replace("start_jitter = \"15m\"\n", "")
}

pub fn extension(from: (i8, i8), until: (i8, i8)) -> GrantedExtension {
    GrantedExtension {
        rule_set: "weeknight".to_owned(),
        rule_id: "bedtime".to_owned(),
        logical_date: date(2026, 11, 19),
        granted_at: at(19, from.0, from.1).timestamp(),
        until: at(19, until.0, until.1).timestamp(),
        reason: "talking to my sister".to_owned(),
    }
}
