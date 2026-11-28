use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard, PoisonError};

use jiff::{SignedDuration, Timestamp, ZonedDateTime, civil};
use selflow_core::challenge::{ExtensionAnswer, Refusal, request_exception, request_extension};
use selflow_core::config::{Action, Config, ConfigError, Rule, RuleSet, parse};
use selflow_core::enforce::{
    Enforcement, GrantedException, GrantedExtension, evaluate, started_rule,
};
use selflow_core::plan::{RuleWindow, windows_around};
use selflow_core::timing::logical_date;
use selflow_core::versions::{TimedChange, config_at, plan_save, shown, stored_changes, waiting};
use selflow_i18n::catalog::default_config;
use selflow_i18n::language::Language;
use selflow_i18n::localizer::Localizer;
use selflow_protocol::ipc::{
    ChallengeView, ChangeView, ConfigStatusView, ExtensionSource, LockView, MessageView,
    PositionedMessage, RefusalView, ReportEntry, ReportView, Request, Response, StatusView,
};
use serde_json::{Value, json};

use super::constants::{EXCEPTION_START_TTL, TEST_LOCK_MAX, TEST_LOCK_MIN};
use super::types::{ExceptionDay, IpcError, Shared};
use crate::enforcer::PowerMode;
use crate::engine::{ParsedVersions, config_in_force, event_detail, stored_history};
use crate::store::{Event, EventKind, Store, StoreError};

