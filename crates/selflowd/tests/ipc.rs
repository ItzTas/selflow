mod support;

use std::sync::{Arc, Mutex};

use jiff::civil::date;
use jiff::{SignedDuration, Timestamp};
use selflow_core::config::{Action, parse};
use selflow_core::plan::{RuleWindow, windows_for};
use selflow_core::timing::Seed;
use selflow_i18n::catalog::default_config;
use selflow_i18n::language::Language;
use selflow_platform::transport::Stream;
use selflow_protocol::ipc::{
    ChallengeView, ChangeView, ExtensionSource, MessageView, RefusalView, ReportEntry, Request,
    Response,
};
use selflowd::enforcer::PowerMode;
use selflowd::ipc::handle;
use selflowd::store::{Event, EventKind, Store};
use serde_json::{Value, json};
use support::{
    Call, LIGHTS_OUT, REMINDER, TestClock, VALID, at, brt, engine_on, launch, m1_store, no_jitter,
    sample_view, save_at, save_text, setup, shared_over, start, start_engine_with, start_over,
    start_with, start_with_config, subscribe,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::watch;

fn with_weekend() -> String {
    format!(
        "{}{}",
        no_jitter(),
        r#"
[[rule_set]]
name = "weekend"
days = ["fri", "sat"]

[rule_set.challenges]
wait = "5m"
reason = true
extensions = 2
extension_length = "10m"

[[rule_set.rule]]
id = "late"
label = "Late night"
start = "01:00"
end = "wake"
lock = true
"#
    )
}

fn test_lock(rule: Option<&str>, minutes: i64) -> Request {
    Request::TestLock {
        rule: rule.map(str::to_owned),
        duration: SignedDuration::from_mins(minutes),
    }
}

fn error(id: &str, args: &[(&str, &str)]) -> Response {
    Response::Error(MessageView {
        id: id.to_owned(),
        args: args
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
    })
}

fn detail(event: &Event) -> Value {
    serde_json::from_str(event.detail.as_deref().unwrap()).unwrap()
}

fn timeline(changes: &[ChangeView]) -> Vec<(&str, Timestamp)> {
    changes
        .iter()
        .map(|change| (change.message.id.as_str(), change.effective_at))
        .collect()
}

#[tokio::test]
async fn an_invalid_file_is_rejected_with_positions_and_not_saved() {
    let (client, shared) = start(at(18, 10, 0)).await;
    let text = no_jitter().replace(r#"wait = "2m""#, r#"wait = "soon""#);
    let Response::Invalid { errors } = client.request(Request::SaveConfig { text }).await else {
        panic!("expected Invalid")
    };
    assert_eq!(
        (errors[0].message.id.as_str(), errors[0].line),
        ("config-bad-duration", 15)
    );
    assert_eq!(shared.store.lock().unwrap().latest_text().unwrap(), None);
}

#[tokio::test]
async fn a_save_applies_its_tightening_now_and_dates_its_loosening() {
    let (client, shared, _clock) = start_with(VALID, PowerMode::Real, None, at(18, 12, 0)).await;
    let text = VALID
        .replace(r#"bedtime = "23:00""#, r#"bedtime = "22:00""#)
        .replace("extensions = 1", "extensions = 3");
    let Response::Saved { saved_at, changes } = client.request(Request::SaveConfig { text }).await
    else {
        panic!("expected Saved")
    };
    assert_eq!(saved_at, at(18, 12, 0).timestamp());
    assert_eq!(
        timeline(&changes),
        [
            ("change-rule-starts-earlier", at(18, 12, 0).timestamp()),
            ("change-more-extensions", at(19, 12, 0).timestamp()),
        ]
    );
    let events = shared
        .store
        .lock()
        .unwrap()
        .events_for(date(2026, 11, 18))
        .unwrap();
    let saved: Vec<Value> = events
        .iter()
        .filter(|event| event.kind == EventKind::ConfigSaved)
        .map(detail)
        .collect();
    assert_eq!(saved, vec![json!({ "now": 5, "waiting": 5 })]);
}

#[tokio::test]
async fn the_config_status_lists_each_waiting_change() {
    let (client, _shared, clock) = start_with(VALID, PowerMode::Real, None, at(18, 12, 0)).await;
    let more = VALID.replace("extensions = 1", "extensions = 3");
    let shorter_wait = more.replace(r#"wait = "2m""#, r#"wait = "1m""#);
    client.request(Request::SaveConfig { text: more }).await;
    clock.set(at(18, 14, 0));
    client
        .request(Request::SaveConfig { text: shorter_wait })
        .await;
    let Response::ConfigStatus(status) = client.request(Request::ConfigStatus).await else {
        panic!("expected ConfigStatus")
    };
    assert_eq!(status.last_saved_at, Some(at(18, 14, 0).timestamp()));
    assert_eq!(
        timeline(&status.pending),
        [
            ("change-more-extensions", at(19, 12, 0).timestamp()),
            ("change-wait-shorter", at(19, 14, 0).timestamp()),
        ]
    );
    let Response::Report(report) = client
        .request(Request::Report {
            logical_date: Some(date(2026, 11, 18)),
        })
        .await
    else {
        panic!("expected Report")
    };
    assert_eq!(report.pending, status.pending);
    assert_eq!(
        report.entries[0].message,
        MessageView {
            id: "report-config-saved".to_owned(),
            args: vec![
                ("now".to_owned(), "0".to_owned()),
                ("waiting".to_owned(), "5".to_owned()),
            ],
        }
    );
}

#[tokio::test]
async fn the_first_save_takes_effect_at_once_even_when_it_loosens() {
    let (client, _shared) = start(at(18, 10, 0)).await;
    let text = no_jitter().replace(r#"loosening_delay = "24h""#, r#"loosening_delay = "12h""#);
    let Response::Saved { saved_at, changes } = client.request(Request::SaveConfig { text }).await
    else {
        panic!("expected Saved")
    };
    assert!(
        changes
            .iter()
            .any(|change| change.message.id == "change-delay-shorter")
    );
    assert!(changes.iter().all(|change| change.effective_at == saved_at));
}

#[tokio::test]
async fn with_nothing_saved_the_status_uses_the_environment_language() {
    let (client, _shared, _clock) = start_over(
        Store::open_in_memory().unwrap(),
        Language::PtBr,
        at(18, 10, 0),
    )
    .await;
    let Response::Config { text } = client.request(Request::GetConfig).await else {
        panic!("expected Config")
    };
    let Response::Status(status) = client.request(Request::Status).await else {
        panic!("expected Status")
    };
    assert_eq!(status.language, "pt-BR");
    assert_eq!(parse(&text).unwrap().language, "pt-BR");
}

#[tokio::test]
async fn the_status_names_the_next_lock_and_skips_rules_without_one() {
    let text = format!(
        "{}\n[[rule_set.rule]]\nid = \"stretch\"\nstart = \"22:00\"\n",
        no_jitter()
    );
    let (client, _shared, _clock) =
        start_with(&text, PowerMode::LogOnly, None, at(19, 21, 0)).await;
    let Response::Status(status) = client.request(Request::Status).await else {
        panic!("expected Status")
    };
    assert_eq!(status.next_lock_at, Some(at(19, 23, 0).timestamp()));
}

#[tokio::test]
async fn after_an_upgrade_the_first_edit_shows_the_old_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/selflow.db");
    m1_store(&path);
    let (client, _shared, _clock) =
        start_over(Store::open(&path).unwrap(), Language::En, at(19, 12, 0)).await;
    let Response::Config { text } = client.request(Request::GetConfig).await else {
        panic!("expected Config")
    };
    assert!(parse(&text).is_ok());
    assert!(text.starts_with(default_config(Language::En)));
    assert!(text.contains("\n# [[rule_set.lock]]\n"));
    let Response::Report(report) = client
        .request(Request::Report {
            logical_date: Some(date(2026, 11, 18)),
        })
        .await
    else {
        panic!("expected Report")
    };
    let ids: Vec<&str> = report
        .entries
        .iter()
        .map(|entry| entry.message.id.as_str())
        .collect();
    assert_eq!(ids, ["report-config-saved-tighten"]);
}

#[tokio::test]
async fn grants_one_extension_after_the_wait() {
    let (client, _shared, clock) = start_with_config(at(19, 23, 3)).await;
    let ask = || Request::RequestExtension {
        reason: "talking to my sister".into(),
        phrase: String::new(),
        source: ExtensionSource::Screen,
    };
    assert_eq!(
        client.request(ask()).await,
        Response::ExtensionGranted {
            until: at(19, 23, 23).timestamp()
        }
    );
    clock.set(at(19, 23, 30));
    assert_eq!(
        client.request(ask()).await,
        Response::ExtensionRefused(RefusalView::NoBudget)
    );
}

#[tokio::test]
async fn an_extension_from_a_terminal_is_marked_in_the_report() {
    let (client, _shared, _clock) = start_with_config(at(19, 23, 3)).await;
    let ask = Request::RequestExtension {
        reason: "talking to my sister".into(),
        phrase: String::new(),
        source: ExtensionSource::Terminal,
    };
    assert_eq!(
        client.request(ask).await,
        Response::ExtensionGranted {
            until: at(19, 23, 23).timestamp()
        }
    );
    let Response::Report(report) = client
        .request(Request::Report {
            logical_date: Some(date(2026, 11, 19)),
        })
        .await
    else {
        panic!("expected Report")
    };
    assert_eq!(
        report.entries,
        vec![ReportEntry {
            at: at(19, 23, 3).timestamp(),
            message: MessageView {
                id: "report-extension-terminal".to_owned(),
                args: vec![
                    ("label".to_owned(), "bedtime".to_owned()),
                    ("minutes".to_owned(), "20".to_owned()),
                    ("reason".to_owned(), "talking to my sister".to_owned()),
                ],
            },
        }]
    );
}

#[tokio::test]
async fn releasing_is_refused_during_a_lock() {
    let (client, _shared, _clock) = start_with(
        &no_jitter(),
        PowerMode::Real,
        Some(sample_view()),
        at(19, 23, 0),
    )
    .await;
    assert_eq!(
        client.request(Request::ReleaseScreen).await,
        error("ipc-release-locked", &[])
    );
    assert_eq!(client.releases(), 0);
}

#[tokio::test]
async fn releasing_with_nothing_locked_starts_one_client() {
    let (client, _shared) = start(at(19, 12, 0)).await;
    assert_eq!(
        client.request(Request::ReleaseScreen).await,
        Response::Released
    );
    assert_eq!(client.releases(), 1);
}

#[tokio::test]
async fn a_lock_subscriber_gets_the_view_then_the_end() {
    let (mut subscriber, views) = subscribe().await;
    assert_eq!(subscriber.next().await, Some(Response::LockEnded));
    views.send(Some(sample_view())).unwrap();
    assert_eq!(subscriber.next().await, Some(Response::Lock(sample_view())));
    views.send(None).unwrap();
    assert_eq!(subscriber.next().await, Some(Response::LockEnded));
}

async fn raw_connection() -> (tempfile::TempDir, Stream) {
    let (_, views) = watch::channel(None);
    let (dir, stream, _shared, _screen) = launch(
        Store::open_in_memory().unwrap(),
        Arc::new(TestClock::new(at(19, 23, 0))),
        views,
        PowerMode::Real,
        Language::En,
    )
    .await;
    (dir, stream)
}

async fn is_closed_within(stream: &mut Stream, secs: u64) -> bool {
    let mut buffer = [0; 8];
    let read = tokio::time::timeout(
        std::time::Duration::from_secs(secs),
        stream.read(&mut buffer),
    )
    .await;
    matches!(read, Ok(Ok(0)))
}

#[tokio::test]
async fn a_client_on_another_version_is_answered_then_closed() {
    let (_dir, mut stream) = raw_connection().await;
    stream.write_all(b"SLFW\x02\0\0\0").await.unwrap();
    let mut answer = [0; 8];
    stream.read_exact(&mut answer).await.unwrap();
    assert_eq!(&answer, b"SLFW\x01\0\0\0");
    assert!(is_closed_within(&mut stream, 1).await);
}

#[tokio::test]
async fn a_silent_connection_is_closed_after_the_handshake_timeout() {
    let (_dir, mut stream) = raw_connection().await;
    assert!(is_closed_within(&mut stream, 3).await);
}

#[tokio::test]
async fn a_partial_hello_is_closed_after_the_handshake_timeout() {
    let (_dir, mut stream) = raw_connection().await;
    stream.write_all(b"SLF").await.unwrap();
    assert!(is_closed_within(&mut stream, 3).await);
}

#[tokio::test]
async fn a_subscriber_stays_open_past_the_handshake_timeout() {
    let (mut subscriber, views) = subscribe().await;
    assert_eq!(subscriber.next().await, Some(Response::LockEnded));
    tokio::time::sleep(std::time::Duration::from_millis(2_500)).await;
    views.send(Some(sample_view())).unwrap();
    assert_eq!(subscriber.next().await, Some(Response::Lock(sample_view())));
}

#[tokio::test]
async fn a_test_lock_is_refused_with_real_power() {
    let (client, _shared, _clock) =
        start_with(&no_jitter(), PowerMode::Real, None, at(19, 20, 0)).await;
    assert_eq!(
        client.request(test_lock(None, 5)).await,
        error("ipc-test-lock-real-power", &[])
    );
}

#[tokio::test]
async fn a_test_lock_lasts_from_one_minute_to_two_hours() {
    let (client, _shared, clock) =
        start_with(&no_jitter(), PowerMode::LogOnly, None, at(19, 20, 0)).await;
    let too_short = Request::TestLock {
        rule: None,
        duration: SignedDuration::from_secs(59),
    };
    for request in [too_short, test_lock(None, 121)] {
        assert_eq!(
            client.request(request).await,
            error("ipc-test-lock-duration", &[("min", "1"), ("max", "120")])
        );
    }
    assert_eq!(
        client.request(test_lock(None, 1)).await,
        Response::TestLockStarted {
            label: "bedtime".to_owned(),
            ends_at: at(19, 20, 1).timestamp(),
        }
    );
    clock.set(at(19, 20, 1));
    assert_eq!(
        client.request(test_lock(None, 120)).await,
        Response::TestLockStarted {
            label: "bedtime".to_owned(),
            ends_at: at(19, 22, 1).timestamp(),
        }
    );
}

#[tokio::test]
async fn a_second_test_lock_waits_for_the_open_one_to_end() {
    let (client, _shared, clock) =
        start_with(&no_jitter(), PowerMode::LogOnly, None, at(19, 20, 0)).await;
    assert!(matches!(
        client.request(test_lock(None, 10)).await,
        Response::TestLockStarted { .. }
    ));
    for minute in [0, 9] {
        clock.set(at(19, 20, minute));
        assert_eq!(
            client.request(test_lock(None, 5)).await,
            error("ipc-test-lock-busy", &[])
        );
    }
    clock.set(at(19, 20, 10));
    assert_eq!(
        client.request(test_lock(None, 5)).await,
        Response::TestLockStarted {
            label: "bedtime".to_owned(),
            ends_at: at(19, 20, 15).timestamp(),
        }
    );
}

#[tokio::test]
async fn a_test_lock_waits_for_the_lock_on_screen_to_end() {
    let (client, _shared, _clock) = start_with(
        &no_jitter(),
        PowerMode::LogOnly,
        Some(sample_view()),
        at(19, 20, 0),
    )
    .await;
    assert_eq!(
        client.request(test_lock(None, 5)).await,
        error("ipc-test-lock-busy", &[])
    );
}

#[tokio::test]
async fn a_test_lock_needs_a_rule_for_today() {
    let (client, _shared, _clock) =
        start_with(&no_jitter(), PowerMode::LogOnly, None, at(20, 20, 0)).await;
    assert_eq!(
        client.request(test_lock(None, 5)).await,
        error("ipc-test-lock-no-rule", &[])
    );
}

#[tokio::test]
async fn an_unknown_rule_is_refused_with_the_valid_ones() {
    let (client, _shared, _clock) =
        start_with(&with_weekend(), PowerMode::LogOnly, None, at(19, 20, 0)).await;
    assert_eq!(
        client.request(test_lock(Some("weeknight/nap"), 5)).await,
        error(
            "ipc-test-lock-unknown-rule",
            &[
                ("rule", "weeknight/nap"),
                ("rules", "weeknight/bedtime, weekend/late")
            ]
        )
    );
}

#[tokio::test]
async fn a_test_lock_starts_now_with_todays_first_rule() {
    let (client, shared, _clock) =
        start_with(&no_jitter(), PowerMode::LogOnly, None, at(19, 20, 0)).await;
    assert_eq!(
        client.request(test_lock(None, 10)).await,
        Response::TestLockStarted {
            label: "bedtime".to_owned(),
            ends_at: at(19, 20, 10).timestamp(),
        }
    );
    let config = parse(&no_jitter()).unwrap();
    let rule = windows_for(&config, Seed(0), date(2026, 11, 19), &brt()).remove(0);
    assert_eq!(
        rule.action,
        Action::Lock {
            shutdown_after: Some(SignedDuration::from_mins(30))
        }
    );
    let started = shared
        .store
        .lock()
        .unwrap()
        .night_log(&[date(2026, 11, 19)])
        .unwrap()
        .started;
    assert_eq!(
        started,
        vec![RuleWindow {
            rule_id: format!("bedtime#test-{}", at(19, 20, 0).timestamp().as_second()),
            starts_at: at(19, 20, 0),
            ends_at: at(19, 20, 10),
            action: Action::Lock {
                shutdown_after: None
            },
            test: true,
            ..rule
        }]
    );
}

#[tokio::test]
async fn a_test_lock_can_use_a_rule_from_another_day() {
    let (client, shared, _clock) =
        start_with(&with_weekend(), PowerMode::LogOnly, None, at(19, 20, 0)).await;
    assert_eq!(
        client.request(test_lock(Some("weekend/late"), 5)).await,
        Response::TestLockStarted {
            label: "Late night".to_owned(),
            ends_at: at(19, 20, 5).timestamp(),
        }
    );
    let started = shared
        .store
        .lock()
        .unwrap()
        .night_log(&[date(2026, 11, 19)])
        .unwrap()
        .started;
    assert_eq!(
        started
            .iter()
            .map(|window| (window.set.as_str(), window.rule_id.as_str(), window.test))
            .collect::<Vec<_>>(),
        vec![(
            "weekend",
            format!("late#test-{}", at(19, 20, 0).timestamp().as_second()).as_str(),
            true
        )]
    );
    assert_eq!(started[0].challenges.extensions, 2);
}

#[tokio::test]
async fn an_extension_in_a_test_lock_is_recorded_as_a_test() {
    let (client, shared, clock) =
        start_with(&no_jitter(), PowerMode::LogOnly, None, at(19, 20, 0)).await;
    client.request(test_lock(None, 30)).await;
    clock.set(at(19, 20, 2));
    assert_eq!(
        client
            .request(Request::RequestExtension {
                reason: "checking the lock screen".into(),
                phrase: String::new(),
                source: ExtensionSource::Screen,
            })
            .await,
        Response::ExtensionGranted {
            until: at(19, 20, 22).timestamp()
        }
    );
    let events = shared
        .store
        .lock()
        .unwrap()
        .events_for(date(2026, 11, 19))
        .unwrap();
    let granted: Vec<Value> = events
        .iter()
        .filter(|event| event.kind == EventKind::ExtensionGranted)
        .map(detail)
        .collect();
    assert_eq!(
        granted,
        vec![json!({
            "label": "bedtime",
            "minutes": 20,
            "reason": "checking the lock screen",
            "test": true
        })]
    );
}

#[tokio::test]
async fn the_report_tells_test_locks_apart() {
    let (client, shared, _clock) = start_with_config(at(20, 12, 0)).await;
    let label = json!({ "label": "bedtime", "test": true }).to_string();
    let extension = json!({
        "label": "bedtime",
        "minutes": 20,
        "reason": "checking the lock screen",
        "test": true
    })
    .to_string();
    let from_terminal = json!({
        "label": "bedtime",
        "minutes": 20,
        "reason": "checking the terminal",
        "source": "terminal",
        "test": true
    })
    .to_string();
    let events = [
        (EventKind::LockStarted, 0, &label),
        (EventKind::ExtensionGranted, 2, &extension),
        (EventKind::ExtensionGranted, 3, &from_terminal),
        (EventKind::LockEnded, 4, &label),
    ];
    for (kind, minute, detail) in events {
        shared
            .store
            .lock()
            .unwrap()
            .record_event(&Event {
                at: at(19, 20, minute).timestamp(),
                logical_date: date(2026, 11, 19),
                kind,
                key: format!("weeknight#test-1/bedtime/{minute}"),
                detail: Some(detail.clone()),
            })
            .unwrap();
    }
    let Response::Report(report) = client
        .request(Request::Report {
            logical_date: Some(date(2026, 11, 19)),
        })
        .await
    else {
        panic!("expected Report")
    };
    let entry = |minute, id: &str, args: &[(&str, &str)]| ReportEntry {
        at: at(19, 20, minute).timestamp(),
        message: MessageView {
            id: id.to_owned(),
            args: args
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        },
    };
    assert_eq!(
        report.entries,
        vec![
            entry(0, "report-lock-started-test", &[("label", "bedtime")]),
            entry(
                2,
                "report-extension-test",
                &[
                    ("label", "bedtime"),
                    ("minutes", "20"),
                    ("reason", "checking the lock screen")
                ]
            ),
            entry(
                3,
                "report-extension-terminal-test",
                &[
                    ("label", "bedtime"),
                    ("minutes", "20"),
                    ("reason", "checking the terminal")
                ]
            ),
            entry(4, "report-lock-ended-test", &[("label", "bedtime")]),
        ]
    );
}

fn start_exception(rule: &str) -> Request {
    Request::StartException {
        rule: rule.to_owned(),
    }
}

fn confirm_exception(rule: &str, reason: &str) -> Request {
    Request::ConfirmException {
        rule: rule.to_owned(),
        reason: reason.to_owned(),
        phrase: String::new(),
    }
}

#[tokio::test]
async fn an_exception_is_refused_while_its_lock_is_on_screen() {
    let (client, shared, _clock) = start_with(
        &no_jitter(),
        PowerMode::Real,
        Some(sample_view()),
        at(19, 23, 3),
    )
    .await;
    shared
        .store
        .lock()
        .unwrap()
        .start_exception("bedtime", date(2026, 11, 19), at(19, 22, 50).timestamp())
        .unwrap();
    let locked = error("ipc-exception-locked", &[("label", "bedtime")]);
    assert_eq!(client.request(start_exception("bedtime")).await, locked);
    assert_eq!(
        client
            .request(confirm_exception("bedtime", "on call tonight"))
            .await,
        locked
    );
    let log = shared
        .store
        .lock()
        .unwrap()
        .night_log(&[date(2026, 11, 19)])
        .unwrap();
    assert_eq!(log.exceptions, vec![]);
}

#[tokio::test]
async fn an_exception_is_refused_during_its_own_extension() {
    let text = no_jitter().replace("extensions = 1", "extensions = 2");
    let (client, shared, clock) = start_with(&text, PowerMode::Real, None, at(19, 23, 3)).await;
    client
        .request(Request::RequestExtension {
            reason: "talking to my sister".into(),
            phrase: String::new(),
            source: ExtensionSource::Terminal,
        })
        .await;
    shared
        .store
        .lock()
        .unwrap()
        .start_exception("bedtime", date(2026, 11, 19), at(19, 23, 4).timestamp())
        .unwrap();
    clock.set(at(19, 23, 10));
    let locked = error("ipc-exception-locked", &[("label", "bedtime")]);
    assert_eq!(client.request(start_exception("bedtime")).await, locked);
    assert_eq!(
        client
            .request(confirm_exception("bedtime", "on call tonight"))
            .await,
        locked
    );
    let log = shared
        .store
        .lock()
        .unwrap()
        .night_log(&[date(2026, 11, 19)])
        .unwrap();
    assert_eq!(log.exceptions, vec![]);
}

#[test]
fn an_exception_takes_the_stricter_challenges_of_its_started_window() {
    let strict = no_jitter().replace(
        r#"wait = "2m""#,
        "wait = \"5m\"\nphrase = \"Rest is enough\"",
    );
    let (mut engine, _calls, clock, store) = start_engine_with(&strict, at(19, 22, 45));
    save_at(&store.lock().unwrap(), &no_jitter(), at(18, 22, 50));
    engine.tick().unwrap();
    clock.set(at(19, 22, 51));
    let shared = shared_over(store, clock.clone(), PowerMode::LogOnly);
    assert_eq!(
        handle(start_exception("bedtime"), &shared),
        Response::ExceptionChallenge(ChallengeView {
            label: "bedtime".to_owned(),
            available_at: at(19, 22, 56).timestamp(),
            reason_required: true,
            phrase: Some("Rest is enough".to_owned()),
            extensions_left: 1,
        })
    );
    clock.set(at(19, 22, 56));
    assert_eq!(
        handle(confirm_exception("bedtime", "on call tonight"), &shared),
        Response::ExceptionRefused(RefusalView::PhraseMismatch)
    );
}

#[tokio::test]
async fn an_exception_needs_an_extension_left_for_its_rule() {
    let text = format!(
        "{}\n[[rule_set.rule]]\nid = \"nap\"\nstart = \"14:00\"\nend = \"14:30\"\nlock = true\n",
        no_jitter()
    );
    let (client, _shared, clock) = start_with(&text, PowerMode::Real, None, at(19, 14, 3)).await;
    client
        .request(Request::RequestExtension {
            reason: "talking to my sister".into(),
            phrase: String::new(),
            source: ExtensionSource::Terminal,
        })
        .await;
    clock.set(at(19, 14, 40));
    let Response::ExceptionChallenge(challenge) = client.request(start_exception("nap")).await
    else {
        panic!("expected ExceptionChallenge")
    };
    assert_eq!(challenge.extensions_left, 0);
    clock.set(at(19, 14, 42));
    assert_eq!(
        client
            .request(confirm_exception("nap", "on call tonight"))
            .await,
        Response::ExceptionRefused(RefusalView::NoBudget)
    );
}

#[test]
fn an_exception_is_granted_after_the_wait_and_shows_in_the_report() {
    let (mut engine, calls, clock, store) = setup(at(19, 20, 0));
    let shared = shared_over(store, clock.clone(), PowerMode::LogOnly);
    assert_eq!(
        handle(start_exception("bedtime"), &shared),
        Response::ExceptionChallenge(ChallengeView {
            label: "bedtime".to_owned(),
            available_at: at(19, 20, 2).timestamp(),
            reason_required: true,
            phrase: None,
            extensions_left: 1,
        })
    );
    clock.set(at(19, 20, 1));
    assert_eq!(
        handle(confirm_exception("bedtime", "on call tonight"), &shared),
        Response::ExceptionRefused(RefusalView::TooEarly {
            available_at: at(19, 20, 2).timestamp()
        })
    );
    clock.set(at(19, 20, 2));
    assert_eq!(
        handle(confirm_exception("bedtime", "on call tonight"), &shared),
        Response::ExceptionGranted {
            label: "bedtime".to_owned()
        }
    );
    assert_eq!(
        handle(start_exception("bedtime"), &shared),
        error("ipc-exception-done", &[("label", "bedtime")])
    );
    let request = Request::Report {
        logical_date: Some(date(2026, 11, 19)),
    };
    let Response::Report(report) = handle(request, &shared) else {
        panic!("expected Report")
    };
    assert_eq!(
        report.entries,
        vec![ReportEntry {
            at: at(19, 20, 2).timestamp(),
            message: MessageView {
                id: "report-exception".to_owned(),
                args: vec![
                    ("label".to_owned(), "bedtime".to_owned()),
                    ("reason".to_owned(), "on call tonight".to_owned()),
                ],
            },
        }]
    );
    clock.set(at(19, 23, 0));
    engine.tick().unwrap();
    assert_eq!(calls.take(), vec![Call::Hide]);
}

#[tokio::test]
async fn an_exception_start_expires_ten_minutes_after_its_wait() {
    let (client, _shared, clock) = start_with_config(at(19, 9, 0)).await;
    client.request(start_exception("bedtime")).await;
    clock.set(at(19, 22, 59));
    assert_eq!(
        client
            .request(confirm_exception("bedtime", "on call tonight"))
            .await,
        error("ipc-exception-not-started", &[])
    );
}

#[tokio::test]
async fn a_start_after_the_old_one_expired_waits_from_now() {
    let (client, _shared, clock) = start_with_config(at(19, 9, 0)).await;
    client.request(start_exception("bedtime")).await;
    clock.set(at(19, 22, 50));
    let Response::ExceptionChallenge(challenge) = client.request(start_exception("bedtime")).await
    else {
        panic!("expected ExceptionChallenge")
    };
    assert_eq!(challenge.available_at, at(19, 22, 52).timestamp());
    clock.set(at(19, 22, 52));
    assert_eq!(
        client
            .request(confirm_exception("bedtime", "on call tonight"))
            .await,
        Response::ExceptionGranted {
            label: "bedtime".to_owned()
        }
    );
}

#[tokio::test]
async fn an_exception_confirmed_without_a_start_is_refused() {
    let (client, _shared, _clock) = start_with_config(at(19, 20, 0)).await;
    assert_eq!(
        client
            .request(confirm_exception("bedtime", "on call tonight"))
            .await,
        error("ipc-exception-not-started", &[])
    );
}

#[test]
fn a_notice_only_rule_is_skipped_at_once_and_reported_plainly() {
    let text = REMINDER.replace(
        "[[rule_set.rule]]",
        "[rule_set.challenges]\nextensions = 0\n\n[[rule_set.rule]]",
    );
    let (mut engine, calls, clock, store) = start_engine_with(&text, at(19, 14, 0));
    let shared = shared_over(store, clock.clone(), PowerMode::LogOnly);
    assert_eq!(
        handle(start_exception("stretch"), &shared),
        Response::ExceptionGranted {
            label: "stretch".to_owned()
        }
    );
    assert_eq!(
        handle(start_exception("stretch"), &shared),
        error("ipc-exception-done", &[("label", "stretch")])
    );
    clock.set(at(19, 15, 0));
    engine.tick().unwrap();
    assert_eq!(calls.take(), vec![Call::Hide]);
    let request = Request::Report {
        logical_date: Some(date(2026, 11, 19)),
    };
    let Response::Report(report) = handle(request, &shared) else {
        panic!("expected Report")
    };
    assert_eq!(
        report.entries,
        vec![ReportEntry {
            at: at(19, 14, 0).timestamp(),
            message: MessageView {
                id: "report-exception-plain".to_owned(),
                args: vec![("label".to_owned(), "stretch".to_owned())],
            },
        }]
    );
}

#[tokio::test]
async fn an_unknown_rule_is_refused_with_todays_rules() {
    let text = with_weekend().replace(
        "\n[[rule_set]]\nname = \"weekend\"",
        "\n[[rule_set.rule]]\nid = \"stretch\"\nstart = \"22:00\"\n\n[[rule_set]]\nname = \"weekend\"",
    );
    let (client, _shared, _clock) =
        start_with(&text, PowerMode::LogOnly, None, at(19, 20, 0)).await;
    let unknown = error(
        "ipc-exception-unknown-rule",
        &[("rule", "late"), ("rules", "bedtime, stretch")],
    );
    assert_eq!(client.request(start_exception("late")).await, unknown);
    assert_eq!(
        client
            .request(confirm_exception("late", "on call tonight"))
            .await,
        unknown
    );
}

#[test]
fn the_wait_of_an_exception_carries_across_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/selflow.db");
    let repeat = LIGHTS_OUT.replace(r#"shutdown = "once""#, r#"shutdown = "repeat""#);
    save_text(&Store::open(&path).unwrap(), &repeat);
    let requested_at = at(20, 2, 0)
        .checked_add(SignedDuration::from_secs(30))
        .unwrap();
    let clock = Arc::new(TestClock::new(requested_at.clone()));
    let first_run = shared_over(
        Arc::new(Mutex::new(Store::open(&path).unwrap())),
        clock.clone(),
        PowerMode::LogOnly,
    );
    let Response::ExceptionChallenge(challenge) = handle(start_exception("lights-out"), &first_run)
    else {
        panic!("expected ExceptionChallenge")
    };
    assert_eq!(
        challenge.available_at,
        requested_at
            .checked_add(SignedDuration::from_mins(2))
            .unwrap()
            .timestamp()
    );
    drop(first_run);
    clock.set(at(20, 2, 3));
    let store = Arc::new(Mutex::new(Store::open(&path).unwrap()));
    let second_run = shared_over(store.clone(), clock.clone(), PowerMode::LogOnly);
    assert_eq!(
        handle(
            confirm_exception("lights-out", "the update has to finish"),
            &second_run
        ),
        Response::ExceptionGranted {
            label: "lights-out".to_owned()
        }
    );
    let (mut engine, calls) = engine_on(store, clock);
    engine.tick().unwrap();
    assert_eq!(calls.take(), vec![Call::Hide]);
}
