mod constants;
mod types;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use jiff::{SignedDuration, Timestamp, ZonedDateTime, civil, tz::TimeZone};
use selflow_core::config::{Action, Config, parse};
use selflow_core::enforce::{ActiveLock, DayLog, Enforcement, Notice, NoticeKind, evaluate};
use selflow_core::plan::{RuleWindow, windows_around};
use selflow_core::timing::{Seed, logical_date};
use selflow_core::versions::{History, SavedConfig, StoredVersion, config_at, rule_since};
use selflow_i18n::language::Language;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_protocol::ipc::LockView;
use serde_json::{Value, json};
use tokio::sync::mpsc::Receiver;

use crate::clock::Clock;
use crate::store::{Event, EventKind, Store, StoreError};

pub use constants::{MAX_SLEEP, SHUTDOWN_NOTICE, WARNING_GRACE};
pub use types::{Enforcer, EngineError};
use types::{Outcome, Record, ShownLock};

pub struct Engine<C: Clock, E: Enforcer> {
    clock: Arc<C>,
    enforcer: E,
    store: Arc<Mutex<Store>>,
    seed: Seed,
    parsed: HashMap<i64, Config>,
    config: Config,
    localizers: HashMap<Language, Localizer>,
    sent: HashSet<(String, Timestamp)>,
    shown: HashMap<String, ShownLock>,
}

impl<C: Clock, E: Enforcer> Engine<C, E> {
    pub fn new(clock: Arc<C>, enforcer: E, store: Arc<Mutex<Store>>) -> Result<Self, EngineError> {
        let seed = locked(&store).seed()?;
        Ok(Engine {
            clock,
            enforcer,
            store,
            seed,
            parsed: HashMap::new(),
            config: Config::empty(),
            localizers: HashMap::new(),
            sent: HashSet::new(),
            shown: HashMap::new(),
        })
    }

    pub fn tick(&mut self) -> Result<Timestamp, EngineError> {
        let now = self.clock.now();
        let instant = now.timestamp();
        let (log, history) = self.load(&now)?;
        let evaluation = evaluate(&now, &self.config, self.seed, &log);
        let language = Language::from_tag(&self.config.language).unwrap_or(Language::En);
        let mut records = self.send_notices(&now, &evaluation.notices, &log, language);
        let outcome = match &evaluation.enforcement {
            Enforcement::Free => self.release(instant),
            Enforcement::Locked(lock) => {
                let earliest = earliest_shutdown(&history, &lock.window, instant);
                self.enforce(&log, lock, earliest, instant, language)
            }
        };
        records.extend(outcome.records);
        self.record(&records);
        if outcome.shut_down {
            self.enforcer.shut_down();
        }
        Ok(evaluation
            .next_change_at
            .into_iter()
            .chain(outcome.pending_shutdown)
            .fold(later_by(instant, MAX_SLEEP), Ord::min))
    }

    fn load(&mut self, now: &ZonedDateTime) -> Result<(DayLog, History), EngineError> {
        let shared = Arc::clone(&self.store);
        let store = locked(&shared);
        let history = history_with(&store, &mut self.parsed)?;
        self.config = config_at(&history, now.timestamp());
        let today = logical_date(now, self.config.day_starts_at);
        let nights: Vec<civil::Date> = today.yesterday().into_iter().chain(Some(today)).collect();
        Ok((store.night_log(&nights)?, history))
    }

    fn record(&self, records: &[Record]) {
        let store = locked(&self.store);
        for record in records {
            if let Err(error) = write(&store, record) {
                tracing::error!(%error, ?record, "couldn't record an enforcement step");
            }
        }
    }

    pub async fn run(mut self, mut wake: Receiver<()>) -> Result<(), EngineError> {
        loop {
            let pause = match self.tick() {
                Ok(wake_at) => self
                    .clock
                    .now()
                    .timestamp()
                    .duration_until(wake_at)
                    .clamp(SignedDuration::ZERO, MAX_SLEEP),
                Err(error) => {
                    tracing::error!(%error, "enforcement tick failed");
                    MAX_SLEEP
                }
            };
            let open = tokio::select! {
                () = tokio::time::sleep(pause.unsigned_abs()) => true,
                message = wake.recv() => message.is_some(),
            };
            if !open {
                return Ok(());
            }
        }
    }

