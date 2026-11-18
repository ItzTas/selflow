mod support;

use selflow_core::config::parse;
use selflow_protocol::ipc::{ChangeKindView, RefusalView, Request, Response};
use selflowd::ipc::{IpcError, bind};
use support::{at, no_jitter, sample_view, start, start_with_config, subscribe};

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
    assert_eq!(changes[0].id, "change-lock-starts-later");
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
async fn a_second_daemon_cannot_take_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    let _first = bind(&path).unwrap();
    assert!(matches!(bind(&path), Err(IpcError::AlreadyRunning)));
}
