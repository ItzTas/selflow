mod constants;
mod types;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use jiff::{SignedDuration, Timestamp, ZonedDateTime, civil, tz::TimeZone};
use selflow_core::config::{Config, parse};
use selflow_core::enforce::{ActiveLock, Enforcement, NightLog, Warning, evaluate};
use selflow_core::timing::{Seed, logical_date};
use selflow_core::versions::{StoredVersion, in_force};
use selflow_i18n::language::Language;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_protocol::ipc::LockView;
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
    version: Option<i64>,
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
            version: None,
            config: Config::empty(),
            localizers: HashMap::new(),
            sent: HashSet::new(),
            shown: HashMap::new(),
        })
    }

    pub fn tick(&mut self) -> Result<Timestamp, EngineError> {
        let now = self.clock.now();
        let instant = now.timestamp();
        let log = self.load(&now)?;
        let evaluation = evaluate(&now, &self.config, self.seed, &log);
        let language = Language::from_tag(&self.config.language).unwrap_or(Language::En);
        self.send_warnings(&now, &evaluation.warnings, language);
        let outcome = match &evaluation.enforcement {
            Enforcement::Free => self.release(instant),
            Enforcement::Locked(lock) => self.enforce(&log, lock, instant, language),
        };
        self.record(&outcome.records);
        if outcome.shut_down {
            self.enforcer.shut_down();
        }
        Ok(evaluation
            .next_change_at
            .into_iter()
            .chain(outcome.pending_shutdown)
            .fold(later_by(instant, MAX_SLEEP), Ord::min))
    }

    fn load(&mut self, now: &ZonedDateTime) -> Result<NightLog, EngineError> {
        let shared = Arc::clone(&self.store);
        let store = locked(&shared);
        self.refresh_config(&store, now.timestamp())?;
        let today = logical_date(now, self.config.day_starts_at);
        let nights: Vec<civil::Date> = today.yesterday().into_iter().chain(Some(today)).collect();
        Ok(store.night_log(&nights)?)
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

    fn refresh_config(&mut self, store: &Store, now: Timestamp) -> Result<(), EngineError> {
        let versions = store.versions()?;
        let version = in_force(&versions, now);
        let id = version.map(|version| version.id);
        if id == self.version {
            return Ok(());
        }
        self.config = parsed(version)?;
        self.version = id;
        Ok(())
    }

    fn send_warnings(&mut self, now: &ZonedDateTime, warnings: &[Warning], language: Language) {
        let instant = now.timestamp();
        let since = earlier_by(instant, WARNING_GRACE);
        for warning in warnings
            .iter()
            .filter(|warning| since < warning.at && warning.at <= instant)
        {
            if !self.sent.insert((warning.key.clone(), warning.at)) {
                continue;
            }
            let localizer = self
                .localizers
                .entry(language)
                .or_insert_with(|| Localizer::new(language));
            let args = warning_args(warning, now.time_zone());
            let title = localizer.text("warning-title", &args);
            let body = localizer.text(&format!("warning-body-{}", warning.variant), &args);
            self.enforcer.warn(title, body);
        }
    }

    fn enforce(
        &mut self,
        log: &NightLog,
        lock: &ActiveLock,
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
            });
        let shutdown_at = lock
            .shutdown_at
            .map(|at| at.max(later_by(shown.first_shown, SHUTDOWN_NOTICE)));
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
            ))
        };
        let mut records = Vec::new();
        let recorded = log.started.iter().any(|started| {
            started.key() == window.key() && started.logical_date == window.logical_date
        });
        if !recorded {
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
    parsed(in_force(&store.versions()?, now))
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

fn parsed(version: Option<&StoredVersion>) -> Result<Config, EngineError> {
    let Some(version) = version else {
        return Ok(Config::empty());
    };
    parse(&version.text).map_err(|errors| EngineError::StoredConfig {
        id: version.id,
        errors,
    })
}

fn warning_args(warning: &Warning, tz: &TimeZone) -> [(&'static str, Arg); 3] {
    let time = warning
        .lock_at
        .to_zoned_datetime(tz.clone())
        .strftime("%H:%M");
    [
        ("label", Arg::Text(warning.label.clone())),
        ("time", Arg::Text(time.to_string())),
        (
            "minutes",
            Arg::Number(warning.at.duration_until(warning.lock_at).as_mins()),
        ),
    ]
}

fn lock_event(
    at: Timestamp,
    key: &str,
    label: &str,
    logical_date: civil::Date,
    kind: EventKind,
) -> Event {
    Event {
        at,
        logical_date,
        kind,
        key: key.to_owned(),
        detail: Some(serde_json::json!({ "label": label }).to_string()),
    }
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
