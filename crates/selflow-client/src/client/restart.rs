use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use selflow_platform::service::{daemon_binary, restart_daemon};
use selflow_platform::transport::Stream;
use selflow_protocol::ipc::{HANDSHAKE_TIMEOUT, MessageReader, PROTOCOL_VERSION};
use tokio::time::sleep;

use super::hello;
use super::types::{ClientError, DaemonService, Hello, RestartOnce, Systemd};

const PROTOCOL_VERSION_FLAG: &str = "--protocol-version";
const EXIT_POLL: Duration = Duration::from_millis(10);
const RECONNECT_POLL: Duration = Duration::from_millis(100);
const RECONNECT_WAIT: Duration = Duration::from_secs(10);

pub static SYSTEM_SERVICE: RestartOnce<Systemd> = RestartOnce::new(Systemd);

impl<S: DaemonService> RestartOnce<S> {
    pub const fn new(inner: S) -> Self {
        RestartOnce {
            inner,
            used: AtomicBool::new(false),
        }
    }
}

impl<S: DaemonService> DaemonService for RestartOnce<S> {
    fn installed_version(&self) -> Option<u32> {
        self.inner.installed_version()
    }

    fn restart(&self) -> bool {
        if self.used.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.inner.restart()
    }
}

impl DaemonService for Systemd {
    fn installed_version(&self) -> Option<u32> {
        let mut child = Command::new(daemon_binary().ok()?)
            .arg(PROTOCOL_VERSION_FLAG)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        if !exit_within(&mut child, HANDSHAKE_TIMEOUT)?.success() {
            return None;
        }
        let mut printed = String::new();
        child.stdout.take()?.read_to_string(&mut printed).ok()?;
        printed.trim().parse().ok()
    }

    fn restart(&self) -> bool {
        restart_daemon().is_ok()
    }
}

pub(super) async fn open(
    path: &Path,
    service: &dyn DaemonService,
) -> Result<MessageReader<Stream>, ClientError> {
    let running = match hello(path).await? {
        Hello::Ready(stream) => return Ok(stream),
        Hello::Mismatch(running) => running,
    };
    let installed = service.installed_version();
    let restarted = installed == Some(PROTOCOL_VERSION) && service.restart();
    if !restarted {
        return Err(ClientError::VersionMismatch { running, installed });
    }
    let deadline = Instant::now() + RECONNECT_WAIT;
    loop {
        sleep(RECONNECT_POLL).await;
        let outcome = ready(path, installed).await;
        if outcome.is_ok() || Instant::now() >= deadline {
            return outcome;
        }
    }
}

async fn ready(path: &Path, installed: Option<u32>) -> Result<MessageReader<Stream>, ClientError> {
    match hello(path).await? {
        Hello::Ready(stream) => Ok(stream),
        Hello::Mismatch(running) => Err(ClientError::VersionMismatch { running, installed }),
    }
}

fn exit_within(child: &mut Child, limit: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => thread::sleep(EXIT_POLL),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}
