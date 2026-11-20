use std::io;

use selflow_platform::transport::Stream;
use thiserror::Error;
use tokio::io::BufReader;
use tokio::runtime::Runtime;

pub struct Client {
    pub(super) runtime: Runtime,
    pub(super) stream: BufReader<Stream>,
}

pub struct LockFeed {
    pub(super) stream: BufReader<Stream>,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("the daemon is unreachable")]
    Unreachable,
    #[error("talking to the daemon failed: {0}")]
    Protocol(#[source] io::Error),
}
