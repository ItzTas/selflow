use std::sync::mpsc;
use std::thread;

use jiff::{SignedDuration, Timestamp};
use selflow_client::client::{Client, ClientError, LockFeed, exchange};
use selflow_platform::transport::listen;
use selflow_protocol::ipc::{
    HELLO_MAGIC, LockView, Request, Response, hello_as_daemon, message_reader, read_message,
    write_message,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
                let mut stream = listener.accept().await.unwrap();
                hello_as_daemon(&mut stream).await.unwrap();
                let mut stream = message_reader(stream);
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
        let mut stream = listener.accept().await.unwrap();
        hello_as_daemon(&mut stream).await.unwrap();
        let mut stream = message_reader(stream);
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

#[tokio::test]
async fn a_daemon_that_never_answers_the_hello_is_a_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    let listener = listen(&path).unwrap();
    tokio::spawn(async move {
        let mut stream = listener.accept().await.unwrap();
        let mut sink = [0; 64];
        while stream.read(&mut sink).await.is_ok_and(|read| read > 0) {}
    });
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        exchange(&path, &Request::Status),
    )
    .await
    .expect("the client gave up on its own");
    assert!(matches!(
        outcome,
        Err(ClientError::VersionMismatch {
            running: None,
            installed: None
        })
    ));
}

#[tokio::test]
async fn a_daemon_on_another_version_is_a_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    let listener = listen(&path).unwrap();
    tokio::spawn(async move {
        let mut stream = listener.accept().await.unwrap();
        let mut hello = [0; 8];
        stream.read_exact(&mut hello).await.unwrap();
        let mut answer = HELLO_MAGIC.to_vec();
        answer.extend_from_slice(&999u32.to_le_bytes());
        stream.write_all(&answer).await.unwrap();
    });
    let outcome = exchange(&path, &Request::Status).await;
    assert!(matches!(
        outcome,
        Err(ClientError::VersionMismatch {
            running: Some(999),
            installed: None
        })
    ));
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
