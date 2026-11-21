use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard, PoisonError};

use jiff::{SignedDuration, Timestamp, ZonedDateTime, civil};
use selflow_core::challenge::{ExtensionAnswer, Refusal, request_extension};
use selflow_core::config::{Action, Config, ConfigError, Rule, RuleSet, parse};
use selflow_core::enforce::{Enforcement, GrantedExtension, evaluate};
use selflow_core::plan::{LockWindow, windows_around};
use selflow_core::timing::logical_date;
use selflow_core::versions::{TimedChange, config_at, plan_save, shown, stored_changes, waiting};
use selflow_i18n::catalog::default_config;
use selflow_i18n::language::Language;
use selflow_i18n::localizer::Localizer;
use selflow_protocol::ipc::{
    ChangeView, ConfigStatusView, LockView, MessageView, PositionedMessage, RefusalView,
    ReportEntry, ReportView, Request, Response, StatusView,
};
use serde_json::{Value, json};

use super::constants::{TEST_LOCK_MAX, TEST_LOCK_MIN};
use super::types::{IpcError, Shared};
use crate::enforcer::PowerMode;
use crate::engine::{config_in_force, event_detail, stored_history};
use crate::store::{Event, EventKind, Store};

pub fn handle(request: Request, shared: &Shared) -> Response {
    let now = shared.clock.now();
    let store = locked(&shared.store);
    let answer = match request {
        Request::Status => status(&store, &now, shared),
        Request::SubscribeLock => Ok(lock_state(&shared.views.borrow())),
        Request::RequestExtension { reason, phrase } => {
            extend(&store, &now, &ExtensionAnswer { reason, phrase })
        }
        Request::GetConfig => config_text(&store, shared.env_language),
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

fn status(store: &Store, now: &ZonedDateTime, shared: &Shared) -> Result<Response, IpcError> {
    let instant = now.timestamp();
    let history = stored_history(store)?;
    let config = config_at(&history, instant);
    let next_lock_at = windows_around(&config, store.seed()?, now)
        .iter()
        .map(|window| window.starts_at.timestamp())
        .filter(|starts_at| *starts_at > instant)
        .min();
    let language = if history.saved.is_empty() {
        shared.env_language.tag().to_owned()
    } else {
        config.language
    };
    Ok(Response::Status(StatusView {
        language,
        locked: shared.views.borrow().clone(),
        next_lock_at,
    }))
}

fn config_text(store: &Store, language: Language) -> Result<Response, IpcError> {
    if let Some(text) = store.latest_text()? {
        return Ok(Response::Config { text });
    }
    let default = default_config(language);
    let Some(old) = store.m1_text()? else {
        return Ok(Response::Config {
            text: default.to_owned(),
        });
    };
    let note = Localizer::new(language).text("config-before-update", &[]);
    let commented: Vec<String> = old
        .lines()
        .map(|line| match line {
            "" => "#".to_owned(),
            _ => format!("# {line}"),
        })
        .collect();
    Ok(Response::Config {
        text: format!("{default}\n# {note}\n{}\n", commented.join("\n")),
    })
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
    let history = stored_history(store)?;
    let planned = plan_save(&history, &proposed, saved_at);
    let changes = stored_changes(&planned);
    let id = store.save_version(&text, saved_at, &changes)?;
    let (applied, later): (Vec<_>, Vec<_>) = changes
        .iter()
        .partition(|change| change.effective_at <= saved_at);
    record(
        store,
        &Event {
            at: saved_at,
            logical_date: logical_date(now, config_at(&history, saved_at).day_starts_at),
            kind: EventKind::ConfigSaved,
            key: id.to_string(),
            detail: Some(json!({ "now": applied.len(), "waiting": later.len() }).to_string()),
        },
    );
    Ok(Response::Saved {
        saved_at,
        changes: change_views(&planned),
    })
}

fn config_status(store: &Store, now: Timestamp) -> Result<Response, IpcError> {
    let history = stored_history(store)?;
    Ok(Response::ConfigStatus(ConfigStatusView {
        last_saved_at: history.saved.last().map(|version| version.saved_at),
        pending: change_views(&waiting(&history, now)),
    }))
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
    let mut seen = HashSet::new();
    let valid: Vec<String> = rules
        .iter()
        .map(slug_of)
        .filter(|slug| seen.insert(slug.clone()))
        .collect();
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
    let history = stored_history(store)?;
    let day = match requested {
        Some(day) => day,
        None => {
            let today = logical_date(now, config_at(&history, now.timestamp()).day_starts_at);
            today.yesterday().unwrap_or(today)
        }
    };
    Ok(Response::Report(ReportView {
        logical_date: day,
        entries: store.events_for(day)?.iter().map(report_entry).collect(),
        pending: change_views(&waiting(&history, now.timestamp())),
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
        EventKind::ConfigSaved => match &detail["kind"] {
            Value::String(kind) => (format!("report-config-saved-{kind}"), &[]),
            _ => ("report-config-saved".to_owned(), &["now", "waiting"]),
        },
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

fn change_views(changes: &[TimedChange]) -> Vec<ChangeView> {
    shown(changes)
        .into_iter()
        .map(|change| ChangeView {
            message: message(change.id, &change.args),
            effective_at: change.effective_at,
        })
        .collect()
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
