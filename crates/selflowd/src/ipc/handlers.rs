use std::sync::{Mutex, MutexGuard, PoisonError};

use jiff::{SignedDuration, Timestamp, ZonedDateTime, civil};
use selflow_core::challenge::{ExtensionAnswer, Refusal, request_extension};
use selflow_core::config::{Action, Config, ConfigError, Rule, RuleSet, parse};
use selflow_core::enforce::{Enforcement, GrantedExtension, evaluate};
use selflow_core::plan::{LockWindow, windows_around};
use selflow_core::timing::logical_date;
use selflow_core::versions::{
    Change, ChangeKind, Classification, StoredVersion, classify, effective_at, in_force, pending,
};
use selflow_i18n::catalog::default_config;
use selflow_i18n::language::Language;
use selflow_protocol::ipc::{
    ChangeKindView, ConfigStatusView, LockView, MessageView, PendingView, PositionedMessage,
    RefusalView, ReportEntry, ReportView, Request, Response, StatusView,
};
use serde_json::{Value, json};
use tokio::sync::watch;

use super::constants::{TEST_LOCK_MAX, TEST_LOCK_MIN};
use super::types::{IpcError, Shared};
use crate::enforcer::PowerMode;
use crate::engine::{EngineError, config_in_force, event_detail};
use crate::store::{Event, EventKind, Store};

pub fn handle(request: Request, shared: &Shared) -> Response {
    let now = shared.clock.now();
    let store = locked(&shared.store);
    let answer = match request {
        Request::Status => status(&store, &now, &shared.views),
        Request::SubscribeLock => Ok(lock_state(&shared.views.borrow())),
        Request::RequestExtension { reason, phrase } => {
            extend(&store, &now, &ExtensionAnswer { reason, phrase })
        }
        Request::GetConfig => config_text(&store),
        Request::SaveConfig { text } => save(&store, &now, text),
        Request::ConfigStatus => config_status(&store, now.timestamp()),
        Request::Report { logical_date } => report(&store, &now, logical_date),
        Request::TestLock { rule, duration } => {
            test_lock(&store, &now, shared, rule.as_deref(), duration)
        }
    };
    drop(store);
    let response = answer.unwrap_or_else(|error| {
        tracing::error!(%error, "couldn't answer a request");
        Response::Error(message("ipc-failed", &[]))
    });
    if matches!(
        response,
        Response::Saved { .. }
            | Response::ExtensionGranted { .. }
            | Response::TestLockStarted { .. }
    ) {
        let _ = shared.wake.try_send(());
    }
    response
}

pub(super) fn lock_state(view: &Option<LockView>) -> Response {
    match view {
        Some(view) => Response::Lock(view.clone()),
        None => Response::LockEnded,
    }
}

fn status(
    store: &Store,
    now: &ZonedDateTime,
    views: &watch::Receiver<Option<LockView>>,
) -> Result<Response, IpcError> {
    let instant = now.timestamp();
    let versions = store.versions()?;
    let current = in_force(&versions, instant);
    let config = config_of(current)?;
    let next_lock_at = windows_around(&config, store.seed()?, now)
        .iter()
        .map(|window| window.starts_at.timestamp())
        .filter(|starts_at| *starts_at > instant)
        .min();
    let language = match current {
        Some(_) => config.language,
        None => Language::from_env().tag().to_owned(),
    };
    Ok(Response::Status(StatusView {
        language,
        locked: views.borrow().clone(),
        next_lock_at,
    }))
}

fn config_text(store: &Store) -> Result<Response, IpcError> {
    let text = store
        .latest_text()?
        .unwrap_or_else(|| default_config(Language::from_env()).to_owned());
    Ok(Response::Config { text })
}

fn save(store: &Store, now: &ZonedDateTime, text: String) -> Result<Response, IpcError> {
    let proposed = match parse(&text) {
        Ok(config) => config,
        Err(errors) => {
            return Ok(Response::Invalid {
                errors: errors.iter().map(positioned).collect(),
            });
        }
    };
    let saved_at = now.timestamp();
    let versions = store.versions()?;
    let current_version = in_force(&versions, saved_at);
    let current = config_of(current_version)?;
    let Classification { kind, changes } = classify(&current, &proposed);
    let takes_effect = current_version.map_or(saved_at, |_| effective_at(kind, saved_at, &current));
    let id = store.save_version(&text, saved_at, takes_effect, kind)?;
    let kind = kind_view(kind);
    record(
        store,
        &Event {
            at: saved_at,
            logical_date: logical_date(now, current.day_starts_at),
            kind: EventKind::ConfigSaved,
            key: id.to_string(),
            detail: Some(json!({ "kind": &kind }).to_string()),
        },
    );
    Ok(Response::Saved {
        kind,
        effective_at: takes_effect,
        changes: changes.iter().map(change_message).collect(),
    })
}

