mod constants;
mod types;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use jiff::{SignedDuration, Timestamp, ToSpan, Unit, ZonedDateTime, civil, tz::TimeZone};
use selflow_core::agent::ActiveAgentStep;
use selflow_core::config::{Action, Config, parse};
use selflow_core::enforce::{
    ActiveLock, DayLog, Enforcement, Evaluation, Notice, NoticeKind, evaluate,
};
use selflow_core::plan::{RuleWindow, windows_around};
use selflow_core::timing::{Seed, logical_date};
use selflow_core::versions::{History, SavedConfig, StoredVersion, config_at, rule_since};
use selflow_i18n::language::Language;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_protocol::ipc::LockView;
use serde_json::{Value, json};
use tokio::sync::mpsc::Receiver;

use crate::agent::Directives;
use crate::clock::Clock;
use crate::store::{Announcement, BootState, Event, EventKind, Store, StoreError, StoredLock};

pub use constants::{MAX_SLEEP, SHUTDOWN_NOTICE, WARNING_GRACE};
pub use types::{Enforcer, EngineError, ParsedVersions};
use types::{Outcome, Record};

pub struct Engine<C: Clock, E: Enforcer> {
    clock: Arc<C>,
    enforcer: E,
    store: Arc<Mutex<Store>>,
    boot_id: String,
    parsed: ParsedVersions,
    seed: Seed,
    config: Config,
    localizers: HashMap<Language, Localizer>,
    sent: HashSet<(String, Timestamp)>,
    announced: HashMap<(String, Timestamp), Timestamp>,
    shut_down: HashSet<String>,
    shown: HashMap<String, StoredLock>,
}

impl<C: Clock, E: Enforcer> Engine<C, E> {
    pub fn new(
        clock: Arc<C>,
        enforcer: E,
        store: Arc<Mutex<Store>>,
        boot_id: String,
        parsed: ParsedVersions,
    ) -> Result<Self, EngineError> {
        let since = clock.now().date().saturating_sub(2.days());
        let (seed, state) = {
            let store = locked(&store);
            (store.seed()?, restored(&store, &boot_id, since))
        };
        Ok(Engine {
            clock,
            enforcer,
            store,
            boot_id,
            parsed,
            seed,
            config: Config::empty(),
            localizers: HashMap::new(),
            sent: state.sent.into_iter().collect(),
            announced: state
                .announced
                .into_iter()
                .map(|announcement| {
                    let identity = (announcement.rule_key, announcement.starts_at);
                    (identity, announcement.announced_at)
                })
                .collect(),
            shut_down: state.shut_down.into_iter().collect(),
            shown: state
                .locks
                .into_iter()
                .map(|lock| (lock.identity.clone(), lock))
                .collect(),
        })
    }