pub fn handle(request: Request, shared: &Shared) -> Response {
    let now = shared.clock.now();
    let store = locked(&shared.store);
    let answer = match request {
        Request::Status => status(&store, &now, shared),
        Request::SubscribeLock => Ok(lock_state(&shared.views.borrow())),
        Request::RequestExtension {
            reason,
            phrase,
            source,
        } => extend(
            &store,
            &shared.parsed,
            &now,
            &ExtensionAnswer { reason, phrase },
            source,
        ),
        Request::GetConfig => config_text(&store, shared.env_language),
        Request::SaveConfig { text } => save(&store, &shared.parsed, &now, text),
        Request::ConfigStatus => config_status(&store, &shared.parsed, now.timestamp()),
        Request::Report { logical_date } => report(&store, &shared.parsed, &now, logical_date),
        Request::TestLock { rule, duration } => {
            test_lock(&store, &now, shared, rule.as_deref(), duration)
        }
        Request::ReleaseScreen => Ok(release_screen(shared)),
        Request::StartException { rule } => start_exception(&store, &shared.parsed, &now, &rule),
        Request::ConfirmException {
            rule,
            reason,
            phrase,
        } => confirm_exception(
            &store,
            &shared.parsed,
            &now,
            &rule,
            &ExtensionAnswer { reason, phrase },
        ),
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
            | Response::ExceptionGranted { .. }
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
    let history = stored_history(store, &shared.parsed)?;
    let config = config_at(&history, instant);
    let next_lock_at = windows_around(&config, store.seed()?, now)
        .iter()
        .filter(|window| window.action.locks())
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

fn save(
    store: &Store,
    parsed: &ParsedVersions,
    now: &ZonedDateTime,
    text: String,
) -> Result<Response, IpcError> {
    let proposed = match parse(&text) {
        Ok(config) => config,
        Err(errors) => {
            return Ok(Response::Invalid {
                errors: errors.iter().map(positioned).collect(),
            });
        }
    };
    let saved_at = now.timestamp();
    let history = stored_history(store, parsed)?;
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

fn config_status(
    store: &Store,
    parsed: &ParsedVersions,
    now: Timestamp,
) -> Result<Response, IpcError> {
    let history = stored_history(store, parsed)?;
    Ok(Response::ConfigStatus(ConfigStatusView {
        last_saved_at: history.saved.last().map(|version| version.saved_at),
        pending: change_views(&waiting(&history, now)),
    }))
}

fn release_screen(shared: &Shared) -> Response {
    if shared.views.borrow().is_some() {
        return Response::Error(message("ipc-release-locked", &[]));
    }
    shared.screen.release();
    Response::Released
}

fn extend(
    store: &Store,
    parsed: &ParsedVersions,
    now: &ZonedDateTime,
    answer: &ExtensionAnswer,
    source: ExtensionSource,
) -> Result<Response, IpcError> {
    let config = config_in_force(store, parsed, now.timestamp())?;
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
        &granted_event(&extension, &lock.window.label, source, lock.window.test),
    );
    Ok(Response::ExtensionGranted {
        until: extension.until,
    })
}

fn start_exception(
    store: &Store,
    parsed: &ParsedVersions,
    now: &ZonedDateTime,
    rule_id: &str,
) -> Result<Response, IpcError> {
    let day = exception_day(store, parsed, now)?;
    let rule = match skippable(&day, now, rule_id) {
        Ok(rule) => rule,
        Err(refusal) => return Ok(Response::Error(refusal)),
    };
    let requested_at = open_start(store, &rule, day.today, now.timestamp())?;
    if !rule.enforces() {
        let unasked = ExtensionAnswer {
            reason: String::new(),
            phrase: String::new(),
        };
        return answer_exception(store, now, &day, &rule, requested_at, &unasked);
    }
    Ok(Response::ExceptionChallenge(challenge_view(
        &rule,
        requested_at,
        extensions_left(&rule, &day),
    )))
}

fn confirm_exception(
    store: &Store,
    parsed: &ParsedVersions,
    now: &ZonedDateTime,
    rule_id: &str,
    answer: &ExtensionAnswer,
) -> Result<Response, IpcError> {
    let day = exception_day(store, parsed, now)?;
    let rule = match skippable(&day, now, rule_id) {
        Ok(rule) => rule,
        Err(refusal) => return Ok(Response::Error(refusal)),
    };
    let instant = now.timestamp();
    let Some(requested_at) = store
        .exception_requested_at(&rule.id, day.today)?
        .filter(|requested_at| instant <= start_open_until(&rule, *requested_at))
    else {
        return Ok(Response::Error(message("ipc-exception-not-started", &[])));
    };
    answer_exception(store, now, &day, &rule, requested_at, answer)
}

fn open_start(
    store: &Store,
    rule: &Rule,
    today: civil::Date,
    now: Timestamp,
) -> Result<Timestamp, StoreError> {
    let requested_at = store.start_exception(&rule.id, today, now)?;
    if now <= start_open_until(rule, requested_at) {
        return Ok(requested_at);
    }
    store.restart_exception(&rule.id, today, now)?;
    Ok(now)
}

fn start_open_until(rule: &Rule, requested_at: Timestamp) -> Timestamp {
    requested_at
        .checked_add(rule.challenges.wait.saturating_add(EXCEPTION_START_TTL))
        .unwrap_or(Timestamp::MAX)
}

fn answer_exception(
    store: &Store,
    now: &ZonedDateTime,
    day: &ExceptionDay,
    rule: &Rule,
    requested_at: Timestamp,
    answer: &ExtensionAnswer,
) -> Result<Response, IpcError> {
    let left = extensions_left(rule, day);
    let exception =
        match request_exception(now.timestamp(), requested_at, rule, day.today, left, answer) {
            Ok(exception) => exception,
            Err(refusal) => return Ok(Response::ExceptionRefused(refusal_view(refusal))),
        };
    store.grant_exception(&exception)?;
    record(store, &exception_event(&exception, &rule.label));
    Ok(Response::ExceptionGranted {
        label: rule.label.clone(),
    })
}

fn exception_day(
    store: &Store,
    parsed: &ParsedVersions,
    now: &ZonedDateTime,
) -> Result<ExceptionDay, IpcError> {
    let config = config_in_force(store, parsed, now.timestamp())?;
    let today = logical_date(now, config.day_starts_at);
    let nights: Vec<civil::Date> = today.yesterday().into_iter().chain(Some(today)).collect();
    Ok(ExceptionDay {
        log: store.night_log(&nights)?,
        seed: store.seed()?,
        config,
        today,
    })
}

fn skippable(day: &ExceptionDay, now: &ZonedDateTime, rule_id: &str) -> Result<Rule, MessageView> {
    let rules = day.config.rules_on(day.today.weekday());
    let Some(rule) = rules.iter().find(|rule| rule.id == rule_id) else {
        let ids: Vec<&str> = rules.iter().map(|rule| rule.id.as_str()).collect();
        return Err(message(
            "ipc-exception-unknown-rule",
            &[("rule", rule_id.to_owned()), ("rules", ids.join(", "))],
        ));
    };
    let label = [("label", rule.label.clone())];
    if locked_by(day, now, rule_id) {
        return Err(message("ipc-exception-locked", &label));
    }
    if skipped(day, rule_id) {
        return Err(message("ipc-exception-done", &label));
    }
    Ok(with_started_window(rule, day))
}

fn with_started_window(rule: &Rule, day: &ExceptionDay) -> Rule {
    let started = day
        .log
        .started
        .iter()
        .find(|window| window.rule_id == rule.id && window.logical_date == day.today);
    started_rule(rule, started)
}

fn locked_by(day: &ExceptionDay, now: &ZonedDateTime, rule_id: &str) -> bool {
    let instant = now.timestamp();
    let extended = day.log.extensions.iter().any(|extension| {
        extension.rule_id == rule_id
            && extension.logical_date == day.today
            && extension.granted_at <= instant
            && instant < extension.until
    });
    extended
        || matches!(
            evaluate(now, &day.config, day.seed, &day.log).enforcement,
            Enforcement::Locked(lock) if lock.window.rule_id == rule_id
        )
}

fn skipped(day: &ExceptionDay, rule_id: &str) -> bool {
    let granted = day
        .log
        .exceptions
        .iter()
        .any(|exception| exception.rule_id == rule_id && exception.logical_date == day.today);
    granted
        || day
            .config
            .exception_on(day.today)
            .is_some_and(|exception| exception.covers(rule_id))
}

fn extensions_left(rule: &Rule, day: &ExceptionDay) -> u32 {
    let used = day
        .log
        .extensions
        .iter()
        .filter(|extension| extension.rule_id == rule.id && extension.logical_date == day.today)
        .count();
    rule.challenges
        .extensions
        .saturating_sub(u32::try_from(used).unwrap_or(u32::MAX))
}

fn challenge_view(rule: &Rule, requested_at: Timestamp, extensions_left: u32) -> ChallengeView {
    let challenges = &rule.challenges;
    ChallengeView {
        label: rule.label.clone(),
        available_at: requested_at
            .checked_add(challenges.wait)
            .unwrap_or(Timestamp::MAX),
        reason_required: challenges.reason,
        phrase: challenges.phrase.clone(),
        extensions_left,
    }
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
    let config = config_in_force(store, &shared.parsed, now.timestamp())?;
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
    let window = RuleWindow {
        set: set.name.clone(),
        rule_id: format!("{}#test-{}", lock.id, now.timestamp().as_second()),
        label: lock.label.clone(),
        logical_date: today,
        starts_at: now.clone(),
        ends_at: now.checked_add(duration)?,
        notices: lock.notice_offsets(),
        message: lock.message.clone(),
        action: Action::Lock {
            shutdown_after: None,
        },
        challenges: lock.challenges.clone(),
        test: true,
        agent: Vec::new(),
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
    set.rules.iter().filter(|rule| rule.locks())
}

fn report(
    store: &Store,
    parsed: &ParsedVersions,
    now: &ZonedDateTime,
    requested: Option<civil::Date>,
) -> Result<Response, IpcError> {
    let history = stored_history(store, parsed)?;
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
            extension_id(&detail).to_owned(),
            &["label", "minutes", "reason"],
        ),
        EventKind::Shutdown => ("report-shutdown".to_owned(), &["label"]),
        EventKind::RuleShutdown => ("report-rule-shutdown".to_owned(), &["label"]),
        EventKind::ExceptionGranted => exception_report(&detail),
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

fn exception_report(detail: &Value) -> (String, &'static [&'static str]) {
    if detail["reason"].as_str().is_none_or(str::is_empty) {
        return ("report-exception-plain".to_owned(), &["label"]);
    }
    ("report-exception".to_owned(), &["label", "reason"])
}

fn extension_id(detail: &Value) -> &'static str {
    if detail["source"] == "terminal" {
        return "report-extension-terminal";
    }
    "report-extension"
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

fn granted_event(
    extension: &GrantedExtension,
    label: &str,
    source: ExtensionSource,
    test: bool,
) -> Event {
    let minutes = extension
        .granted_at
        .duration_until(extension.until)
        .as_mins();
    let mut detail = json!({ "label": label, "minutes": minutes, "reason": extension.reason });
    if source == ExtensionSource::Terminal {
        detail["source"] = json!(source);
    }
    Event {
        at: extension.granted_at,
        logical_date: extension.logical_date,
        kind: EventKind::ExtensionGranted,
        key: format!(
            "{}/{}/{}/{}",
            extension.rule_set, extension.rule_id, extension.logical_date, extension.granted_at
        ),
        detail: Some(event_detail(detail, test)),
    }
}

fn exception_event(exception: &GrantedException, label: &str) -> Event {
    Event {
        at: exception.granted_at,
        logical_date: exception.logical_date,
        kind: EventKind::ExceptionGranted,
        key: format!("{}/{}", exception.rule_id, exception.logical_date),
        detail: Some(json!({ "label": label, "reason": exception.reason }).to_string()),
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