fn config_status(store: &Store, now: Timestamp) -> Result<Response, IpcError> {
    let versions = store.versions()?;
    let current = in_force(&versions, now);
    let waiting = pending(&versions, now)
        .map(|version| pending_view(current, version))
        .transpose()?;
    Ok(Response::ConfigStatus(ConfigStatusView {
        in_force_saved_at: current.map(|version| version.saved_at),
        pending: waiting,
    }))
}

fn pending_view(
    current: Option<&StoredVersion>,
    version: &StoredVersion,
) -> Result<PendingView, IpcError> {
    let changes = classify(&config_of(current)?, &config_of(Some(version))?).changes;
    Ok(PendingView {
        saved_at: version.saved_at,
        effective_at: version.effective_at,
        changes: changes.iter().map(change_message).collect(),
    })
}

fn extend(
    store: &Store,
    now: &ZonedDateTime,
    answer: &ExtensionAnswer,
) -> Result<Response, IpcError> {
    let config = config_in_force(store, now.timestamp())?;
    let today = logical_date(now, config.day_starts_at);
    let nights: Vec<civil::Date> = today.yesterday().into_iter().chain(Some(today)).collect();
    let log = store.night_log(&nights)?;
    let Enforcement::Locked(lock) = evaluate(now, &config, store.seed()?, &log).enforcement else {
        return Ok(Response::Error(message("ipc-not-locked", &[])));
    };
    let extension = match request_extension(now.timestamp(), &lock, answer) {
        Ok(extension) => extension,
        Err(refusal) => return Ok(Response::ExtensionRefused(refusal_view(refusal))),
    };
    store.record_extension(&extension)?;
    record(
        store,
        &granted_event(&extension, &lock.window.label, lock.window.test),
    );
    Ok(Response::ExtensionGranted {
        until: extension.until,
    })
}

fn test_lock(
    store: &Store,
    now: &ZonedDateTime,
    shared: &Shared,
    rule: Option<&str>,
    duration: SignedDuration,
) -> Result<Response, IpcError> {
    if shared.power == PowerMode::Real {
        return Ok(Response::Error(message("ipc-test-lock-real-power", &[])));
    }
    if !(TEST_LOCK_MIN..=TEST_LOCK_MAX).contains(&duration) {
        let bounds = [
            ("min", TEST_LOCK_MIN.as_mins().to_string()),
            ("max", TEST_LOCK_MAX.as_mins().to_string()),
        ];
        return Ok(Response::Error(message("ipc-test-lock-duration", &bounds)));
    }
    if shared.views.borrow().is_some() {
        return Ok(Response::Error(message("ipc-test-lock-busy", &[])));
    }
    let config = config_in_force(store, now.timestamp())?;
    let today = logical_date(now, config.day_starts_at);
    let nights: Vec<civil::Date> = today.yesterday().into_iter().chain(Some(today)).collect();
    let open_test = store
        .night_log(&nights)?
        .started
        .iter()
        .any(|window| window.test && window.ends_at.timestamp() > now.timestamp());
    if open_test {
        return Ok(Response::Error(message("ipc-test-lock-busy", &[])));
    }
    let (set, lock) = match chosen_rule(&config, today, rule) {
        Ok(chosen) => chosen,
        Err(refusal) => return Ok(Response::Error(refusal)),
    };
    let window = LockWindow {
        rule_set: format!("{}#test-{}", set.name, now.timestamp().as_second()),
        rule_id: lock.id.clone(),
        label: lock.label.clone(),
        logical_date: today,
        starts_at: now.clone(),
        ends_at: now.checked_add(duration)?,
        warnings: lock.notice_offsets(),
        shutdown_after: None,
        challenges: lock.challenges.clone(),
        test: true,
    };
    store.record_started(&window)?;
    Ok(Response::TestLockStarted {
        label: window.label,
        ends_at: window.ends_at.timestamp(),
    })
}

fn chosen_rule<'a>(
    config: &'a Config,
    today: civil::Date,
    slug: Option<&str>,
) -> Result<(&'a RuleSet, &'a Rule), MessageView> {
    let Some(slug) = slug else {
        return config
            .rule_set_for(today.weekday())
            .and_then(|set| lock_rules(set).next().map(|lock| (set, lock)))
            .ok_or_else(|| message("ipc-test-lock-no-rule", &[]));
    };
    let rules: Vec<(&RuleSet, &Rule)> = config
        .rule_sets
        .iter()
        .flat_map(|set| lock_rules(set).map(move |lock| (set, lock)))
        .collect();
    let slug_of = |(set, lock): &(&RuleSet, &Rule)| format!("{}/{}", set.name, lock.id);
    if let Some(found) = rules.iter().find(|rule| slug_of(rule) == slug) {
        return Ok(*found);
    }
    let valid: Vec<String> = rules.iter().map(slug_of).collect();
    Err(message(
        "ipc-test-lock-unknown-rule",
        &[("rule", slug.to_owned()), ("rules", valid.join(", "))],
    ))
}

