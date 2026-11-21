mod support;

use jiff::SignedDuration;
use jiff::civil::date;
use selflow_core::config::parse;
use selflow_core::plan::{LockWindow, windows_for};
use selflow_core::timing::Seed;
use selflow_protocol::ipc::{
    ChangeKindView, MessageView, RefusalView, ReportEntry, Request, Response,
};
use selflowd::enforcer::PowerMode;
use selflowd::store::{Event, EventKind};
use serde_json::{Value, json};
use support::{at, brt, no_jitter, sample_view, start, start_with, start_with_config, subscribe};

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
async fn a_loosening_save_says_when_it_takes_effect() {
    let (client, _shared) = start(at(18, 10, 0)).await;
    let first = client
        .request(Request::SaveConfig { text: no_jitter() })
        .await;
    assert!(matches!(
        first,
        Response::Saved {
            kind: ChangeKindView::Tighten,
            ..
        }
    ));
    let later = no_jitter().replace(r#"bedtime = "23:00""#, r#"bedtime = "01:00""#);
    let Response::Saved {
        kind,
        effective_at,
        changes,
    } = client.request(Request::SaveConfig { text: later }).await
    else {
        panic!("expected Saved")
    };
    assert_eq!(
        (kind, effective_at),
        (ChangeKindView::Loosen, at(19, 10, 0).timestamp())
    );
    assert_eq!(changes[0].id, "change-rule-starts-later");
    let Response::ConfigStatus(status) = client.request(Request::ConfigStatus).await else {
        panic!("expected ConfigStatus")
    };
    assert_eq!(
        status.pending.map(|p| p.effective_at),
        Some(at(19, 10, 0).timestamp())
    );
}

#[tokio::test]
async fn the_first_save_takes_effect_at_once_even_when_it_loosens() {
    let (client, _shared) = start(at(18, 10, 0)).await;
    let text = no_jitter().replace(r#"loosening_delay = "24h""#, r#"loosening_delay = "12h""#);
    let Response::Saved {
        kind, effective_at, ..
    } = client.request(Request::SaveConfig { text }).await
    else {
        panic!("expected Saved")
    };
    assert_eq!(
        (kind, effective_at),
        (ChangeKindView::Loosen, at(18, 10, 0).timestamp())
    );
}

#[tokio::test]
async fn with_nothing_saved_the_status_uses_the_default_files_language() {
    let (client, _shared) = start(at(18, 10, 0)).await;
    let Response::Config { text } = client.request(Request::GetConfig).await else {
        panic!("expected Config")
    };
    let Response::Status(status) = client.request(Request::Status).await else {
        panic!("expected Status")
    };
    assert_eq!(status.language, parse(&text).unwrap().language);
}

#[tokio::test]
async fn grants_one_extension_after_the_wait() {
    let (client, _shared, clock) = start_with_config(at(19, 23, 3)).await;
    let ask = || Request::RequestExtension {
        reason: "talking to my sister".into(),
        phrase: String::new(),
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
async fn a_lock_subscriber_gets_the_view_then_the_end() {
    let (mut subscriber, views) = subscribe().await;
    assert_eq!(subscriber.next().await, Some(Response::LockEnded));
    views.send(Some(sample_view())).unwrap();
    assert_eq!(subscriber.next().await, Some(Response::Lock(sample_view())));
    views.send(None).unwrap();
    assert_eq!(subscriber.next().await, Some(Response::LockEnded));
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
    assert_eq!(rule.shutdown_after, Some(SignedDuration::from_mins(30)));
    let started = shared
        .store
        .lock()
        .unwrap()
        .night_log(&[date(2026, 11, 19)])
        .unwrap()
        .started;
    assert_eq!(
        started,
        vec![LockWindow {
            rule_set: format!("weeknight#test-{}", at(19, 20, 0).timestamp().as_second()),
            starts_at: at(19, 20, 0),
            ends_at: at(19, 20, 10),
            shutdown_after: None,
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
            .map(|window| (
                window.rule_set.as_str(),
                window.rule_id.as_str(),
                window.test
            ))
            .collect::<Vec<_>>(),
        vec![(
            format!("weekend#test-{}", at(19, 20, 0).timestamp().as_second()).as_str(),
            "late",
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
    let events = [
        (EventKind::LockStarted, 0, &label),
        (EventKind::ExtensionGranted, 2, &extension),
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
            entry(4, "report-lock-ended-test", &[("label", "bedtime")]),
        ]
    );
}
