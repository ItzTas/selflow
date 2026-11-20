use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ListenError {
    #[error("another daemon is already answering on the socket")]
    AlreadyRunning,
    #[error("socket error: {0}")]
    Io(#[from] io::Error),
}