    fn send_notices(
        &mut self,
        now: &ZonedDateTime,
        notices: &[Notice],
        log: &DayLog,
        language: Language,
    ) -> Vec<Record> {
        let instant = now.timestamp();
        let since = earlier_by(instant, WARNING_GRACE);
        let due: Vec<&Notice> = notices
            .iter()
            .filter(|notice| since < notice.at && notice.at <= instant)
            .filter(|notice| self.sent.insert((notice.key.clone(), notice.at)))
            .collect();
        if due.is_empty() {
            return Vec::new();
        }
        let localizer = self
            .localizers
            .entry(language)
            .or_insert_with(|| Localizer::new(language));
        for notice in &due {
            let (title, body) = rendered(localizer, notice, now.time_zone());
            self.enforcer.warn(title, body);
        }
        let windows = windows_around(&self.config, self.seed, now);
        due.iter()
            .filter_map(|notice| {
                windows.iter().find(|window| {
                    window.key() == notice.key && window.starts_at.timestamp() == notice.moment
                })
            })
            .filter(|window| window.action != Action::Nothing && !recorded(log, window))
            .map(|window| Record::Started(window.clone()))
            .collect()
    }

    fn enforce(
        &mut self,
        log: &DayLog,
        lock: &ActiveLock,
        earliest_shutdown: Timestamp,
        now: Timestamp,
        language: Language,
    ) -> Outcome {
        let window = &lock.window;
        let identity = format!(
            "{}/{}/{}",
            window.key(),
            window.logical_date,
            lock.locked_since
        );
        let shown = self
            .shown
            .entry(identity.clone())
            .or_insert_with(|| ShownLock {
                label: window.label.clone(),
                logical_date: window.logical_date,
                first_shown: now,
                ended: false,
                shut_down: false,
                test: window.test,
            });
        let notice = later_by(shown.first_shown, SHUTDOWN_NOTICE);
        let shutdown_at = lock
            .shutdown_at
            .map(|at| at.max(earliest_shutdown).max(notice))
            .filter(|at| *at < window.ends_at.timestamp());
        self.enforcer.show_lock(LockView {
            shutdown_at,
            ..lock_view(lock, language.tag())
        });
        let event = |kind| {
            Record::Event(lock_event(
                now,
                &identity,
                &shown.label,
                window.logical_date,
                kind,
                shown.test,
            ))
        };
        let mut records = Vec::new();
        if !recorded(log, window) {
            records.push(Record::Started(window.clone()));
        }
        records.push(event(EventKind::LockStarted));
        let due = shutdown_at.is_some_and(|at| at <= now);
        if !due || shown.shut_down {
            return Outcome {
                records,
                pending_shutdown: shutdown_at.filter(|at| now < *at),
                shut_down: false,
            };
        }
        records.push(event(EventKind::Shutdown));
        shown.shut_down = true;
        Outcome {
            records,
            pending_shutdown: None,
            shut_down: true,
        }
    }

    fn release(&mut self, now: Timestamp) -> Outcome {
        self.enforcer.hide_lock();
        let mut records = Vec::new();
        for (identity, shown) in self.shown.iter_mut().filter(|(_, shown)| !shown.ended) {
            shown.ended = true;
            records.push(Record::Event(lock_event(
                now,
                identity,
                &shown.label,
                shown.logical_date,
                EventKind::LockEnded,
                shown.test,
            )));
        }
        Outcome {
            records,
            pending_shutdown: None,
            shut_down: false,
        }
    }
}

fn write(store: &Store, record: &Record) -> Result<(), StoreError> {
    match record {
        Record::Started(window) => store.record_started(window),
        Record::Event(event) => store.record_event(event).map(drop),
    }
}

pub fn config_in_force(store: &Store, now: Timestamp) -> Result<Config, EngineError> {
    Ok(config_at(&stored_history(store)?, now))
}

pub fn stored_history(store: &Store) -> Result<History, EngineError> {
    history_with(store, &mut HashMap::new())
}

