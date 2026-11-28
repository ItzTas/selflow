#![allow(dead_code)]

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jiff::{
    SignedDuration, ZonedDateTime,
    civil::date,
    tz::{self, TimeZone},
};
use rusqlite::{Connection, params};
use selflow_core::config::parse;
use selflow_core::enforce::GrantedExtension;
use selflow_core::plan::windows_for;
use selflow_core::timing::Seed;
use selflow_core::versions::{plan_save, stored_changes};
use selflow_i18n::language::Language;
use selflow_platform::transport::{Stream, connect, listen};
use selflow_protocol::ipc::{
    LockView, MessageReader, PROTOCOL_VERSION, Request, Response, hello_as_client, message_reader,
    read_message, write_message,
};
use selflowd::clock::Clock;
use selflowd::enforcer::{PowerMode, ScreenControl};
use selflowd::engine::{Enforcer, Engine, stored_history};
use selflowd::ipc::{Shared, serve};
use selflowd::store::Store;
use tempfile::TempDir;
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
    last_title: Arc<Mutex<Option<String>>>,
}

impl Calls {
    pub fn take(&self) -> Vec<Call> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }

    pub fn last_view(&self) -> Option<LockView> {
        self.last_view.lock().unwrap().clone()
    }

    pub fn last_title(&self) -> Option<String> {
        self.last_title.lock().unwrap().clone()
    }

    fn push(&self, call: Call) {
        self.calls.lock().unwrap().push(call);
    }
}

pub struct FakeEnforcer(Calls);

