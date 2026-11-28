use std::io;
use std::process::ExitStatus;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("running the service manager failed: {0}")]
    Io(#[from] io::Error),
    #[error("the service manager has no unit for the daemon")]
    NoUnit,
    #[error("the service manager failed ({0})")]
    Failed(ExitStatus),
}