fn lock_rules(set: &RuleSet) -> impl Iterator<Item = &Rule> {
    set.rules
        .iter()
        .filter(|rule| matches!(rule.action, Action::Lock { .. }))
}

fn report(
    store: &Store,
    now: &ZonedDateTime,
    requested: Option<civil::Date>,
) -> Result<Response, IpcError> {
    let day = match requested {
        Some(day) => day,
        None => {
            let today = logical_date(now, config_in_force(store, now.timestamp())?.day_starts_at);
            today.yesterday().unwrap_or(today)
        }
    };
    let mut entries: Vec<ReportEntry> = store.events_for(day)?.iter().map(report_entry).collect();
    let versions = store.versions()?;
    entries.extend(
        pending(&versions, now.timestamp()).map(|version| ReportEntry {
            at: version.effective_at,
            message: message(
                "report-pending",
                &[("when", version.effective_at.to_string())],
            ),
        }),
    );
    Ok(Response::Report(ReportView {
        logical_date: day,
        entries,
    }))
}

fn report_entry(event: &Event) -> ReportEntry {
    let detail = detail_of(event);
    let field = |name: &str| match &detail[name] {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let (mut id, names): (String, &[&str]) = match event.kind {
        EventKind::LockStarted => ("report-lock-started".to_owned(), &["label"]),
        EventKind::LockEnded => ("report-lock-ended".to_owned(), &["label"]),
        EventKind::ExtensionGranted => (
            "report-extension".to_owned(),
            &["label", "minutes", "reason"],
        ),
        EventKind::Shutdown => ("report-shutdown".to_owned(), &["label"]),
        EventKind::ConfigSaved => (format!("report-config-saved-{}", field("kind")), &[]),
    };
    if detail["test"] == true && event.kind != EventKind::Shutdown {
        id.push_str("-test");
    }
    ReportEntry {
        at: event.at,
        message: MessageView {
            id,
            args: names
                .iter()
                .map(|name| ((*name).to_owned(), field(name)))
                .collect(),
        },
    }
}

fn detail_of(event: &Event) -> Value {
    let Some(text) = &event.detail else {
        return Value::Null;
    };
    serde_json::from_str(text).unwrap_or_else(|error| {
        tracing::warn!(%error, key = %event.key, "an event has an unreadable detail");
        Value::Null
    })
}

fn granted_event(extension: &GrantedExtension, label: &str, test: bool) -> Event {
    let minutes = extension
        .granted_at
        .duration_until(extension.until)
        .as_mins();
    Event {
        at: extension.granted_at,
        logical_date: extension.logical_date,
        kind: EventKind::ExtensionGranted,
        key: format!(
            "{}/{}/{}/{}",
            extension.rule_set, extension.rule_id, extension.logical_date, extension.granted_at
        ),
        detail: Some(event_detail(
            json!({ "label": label, "minutes": minutes, "reason": extension.reason }),
            test,
        )),
    }
}

fn record(store: &Store, event: &Event) {
    if let Err(error) = store.record_event(event) {
        tracing::error!(%error, ?event, "couldn't record an event");
    }
}

fn config_of(version: Option<&StoredVersion>) -> Result<Config, IpcError> {
    let Some(version) = version else {
        return Ok(Config::empty());
    };
    parse(&version.text).map_err(|errors| {
        IpcError::from(EngineError::StoredConfig {
            id: version.id,
            errors,
        })
    })
}

fn kind_view(kind: ChangeKind) -> ChangeKindView {
    match kind {
        ChangeKind::Tighten => ChangeKindView::Tighten,
        ChangeKind::Loosen => ChangeKindView::Loosen,
        ChangeKind::Neutral => ChangeKindView::Neutral,
    }
}

fn refusal_view(refusal: Refusal) -> RefusalView {
    match refusal {
        Refusal::TooEarly { available_at } => RefusalView::TooEarly { available_at },
        Refusal::NoBudget => RefusalView::NoBudget,
        Refusal::ReasonTooShort { min_chars } => RefusalView::ReasonTooShort { min_chars },
        Refusal::PhraseMismatch => RefusalView::PhraseMismatch,
    }
}

fn positioned(error: &ConfigError) -> PositionedMessage {
    PositionedMessage {
        message: message(error.id, &error.args),
        line: error.line,
        column: error.column,
    }
}

fn change_message(change: &Change) -> MessageView {
    message(change.id, &change.args)
}

fn message(id: &str, args: &[(&str, String)]) -> MessageView {
    MessageView {
        id: id.to_owned(),
        args: args
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    }
}

fn locked(store: &Mutex<Store>) -> MutexGuard<'_, Store> {
    store.lock().unwrap_or_else(PoisonError::into_inner)
}