impl Enforcer for FakeEnforcer {
    fn warn(&mut self, title: String, body: String) {
        *self.0.last_title.lock().unwrap() = Some(title);
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

#[derive(Default)]
pub struct FakeScreen {
    releases: AtomicU32,
}

impl FakeScreen {
    pub fn releases(&self) -> u32 {
        self.releases.load(Ordering::SeqCst)
    }
}

impl ScreenControl for FakeScreen {
    fn release(&self) {
        self.releases.fetch_add(1, Ordering::SeqCst);
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

pub fn start_engine_with(
    text: &str,
    now: ZonedDateTime,
) -> (TestEngine, Calls, Arc<TestClock>, Arc<Mutex<Store>>) {
    let store = Store::open_in_memory().unwrap();
    save_text(&store, text);
    setup_over(store, now)
}

pub const BOOT: &str = "boot-1";

pub fn engine_on(store: Arc<Mutex<Store>>, clock: Arc<TestClock>) -> (TestEngine, Calls) {
    engine_on_boot(store, clock, BOOT)
}

pub fn engine_on_boot(
    store: Arc<Mutex<Store>>,
    clock: Arc<TestClock>,
    boot: &str,
) -> (TestEngine, Calls) {
    let calls = Calls::default();
    let engine = Engine::new(clock, FakeEnforcer(calls.clone()), store, boot.to_owned()).unwrap();
    (engine, calls)
}

pub fn steps(calls: &Calls) -> Vec<&'static str> {
    calls
        .take()
        .into_iter()
        .filter_map(|call| match call {
            Call::Warn(_) => Some("warn"),
            Call::ShutDown => Some("shut down"),
            _ => None,
        })
        .collect()
}

pub fn save_no_jitter(store: &Store) {
    save_text(store, &no_jitter());
}

pub fn save_text(store: &Store, text: &str) {
    save_at(store, text, at(18, 10, 0));
}

pub fn save_at(store: &Store, text: &str, when: ZonedDateTime) {
    let saved_at = when.timestamp();
    let planned = plan_save(
        &stored_history(store).unwrap(),
        &parse(text).unwrap(),
        saved_at,
    );
    store
        .save_version(text, saved_at, &stored_changes(&planned))
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

pub fn second(day: i8, hour: i8, minute: i8, second: i64) -> ZonedDateTime {
    at(day, hour, minute)
        .checked_add(SignedDuration::from_secs(second))
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

[[rule_set.rule]]
id = "bedtime"
start = "bedtime"
start_jitter = "15m"
end = "wake"
notices = ["5m", "15m", "1m"]
lock = true
shutdown_after = "30m"
"#;

pub fn no_jitter() -> String {
    VALID.replace("start_jitter = \"15m\"\n", "")
}

pub const REMINDER: &str = r#"
language = "en"
day_starts_at = "05:00"

[[rule_set]]
name = "everyday"
days = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]

[[rule_set.rule]]
id = "stretch"
start = "15:00"
message = ["Stand up for a minute.", "Time to stretch."]
"#;

pub const LIGHTS_OUT: &str = r#"
[[rule_set]]
name = "always"
days = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]

[[rule_set.rule]]
id = "lights-out"
start = "02:00"
end = "04:00"
notices = []
shutdown = "once"
"#;

pub const M1_TEXT: &str = r#"
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

const M1_SCHEMA: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT;
CREATE TABLE config_versions (id INTEGER PRIMARY KEY, saved_at TEXT NOT NULL, effective_at TEXT NOT NULL,
  kind TEXT NOT NULL, text TEXT NOT NULL, superseded INTEGER NOT NULL DEFAULT 0) STRICT;
CREATE TABLE extensions (id INTEGER PRIMARY KEY, rule_set TEXT NOT NULL, rule_id TEXT NOT NULL,
  logical_date TEXT NOT NULL, granted_at TEXT NOT NULL, until TEXT NOT NULL, reason TEXT NOT NULL) STRICT;
CREATE TABLE started_windows (key TEXT NOT NULL, logical_date TEXT NOT NULL, window TEXT NOT NULL,
  PRIMARY KEY (key, logical_date)) STRICT;
CREATE TABLE events (id INTEGER PRIMARY KEY, at TEXT NOT NULL, logical_date TEXT NOT NULL, kind TEXT NOT NULL,
  key TEXT NOT NULL, detail TEXT, UNIQUE (kind, key)) STRICT;
PRAGMA user_version = 1;
";

pub fn m1_store(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(M1_SCHEMA).unwrap();
    let saved_at = "2026-11-18T13:00:00.000000000Z";
    conn.execute(
        "INSERT INTO config_versions (saved_at, effective_at, kind, text) VALUES (?1, ?1, 'tighten', ?2)",
        params![saved_at, M1_TEXT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO events (at, logical_date, kind, key, detail)
         VALUES (?1, '2026-11-18', 'config_saved', '1', '{\"kind\":\"tighten\"}')",
        params![saved_at],
    )
    .unwrap();
    let window = windows_for(
        &parse(&no_jitter()).unwrap(),
        Seed(7),
        date(2026, 11, 19),
        &brt(),
    )
    .remove(0);
    conn.execute(
        "INSERT INTO started_windows (key, logical_date, window) VALUES (?1, '2026-11-19', ?2)",
        params![window.key(), serde_json::to_string(&window).unwrap()],
    )
    .unwrap();
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
    stream: tokio::sync::Mutex<MessageReader<Stream>>,
    screen: Arc<FakeScreen>,
    _dir: TempDir,
}

impl TestClient {
    pub fn releases(&self) -> u32 {
        self.screen.releases()
    }

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
    stream: MessageReader<Stream>,
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
    let (client, shared, _clock) =
        start_over(Store::open_in_memory().unwrap(), Language::En, now).await;
    (client, shared)
}

pub async fn start_with_config(now: ZonedDateTime) -> (TestClient, Arc<Shared>, Arc<TestClock>) {
    let store = Store::open_in_memory().unwrap();
    save_no_jitter(&store);
    start_over(store, Language::En, now).await
}

pub async fn start_with(
    text: &str,
    power: PowerMode,
    view: Option<LockView>,
    now: ZonedDateTime,
) -> (TestClient, Arc<Shared>, Arc<TestClock>) {
    let store = Store::open_in_memory().unwrap();
    save_text(&store, text);
    let clock = Arc::new(TestClock::new(now));
    let (_, views) = watch::channel(view);
    let (dir, stream, shared, screen) =
        launch(store, clock.clone(), views, power, Language::En).await;
    let client = TestClient {
        stream: tokio::sync::Mutex::new(greeted(stream).await),
        screen,
        _dir: dir,
    };
    (client, shared, clock)
}

pub fn shared_over(store: Arc<Mutex<Store>>, clock: Arc<TestClock>, power: PowerMode) -> Shared {
    let (wake, _) = mpsc::channel(1);
    let (_, views) = watch::channel(None);
    Shared {
        store,
        clock,
        wake,
        views,
        power,
        env_language: Language::En,
        screen: Arc::new(FakeScreen::default()),
    }
}

pub async fn subscribe() -> (Subscriber, watch::Sender<Option<LockView>>) {
    let (sender, views) = watch::channel(None);
    let clock = Arc::new(TestClock::new(at(19, 23, 0)));
    let (dir, stream, _shared, _screen) = launch(
        Store::open_in_memory().unwrap(),
        clock,
        views,
        PowerMode::Real,
        Language::En,
    )
    .await;
    let mut stream = greeted(stream).await;
    write_message(stream.get_mut(), &Request::SubscribeLock)
        .await
        .unwrap();
    (Subscriber { stream, _dir: dir }, sender)
}

pub async fn start_over(
    store: Store,
    env_language: Language,
    now: ZonedDateTime,
) -> (TestClient, Arc<Shared>, Arc<TestClock>) {
    let clock = Arc::new(TestClock::new(now));
    let (_, views) = watch::channel(None);
    let (dir, stream, shared, screen) =
        launch(store, clock.clone(), views, PowerMode::Real, env_language).await;
    let client = TestClient {
        stream: tokio::sync::Mutex::new(greeted(stream).await),
        screen,
        _dir: dir,
    };
    (client, shared, clock)
}

pub async fn greeted(mut stream: Stream) -> MessageReader<Stream> {
    let version = hello_as_client(&mut stream).await.unwrap();
    assert_eq!(version, PROTOCOL_VERSION);
    message_reader(stream)
}

pub async fn launch(
    store: Store,
    clock: Arc<dyn Clock>,
    views: watch::Receiver<Option<LockView>>,
    power: PowerMode,
    env_language: Language,
) -> (TempDir, Stream, Arc<Shared>, Arc<FakeScreen>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    let listener = listen(&path).unwrap();
    let (wake, _) = mpsc::channel(1);
    let screen = Arc::new(FakeScreen::default());
    let shared = Arc::new(Shared {
        store: Arc::new(Mutex::new(store)),
        clock,
        wake,
        views,
        power,
        env_language,
        screen: screen.clone(),
    });
    tokio::spawn(serve(listener, Arc::clone(&shared)));
    let stream = connect(&path).await.unwrap();
    (dir, stream, shared, screen)
}
