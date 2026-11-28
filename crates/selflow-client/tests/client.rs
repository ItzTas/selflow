use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use jiff::{SignedDuration, Timestamp};
use selflow_client::client::{
    Client, ClientError, DaemonService, LockFeed, RestartOnce, exchange_with, installed_version_of,
};
use selflow_platform::transport::{Listener, listen};
use selflow_protocol::ipc::{
    HELLO_MAGIC, LockView, PROTOCOL_VERSION, Request, Response, hello_as_daemon, message_reader,
    read_message, write_message,
};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const RECONNECT_WAIT: Duration = Duration::from_secs(1);

type Hellos = Arc<Mutex<Vec<u32>>>;

struct FakeService {
    installed: Option<u32>,
    restarts: Arc<AtomicU32>,
    hellos: Hellos,
    after_restart: Vec<u32>,
}

impl FakeService {
    fn new(installed: Option<u32>, hellos: Hellos, after_restart: &[u32]) -> FakeService {
        FakeService {
            installed,
            restarts: Arc::default(),
            hellos,
            after_restart: after_restart.to_vec(),
        }
    }

    fn restarts(&self) -> u32 {
        self.restarts.load(Ordering::SeqCst)
    }
}

impl DaemonService for FakeService {
    fn installed_version(&self) -> Option<u32> {
        self.installed
    }

    fn restart(&self) -> bool {
        self.restarts.fetch_add(1, Ordering::SeqCst);
        *self.hellos.lock().unwrap() = self.after_restart.clone();
        true
    }

    fn reconnect_wait(&self) -> Duration {
        RECONNECT_WAIT
    }
}

fn no_daemon_service() -> FakeService {
    FakeService::new(None, Arc::default(), &[])
}

struct VersionedDaemon {
    path: PathBuf,
    hellos: Hellos,
    _dir: TempDir,
}

impl VersionedDaemon {
    fn start(version: u32) -> VersionedDaemon {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("selflow/daemon.sock");
        let hellos = Arc::new(Mutex::new(vec![version]));
        tokio::spawn(answer_hellos(listen(&path).unwrap(), Arc::clone(&hellos)));
        VersionedDaemon {
            path,
            hellos,
            _dir: dir,
        }
    }

    fn service(&self, installed: Option<u32>, after_restart: &[u32]) -> FakeService {
        FakeService::new(installed, Arc::clone(&self.hellos), after_restart)
    }
}

fn next_hello(hellos: &Mutex<Vec<u32>>) -> u32 {
    let mut hellos = hellos.lock().unwrap();
    if hellos.len() > 1 {
        return hellos.remove(0);
    }
    hellos[0]
}

async fn answer_hellos(listener: Listener, hellos: Hellos) {
    while let Ok(mut stream) = listener.accept().await {
        let mut hello = [0; 8];
        if stream.read_exact(&mut hello).await.is_err() {
            continue;
        }
        let advertised = next_hello(&hellos);
        let mut answer = HELLO_MAGIC.to_vec();
        answer.extend_from_slice(&advertised.to_le_bytes());
        if stream.write_all(&answer).await.is_err() || advertised != PROTOCOL_VERSION {
            continue;
        }
        let mut stream = message_reader(stream);
        if let Ok(Some(_)) = read_message::<Request>(&mut stream).await {
            write_message(stream.get_mut(), &Response::LockEnded)
                .await
                .unwrap();
        }
    }
}

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
    let response = Client::connect_with(&path, &no_daemon_service())?.request(Request::Status)?;
    assert_eq!(response, Response::LockEnded);
    assert_eq!(daemon.join().unwrap(), Some(Request::Status));
    Ok(())
}

#[test]
fn connecting_to_a_missing_socket_is_unreachable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    assert!(matches!(
        Client::connect_with(&path, &no_daemon_service()),
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
    let mut feed = LockFeed::open_with(&path, &no_daemon_service()).await?;
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
    let service = no_daemon_service();
    let outcome = tokio::time::timeout(
        Duration::from_secs(3),
        exchange_with(&path, &Request::Status, &service),
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
async fn an_outdated_daemon_is_restarted_and_the_request_waits_for_the_new_one()
-> Result<(), ClientError> {
    let daemon = VersionedDaemon::start(999);
    let service = daemon.service(Some(PROTOCOL_VERSION), &[999, 999, 999, PROTOCOL_VERSION]);
    let response = exchange_with(&daemon.path, &Request::Status, &service).await?;
    assert_eq!(response, Response::LockEnded);
    assert_eq!(service.restarts(), 1);
    Ok(())
}

#[tokio::test]
async fn a_restarted_daemon_that_stays_outdated_is_a_mismatch_after_the_wait() {
    let daemon = VersionedDaemon::start(999);
    let service = daemon.service(Some(PROTOCOL_VERSION), &[999]);
    let started = tokio::time::Instant::now();
    let outcome = exchange_with(&daemon.path, &Request::Status, &service).await;
    assert!(started.elapsed() >= RECONNECT_WAIT);
    assert!(matches!(
        outcome,
        Err(ClientError::VersionMismatch {
            running: Some(999),
            installed: Some(PROTOCOL_VERSION)
        })
    ));
    assert_eq!(service.restarts(), 1);
}

#[tokio::test]
async fn no_restart_when_the_installed_daemon_does_not_match() {
    let daemon = VersionedDaemon::start(999);
    let service = daemon.service(Some(999), &[PROTOCOL_VERSION]);
    let outcome = exchange_with(&daemon.path, &Request::Status, &service).await;
    assert!(matches!(
        outcome,
        Err(ClientError::VersionMismatch {
            running: Some(999),
            installed: Some(999)
        })
    ));
    assert_eq!(service.restarts(), 0);
}

#[tokio::test]
async fn no_restart_when_the_installed_version_is_unknown() {
    let daemon = VersionedDaemon::start(999);
    let service = daemon.service(None, &[PROTOCOL_VERSION]);
    let outcome = exchange_with(&daemon.path, &Request::Status, &service).await;
    assert!(matches!(
        outcome,
        Err(ClientError::VersionMismatch {
            running: Some(999),
            installed: None
        })
    ));
    assert_eq!(service.restarts(), 0);
}

#[test]
fn a_process_restarts_the_daemon_at_most_once() {
    let inner = FakeService::new(Some(PROTOCOL_VERSION), Arc::default(), &[PROTOCOL_VERSION]);
    let restarts = Arc::clone(&inner.restarts);
    let service = RestartOnce::new(inner);
    assert!(service.restart());
    assert!(!service.restart());
    assert_eq!(restarts.load(Ordering::SeqCst), 1);
}

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn the_installed_version_is_what_the_binary_prints_before_the_handshake_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("pid");
    let prints = script(dir.path(), "prints", "echo 1");
    let fails = script(dir.path(), "fails", "echo 1\nexit 1");
    let hangs = script(
        dir.path(),
        "hangs",
        &format!("echo $$ > '{}'\nexec sleep 30", pid_file.display()),
    );
    assert_eq!(installed_version_of(&prints), Some(1));
    assert_eq!(installed_version_of(&fails), None);
    let started = Instant::now();
    assert_eq!(installed_version_of(&hangs), None);
    assert!(started.elapsed() < Duration::from_secs(3));
    let pid = fs::read_to_string(&pid_file).unwrap();
    assert!(!Path::new("/proc").join(pid.trim()).exists());
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
