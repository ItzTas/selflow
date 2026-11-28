use std::io;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use selflow_platform::transport::Stream;
use selflow_protocol::ipc::MessageReader;
use thiserror::Error;
use tokio::runtime::Runtime;

use super::restart::RECONNECT_WAIT;

pub struct Client {
    pub(super) runtime: Runtime,
    pub(super) stream: MessageReader<Stream>,
}

pub struct LockFeed {
    pub(super) stream: MessageReader<Stream>,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("the daemon is unreachable")]
    Unreachable,
    #[error("talking to the daemon failed: {0}")]
    Protocol(#[source] io::Error),
    #[error("the daemon and this program speak different protocol versions")]
    VersionMismatch {
        running: Option<u32>,
        installed: Option<u32>,
    },
}

pub trait DaemonService: Send + Sync {
    fn installed_version(&self) -> Option<u32>;
    fn restart(&self) -> bool;

    fn reconnect_wait(&self) -> Duration {
        RECONNECT_WAIT
    }
}

pub struct RestartOnce<S> {
    pub(super) inner: S,
    pub(super) used: AtomicBool,
}

pub struct Systemd;

pub struct NeverRestart;

pub(super) enum Hello {
    Ready(MessageReader<Stream>),
    Mismatch(Option<u32>),
}
