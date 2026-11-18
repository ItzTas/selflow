#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use jiff::{
    ZonedDateTime,
    civil::date,
    tz::{self, TimeZone},
};
use selflow_core::enforce::GrantedExtension;
use selflow_core::versions::ChangeKind;
use selflow_protocol::ipc::{LockView, Request, Response, read_message, write_message};
use selflowd::clock::Clock;
use selflowd::engine::{Enforcer, Engine};
use selflowd::ipc::{Shared, bind, serve};
use selflowd::store::Store;
use tempfile::TempDir;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, watch};

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
pub struct Calls {
    calls: Arc<Mutex<Vec<Call>>>,
    last_view: Arc<Mutex<Option<LockView>>>,
}

impl Calls {
    pub fn take(&self) -> Vec<Call> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }

    pub fn last_view(&self) -> Option<LockView> {
        self.last_view.lock().unwrap().clone()
    }

    fn push(&self, call: Call) {
        self.calls.lock().unwrap().push(call);
    }
}

pub struct FakeEnforcer(Calls);

impl Enforcer for FakeEnforcer {
    fn warn(&mut self, _title: String, body: String) {
        self.0.push(Call::Warn(body));
    }

    fn show_lock(&mut self, view: LockView) {
        self.0
            .push(Call::Show(view.label.clone(), view.extensions_left));
        *self.0.last_view.lock().unwrap() = Some(view);
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
    save_no_jitter(&store);
    setup_over(store, now)
}

pub fn setup_over(
    store: Store,
    now: ZonedDateTime,
) -> (TestEngine, Calls, Arc<TestClock>, Arc<Mutex<Store>>) {
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

pub fn save_no_jitter(store: &Store) {
    let saved_at = at(18, 10, 0).timestamp();
    store
        .save_version(&no_jitter(), saved_at, saved_at, ChangeKind::Tighten)
        .unwrap();
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

pub fn sample_view() -> LockView {
    LockView {
        label: "bedtime".to_owned(),
        ends_at: at(20, 7, 0).timestamp(),
        extension_available_at: at(19, 23, 2).timestamp(),
        extensions_left: 1,
        extension_minutes: 20,
        reason_required: true,
        phrase: None,
        shutdown_at: Some(at(19, 23, 30).timestamp()),
        language: "pt-BR".to_owned(),
    }
}

pub async fn wait_for(limit: Duration, condition: impl Fn() -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        if condition() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub struct TestClient {
    stream: tokio::sync::Mutex<BufReader<UnixStream>>,
    _dir: TempDir,
}

impl TestClient {
    pub async fn request(&self, request: Request) -> Response {
        let mut stream = self.stream.lock().await;
        write_message(stream.get_mut(), &request).await.unwrap();
        read_message(&mut *stream)
            .await
            .unwrap()
            .expect("the daemon answered")
    }
}

pub struct Subscriber {
    stream: BufReader<UnixStream>,
    _dir: TempDir,
}

impl Subscriber {
    pub async fn next(&mut self) -> Option<Response> {
        tokio::time::timeout(Duration::from_secs(5), read_message(&mut self.stream))
            .await
            .ok()?
            .unwrap()
    }
}

pub async fn start(now: ZonedDateTime) -> (TestClient, Arc<Shared>) {
    let (client, shared, _clock) = start_on(Store::open_in_memory().unwrap(), now).await;
    (client, shared)
}

pub async fn start_with_config(now: ZonedDateTime) -> (TestClient, Arc<Shared>, Arc<TestClock>) {
    let store = Store::open_in_memory().unwrap();
    save_no_jitter(&store);
    start_on(store, now).await
}

pub async fn subscribe() -> (Subscriber, watch::Sender<Option<LockView>>) {
    let (sender, views) = watch::channel(None);
    let clock = Arc::new(TestClock::new(at(19, 23, 0)));
    let (dir, stream, _shared) = launch(Store::open_in_memory().unwrap(), clock, views).await;
    let mut stream = BufReader::new(stream);
    write_message(stream.get_mut(), &Request::SubscribeLock)
        .await
        .unwrap();
    (Subscriber { stream, _dir: dir }, sender)
}

async fn start_on(store: Store, now: ZonedDateTime) -> (TestClient, Arc<Shared>, Arc<TestClock>) {
    let clock = Arc::new(TestClock::new(now));
    let (_, views) = watch::channel(None);
    let (dir, stream, shared) = launch(store, clock.clone(), views).await;
    let client = TestClient {
        stream: tokio::sync::Mutex::new(BufReader::new(stream)),
        _dir: dir,
    };
    (client, shared, clock)
}

async fn launch(
    store: Store,
    clock: Arc<dyn Clock>,
    views: watch::Receiver<Option<LockView>>,
) -> (TempDir, UnixStream, Arc<Shared>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    let listener = bind(&path).unwrap();
    let (wake, _) = mpsc::channel(1);
    let shared = Arc::new(Shared {
        store: Arc::new(Mutex::new(store)),
        clock,
        wake,
        views,
    });
    tokio::spawn(serve(listener, Arc::clone(&shared)));
    let stream = UnixStream::connect(&path).await.unwrap();
    (dir, stream, shared)
}
