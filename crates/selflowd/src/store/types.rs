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
    AgentMessage,
    AgentDenied,
    AgentUnattended,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    pub rule_key: String,
    pub starts_at: Timestamp,
    pub announced_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredLock {
    pub identity: String,
    pub label: String,
    pub logical_date: civil::Date,
    pub test: bool,
    pub ended: bool,
    pub shutdown_on_screen: Option<Timestamp>,
    pub shut_down: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BootState {
    pub announced: Vec<Announcement>,
    pub sent: Vec<(String, Timestamp)>,
    pub shut_down: Vec<String>,
    pub locks: Vec<StoredLock>,
}
