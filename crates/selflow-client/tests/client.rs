use std::sync::mpsc;
use std::thread;

use jiff::{SignedDuration, Timestamp};
use selflow_client::client::{Client, ClientError, LockFeed};
use selflow_platform::transport::listen;
use selflow_protocol::ipc::{
    LockView, Request, Response, message_reader, read_message, write_message,
};

#[test]
fn a_request_gets_its_answer() -> Result<(), ClientError> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    let (listening, ready) = mpsc::channel();
    let daemon = {
        let path = path.clone();
        thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let listener = listen(&path).unwrap();
                listening.send(()).unwrap();
                let mut stream = message_reader(listener.accept().await.unwrap());
                let request: Option<Request> = read_message(&mut stream).await.unwrap();
                write_message(stream.get_mut(), &Response::LockEnded)
                    .await
                    .unwrap();
                request
            })
        })
    };
    ready.recv().unwrap();
    let response = Client::connect(&path)?.request(Request::Status)?;
    assert_eq!(response, Response::LockEnded);
    assert_eq!(daemon.join().unwrap(), Some(Request::Status));
    Ok(())
}

#[test]
fn connecting_to_a_missing_socket_is_unreachable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    assert!(matches!(
        Client::connect(&path),
        Err(ClientError::Unreachable)
    ));
}

#[tokio::test]
async fn a_lock_feed_reads_what_the_daemon_sends() -> Result<(), ClientError> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    let listener = listen(&path).unwrap();
    let daemon = tokio::spawn(async move {
        let mut stream = message_reader(listener.accept().await.unwrap());
        let request: Option<Request> = read_message(&mut stream).await.unwrap();
        write_message(stream.get_mut(), &Response::Lock(view()))
            .await
            .unwrap();
        write_message(stream.get_mut(), &Response::LockEnded)
            .await
            .unwrap();
        request
    });
    let mut feed = LockFeed::open(&path).await?;
    assert_eq!(feed.next().await?, Some(Response::Lock(view())));
    assert_eq!(feed.next().await?, Some(Response::LockEnded));
    assert_eq!(feed.next().await?, None);
    assert_eq!(daemon.await.unwrap(), Some(Request::SubscribeLock));
    Ok(())
}

fn view() -> LockView {
    let start = Timestamp::UNIX_EPOCH;
    LockView {
        label: "bedtime".into(),
        ends_at: start + SignedDuration::from_hours(8),
        extension_available_at: start + SignedDuration::from_mins(2),
        extensions_left: 1,
        extension_minutes: 20,
        reason_required: true,
        phrase: None,
        shutdown_at: None,
        language: "en".into(),
    }
}