fn history_with(store: &Store, parsed: &mut HashMap<i64, Config>) -> Result<History, EngineError> {
    let (versions, changes) = store.history_rows()?;
    let saved = versions
        .iter()
        .map(|version| {
            Ok(SavedConfig {
                id: version.id,
                saved_at: version.saved_at,
                config: config_of(version, parsed)?,
            })
        })
        .collect::<Result<_, EngineError>>()?;
    Ok(History { saved, changes })
}

fn config_of(
    version: &StoredVersion,
    parsed: &mut HashMap<i64, Config>,
) -> Result<Config, EngineError> {
    if let Some(config) = parsed.get(&version.id) {
        return Ok(config.clone());
    }
    let config = parse(&version.text).map_err(|errors| EngineError::StoredConfig {
        id: version.id,
        errors,
    })?;
    parsed.insert(version.id, config.clone());
    Ok(config)
}

fn recorded(log: &DayLog, window: &RuleWindow) -> bool {
    log.started
        .iter()
        .any(|started| started.key() == window.key() && started.logical_date == window.logical_date)
}

fn earliest_shutdown(history: &History, window: &RuleWindow, now: Timestamp) -> Timestamp {
    let Action::Lock {
        shutdown_after: Some(after),
    } = window.action
    else {
        return Timestamp::MIN;
    };
    rule_since(history, now, window.logical_date.weekday(), &window.rule_id)
        .map_or(Timestamp::MIN, |since| later_by(since, after))
}

pub fn lock_view(lock: &ActiveLock, language: &str) -> LockView {
    let window = &lock.window;
    let challenges = &window.challenges;
    LockView {
        label: window.label.clone(),
        ends_at: window.ends_at.timestamp(),
        extension_available_at: lock.extension_available_at,
        extensions_left: lock.extensions_left,
        extension_minutes: challenges.extension_length.as_mins(),
        reason_required: challenges.reason,
        phrase: challenges.phrase.clone(),
        shutdown_at: lock.shutdown_at,
        language: language.to_owned(),
    }
}

fn rendered(localizer: &Localizer, notice: &Notice, tz: &TimeZone) -> (String, String) {
    let args = notice_args(notice, tz);
    let (title, body) = match notice.kind {
        NoticeKind::Lock => ("notice-title", "warning-body"),
        NoticeKind::ShutDown => ("notice-shutdown-title", "notice-shutdown-body"),
        NoticeKind::Reminder => ("notice-title", "notice-reminder-body"),
    };
    let body = notice
        .message
        .clone()
        .unwrap_or_else(|| localizer.text(&format!("{body}-{}", notice.variant), &args));
    (localizer.text(title, &args), body)
}

fn notice_args(notice: &Notice, tz: &TimeZone) -> [(&'static str, Arg); 3] {
    let time = notice
        .moment
        .to_zoned_datetime(tz.clone())
        .strftime("%H:%M");
    [
        ("label", Arg::Text(notice.label.clone())),
        ("time", Arg::Text(time.to_string())),
        (
            "minutes",
            Arg::Number(notice.at.duration_until(notice.moment).as_mins()),
        ),
    ]
}

fn lock_event(
    at: Timestamp,
    key: &str,
    label: &str,
    logical_date: civil::Date,
    kind: EventKind,
    test: bool,
) -> Event {
    Event {
        at,
        logical_date,
        kind,
        key: key.to_owned(),
        detail: Some(event_detail(json!({ "label": label }), test)),
    }
}

pub(crate) fn event_detail(mut detail: Value, test: bool) -> String {
    if test {
        detail["test"] = Value::Bool(true);
    }
    detail.to_string()
}

fn locked(store: &Mutex<Store>) -> MutexGuard<'_, Store> {
    store.lock().unwrap_or_else(PoisonError::into_inner)
}

fn later_by(at: Timestamp, duration: SignedDuration) -> Timestamp {
    at.checked_add(duration).unwrap_or(Timestamp::MAX)
}

fn earlier_by(at: Timestamp, duration: SignedDuration) -> Timestamp {
    at.checked_sub(duration).unwrap_or(Timestamp::MIN)
}