    pub fn tick(&mut self) -> Result<Timestamp, EngineError> {
        let now = self.clock.now();
        let instant = now.timestamp();
        let (log, history) = self.load(&now)?;
        let evaluation = evaluate(&now, &self.config, self.seed, &log);
        let language = Language::from_tag(&self.config.language).unwrap_or(Language::En);
        self.enforcer.direct_agents(Directives {
            steps: evaluation.agent.clone(),
            allowed_commands: self.config.agent_allowed_commands.clone(),
            language,
        });
        let mut records = self.send_notices(&now, &evaluation, &log, language);
        records.extend(agent_started(&log, &evaluation.agent));
        let rule = evaluation
            .shutdown
            .as_ref()
            .filter(|window| !self.shut_down.contains(&shutdown_key(window)));
        let outcome = match &evaluation.enforcement {
            Enforcement::Free => self.free(&log, rule, &now, language),
            Enforcement::Locked(lock) => {
                let own = lock
                    .shutdown_at
                    .map(|at| at.max(earliest_shutdown(&history, &lock.window, instant)));
                self.enforce(&log, lock, own, rule, instant, language)
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
        let history = stored_history(&store, &self.parsed)?;
        self.config = config_at(&history, now.timestamp());
        let today = logical_date(now, self.config.day_starts_at);
        let nights: Vec<civil::Date> = today.yesterday().into_iter().chain(Some(today)).collect();
        Ok((store.night_log(&nights)?, history))
    }

    fn record(&self, records: &[Record]) {
        let store = locked(&self.store);
        for record in records {
            record_on(&store, record);
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
        evaluation: &Evaluation,
        log: &DayLog,
        language: Language,
    ) -> Vec<Record> {
        let instant = now.timestamp();
        let since = earlier_by(instant, WARNING_GRACE);
        let due: Vec<&Notice> = evaluation
            .notices
            .iter()
            .filter(|notice| since < notice.at && notice.at <= instant)
            .filter(|notice| self.sent.insert((notice.key.clone(), notice.at)))
            .collect();
        if due.is_empty() {
            return Vec::new();
        }
        let visible = matches!(evaluation.enforcement, Enforcement::Free);
        for notice in &due {
            let sent =
                locked(&self.store).record_notice_sent(&self.boot_id, &notice.key, notice.at);
            log_failed_write(sent, "a notice as sent");
        }
        for notice in due
            .iter()
            .filter(|notice| visible && notice.kind == NoticeKind::ShutDown)
        {
            let announcement = Announcement {
                rule_key: notice.key.clone(),
                starts_at: notice.moment,
                announced_at: instant,
            };
            announce(
                &self.store,
                &self.boot_id,
                &mut self.announced,
                announcement,
            );
        }
        let localizer = localizer_for(&mut self.localizers, language);
        for notice in &due {
            let told = as_told(notice, &self.announced, instant);
            let (title, body) = rendered(localizer, &told, now.time_zone());
            self.enforcer.warn(title, body);
        }
        let windows = windows_around(&self.config, self.seed, now);
        due.iter()
            .filter_map(|notice| {
                windows.iter().find(|window| {
                    window.key() == notice.key && window.starts_at.timestamp() == notice.moment
                })
            })
            .filter(|window| window.enforces() && !recorded(log, window))
            .map(|window| Record::Started(window.clone()))
            .collect()
    }

    fn enforce(
        &mut self,
        log: &DayLog,
        lock: &ActiveLock,
        own_shutdown: Option<Timestamp>,
        rule: Option<&RuleWindow>,
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
            .or_insert_with(|| StoredLock {
                identity: identity.clone(),
                label: window.label.clone(),
                logical_date: window.logical_date,
                test: window.test,
                ended: false,
                shutdown_on_screen: None,
                shut_down: false,
            });
        let wanted = own_shutdown
            .into_iter()
            .chain(rule.map(|rule| rule.starts_at.timestamp()))
            .min();
        let shutdown_at = shown_shutdown(shown, wanted, now, window.ends_at.timestamp());
        save_lock(&self.store, &self.boot_id, shown);
        if let Some(rule) = rule.filter(|_| shutdown_at.is_some()) {
            let announcement = Announcement {
                rule_key: rule.key(),
                starts_at: rule.starts_at.timestamp(),
                announced_at: now,
            };
            announce(
                &self.store,
                &self.boot_id,
                &mut self.announced,
                announcement,
            );
        }
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
        let mut records: Vec<Record> = started_record(log, window)
            .into_iter()
            .chain(rule.and_then(|rule| started_record(log, rule)))
            .collect();
        records.push(event(EventKind::LockStarted));
        let due = shutdown_at.is_some_and(|at| at <= now);
        if !due || shown.shut_down {
            return Outcome {
                records,
                pending_shutdown: shutdown_at.filter(|at| now < *at),
                shut_down: false,
            };
        }
        if own_shutdown.is_some_and(|at| at <= now) {
            records.push(event(EventKind::Shutdown));
        }
        shown.shut_down = true;
        save_lock(&self.store, &self.boot_id, shown);
        if let Some(rule) = rule {
            shutdown_done(&self.store, &self.boot_id, &mut self.shut_down, rule);
            records.push(rule_shutdown(rule, now));
        }
        Outcome {
            records,
            pending_shutdown: None,
            shut_down: true,
        }
    }

    fn free(
        &mut self,
        log: &DayLog,
        rule: Option<&RuleWindow>,
        now: &ZonedDateTime,
        language: Language,
    ) -> Outcome {
        let instant = now.timestamp();
        self.release(instant);
        let Some(rule) = rule else {
            return Outcome {
                records: Vec::new(),
                pending_shutdown: None,
                shut_down: false,
            };
        };
        let mut records: Vec<Record> = started_record(log, rule).into_iter().collect();
        let Some(shutdown_at) = self.announced_shutdown(rule, now, language) else {
            return Outcome {
                records,
                pending_shutdown: None,
                shut_down: false,
            };
        };
        if instant < shutdown_at {
            return Outcome {
                records,
                pending_shutdown: Some(shutdown_at),
                shut_down: false,
            };
        }
        shutdown_done(&self.store, &self.boot_id, &mut self.shut_down, rule);
        records.push(rule_shutdown(rule, instant));
        Outcome {
            records,
            pending_shutdown: None,
            shut_down: true,
        }
    }

    fn announced_shutdown(
        &mut self,
        rule: &RuleWindow,
        now: &ZonedDateTime,
        language: Language,
    ) -> Option<Timestamp> {
        let instant = now.timestamp();
        let starts_at = rule.starts_at.timestamp();
        let ends_at = rule.ends_at.timestamp();
        let identity = (rule.key(), starts_at);
        let announced = announced_shutdown_at(self.announced.get(&identity), starts_at, instant);
        if let Some(shutdown_at) = announced {
            return (shutdown_at < ends_at).then_some(shutdown_at);
        }
        let shutdown_at = later_by(instant, SHUTDOWN_NOTICE);
        if shutdown_at >= ends_at {
            return None;
        }
        let announcement = Announcement {
            rule_key: identity.0.clone(),
            starts_at,
            announced_at: instant,
        };
        let stored = locked(&self.store).replace_announcement(&self.boot_id, &announcement);
        log_failed_write(stored, "a shutdown announcement");
        self.announced.insert(identity, instant);
        let localizer = localizer_for(&mut self.localizers, language);
        let (title, body) = shutdown_now(localizer, &rule.label, shutdown_at, now.time_zone());
        self.enforcer.warn(title, body);
        Some(shutdown_at)
    }

    fn release(&mut self, now: Timestamp) {
        self.enforcer.hide_lock();
        let store = locked(&self.store);
        for shown in self.shown.values_mut().filter(|shown| !shown.ended) {
            let ended = Record::Event(lock_event(
                now,
                &shown.identity,
                &shown.label,
                shown.logical_date,
                EventKind::LockEnded,
                shown.test,
            ));
            record_on(&store, &ended);
            shown.ended = true;
            log_failed_write(store.save_lock(&self.boot_id, shown), "a lock as ended");
        }
    }
}

fn restored(store: &Store, boot_id: &str, since: civil::Date) -> BootState {
    if let Err(error) = store.forget_other_boots(boot_id, since) {
        tracing::error!(%error, "couldn't forget the enforcement state of earlier boots");
    }
    store.boot_state(boot_id, since).unwrap_or_else(|error| {
        tracing::error!(%error, "couldn't restore this boot's enforcement state, starting empty");
        BootState::default()
    })
}

fn announce(
    store: &Mutex<Store>,
    boot_id: &str,
    announced: &mut HashMap<(String, Timestamp), Timestamp>,
    announcement: Announcement,
) {
    log_failed_write(
        locked(store).record_announcement(boot_id, &announcement),
        "a shutdown announcement",
    );
    announced
        .entry((announcement.rule_key, announcement.starts_at))
        .or_insert(announcement.announced_at);
}

fn save_lock(store: &Mutex<Store>, boot_id: &str, lock: &StoredLock) {
    log_failed_write(locked(store).save_lock(boot_id, lock), "a lock on screen");
}

fn shutdown_done(
    store: &Mutex<Store>,
    boot_id: &str,
    shut_down: &mut HashSet<String>,
    rule: &RuleWindow,
) {
    let key = shutdown_key(rule);
    log_failed_write(
        locked(store).record_shutdown_done(boot_id, &key),
        "a rule's shutdown",
    );
    shut_down.insert(key);
}

fn log_failed_write(written: Result<(), StoreError>, what: &str) {
    if let Err(error) = written {
        tracing::error!(%error, "couldn't store {what}");
    }
}

fn shown_shutdown(
    shown: &mut StoredLock,
    wanted: Option<Timestamp>,
    now: Timestamp,
    ends_at: Timestamp,
) -> Option<Timestamp> {
    let on_screen = shown
        .shutdown_on_screen
        .filter(|at| still_on_time(*at, now))
        .unwrap_or(Timestamp::MAX);
    let floor = later_by(now, SHUTDOWN_NOTICE).min(on_screen);
    shown.shutdown_on_screen = wanted.map(|at| at.max(floor)).filter(|at| *at < ends_at);
    shown.shutdown_on_screen
}

fn as_told(
    notice: &Notice,
    announced: &HashMap<(String, Timestamp), Timestamp>,
    now: Timestamp,
) -> Notice {
    if notice.kind != NoticeKind::ShutDown {
        return notice.clone();
    }
    let identity = (notice.key.clone(), notice.moment);
    let moment = announced_shutdown_at(announced.get(&identity), notice.moment, now)
        .unwrap_or_else(|| notice.moment.max(later_by(now, SHUTDOWN_NOTICE)));
    Notice {
        at: now,
        moment,
        ..notice.clone()
    }
}

fn announced_shutdown_at(
    announced: Option<&Timestamp>,
    starts_at: Timestamp,
    now: Timestamp,
) -> Option<Timestamp> {
    announced
        .map(|at| starts_at.max(later_by(*at, SHUTDOWN_NOTICE)))
        .filter(|at| still_on_time(*at, now))
}

fn still_on_time(shutdown_at: Timestamp, now: Timestamp) -> bool {
    now <= later_by(shutdown_at, SHUTDOWN_NOTICE)
}

fn agent_started(log: &DayLog, steps: &[ActiveAgentStep]) -> Vec<Record> {
    let mut windows: Vec<&RuleWindow> = Vec::new();
    for window in steps.iter().map(|active| &active.window) {
        if recorded(log, window) || windows.iter().any(|kept| same_window(kept, window)) {
            continue;
        }
        windows.push(window);
    }
    windows
        .into_iter()
        .map(|window| Record::Started(window.clone()))
        .collect()
}

fn started_record(log: &DayLog, window: &RuleWindow) -> Option<Record> {
    starts_earlier_than_recorded(log, window).then(|| Record::Started(window.clone()))
}

fn shutdown_key(window: &RuleWindow) -> String {
    format!("{}/{}", window.key(), window.logical_date)
}

fn rule_shutdown(window: &RuleWindow, at: Timestamp) -> Record {
    let repeat = matches!(window.action, Action::ShutDown { repeat: true });
    let once = shutdown_key(window);
    let key = if repeat { format!("{once}/{at}") } else { once };
    Record::Event(Event {
        at,
        logical_date: window.logical_date,
        kind: EventKind::RuleShutdown,
        key,
        detail: Some(
            json!({ "label": window.label, "rule": window.rule_id, "repeat": repeat }).to_string(),
        ),
    })
}

fn record_on(store: &Store, record: &Record) {
    if let Err(error) = write(store, record) {
        tracing::error!(%error, ?record, "couldn't record an enforcement step");
    }
}

fn write(store: &Store, record: &Record) -> Result<(), StoreError> {
    match record {
        Record::Started(window) => store.record_started(window),
        Record::Event(event) => store.record_event(event).map(drop),
    }
}

pub fn config_in_force(
    store: &Store,
    parsed: &ParsedVersions,
    now: Timestamp,
) -> Result<Config, EngineError> {
    Ok(config_at(&stored_history(store, parsed)?, now))
}

pub fn stored_history(store: &Store, parsed: &ParsedVersions) -> Result<History, EngineError> {
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

fn config_of(version: &StoredVersion, parsed: &ParsedVersions) -> Result<Arc<Config>, EngineError> {
    if let Some(config) = parsed.get(version.id) {
        return Ok(config);
    }
    let config = parse(&version.text).map_err(|errors| EngineError::StoredConfig {
        id: version.id,
        errors,
    })?;
    Ok(parsed.keep(version.id, config))
}

fn starts_earlier_than_recorded(log: &DayLog, window: &RuleWindow) -> bool {
    log.started
        .iter()
        .find(|started| same_window(started, window))
        .is_none_or(|started| window.starts_at.timestamp() < started.starts_at.timestamp())
}

fn same_window(started: &RuleWindow, window: &RuleWindow) -> bool {
    started.key() == window.key() && started.logical_date == window.logical_date
}

fn recorded(log: &DayLog, window: &RuleWindow) -> bool {
    log.started
        .iter()
        .any(|started| same_window(started, window))
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
    let until = notice.at.duration_until(notice.moment);
    [
        ("label", Arg::Text(notice.label.clone())),
        ("time", Arg::Text(clock_time(notice.moment, tz))),
        (
            "minutes",
            Arg::Number(until.round(Unit::Minute).unwrap_or(until).as_mins()),
        ),
    ]
}

fn shutdown_now(
    localizer: &Localizer,
    label: &str,
    at: Timestamp,
    tz: &TimeZone,
) -> (String, String) {
    let time = ("time", Arg::Text(clock_time(at, tz)));
    let title = localizer.text(
        "notice-shutdown-title",
        &[("label", Arg::Text(label.to_owned())), time.clone()],
    );
    (title, localizer.text("notice-shutdown-now", &[time]))
}

pub(crate) fn clock_time(at: Timestamp, tz: &TimeZone) -> String {
    at.to_zoned_datetime(tz.clone())
        .strftime("%H:%M")
        .to_string()
}

fn localizer_for(localizers: &mut HashMap<Language, Localizer>, language: Language) -> &Localizer {
    localizers
        .entry(language)
        .or_insert_with(|| Localizer::new(language))
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
