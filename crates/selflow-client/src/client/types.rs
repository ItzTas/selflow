use std::io;

use selflow_platform::transport::Stream;
use selflow_protocol::ipc::MessageReader;
use thiserror::Error;
use tokio::runtime::Runtime;

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
