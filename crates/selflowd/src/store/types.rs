use jiff::{Timestamp, civil};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("time error: {0}")]
    Time(#[from] jiff::Error),
    #[error("unknown kind in the database: {0}")]
    UnknownKind(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    LockStarted,
    LockEnded,
    ExtensionGranted,
    Shutdown,
    ConfigSaved,
    RuleShutdown,
    ExceptionGranted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub at: Timestamp,
    pub logical_date: civil::Date,
    pub kind: EventKind,
    pub key: String,
    pub detail: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ShutdownDetail {
    pub(super) rule: String,
}
